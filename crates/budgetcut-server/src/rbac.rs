//! Role-based access control (§9). **The server is the only enforcement
//! boundary** — these pure functions decide whether a caller may write or read
//! an op, and the handlers/WS layer call them on every op and every stream.
//!
//! Department-scoped roles see and edit only their assigned categories; the
//! server filters their op stream and snapshot, never trusting the client to
//! hide out-of-scope financial data.

use budgetcut_core::ids::CategoryId;
use budgetcut_core::ops::{Op, OpKind};
use budgetcut_core::Budget;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Admin,
    Editor,
    DepartmentEditor,
    Commenter,
    Viewer,
    DepartmentViewer,
}

impl Role {
    fn is_department_scoped(self) -> bool {
        matches!(self, Role::DepartmentEditor | Role::DepartmentViewer)
    }
    fn can_write_any(self) -> bool {
        matches!(self, Role::Owner | Role::Admin | Role::Editor)
    }
}

/// A user's membership on a budget: their role and, for department-scoped
/// roles, the set of categories they're confined to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub role: Role,
    #[serde(default)]
    pub scope: Vec<CategoryId>,
}

impl Membership {
    pub fn full(role: Role) -> Self {
        Self {
            role,
            scope: vec![],
        }
    }
    pub fn department(role: Role, scope: Vec<CategoryId>) -> Self {
        Self { role, scope }
    }
}

/// Why an op was denied (returned to the client as a rejection reason).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Denied {
    #[error("read-only role cannot write")]
    ReadOnly,
    #[error("department-scoped role cannot edit shared tools")]
    SharedToolForbidden,
    #[error("op is outside your assigned categories")]
    OutOfScope,
}

/// The category an op affects, if it is tied to one. Shared tools (globals,
/// fringes, units, currencies, production totals, charges, credits) return
/// `None` — they're not department-partitioned.
pub fn op_category(kind: &OpKind, budget: &Budget) -> Option<CategoryId> {
    match kind {
        OpKind::InsertCategory(c) => Some(c.id),
        OpKind::RemoveCategory(id) => Some(*id),
        OpKind::InsertAccount(a) => Some(a.category),
        OpKind::RemoveAccount(id) => budget.accounts.get(id).map(|a| a.category),
        OpKind::InsertDetail(d) => budget.accounts.get(&d.account).map(|a| a.category),
        OpKind::RemoveDetail(id) => budget
            .details
            .get(id)
            .and_then(|d| budget.accounts.get(&d.account))
            .map(|a| a.category),
        OpKind::SetDetailField { detail, .. } => budget
            .details
            .get(detail)
            .and_then(|d| budget.accounts.get(&d.account))
            .map(|a| a.category),
        OpKind::InsertActual(a) => budget.accounts.get(&a.account).map(|acc| acc.category),
        OpKind::RemoveActual(id) => budget
            .actuals
            .get(id)
            .and_then(|a| budget.accounts.get(&a.account))
            .map(|acc| acc.category),
        OpKind::InsertPurchaseOrder(p) => budget.accounts.get(&p.account).map(|acc| acc.category),
        OpKind::RemovePurchaseOrder(id) => budget
            .purchase_orders
            .get(id)
            .and_then(|p| budget.accounts.get(&p.account))
            .map(|acc| acc.category),
        _ => None,
    }
}

/// May this membership *write* this op against the current authoritative state?
pub fn can_write(m: &Membership, op: &Op, budget: &Budget) -> Result<(), Denied> {
    match m.role {
        Role::Viewer | Role::DepartmentViewer | Role::Commenter => Err(Denied::ReadOnly),
        r if r.can_write_any() => Ok(()),
        Role::DepartmentEditor => match op_category(&op.kind, budget) {
            None => Err(Denied::SharedToolForbidden),
            Some(cat) => {
                if m.scope.contains(&cat) {
                    Ok(())
                } else {
                    Err(Denied::OutOfScope)
                }
            }
        },
        _ => unreachable!(),
    }
}

