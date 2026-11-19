//! One exclusive Common loan for actual Current/parent value IO and source
//! counters. Pure DATA folding runs after this child ends. WorldRules' existing
//! paid capture is separate: it must never nest its own child in this callback.
use super::NormalProjectionCurrentWorkV1;
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("projection source counter accounting")
}
fn paid(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
impl NormalProjectionCurrentWorkV1<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn with_source_io_v1<M, R, E, C, P, V, PE, PC, T>(
        &mut self,
        overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        common: &mut FormalBoundedContextV1<'_>,
        operation: impl FnOnce(
            &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut dyn FnMut() -> bool>,
            &mut WorldFieldReadContextV1<'_, P, V, &mut PE, &mut dyn FnMut() -> bool>,
            &mut NormalProjectionCurrentWorkV1<'_>,
        ) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        common.consume_steps(1).map_err(paid)?;
        let work = self;
        let (_, meter, stop) = common.parts();
        let lease = meter.begin_child().map_err(paid)?;
        let original_steps = [
            work.module_states.states_remaining,
            overlay.values.io.nodes_remaining,
            read.values.io.nodes_remaining,
            work.scan.nodes_remaining,
            work.scan.leaves_remaining,
            work.typed.json.cells_remaining,
            work.typed.json.json_nodes_remaining,
            work.primitives.records_remaining,
            work.inventory.records_remaining,
            work.inventory.index_steps_remaining,
            work.processing.steps_remaining,
            work.originals.facts_remaining,
            work.events.structure.values_remaining,
            work.events.canonical.visits_remaining,
            work.canonical.visits_remaining,
            (*work.pending_entries) as u64,
            work.factory.cells_remaining,
            work.factory.scan.nodes_remaining,
            work.factory.scan.leaves_remaining,
            work.industry.cells_remaining,
            work.industry.scan.nodes_remaining,
            work.industry.scan.leaves_remaining,
            work.logistics.cells_remaining,
            work.logistics.scan.nodes_remaining,
            work.logistics.scan.leaves_remaining,
            work.controls.records_remaining,
            work.controls.index_steps_remaining,
            work.runtime_inner_scan.nodes_remaining,
            work.runtime_inner_scan.leaves_remaining,
            work.cognition_json.json.cells_remaining,
            work.cognition_json.json.json_nodes_remaining,
            work.cognition_json.paths_remaining,
        ];
        let limit_steps = lease.remaining_steps() / 32;
        work.controls.records_remaining = work.controls.records_remaining.min(limit_steps);
        work.controls.index_steps_remaining = work.controls.index_steps_remaining.min(limit_steps);
        overlay.values.io.nodes_remaining = overlay.values.io.nodes_remaining.min(limit_steps) as _;
        read.values.io.nodes_remaining = read.values.io.nodes_remaining.min(limit_steps) as _;
        work.scan.nodes_remaining = work.scan.nodes_remaining.min(limit_steps) as _;
        work.scan.leaves_remaining = work.scan.leaves_remaining.min(limit_steps) as _;
        work.typed.json.cells_remaining = work.typed.json.cells_remaining.min(limit_steps) as _;
        work.typed.json.json_nodes_remaining =
            work.typed.json.json_nodes_remaining.min(limit_steps) as _;
        work.primitives.records_remaining = work.primitives.records_remaining.min(limit_steps) as _;
        work.inventory.records_remaining = work.inventory.records_remaining.min(limit_steps) as _;
        work.inventory.index_steps_remaining =
            work.inventory.index_steps_remaining.min(limit_steps) as _;
        work.processing.steps_remaining = work.processing.steps_remaining.min(limit_steps) as _;
        work.originals.facts_remaining = work.originals.facts_remaining.min(limit_steps) as _;
        work.events.structure.values_remaining =
            work.events.structure.values_remaining.min(limit_steps) as _;
        work.events.canonical.visits_remaining =
            work.events.canonical.visits_remaining.min(limit_steps) as _;
        work.canonical.visits_remaining = work.canonical.visits_remaining.min(limit_steps) as _;
        *work.pending_entries = ((*work.pending_entries) as u64).min(limit_steps) as _;
        work.factory.cells_remaining = work.factory.cells_remaining.min(limit_steps) as _;
        work.factory.scan.nodes_remaining = work.factory.scan.nodes_remaining.min(limit_steps) as _;
        work.factory.scan.leaves_remaining =
            work.factory.scan.leaves_remaining.min(limit_steps) as _;
        work.industry.cells_remaining = work.industry.cells_remaining.min(limit_steps) as _;
        work.industry.scan.nodes_remaining =
            work.industry.scan.nodes_remaining.min(limit_steps) as _;
        work.industry.scan.leaves_remaining =
            work.industry.scan.leaves_remaining.min(limit_steps) as _;
        work.logistics.cells_remaining = work.logistics.cells_remaining.min(limit_steps) as _;
        work.logistics.scan.nodes_remaining =
            work.logistics.scan.nodes_remaining.min(limit_steps) as _;
        work.logistics.scan.leaves_remaining =
            work.logistics.scan.leaves_remaining.min(limit_steps) as _;
        work.module_states.states_remaining = work.module_states.states_remaining.min(limit_steps);
        work.runtime_inner_scan.nodes_remaining =
            work.runtime_inner_scan.nodes_remaining.min(limit_steps);
        work.runtime_inner_scan.leaves_remaining =
            work.runtime_inner_scan.leaves_remaining.min(limit_steps);
        work.cognition_json.json.cells_remaining =
            work.cognition_json.json.cells_remaining.min(limit_steps);
        work.cognition_json.json.json_nodes_remaining = work
            .cognition_json
            .json
            .json_nodes_remaining
            .min(limit_steps);
        work.cognition_json.paths_remaining = work.cognition_json.paths_remaining.min(limit_steps);
        let admitted_steps = [
            work.module_states.states_remaining,
            overlay.values.io.nodes_remaining,
            read.values.io.nodes_remaining,
            work.scan.nodes_remaining,
            work.scan.leaves_remaining,
            work.typed.json.cells_remaining,
            work.typed.json.json_nodes_remaining,
            work.primitives.records_remaining,
            work.inventory.records_remaining,
            work.inventory.index_steps_remaining,
            work.processing.steps_remaining,
            work.originals.facts_remaining,
            work.events.structure.values_remaining,
            work.events.canonical.visits_remaining,
            work.canonical.visits_remaining,
            (*work.pending_entries) as u64,
            work.factory.cells_remaining,
            work.factory.scan.nodes_remaining,
            work.factory.scan.leaves_remaining,
            work.industry.cells_remaining,
            work.industry.scan.nodes_remaining,
            work.industry.scan.leaves_remaining,
            work.logistics.cells_remaining,
            work.logistics.scan.nodes_remaining,
            work.logistics.scan.leaves_remaining,
            work.controls.records_remaining,
            work.controls.index_steps_remaining,
            work.runtime_inner_scan.nodes_remaining,
            work.runtime_inner_scan.leaves_remaining,
            work.cognition_json.json.cells_remaining,
            work.cognition_json.json.json_nodes_remaining,
            work.cognition_json.paths_remaining,
        ];
        let original_bytes = [
            work.module_states.logical_bytes_remaining,
            overlay.values.io.read_bytes_remaining,
            read.values.io.read_bytes_remaining,
            overlay.key_work.bytes_remaining,
            read.key_work.bytes_remaining,
            work.scan.read_bytes_remaining,
            work.processing.bytes_remaining,
            work.originals.original_bytes_remaining,
            work.events.structure.bytes_remaining,
            work.events.canonical.bytes_remaining,
            work.canonical.bytes_remaining,
            work.factory.scan.read_bytes_remaining,
            work.industry.scan.read_bytes_remaining,
            work.logistics.scan.read_bytes_remaining,
            work.runtime_inner_scan.read_bytes_remaining,
        ];
        let limit_bytes = lease.remaining_read_bytes() / 15;
        overlay.values.io.read_bytes_remaining =
            overlay.values.io.read_bytes_remaining.min(limit_bytes) as _;
        read.values.io.read_bytes_remaining =
            read.values.io.read_bytes_remaining.min(limit_bytes) as _;
        overlay.key_work.bytes_remaining = overlay.key_work.bytes_remaining.min(limit_bytes) as _;
        read.key_work.bytes_remaining = read.key_work.bytes_remaining.min(limit_bytes) as _;
        work.scan.read_bytes_remaining = work.scan.read_bytes_remaining.min(limit_bytes) as _;
        work.processing.bytes_remaining = work.processing.bytes_remaining.min(limit_bytes) as _;
        work.originals.original_bytes_remaining =
            work.originals.original_bytes_remaining.min(limit_bytes) as _;
        work.events.structure.bytes_remaining =
            work.events.structure.bytes_remaining.min(limit_bytes) as _;
        work.events.canonical.bytes_remaining =
            work.events.canonical.bytes_remaining.min(limit_bytes) as _;
        work.canonical.bytes_remaining = work.canonical.bytes_remaining.min(limit_bytes) as _;
        work.factory.scan.read_bytes_remaining =
            work.factory.scan.read_bytes_remaining.min(limit_bytes) as _;
        work.industry.scan.read_bytes_remaining =
            work.industry.scan.read_bytes_remaining.min(limit_bytes) as _;
        work.logistics.scan.read_bytes_remaining =
            work.logistics.scan.read_bytes_remaining.min(limit_bytes) as _;
        work.module_states.logical_bytes_remaining =
            work.module_states.logical_bytes_remaining.min(limit_bytes);
        work.runtime_inner_scan.read_bytes_remaining = work
            .runtime_inner_scan
            .read_bytes_remaining
            .min(limit_bytes);
        let admitted_bytes = [
            work.module_states.logical_bytes_remaining,
            overlay.values.io.read_bytes_remaining,
            read.values.io.read_bytes_remaining,
            overlay.key_work.bytes_remaining,
            read.key_work.bytes_remaining,
            work.scan.read_bytes_remaining,
            work.processing.bytes_remaining,
            work.originals.original_bytes_remaining,
            work.events.structure.bytes_remaining,
            work.events.canonical.bytes_remaining,
            work.canonical.bytes_remaining,
            work.factory.scan.read_bytes_remaining,
            work.industry.scan.read_bytes_remaining,
            work.logistics.scan.read_bytes_remaining,
            work.runtime_inner_scan.read_bytes_remaining,
        ];
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let stop = std::cell::RefCell::new(stop);
            let mut current_stop = || (overlay.cancelled)() || (stop.borrow_mut())();
            let mut parent_stop = || (read.cancelled)() || (stop.borrow_mut())();
            let mut current = WorldFieldOverlayContextV1 {
                transaction: &mut *overlay.transaction,
                values: ValueReadContext {
                    source: overlay.values.source,
                    memory: overlay.values.memory,
                    class: overlay.values.class,
                    io: &mut *overlay.values.io,
                },
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                take_state_error: &mut overlay.take_state_error,
                cancelled: &mut current_stop as &mut dyn FnMut() -> bool,
            };
            let mut parent = WorldFieldReadContextV1 {
                root: read.root,
                state: read.state,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut parent_stop as &mut dyn FnMut() -> bool,
            };
            operation(&mut current, &mut parent, work)
        }));
        let after_steps = [
            work.module_states.states_remaining,
            overlay.values.io.nodes_remaining,
            read.values.io.nodes_remaining,
            work.scan.nodes_remaining,
            work.scan.leaves_remaining,
            work.typed.json.cells_remaining,
            work.typed.json.json_nodes_remaining,
            work.primitives.records_remaining,
            work.inventory.records_remaining,
            work.inventory.index_steps_remaining,
            work.processing.steps_remaining,
            work.originals.facts_remaining,
            work.events.structure.values_remaining,
            work.events.canonical.visits_remaining,
            work.canonical.visits_remaining,
            (*work.pending_entries) as u64,
            work.factory.cells_remaining,
            work.factory.scan.nodes_remaining,
            work.factory.scan.leaves_remaining,
            work.industry.cells_remaining,
            work.industry.scan.nodes_remaining,
            work.industry.scan.leaves_remaining,
            work.logistics.cells_remaining,
            work.logistics.scan.nodes_remaining,
            work.logistics.scan.leaves_remaining,
            work.controls.records_remaining,
            work.controls.index_steps_remaining,
            work.runtime_inner_scan.nodes_remaining,
            work.runtime_inner_scan.leaves_remaining,
            work.cognition_json.json.cells_remaining,
            work.cognition_json.json.json_nodes_remaining,
            work.cognition_json.paths_remaining,
        ];
        let after_bytes = [
            work.module_states.logical_bytes_remaining,
            overlay.values.io.read_bytes_remaining,
            read.values.io.read_bytes_remaining,
            overlay.key_work.bytes_remaining,
            read.key_work.bytes_remaining,
            work.scan.read_bytes_remaining,
            work.processing.bytes_remaining,
            work.originals.original_bytes_remaining,
            work.events.structure.bytes_remaining,
            work.events.canonical.bytes_remaining,
            work.canonical.bytes_remaining,
            work.factory.scan.read_bytes_remaining,
            work.industry.scan.read_bytes_remaining,
            work.logistics.scan.read_bytes_remaining,
            work.runtime_inner_scan.read_bytes_remaining,
        ];
        let debit = |before: &[u64], after: &[u64]| {
            before
                .iter()
                .zip(after)
                .try_fold(0u64, |sum, (b, a)| sum.checked_add(b.checked_sub(*a)?))
        };
        let debits = debit(&admitted_steps, &after_steps).zip(debit(&admitted_bytes, &after_bytes));
        let result = match (outcome, debits) {
            (Err(payload), _) => {
                overlay.values.io.nodes_remaining = 0;
                read.values.io.nodes_remaining = 0;
                work.scan.nodes_remaining = 0;
                work.scan.leaves_remaining = 0;
                work.typed.json.cells_remaining = 0;
                work.typed.json.json_nodes_remaining = 0;
                work.primitives.records_remaining = 0;
                work.controls.records_remaining = 0;
                work.controls.index_steps_remaining = 0;
                work.inventory.records_remaining = 0;
                work.inventory.index_steps_remaining = 0;
                work.processing.steps_remaining = 0;
                work.originals.facts_remaining = 0;
                work.events.structure.values_remaining = 0;
                work.events.canonical.visits_remaining = 0;
                work.canonical.visits_remaining = 0;
                *work.pending_entries = 0;
                work.factory.cells_remaining = 0;
                work.factory.scan.nodes_remaining = 0;
                work.factory.scan.leaves_remaining = 0;
                work.industry.cells_remaining = 0;
                work.industry.scan.nodes_remaining = 0;
                work.industry.scan.leaves_remaining = 0;
                work.logistics.cells_remaining = 0;
                work.logistics.scan.nodes_remaining = 0;
                work.logistics.scan.leaves_remaining = 0;
                overlay.values.io.read_bytes_remaining = 0;
                read.values.io.read_bytes_remaining = 0;
                overlay.key_work.bytes_remaining = 0;
                read.key_work.bytes_remaining = 0;
                work.scan.read_bytes_remaining = 0;
                work.runtime_inner_scan.nodes_remaining = 0;
                work.runtime_inner_scan.leaves_remaining = 0;
                work.runtime_inner_scan.read_bytes_remaining = 0;
                work.cognition_json.json.cells_remaining = 0;
                work.cognition_json.json.json_nodes_remaining = 0;
                work.cognition_json.paths_remaining = 0;
                work.processing.bytes_remaining = 0;
                work.originals.original_bytes_remaining = 0;
                work.events.structure.bytes_remaining = 0;
                work.events.canonical.bytes_remaining = 0;
                work.canonical.bytes_remaining = 0;
                work.factory.scan.read_bytes_remaining = 0;
                work.industry.scan.read_bytes_remaining = 0;
                work.logistics.scan.read_bytes_remaining = 0;
                work.module_states.states_remaining = 0;
                work.module_states.logical_bytes_remaining = 0;
                drop(lease);
                std::panic::resume_unwind(payload)
            }
            (Ok(_), None) => {
                overlay.values.io.nodes_remaining = 0;
                read.values.io.nodes_remaining = 0;
                work.scan.nodes_remaining = 0;
                work.scan.leaves_remaining = 0;
                work.typed.json.cells_remaining = 0;
                work.typed.json.json_nodes_remaining = 0;
                work.primitives.records_remaining = 0;
                work.controls.records_remaining = 0;
                work.controls.index_steps_remaining = 0;
                work.inventory.records_remaining = 0;
                work.inventory.index_steps_remaining = 0;
                work.processing.steps_remaining = 0;
                work.originals.facts_remaining = 0;
                work.events.structure.values_remaining = 0;
                work.events.canonical.visits_remaining = 0;
                work.canonical.visits_remaining = 0;
                *work.pending_entries = 0;
                work.factory.cells_remaining = 0;
                work.factory.scan.nodes_remaining = 0;
                work.factory.scan.leaves_remaining = 0;
                work.industry.cells_remaining = 0;
                work.industry.scan.nodes_remaining = 0;
                work.industry.scan.leaves_remaining = 0;
                work.logistics.cells_remaining = 0;
                work.logistics.scan.nodes_remaining = 0;
                work.logistics.scan.leaves_remaining = 0;
                overlay.values.io.read_bytes_remaining = 0;
                read.values.io.read_bytes_remaining = 0;
                overlay.key_work.bytes_remaining = 0;
                read.key_work.bytes_remaining = 0;
                work.scan.read_bytes_remaining = 0;
                work.runtime_inner_scan.nodes_remaining = 0;
                work.runtime_inner_scan.leaves_remaining = 0;
                work.runtime_inner_scan.read_bytes_remaining = 0;
                work.cognition_json.json.cells_remaining = 0;
                work.cognition_json.json.json_nodes_remaining = 0;
                work.cognition_json.paths_remaining = 0;
                work.processing.bytes_remaining = 0;
                work.originals.original_bytes_remaining = 0;
                work.events.structure.bytes_remaining = 0;
                work.events.canonical.bytes_remaining = 0;
                work.canonical.bytes_remaining = 0;
                work.factory.scan.read_bytes_remaining = 0;
                work.industry.scan.read_bytes_remaining = 0;
                work.logistics.scan.read_bytes_remaining = 0;
                work.module_states.states_remaining = 0;
                work.module_states.logical_bytes_remaining = 0;
                drop(lease);
                return Err(invalid());
            }
            (Ok(result), Some((steps, bytes))) => {
                let charge = lease.consume(steps, bytes);
                lease.finish();
                // Every lane was individually clamped to a disjoint fraction. Successful
                // reconciliation can only subtract actual work; restore no consumed work.
                if let Err(error) = charge {
                    overlay.values.io.nodes_remaining = 0;
                    read.values.io.nodes_remaining = 0;
                    work.scan.nodes_remaining = 0;
                    work.scan.leaves_remaining = 0;
                    work.typed.json.cells_remaining = 0;
                    work.typed.json.json_nodes_remaining = 0;
                    work.primitives.records_remaining = 0;
                    work.controls.records_remaining = 0;
                    work.controls.index_steps_remaining = 0;
                    work.inventory.records_remaining = 0;
                    work.inventory.index_steps_remaining = 0;
                    work.processing.steps_remaining = 0;
                    work.originals.facts_remaining = 0;
                    work.events.structure.values_remaining = 0;
                    work.events.canonical.visits_remaining = 0;
                    work.canonical.visits_remaining = 0;
                    *work.pending_entries = 0;
                    work.factory.cells_remaining = 0;
                    work.factory.scan.nodes_remaining = 0;
                    work.factory.scan.leaves_remaining = 0;
                    work.industry.cells_remaining = 0;
                    work.industry.scan.nodes_remaining = 0;
                    work.industry.scan.leaves_remaining = 0;
                    work.logistics.cells_remaining = 0;
                    work.logistics.scan.nodes_remaining = 0;
                    work.logistics.scan.leaves_remaining = 0;
                    overlay.values.io.read_bytes_remaining = 0;
                    read.values.io.read_bytes_remaining = 0;
                    overlay.key_work.bytes_remaining = 0;
                    read.key_work.bytes_remaining = 0;
                    work.scan.read_bytes_remaining = 0;
                    work.runtime_inner_scan.nodes_remaining = 0;
                    work.runtime_inner_scan.leaves_remaining = 0;
                    work.runtime_inner_scan.read_bytes_remaining = 0;
                    work.cognition_json.json.cells_remaining = 0;
                    work.cognition_json.json.json_nodes_remaining = 0;
                    work.cognition_json.paths_remaining = 0;
                    work.processing.bytes_remaining = 0;
                    work.originals.original_bytes_remaining = 0;
                    work.events.structure.bytes_remaining = 0;
                    work.events.canonical.bytes_remaining = 0;
                    work.canonical.bytes_remaining = 0;
                    work.factory.scan.read_bytes_remaining = 0;
                    work.industry.scan.read_bytes_remaining = 0;
                    work.logistics.scan.read_bytes_remaining = 0;
                    work.module_states.states_remaining = 0;
                    work.module_states.logical_bytes_remaining = 0;
                    return Err(paid(error));
                }
                result
            }
        };
        overlay.values.io.nodes_remaining =
            (original_steps[1] - (admitted_steps[1] - after_steps[1])) as _;
        read.values.io.nodes_remaining =
            (original_steps[2] - (admitted_steps[2] - after_steps[2])) as _;
        work.scan.nodes_remaining = (original_steps[3] - (admitted_steps[3] - after_steps[3])) as _;
        work.scan.leaves_remaining =
            (original_steps[4] - (admitted_steps[4] - after_steps[4])) as _;
        work.typed.json.cells_remaining =
            (original_steps[5] - (admitted_steps[5] - after_steps[5])) as _;
        work.typed.json.json_nodes_remaining =
            (original_steps[6] - (admitted_steps[6] - after_steps[6])) as _;
        work.primitives.records_remaining =
            (original_steps[7] - (admitted_steps[7] - after_steps[7])) as _;
        work.inventory.records_remaining =
            (original_steps[8] - (admitted_steps[8] - after_steps[8])) as _;
        work.inventory.index_steps_remaining =
            (original_steps[9] - (admitted_steps[9] - after_steps[9])) as _;
        work.processing.steps_remaining =
            (original_steps[10] - (admitted_steps[10] - after_steps[10])) as _;
        work.originals.facts_remaining =
            (original_steps[11] - (admitted_steps[11] - after_steps[11])) as _;
        work.events.structure.values_remaining =
            (original_steps[12] - (admitted_steps[12] - after_steps[12])) as _;
        work.events.canonical.visits_remaining =
            (original_steps[13] - (admitted_steps[13] - after_steps[13])) as _;
        work.canonical.visits_remaining =
            (original_steps[14] - (admitted_steps[14] - after_steps[14])) as _;
        *work.pending_entries = (original_steps[15] - (admitted_steps[15] - after_steps[15])) as _;
        work.factory.cells_remaining =
            (original_steps[16] - (admitted_steps[16] - after_steps[16])) as _;
        work.factory.scan.nodes_remaining =
            (original_steps[17] - (admitted_steps[17] - after_steps[17])) as _;
        work.factory.scan.leaves_remaining =
            (original_steps[18] - (admitted_steps[18] - after_steps[18])) as _;
        work.industry.cells_remaining =
            (original_steps[19] - (admitted_steps[19] - after_steps[19])) as _;
        work.industry.scan.nodes_remaining =
            (original_steps[20] - (admitted_steps[20] - after_steps[20])) as _;
        work.industry.scan.leaves_remaining =
            (original_steps[21] - (admitted_steps[21] - after_steps[21])) as _;
        work.logistics.cells_remaining =
            (original_steps[22] - (admitted_steps[22] - after_steps[22])) as _;
        work.logistics.scan.nodes_remaining =
            (original_steps[23] - (admitted_steps[23] - after_steps[23])) as _;
        work.logistics.scan.leaves_remaining =
            (original_steps[24] - (admitted_steps[24] - after_steps[24])) as _;
        work.controls.records_remaining =
            original_steps[25] - (admitted_steps[25] - after_steps[25]);
        work.controls.index_steps_remaining =
            original_steps[26] - (admitted_steps[26] - after_steps[26]);
        overlay.values.io.read_bytes_remaining =
            (original_bytes[1] - (admitted_bytes[1] - after_bytes[1])) as _;
        read.values.io.read_bytes_remaining =
            (original_bytes[2] - (admitted_bytes[2] - after_bytes[2])) as _;
        overlay.key_work.bytes_remaining =
            (original_bytes[3] - (admitted_bytes[3] - after_bytes[3])) as _;
        read.key_work.bytes_remaining =
            (original_bytes[4] - (admitted_bytes[4] - after_bytes[4])) as _;
        work.scan.read_bytes_remaining =
            (original_bytes[5] - (admitted_bytes[5] - after_bytes[5])) as _;
        work.processing.bytes_remaining =
            (original_bytes[6] - (admitted_bytes[6] - after_bytes[6])) as _;
        work.originals.original_bytes_remaining =
            (original_bytes[7] - (admitted_bytes[7] - after_bytes[7])) as _;
        work.events.structure.bytes_remaining =
            (original_bytes[8] - (admitted_bytes[8] - after_bytes[8])) as _;
        work.events.canonical.bytes_remaining =
            (original_bytes[9] - (admitted_bytes[9] - after_bytes[9])) as _;
        work.canonical.bytes_remaining =
            (original_bytes[10] - (admitted_bytes[10] - after_bytes[10])) as _;
        work.factory.scan.read_bytes_remaining =
            (original_bytes[11] - (admitted_bytes[11] - after_bytes[11])) as _;
        work.industry.scan.read_bytes_remaining =
            (original_bytes[12] - (admitted_bytes[12] - after_bytes[12])) as _;
        work.logistics.scan.read_bytes_remaining =
            (original_bytes[13] - (admitted_bytes[13] - after_bytes[13])) as _;
        work.module_states.states_remaining =
            original_steps[0] - (admitted_steps[0] - after_steps[0]);
        work.module_states.logical_bytes_remaining =
            original_bytes[0] - (admitted_bytes[0] - after_bytes[0]);
        work.runtime_inner_scan.nodes_remaining =
            original_steps[27] - (admitted_steps[27] - after_steps[27]);
        work.runtime_inner_scan.leaves_remaining =
            original_steps[28] - (admitted_steps[28] - after_steps[28]);
        work.cognition_json.json.cells_remaining =
            original_steps[29] - (admitted_steps[29] - after_steps[29]);
        work.cognition_json.json.json_nodes_remaining =
            original_steps[30] - (admitted_steps[30] - after_steps[30]);
        work.cognition_json.paths_remaining =
            original_steps[31] - (admitted_steps[31] - after_steps[31]);
        work.runtime_inner_scan.read_bytes_remaining =
            original_bytes[14] - (admitted_bytes[14] - after_bytes[14]);
        // Reconcile even a failed read before returning its original typed cause.
        let value = result?;
        common.consume_steps(0).map_err(paid)?;
        if (overlay.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(value)
    }
}
