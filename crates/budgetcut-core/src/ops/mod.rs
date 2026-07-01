//! Mutation ops + the per-field Last-Write-Wins reducer (§8).
//!
//! This is the other half of the keystone (§4): the **same** [`Document::apply`]
//! runs optimistically on the client and authoritatively on the server, so both
//! converge to identical state. There is no CRDT — just per-field LWW keyed by
//! [`Hlc`], with idempotent application (dedupe by [`OpId`]).
//!
//! Convergence guarantee: for a given *set* of ops, the materialized
//! [`Budget`] is independent of application order. We achieve this with:
//!
//! * **Per-field registers** — each field stores the HLC of its last writer;
//!   a write applies only if it strictly exceeds the stored HLC.
//! * **Existence registers** — insert and remove of an entity contend on one
//!   register; the higher HLC decides whether the entity exists.
//! * **A pending buffer** — a field op that arrives before its entity's insert
//!   is held and flushed when the insert lands, so reordering across the
//!   network can't drop edits.
//!
//! Invariant assumed by callers: an entity's `Insert` is causally first (its
//! HLC is the lowest among that entity's ops). Clients generate ops in HLC
//! order, so this always holds in practice.

use std::collections::{HashMap, HashSet};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::hlc::Hlc;
use crate::ids::*;
use crate::model::*;

mod fields {
    //! Stable numeric field tags, combined with an entity uuid into a register
    //! key. `EXISTS` is shared by insert/remove.
    pub const EXISTS: u16 = 0;
    // Detail fields
    pub const D_DESC: u16 = 1;
    pub const D_NAME: u16 = 2;
    pub const D_AMOUNT: u16 = 3;
    pub const D_MULT: u16 = 4;
    pub const D_RATE: u16 = 5;
    pub const D_UNIT: u16 = 6;
    pub const D_CURRENCY: u16 = 7;
    pub const D_POSITION: u16 = 8;
    pub const D_FRINGES: u16 = 9;
    pub const D_GROUPS: u16 = 10;
    pub const D_LOCATION: u16 = 11;
    pub const D_SET: u16 = 12;
    pub const D_GLCODE: u16 = 13;
    pub const D_NOTES: u16 = 14;
    // Global value
    pub const G_VALUE: u16 = 20;

    /// Every settable detail field tag — seeded on `InsertDetail` so the
    /// payload claims its registers at the insert's HLC.
    pub const DETAIL_TAGS: [u16; 14] = [
        D_DESC, D_NAME, D_AMOUNT, D_MULT, D_RATE, D_UNIT, D_CURRENCY, D_POSITION, D_FRINGES,
        D_GROUPS, D_LOCATION, D_SET, D_GLCODE, D_NOTES,
    ];
}

/// A single settable field on a [`Detail`], carrying its new value. Each maps
/// to a distinct register tag so independent fields never contend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailField {
    Description(String),
    Name(Option<String>),
    Amount(Formula),
    Multiplier(Formula),
    Rate(Formula),
    Unit(UnitId),
    Currency(CurrencyId),
    Position(Decimal),
    /// Whole-list LWW (granular per-fringe ops can come later).
    Fringes(Vec<AppliedFringe>),
    Groups(Vec<GroupId>),
    Location(Option<LocationId>),
    Set(Option<SetId>),
    GlCode(Option<String>),
    Notes(Option<String>),
}

impl DetailField {
    fn tag(&self) -> u16 {
        match self {
            DetailField::Description(_) => fields::D_DESC,
            DetailField::Name(_) => fields::D_NAME,
            DetailField::Amount(_) => fields::D_AMOUNT,
            DetailField::Multiplier(_) => fields::D_MULT,
            DetailField::Rate(_) => fields::D_RATE,
            DetailField::Unit(_) => fields::D_UNIT,
            DetailField::Currency(_) => fields::D_CURRENCY,
            DetailField::Position(_) => fields::D_POSITION,
            DetailField::Fringes(_) => fields::D_FRINGES,
            DetailField::Groups(_) => fields::D_GROUPS,
            DetailField::Location(_) => fields::D_LOCATION,
            DetailField::Set(_) => fields::D_SET,
            DetailField::GlCode(_) => fields::D_GLCODE,
            DetailField::Notes(_) => fields::D_NOTES,
        }
    }

