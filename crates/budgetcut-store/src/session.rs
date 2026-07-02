//! A live editing session: an in-memory [`Document`] backed by a [`Store`],
//! with an [`HlcClock`] for minting op timestamps. Every edit becomes an op,
//! is applied through the shared core reducer, and is persisted to the log.

use budgetcut_core::actuals::Actual;
use budgetcut_core::ids::{DetailId, GlobalId, UserId};
use budgetcut_core::po::PurchaseOrder;
use budgetcut_core::scheduling::Strip;
use budgetcut_core::settlement::Receipt;
use budgetcut_core::{
    evaluate, ApplyResult, Budget, Detail, DetailField, Document, Formula, HlcClock, Op, OpKind,
};
use rust_decimal::Decimal;

use crate::dto::{
    ActualsReportDto, NationalSheetDto, NetflixBudgetDto, NetflixCashFlowDto, NetflixCashInput,
    NetflixCostReportDto, NetflixHeaderInput, NetflixTrialBalanceDto, NetflixTrialInput,
    PurchaseOrdersDto, ScheduleDto, SettlementReportDto, ToolsDto, TopsheetDto, TreeDto,
};
use crate::error::Result;
use crate::store::{now_ms, Store};

/// An open budget: state + persistence + clock.
pub struct Session {
    store: Store,
    doc: Document,
    clock: HlcClock,
    author: UserId,
}

impl Session {
    /// Create a fresh budget from `base` (e.g. a seeded template) and persist it.
    pub fn create(mut store: Store, base: Budget, author: UserId) -> Result<Self> {
        store.create_budget(&base)?;
        Ok(Self {
            store,
            doc: Document::new(base),
            clock: HlcClock::new(author),
            author,
        })
    }

    /// Open an existing budget, replaying its op log and re-seeding the clock
    /// so new ops strictly post-date persisted ones (§8 / HLC restart safety).
    pub fn open(store: Store, author: UserId) -> Result<Self> {
        let (doc, max_hlc) = store.load_document()?;
        let clock = match max_hlc {
            Some(h) => HlcClock::seeded(author, h),
            None => HlcClock::new(author),
        };
        Ok(Self {
            store,
            doc,
            clock,
            author,
        })
    }

    /// The live budget state.
    pub fn budget(&self) -> &Budget {
        &self.doc.budget
    }

    /// Outbox: ops not yet acknowledged by a server.
    pub fn outbox(&self) -> Result<Vec<Op>> {
        self.store.outbox()
    }

    /// Replace the whole budget with `base` (e.g. reload the sample template),
    /// discarding the current op log and re-seeding the clock.
    pub fn reseed(&mut self, base: Budget) -> Result<()> {
        self.store.reset(&base)?;
        self.clock = HlcClock::new(self.author);
        self.doc = Document::new(base);
        Ok(())
    }

    /// Apply an edit: mint an op, run it through the reducer, and (if it took
    /// effect) persist it to the log. Returns the reducer outcome.
    pub fn edit(&mut self, kind: OpKind) -> Result<ApplyResult> {
        let hlc = self.clock.tick(now_ms());
        let op = Op::new(hlc, self.author, kind);
        let outcome = self.doc.apply(&op);
        if matches!(outcome, ApplyResult::Applied | ApplyResult::Buffered) {
            self.store.append_op(&op)?;
        }
        Ok(outcome)
    }

    // ---- convenience editors (the Tauri commands call these) ----

    pub fn insert_detail(&mut self, detail: Detail) -> Result<ApplyResult> {
        self.edit(OpKind::InsertDetail(detail))
    }

    pub fn remove_detail(&mut self, id: DetailId) -> Result<ApplyResult> {
        self.edit(OpKind::RemoveDetail(id))
    }

    pub fn set_detail_field(
        &mut self,
        detail: DetailId,
        field: DetailField,
    ) -> Result<ApplyResult> {
        self.edit(OpKind::SetDetailField { detail, field })
    }

    pub fn set_global(&mut self, global: GlobalId, value: Formula) -> Result<ApplyResult> {
        self.edit(OpKind::SetGlobalValue { global, value })
    }

    pub fn add_actual(&mut self, actual: Actual) -> Result<ApplyResult> {
        self.edit(OpKind::InsertActual(actual))
    }

