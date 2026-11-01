//! Logistics labels and continuous work over the shared authenticated diff.
use super::*;
use crate::incremental_inventory_value_read::InventoryValueReader;
use crate::incremental_record_diff::{self, RecordDiffLabelsV1};
use oasis7_formal_protocol::{
    incremental_state::{StateEntryV1, StateRootV1},
    incremental_value::ValueNodeReader,
};
pub(super) fn visit<R: ValueNodeReader, C: FnMut() -> bool>(
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut LogisticsWorkBudgetV1,
    visitor: impl FnMut(
        Option<&StateEntryV1>,
        Option<&StateEntryV1>,
        &mut InventoryValueReader<'_, '_, R, C>,
    ) -> Result<(), ValueTreeError>,
) -> Result<(), ValueTreeError> {
    incremental_record_diff::visit(
        before,
        after,
        records,
        &mut work.cells_remaining,
        RecordDiffLabelsV1 {
            invalid: "logistics child transition tree",
            cells: "logistics cell work",
            stack: "logistics diff stack",
            tree: genesis::LABELS,
        },
        visitor,
    )
}