/// May this membership *see* this op (for snapshot + live stream filtering)?
pub fn can_read(m: &Membership, kind: &OpKind, budget: &Budget) -> bool {
    if !m.role.is_department_scoped() {
        return true; // full-budget roles read everything
    }
    match op_category(kind, budget) {
        None => true, // shared tools are needed to render any line
        Some(cat) => m.scope.contains(&cat),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use budgetcut_core::ids::*;
    use budgetcut_core::*;
    use rust_decimal::Decimal;

    fn budget_two_depts() -> (Budget, CategoryId, CategoryId, AccountId, AccountId) {
        let mut b = Budget::new("t", budgetcut_core::templates::try_currency());
        let mk_cat = |b: &mut Budget, n: &str| {
            let c = Category {
                id: CategoryId::new(),
                number: n.into(),
                description: Localized::tr(""),
                position: Decimal::ONE,
                atl_btl: None,
                applied_fringes: vec![],
            };
            let id = c.id;
            b.categories.insert(id, c);
            id
        };
        let art = mk_cat(&mut b, "2200");
        let cam = mk_cat(&mut b, "3100");
        let art_acc = Account {
            id: AccountId::new(),
            category: art,
            number: "2201".into(),
            description: Localized::tr(""),
            position: Decimal::ONE,
            show_subtotal: true,
            applied_fringes: vec![],
        };
        let cam_acc = Account {
            id: AccountId::new(),
            category: cam,
            number: "3101".into(),
            description: Localized::tr(""),
            position: Decimal::ONE,
            show_subtotal: true,
            applied_fringes: vec![],
        };
        let (aa, ca) = (art_acc.id, cam_acc.id);
        b.accounts.insert(aa, art_acc);
        b.accounts.insert(ca, cam_acc);
        (b, art, cam, aa, ca)
    }

    fn op(kind: OpKind) -> Op {
        Op::new(Hlc::new(1, 0, UserId::new()), UserId::new(), kind)
    }

    #[test]
    fn viewer_cannot_write() {
        let (b, _art, _cam, art_acc, _) = budget_two_depts();
        let m = Membership::full(Role::Viewer);
        let o = op(OpKind::RemoveAccount(art_acc));
        assert_eq!(can_write(&m, &o, &b), Err(Denied::ReadOnly));
    }

    #[test]
    fn editor_can_write_anything() {
        let (b, _, _, art_acc, _) = budget_two_depts();
        let m = Membership::full(Role::Editor);
        assert!(can_write(&m, &op(OpKind::RemoveAccount(art_acc)), &b).is_ok());
    }

    #[test]
    fn department_editor_confined_to_scope() {
        let (b, art, _cam, art_acc, cam_acc) = budget_two_depts();
        let m = Membership::department(Role::DepartmentEditor, vec![art]);
        // can edit its own department's account
        assert!(can_write(&m, &op(OpKind::RemoveAccount(art_acc)), &b).is_ok());
        // cannot touch the camera department
        assert_eq!(
            can_write(&m, &op(OpKind::RemoveAccount(cam_acc)), &b),
            Err(Denied::OutOfScope)
        );
        // cannot edit shared globals
        let g = Global {
            id: GlobalId::new(),
            name: "X".into(),
            description: Localized::tr(""),
            value: Formula::Const(Decimal::ONE),
            in_budget_total: true,
        };
        assert_eq!(
            can_write(&m, &op(OpKind::InsertGlobal(g)), &b),
            Err(Denied::SharedToolForbidden)
        );
    }

    #[test]
    fn department_viewer_reads_only_its_scope() {
        let (b, art, _cam, art_acc, cam_acc) = budget_two_depts();
        let m = Membership::department(Role::DepartmentViewer, vec![art]);
        assert!(can_read(&m, &OpKind::RemoveAccount(art_acc), &b));
        assert!(!can_read(&m, &OpKind::RemoveAccount(cam_acc), &b));
        // shared tools remain visible
        assert!(can_read(&m, &OpKind::RemoveCurrency(b.base_currency), &b));
    }

    #[test]
    fn full_roles_read_everything() {
        let (b, _, _, _, cam_acc) = budget_two_depts();
        let m = Membership::full(Role::Viewer);
        assert!(can_read(&m, &OpKind::RemoveAccount(cam_acc), &b));
    }
}
