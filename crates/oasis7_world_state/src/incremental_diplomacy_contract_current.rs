//! Current economic-contract data opened from a genuine complete Diplomacy parent.
//! This observation supplies no action, Accepted, or publication permission.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
};

/// Opens exactly one actual Current economic-contract child, including pending replacement
/// or deletion of its outer entry. The outer key and embedded `contract_id` remain
/// independent. The returned record can be read through the original scalar
/// reader; publication must independently recheck its actual Current root.
///
/// The parent qualification authenticates the complete parent namespace. Only
/// this selected Current child's bounded typed closure is traversed here. All
/// IO, key, scan, cell and text allowances belong to the caller and are consumed
/// continuously, including on failure. Any failure poisons the transaction.
#[allow(clippy::too_many_arguments)]
pub fn open_current_diplomacy_contract_record_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    parent: &ReservedValue<DiplomacyFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<DiplomacyRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    super::current::open_current_diplomacy_record_v1(
        DiplomacyFieldV1::EconomicContracts,
        id,
        parent,
        roots,
        tx,
        ctx,
        inner_scan,
        cells,
        max,
        guard,
        live,
    )
}