    fn assign(self, d: &mut Detail) {
        match self {
            DetailField::Description(v) => d.description = v,
            DetailField::Name(v) => d.name = v,
            DetailField::Amount(v) => d.amount = v,
            DetailField::Multiplier(v) => d.multiplier = v,
            DetailField::Rate(v) => d.rate = v,
            DetailField::Unit(v) => d.unit = v,
            DetailField::Currency(v) => d.currency = v,
            DetailField::Position(v) => d.position = v,
            DetailField::Fringes(v) => d.applied_fringes = v,
            DetailField::Groups(v) => d.groups = v,
            DetailField::Location(v) => d.location = v,
            DetailField::Set(v) => d.set = v,
            DetailField::GlCode(v) => d.gl_code = v,
            DetailField::Notes(v) => d.notes = v,
        }
    }
}

/// What an op does. Inserts carry the full entity payload; removes carry an id;
/// field sets carry the target id and the new value.
///
/// Adjacently tagged (`{"op": "...", "data": ...}`): the `Remove*` variants
/// wrap a `#[serde(transparent)]` id that serializes as a bare string, which an
/// *internally* tagged enum cannot represent. Adjacent tagging round-trips every
/// variant — exercised by `serializes_every_opkind_variant`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", content = "data", rename_all = "snake_case")]
pub enum OpKind {
    InsertCategory(Category),
    RemoveCategory(CategoryId),
    InsertAccount(Account),
    RemoveAccount(AccountId),
    InsertDetail(Detail),
    RemoveDetail(DetailId),
    InsertGlobal(Global),
    RemoveGlobal(GlobalId),
    InsertFringe(Fringe),
    RemoveFringe(FringeId),
    InsertUnit(Unit),
    RemoveUnit(UnitId),
    InsertGroup(Group),
    RemoveGroup(GroupId),
    InsertCurrency(Currency),
    RemoveCurrency(CurrencyId),
    InsertProductionTotal(ProductionTotal),
    RemoveProductionTotal(ProductionTotalId),
    /// Record an actual (invoice/expense) against an account (§16 Phase 3).
    InsertActual(crate::actuals::Actual),
    RemoveActual(Uuid),
    /// Record a settlement receipt (fiş) for petty-cash closing ("Hesap Kapama").
    InsertReceipt(crate::settlement::Receipt),
    RemoveReceipt(Uuid),
    /// Add/remove a stripboard strip (§16 scheduling).
    InsertStrip(crate::scheduling::Strip),
    RemoveStrip(Uuid),
    /// Insert/replace (status changes are LWW whole-record) or remove a PO.
    InsertPurchaseOrder(crate::po::PurchaseOrder),
    RemovePurchaseOrder(Uuid),
    SetDetailField {
        detail: DetailId,
        field: DetailField,
    },
    SetGlobalValue {
        global: GlobalId,
        value: Formula,
    },
}

/// A mutation, timestamped and attributed. Idempotent by [`OpId`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Op {
    pub id: OpId,
    pub hlc: Hlc,
    pub author: UserId,
    pub kind: OpKind,
}

impl Op {
    #[must_use]
    pub fn new(hlc: Hlc, author: UserId, kind: OpKind) -> Self {
        Self {
            id: OpId::new(),
            hlc,
            author,
            kind,
        }
    }
}

/// Outcome of applying an op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyResult {
    /// The op won its register(s) and mutated state.
    Applied,
    /// A higher-HLC write already won; the op was a no-op.
    Stale,
    /// Already applied (deduped by op id).
    Idempotent,
    /// Field op held until its entity's insert arrives.
    Buffered,
}

type FieldKey = (Uuid, u16);

