//! ResolveCrisis business-data decisions from a real, paid Current Crisis.
//!
//! This reproduces the crisis-specific legacy rule order. It deliberately does
//! not verify an agent, select a FIFO action, issue an Accepted/RuleOutcome,
//! append an event, charge pricing, or confer publication authority.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
};

/// Exact crisis-only legacy rejection order, after the authoritative agent
/// check that belongs to the future sole root action caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrisisResolutionDataRejectionV1 {
    EmptyId,
    Missing,
    Inactive,
    Expired,
    EmptyStrategy,
}

/// Real Current data, retained in the caller's original Ordinary budget. This
/// is not a minted Selected, Accepted, Rule, or event/publish permission.
pub struct CrisisResolutionDataV1 {
    current: ReservedValue<CrisisState>,
    impact: i64,
}
impl CrisisResolutionDataV1 {
    pub fn current(&self) -> &CrisisState {
        &self.current
    }
    pub fn impact(&self) -> i64 {
        self.impact
    }
    pub fn uses_budget(&self, memory: &oasis7_proto::memory_budget::MemoryBudget) -> bool {
        self.current.uses_budget(memory)
    }
}

/// The only Ready result carries the original paid actual-Current owner.
/// Rejections are ordinary business decisions and do not poison the original
/// transaction; original I/O, budget, foreign scope or cancel errors do.
pub enum CrisisResolutionDataOutcomeV1 {
    Rejected(CrisisResolutionDataRejectionV1),
    Ready(CrisisResolutionDataV1),
}

/// Checks the crisis-specific legacy ResolveCrisis rule on a complete real
/// Current child. The sole root caller must first authenticate the action's
/// submitting agent and current tick. This routine deliberately cannot
/// manufacture that entitlement, an Accepted result or an Event.
///
/// The original World resolver trims the map key and strategy and checks:
/// present -> Active -> time <= expires_at -> nonempty strategy -> signed
/// impact. Success and failure both use max(severity, 1) * 10.
#[allow(clippy::too_many_arguments)]
pub fn decide_current_crisis_resolution_data_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    strategy: &str,
    success: bool,
    now: u64,
    parent: &ReservedValue<DiplomacyFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max_text_bytes: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CrisisResolutionDataOutcomeV1, WorldFieldOverlayErrorV1> {
    use CrisisResolutionDataOutcomeV1::{Ready, Rejected};
    use CrisisResolutionDataRejectionV1 as Why;
    let id = id.trim();
    if id.is_empty() {
        return Ok(Rejected(Why::EmptyId));
    }
    let Some(current) = read_current_diplomacy_crisis_state_v1(
        id,
        parent,
        roots,
        tx,
        ctx,
        scan,
        cells,
        max_text_bytes,
        guard,
        live,
    )?
    else {
        return Ok(Rejected(Why::Missing));
    };
    if current.status != CrisisStatus::Active {
        return Ok(Rejected(Why::Inactive));
    }
    if now > current.expires_at {
        return Ok(Rejected(Why::Expired));
    }
    if strategy.trim().is_empty() {
        return Ok(Rejected(Why::EmptyStrategy));
    }
    const LEGACY_IMPACT_PER_SEVERITY: i64 = 10;
    let magnitude = i64::from(current.severity.max(1)).saturating_mul(LEGACY_IMPACT_PER_SEVERITY);
    let impact = if success { magnitude } else { -magnitude };
    Ok(Ready(CrisisResolutionDataV1 { current, impact }))
}