    pub fn remove_actual(&mut self, id: uuid::Uuid) -> Result<ApplyResult> {
        self.edit(OpKind::RemoveActual(id))
    }

    pub fn add_receipt(&mut self, receipt: Receipt) -> Result<ApplyResult> {
        self.edit(OpKind::InsertReceipt(receipt))
    }

    pub fn remove_receipt(&mut self, id: uuid::Uuid) -> Result<ApplyResult> {
        self.edit(OpKind::RemoveReceipt(id))
    }

    pub fn add_strip(&mut self, strip: Strip) -> Result<ApplyResult> {
        self.edit(OpKind::InsertStrip(strip))
    }

    pub fn remove_strip(&mut self, id: uuid::Uuid) -> Result<ApplyResult> {
        self.edit(OpKind::RemoveStrip(id))
    }

    /// Insert or replace a PO (status changes re-insert the whole record).
    pub fn put_purchase_order(&mut self, po: PurchaseOrder) -> Result<ApplyResult> {
        self.edit(OpKind::InsertPurchaseOrder(po))
    }

    pub fn remove_purchase_order(&mut self, id: uuid::Uuid) -> Result<ApplyResult> {
        self.edit(OpKind::RemovePurchaseOrder(id))
    }

    /// A PO by id (for the approve/convert read-modify-write).
    #[must_use]
    pub fn purchase_order(&self, id: uuid::Uuid) -> Option<PurchaseOrder> {
        self.doc.budget.purchase_orders.get(&id).cloned()
    }

    // ---- views ----

    /// Compute and project the topsheet.
    pub fn topsheet(&self) -> TopsheetDto {
        let r = evaluate(&self.doc.budget);
        TopsheetDto::build(&self.doc.budget, &r)
    }

    /// Compute and project the full editable budget tree.
    pub fn tree(&self) -> TreeDto {
        let r = evaluate(&self.doc.budget);
        TreeDto::build(&self.doc.budget, &r)
    }

    /// Project the budget in the national dizi sheet layout (Ulusal Dizi Formatı).
    pub fn national_sheet(&self) -> NationalSheetDto {
        let r = evaluate(&self.doc.budget);
        NationalSheetDto::build(&self.doc.budget, &r)
    }

    /// Netflix budget topsheet (sections grouped by Netflix CoA band).
    pub fn netflix_budget(&self, header: &NetflixHeaderInput) -> NetflixBudgetDto {
        let r = evaluate(&self.doc.budget);
        NetflixBudgetDto::build(&self.doc.budget, &r, header)
    }

    /// Netflix cost report (actuals / commitments / ETC / EFC vs budget).
    pub fn netflix_cost_report(&self, header: &NetflixHeaderInput) -> NetflixCostReportDto {
        let r = evaluate(&self.doc.budget);
        NetflixCostReportDto::build(&self.doc.budget, &r, header)
    }

    /// Netflix weekly cash-flow (cash-out) matrix.
    pub fn netflix_cash_flow(&self, input: &NetflixCashInput) -> NetflixCashFlowDto {
        NetflixCashFlowDto::build(&self.doc.budget, input)
    }

    /// Netflix trial balance (cash position snapshot).
    pub fn netflix_trial_balance(&self, input: &NetflixTrialInput) -> NetflixTrialBalanceDto {
        NetflixTrialBalanceDto::build(&self.doc.budget, input)
    }

    /// Project the Setup Tools (fringes, globals, units).
    pub fn tools(&self) -> ToolsDto {
        ToolsDto::build(&self.doc.budget)
    }

    /// Estimate-vs-actual / EFC report + invoice lines (§16 Phase 3).
    pub fn actuals(&self) -> ActualsReportDto {
        let r = evaluate(&self.doc.budget);
        ActualsReportDto::build(&self.doc.budget, &r)
    }

    /// Expense settlement ("Hesap Kapama") reconciled against `advance`.
    pub fn settlement(&self, advance: Decimal) -> SettlementReportDto {
        SettlementReportDto::build(&self.doc.budget, advance)
    }

    /// Stripboard + Day-Out-of-Days (§16 scheduling).
    pub fn schedule(&self) -> ScheduleDto {
        ScheduleDto::build(&self.doc.budget)
    }

    /// Purchase orders + committed totals.
    pub fn purchase_orders(&self) -> PurchaseOrdersDto {
        PurchaseOrdersDto::build(&self.doc.budget)
    }
}