/// The replicated document: the materialized [`Budget`] plus the LWW
/// bookkeeping needed to merge concurrent ops. `budget` is the meaningful
/// state (what collaborators see and what calc runs on); the rest is internal.
#[derive(Debug, Clone)]
pub struct Document {
    pub budget: Budget,
    /// HLC of the last writer for each `(entity, field)` register.
    lww: HashMap<FieldKey, Hlc>,
    /// Applied op ids, for idempotent re-delivery.
    applied: HashSet<OpId>,
    /// Field ops awaiting their entity's insert, keyed by entity uuid.
    pending: HashMap<Uuid, Vec<Op>>,
}

impl Document {
    #[must_use]
    pub fn new(budget: Budget) -> Self {
        Self {
            budget,
            lww: HashMap::new(),
            applied: HashSet::new(),
            pending: HashMap::new(),
        }
    }

    #[must_use]
    pub fn applied_count(&self) -> usize {
        self.applied.len()
    }

    /// Apply an op, enforcing idempotency and per-field LWW.
    pub fn apply(&mut self, op: &Op) -> ApplyResult {
        if self.applied.contains(&op.id) {
            return ApplyResult::Idempotent;
        }
        self.applied.insert(op.id);
        self.dispatch(op)
    }

    fn dispatch(&mut self, op: &Op) -> ApplyResult {
        let hlc = op.hlc;
        match &op.kind {
            OpKind::InsertCategory(c) => self.insert(c.id.as_uuid(), hlc, |b| {
                b.categories.insert(c.id, c.clone());
            }),
            OpKind::RemoveCategory(id) => self.remove(id.as_uuid(), hlc, |b| {
                b.categories.remove(id);
            }),
            OpKind::InsertAccount(a) => self.insert(a.id.as_uuid(), hlc, |b| {
                b.accounts.insert(a.id, a.clone());
            }),
            OpKind::RemoveAccount(id) => self.remove(id.as_uuid(), hlc, |b| {
                b.accounts.remove(id);
            }),
            OpKind::InsertDetail(d) => {
                self.insert_seeded(d.id.as_uuid(), hlc, &fields::DETAIL_TAGS, |b| {
                    b.details.insert(d.id, d.clone());
                })
            }
            OpKind::RemoveDetail(id) => self.remove(id.as_uuid(), hlc, |b| {
                b.details.remove(id);
            }),
            OpKind::InsertGlobal(g) => {
                self.insert_seeded(g.id.as_uuid(), hlc, &[fields::G_VALUE], |b| {
                    b.globals.insert(g.id, g.clone());
                })
            }
            OpKind::RemoveGlobal(id) => self.remove(id.as_uuid(), hlc, |b| {
                b.globals.remove(id);
            }),
            OpKind::InsertFringe(f) => self.insert(f.id.as_uuid(), hlc, |b| {
                b.fringes.insert(f.id, f.clone());
            }),
            OpKind::RemoveFringe(id) => self.remove(id.as_uuid(), hlc, |b| {
                b.fringes.remove(id);
            }),
            OpKind::InsertUnit(u) => self.insert(u.id.as_uuid(), hlc, |b| {
                b.units.insert(u.id, u.clone());
            }),
            OpKind::RemoveUnit(id) => self.remove(id.as_uuid(), hlc, |b| {
                b.units.remove(id);
            }),
            OpKind::InsertGroup(g) => self.insert(g.id.as_uuid(), hlc, |b| {
                b.groups.insert(g.id, g.clone());
            }),
            OpKind::RemoveGroup(id) => self.remove(id.as_uuid(), hlc, |b| {
                b.groups.remove(id);
            }),
            OpKind::InsertCurrency(c) => self.insert(c.id.as_uuid(), hlc, |b| {
                b.currencies.insert(c.id, c.clone());
            }),
            OpKind::RemoveCurrency(id) => self.remove(id.as_uuid(), hlc, |b| {
                b.currencies.remove(id);
            }),
            OpKind::InsertProductionTotal(p) => self.insert(p.id.as_uuid(), hlc, |b| {
                b.production_totals.insert(p.id, p.clone());
            }),
            OpKind::RemoveProductionTotal(id) => self.remove(id.as_uuid(), hlc, |b| {
                b.production_totals.remove(id);
            }),
            OpKind::InsertActual(a) => self.insert(a.id, hlc, |b| {
                b.actuals.insert(a.id, a.clone());
            }),
            OpKind::RemoveActual(id) => self.remove(*id, hlc, |b| {
                b.actuals.remove(id);
            }),
            OpKind::InsertReceipt(r) => self.insert(r.id, hlc, |b| {
                b.receipts.insert(r.id, r.clone());
            }),
            OpKind::RemoveReceipt(id) => self.remove(*id, hlc, |b| {
                b.receipts.remove(id);
            }),
            OpKind::InsertStrip(s) => self.insert(s.id, hlc, |b| {
                b.strips.insert(s.id, s.clone());
            }),
            OpKind::RemoveStrip(id) => self.remove(*id, hlc, |b| {
                b.strips.remove(id);
            }),
            OpKind::InsertPurchaseOrder(p) => self.insert(p.id, hlc, |b| {
                b.purchase_orders.insert(p.id, p.clone());
            }),
            OpKind::RemovePurchaseOrder(id) => self.remove(*id, hlc, |b| {
                b.purchase_orders.remove(id);
            }),
            OpKind::SetDetailField { detail, field } => self.set_detail_field(*detail, field, op),
            OpKind::SetGlobalValue { global, value } => self.set_global_value(*global, value, op),
        }
    }

    /// Insert an entity if it wins the existence register, then flush any field
    /// ops that were waiting on it. Seeds no field registers (for entities with
    /// no `SetField` ops).
    fn insert(&mut self, uuid: Uuid, hlc: Hlc, do_insert: impl FnOnce(&mut Budget)) -> ApplyResult {
        self.insert_seeded(uuid, hlc, &[], do_insert)
    }

    /// Insert an entity, claiming both the existence register *and* the given
    /// field registers at this HLC. Seeding the field registers means a later
    /// `SetField` only wins if it strictly post-dates the insert — so a stale
    /// lower-HLC field set can't overwrite a fresh insert, and a buffered edit
    /// for a *previous* incarnation can't leak onto a resurrected one. This is
    /// what makes the merge order-independent without assuming inserts arrive
    /// first.
    fn insert_seeded(
        &mut self,
        uuid: Uuid,
        hlc: Hlc,
        seed_tags: &[u16],
        do_insert: impl FnOnce(&mut Budget),
    ) -> ApplyResult {
        let key = (uuid, fields::EXISTS);
        if !Self::wins(&self.lww, key, hlc) {
            return ApplyResult::Stale;
        }
        self.lww.insert(key, hlc);
        for &tag in seed_tags {
            let k = (uuid, tag);
            if Self::wins(&self.lww, k, hlc) {
                self.lww.insert(k, hlc);
            }
        }
        do_insert(&mut self.budget);
        self.flush_pending(uuid);
        ApplyResult::Applied
    }

    /// Remove an entity if it wins the existence register. Buffered field ops
    /// that can never win again (HLC below this removal) are evicted; any with a
    /// higher HLC are retained in case a later higher-HLC insert resurrects the
    /// entity.
    fn remove(&mut self, uuid: Uuid, hlc: Hlc, do_remove: impl FnOnce(&mut Budget)) -> ApplyResult {
        let key = (uuid, fields::EXISTS);
        if !Self::wins(&self.lww, key, hlc) {
            return ApplyResult::Stale;
        }
        self.lww.insert(key, hlc);
        do_remove(&mut self.budget);
        if let Some(buf) = self.pending.get_mut(&uuid) {
            buf.retain(|op| op.hlc > hlc);
            if buf.is_empty() {
                self.pending.remove(&uuid);
            }
        }
        ApplyResult::Applied
    }

    fn set_detail_field(&mut self, detail: DetailId, field: &DetailField, op: &Op) -> ApplyResult {
        let uuid = detail.as_uuid();
        if !self.budget.details.contains_key(&detail) {
            self.buffer(op);
            return ApplyResult::Buffered;
        }
        let key = (uuid, field.tag());
        if !Self::wins(&self.lww, key, op.hlc) {
            return ApplyResult::Stale;
        }
        self.lww.insert(key, op.hlc);
        if let Some(d) = self.budget.details.get_mut(&detail) {
            field.clone().assign(d);
        }
        ApplyResult::Applied
    }

    fn set_global_value(&mut self, global: GlobalId, value: &Formula, op: &Op) -> ApplyResult {
        let uuid = global.as_uuid();
        if !self.budget.globals.contains_key(&global) {
            self.buffer(op);
            return ApplyResult::Buffered;
        }
        let key = (uuid, fields::G_VALUE);
        if !Self::wins(&self.lww, key, op.hlc) {
            return ApplyResult::Stale;
        }
        self.lww.insert(key, op.hlc);
        if let Some(g) = self.budget.globals.get_mut(&global) {
            g.value = value.clone();
        }
        ApplyResult::Applied
    }

    /// Re-apply field ops that were waiting on `uuid`'s entity.
    fn flush_pending(&mut self, uuid: Uuid) {
        let Some(buffered) = self.pending.remove(&uuid) else {
            return;
        };
        for op in buffered {
            match &op.kind {
                OpKind::SetDetailField { detail, field } => {
                    let key = (detail.as_uuid(), field.tag());
                    if Self::wins(&self.lww, key, op.hlc) {
                        self.lww.insert(key, op.hlc);
                        if let Some(d) = self.budget.details.get_mut(detail) {
                            field.clone().assign(d);
                        }
                    }
                }
                OpKind::SetGlobalValue { global, value } => {
                    let key = (global.as_uuid(), fields::G_VALUE);
                    if Self::wins(&self.lww, key, op.hlc) {
                        self.lww.insert(key, op.hlc);
                        if let Some(g) = self.budget.globals.get_mut(global) {
                            g.value = value.clone();
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The `(entity, field-tag)` a field op targets, if any.
    fn op_field_key(op: &Op) -> Option<FieldKey> {
        match &op.kind {
            OpKind::SetDetailField { detail, field } => Some((detail.as_uuid(), field.tag())),
            OpKind::SetGlobalValue { global, .. } => Some((global.as_uuid(), fields::G_VALUE)),
            _ => None,
        }
    }

    /// Buffer a field op until its entity's insert arrives, keeping only the
    /// highest-HLC op per `(entity, field)` — lower-HLC writes can never win, so
    /// the buffer is bounded to one entry per field rather than growing without
    /// limit.
    fn buffer(&mut self, op: &Op) {
        let Some(key) = Self::op_field_key(op) else {
            return;
        };
        let buf = self.pending.entry(key.0).or_default();
        if let Some(slot) = buf.iter_mut().find(|o| Self::op_field_key(o) == Some(key)) {
            if op.hlc > slot.hlc {
                *slot = op.clone();
            }
        } else {
            buf.push(op.clone());
        }
    }

    /// A write wins iff its HLC strictly exceeds the register's current HLC
    /// (or the register is empty).
    fn wins(lww: &HashMap<FieldKey, Hlc>, key: FieldKey, hlc: Hlc) -> bool {
        lww.get(&key).map_or(true, |existing| hlc > *existing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlc::HlcClock;
    use crate::templates::try_currency;
    use rust_decimal_macros::dec;

    fn detail(account: AccountId, currency: CurrencyId, unit: UnitId) -> Detail {
        Detail {
            id: DetailId::new(),
            account,
            position: dec!(1),
            description: "x".into(),
            name: None,
            amount: Formula::Const(dec!(1)),
            multiplier: Formula::Const(Decimal::ONE),
            rate: Formula::Const(dec!(100)),
            unit,
            currency,
            applied_fringes: vec![],
            groups: vec![],
            location: None,
            set: None,
            gl_code: None,
            notes: None,
        }
    }

    #[test]
    fn idempotent_reapply() {
        let mut doc = Document::new(Budget::new("t", try_currency()));
        let author = UserId::new();
        let mut clk = HlcClock::new(author);
        let unit = Unit {
            id: UnitId::new(),
            code: "F".into(),
            name: Localized::tr(""),
            factor: Decimal::ONE,
        };
        let op = Op::new(clk.tick(1), author, OpKind::InsertUnit(unit));
        assert_eq!(doc.apply(&op), ApplyResult::Applied);
        assert_eq!(doc.apply(&op), ApplyResult::Idempotent);
        assert_eq!(doc.budget.units.len(), 1);
    }

    #[test]
    fn higher_hlc_wins_field() {
        let author = UserId::new();
        let mut clk = HlcClock::new(author);
        let mut b = Budget::new("t", try_currency());
        let unit = Unit {
            id: UnitId::new(),
            code: "F".into(),
            name: Localized::tr(""),
            factor: Decimal::ONE,
        };
        let uid = unit.id;
        b.units.insert(uid, unit);
        let cat = Category {
            id: CategoryId::new(),
            number: "1".into(),
            description: Localized::tr(""),
            position: dec!(1),
            atl_btl: None,
            applied_fringes: vec![],
        };
        let acc = Account {
            id: AccountId::new(),
            category: cat.id,
            number: "1".into(),
            description: Localized::tr(""),
            position: dec!(1),
            show_subtotal: true,
            applied_fringes: vec![],
        };
        let cur = b.base_currency;
        let det = detail(acc.id, cur, uid);
        let did = det.id;
        b.categories.insert(cat.id, cat);
        b.accounts.insert(acc.id, acc);
        b.details.insert(det.id, det);
        let mut doc = Document::new(b);

        let early = clk.tick(10);
        let late = clk.tick(20);
        // Apply the LATER op first, then the earlier; earlier must lose.
        doc.apply(&Op::new(
            late,
            author,
            OpKind::SetDetailField {
                detail: did,
                field: DetailField::Rate(Formula::Const(dec!(999))),
            },
        ));
        doc.apply(&Op::new(
            early,
            author,
            OpKind::SetDetailField {
                detail: did,
                field: DetailField::Rate(Formula::Const(dec!(111))),
            },
        ));
        assert_eq!(doc.budget.details[&did].rate, Formula::Const(dec!(999)));
    }

    #[test]
    fn field_op_before_insert_is_buffered_then_flushed() {
        let author = UserId::new();
        let mut clk = HlcClock::new(author);
        let mut b = Budget::new("t", try_currency());
        let unit = Unit {
            id: UnitId::new(),
            code: "F".into(),
            name: Localized::tr(""),
            factor: Decimal::ONE,
        };
        let uid = unit.id;
        b.units.insert(uid, unit);
        let cat = Category {
            id: CategoryId::new(),
            number: "1".into(),
            description: Localized::tr(""),
            position: dec!(1),
            atl_btl: None,
            applied_fringes: vec![],
        };
        let acc = Account {
            id: AccountId::new(),
            category: cat.id,
            number: "1".into(),
            description: Localized::tr(""),
            position: dec!(1),
            show_subtotal: true,
            applied_fringes: vec![],
        };
        b.categories.insert(cat.id, cat);
        b.accounts.insert(acc.id, acc.clone());
        let cur = b.base_currency;
        let det = detail(acc.id, cur, uid);
        let did = det.id;
        let mut doc = Document::new(b);

        let insert_hlc = clk.tick(10);
        let set_hlc = clk.tick(20);
        // Deliver the field op BEFORE the insert.
        assert_eq!(
            doc.apply(&Op::new(
                set_hlc,
                author,
                OpKind::SetDetailField {
                    detail: did,
                    field: DetailField::Description("late".into())
                }
            )),
            ApplyResult::Buffered
        );
        assert_eq!(
            doc.apply(&Op::new(insert_hlc, author, OpKind::InsertDetail(det))),
            ApplyResult::Applied
        );
        assert_eq!(doc.budget.details[&did].description, "late");
    }
}
