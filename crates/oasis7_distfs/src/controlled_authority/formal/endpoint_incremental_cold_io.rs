//! Complete fixed scalar inventory for the cold qualifier/readmission lane.
//! No quotas are cloned or minted. Formal-ledger operations run only between
//! these exclusive child phases, directly on the very same Common Core.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedChargeErrorV1, FormalBoundedContextV1,
};
use oasis7_proto::memory_budget::MemoryClass;
const CAPACITY: usize = 512;
#[path = "endpoint_incremental_cold_suffix_io.rs"]
mod suffix;
pub(super) use suffix::run_suffix;
fn cold_lanes(work: &mut IwsColdParentWorkV1, mut lane: impl FnMut(&mut u64, bool)) {
    lane(&mut work.mailbox_occurrences, false);
    lane(&mut work.current.raw.instance_scratch.reads, false);
    lane(&mut work.current.raw.instance_scratch.read_bytes, true);
    lane(&mut work.current.raw.instance_scratch.writes, false);
    lane(&mut work.current.raw.instance_scratch.write_bytes, true);
    lane(&mut work.admission.raw.instance_scratch.reads, false);
    lane(&mut work.admission.raw.instance_scratch.read_bytes, true);
    lane(&mut work.admission.raw.instance_scratch.writes, false);
    lane(&mut work.admission.raw.instance_scratch.write_bytes, true);
    lane(&mut work.objects_physical.reads_remaining, false);
    lane(&mut work.objects_physical.read_bytes_remaining, true);
    lane(&mut work.objects_physical.writes_remaining, false);
    lane(&mut work.objects_physical.write_bytes_remaining, true);
    lane(&mut work.objects_values.nodes_remaining, false);
    lane(&mut work.objects_values.read_bytes_remaining, true);
    lane(&mut work.objects_values.write_bytes_remaining, true);
    lane(&mut work.state_recovery.reads_remaining, false);
    lane(&mut work.state_recovery.read_bytes_remaining, true);
    lane(&mut work.state_recovery.writes_remaining, false);
    lane(&mut work.state_recovery.write_bytes_remaining, true);
    lane(&mut work.values_recovery.reads_remaining, false);
    lane(&mut work.values_recovery.read_bytes_remaining, true);
    lane(&mut work.values_recovery.writes_remaining, false);
    lane(&mut work.values_recovery.write_bytes_remaining, true);
    lane(&mut work.read_recovery.nodes_remaining, false);
    lane(&mut work.read_recovery.read_bytes_remaining, true);
    lane(&mut work.read_recovery.write_bytes_remaining, true);
    lane(&mut work.keys_recovery.bytes_remaining, true);
    lane(&mut work.state_ordinary.reads_remaining, false);
    lane(&mut work.state_ordinary.read_bytes_remaining, true);
    lane(&mut work.state_ordinary.writes_remaining, false);
    lane(&mut work.state_ordinary.write_bytes_remaining, true);
    lane(&mut work.values_ordinary.reads_remaining, false);
    lane(&mut work.values_ordinary.read_bytes_remaining, true);
    lane(&mut work.values_ordinary.writes_remaining, false);
    lane(&mut work.values_ordinary.write_bytes_remaining, true);
    lane(&mut work.read_ordinary.nodes_remaining, false);
    lane(&mut work.read_ordinary.read_bytes_remaining, true);
    lane(&mut work.read_ordinary.write_bytes_remaining, true);
    lane(&mut work.keys_ordinary.bytes_remaining, true);
    lane(&mut work.core.objects_remaining, false);
    lane(&mut work.core.original_bytes_remaining, true);
    lane(&mut work.core.new_pages_remaining, false);
    lane(&mut work.core.merkle_reads_remaining, false);
    lane(&mut work.facts.facts_remaining, false);
    lane(&mut work.facts.original_bytes_remaining, true);
    lane(&mut work.router.facts.facts_remaining, false);
    lane(&mut work.router.facts.original_bytes_remaining, true);
    lane(&mut work.router.envelope.bytes_remaining, true);
    lane(&mut work.router.envelope.values_remaining, false);
    lane(&mut work.router.runtime.structure.bytes_remaining, true);
    lane(&mut work.router.runtime.structure.values_remaining, false);
    lane(&mut work.router.runtime.canonical.visits_remaining, false);
    lane(&mut work.router.runtime.canonical.bytes_remaining, true);
    lane(&mut work.router.cognition.bytes_remaining, true);
    lane(&mut work.router.cognition.nodes_remaining, false);
    lane(&mut work.router.cognition.source_reads_remaining, false);
    lane(&mut work.router.cognition.key_bytes_remaining, true);
    lane(&mut work.proof_ledger_read.cells_remaining, false);
    lane(&mut work.proof_ledger_read.json_nodes_remaining, false);
    lane(&mut work.current.key_scan.nodes_remaining, false);
    lane(&mut work.current.key_scan.read_bytes_remaining, true);
    lane(&mut work.current.key_scan.leaves_remaining, false);
    lane(&mut work.current.native_outer_scan.nodes_remaining, false);
    lane(
        &mut work.current.native_outer_scan.read_bytes_remaining,
        true,
    );
    lane(&mut work.current.native_outer_scan.leaves_remaining, false);
    lane(&mut work.current.native_inner_scan.nodes_remaining, false);
    lane(
        &mut work.current.native_inner_scan.read_bytes_remaining,
        true,
    );
    lane(&mut work.current.native_inner_scan.leaves_remaining, false);
    lane(&mut work.current.key_changes_remaining, false);
    lane(&mut work.current.native.legacy_cells_remaining, false);
    lane(&mut work.current.native.primitive.records_remaining, false);
    lane(&mut work.current.native.native.records_remaining, false);
    lane(
        &mut work.current.native.native.typed.json.cells_remaining,
        false,
    );
    lane(
        &mut work.current.native.native.typed.json.json_nodes_remaining,
        false,
    );
    lane(&mut work.current.native.inventory.records_remaining, false);
    lane(
        &mut work.current.native.inventory.index_steps_remaining,
        false,
    );
    lane(&mut work.current.native.factory.cells_remaining, false);
    lane(&mut work.current.native.factory.scan.nodes_remaining, false);
    lane(
        &mut work.current.native.factory.scan.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.current.native.factory.scan.leaves_remaining,
        false,
    );
    lane(&mut work.current.native.industry.cells_remaining, false);
    lane(
        &mut work.current.native.industry.scan.nodes_remaining,
        false,
    );
    lane(
        &mut work.current.native.industry.scan.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.current.native.industry.scan.leaves_remaining,
        false,
    );
    lane(&mut work.current.native.market.outer_rows_remaining, false);
    lane(
        &mut work.current.native.market.changed_cells_remaining,
        false,
    );
    lane(
        &mut work.current.native.market.membership_cells_remaining,
        false,
    );
    lane(&mut work.current.native.logistics.cells_remaining, false);
    lane(
        &mut work.current.native.logistics.scan.nodes_remaining,
        false,
    );
    lane(
        &mut work.current.native.logistics.scan.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.current.native.logistics.scan.leaves_remaining,
        false,
    );
    lane(
        &mut work.current.native.rules.outer_records_remaining,
        false,
    );
    lane(&mut work.current.native.rules.legacy_cells_remaining, false);
    lane(
        &mut work.current.native.rules.composite.json.cells_remaining,
        false,
    );
    lane(
        &mut work
            .current
            .native
            .rules
            .composite
            .json
            .json_nodes_remaining,
        false,
    );
    lane(
        &mut work.current.native.rules.remainder.cells_remaining,
        false,
    );
    lane(&mut work.current.native.controls.records_remaining, false);
    lane(
        &mut work.current.native.controls.index_steps_remaining,
        false,
    );
    lane(&mut work.current.runtime.outer_scan.nodes_remaining, false);
    lane(
        &mut work.current.runtime.outer_scan.read_bytes_remaining,
        true,
    );
    lane(&mut work.current.runtime.outer_scan.leaves_remaining, false);
    lane(&mut work.current.runtime.body_scan.nodes_remaining, false);
    lane(
        &mut work.current.runtime.body_scan.read_bytes_remaining,
        true,
    );
    lane(&mut work.current.runtime.body_scan.leaves_remaining, false);
    for scan in &mut work.current.runtime.resource_scans {
        lane(&mut scan.nodes_remaining, false);
        lane(&mut scan.read_bytes_remaining, true);
        lane(&mut scan.leaves_remaining, false);
    }
    lane(&mut work.current.runtime.cells_remaining, false);
    lane(&mut work.current.runtime.config.cells_remaining, false);
    lane(&mut work.current.runtime.config.json.cells_remaining, false);
    lane(
        &mut work.current.runtime.config.json.json_nodes_remaining,
        false,
    );
    lane(&mut work.current.runtime.resources.cells_remaining, false);
    lane(&mut work.current.runtime.content.objects_remaining, false);
    lane(&mut work.current.runtime.content.bytes_remaining, true);
    lane(&mut work.current.runtime.effects.cells_remaining, false);
    lane(
        &mut work.current.runtime.effects.json_nodes_remaining,
        false,
    );
    lane(
        &mut work.current.runtime.module_authorization.cells_remaining,
        false,
    );
    lane(
        &mut work
            .current
            .runtime
            .module_authorization
            .comparison_bytes_remaining,
        true,
    );
    lane(&mut work.current.runtime.json.cells_remaining, false);
    lane(&mut work.current.runtime.json.json_nodes_remaining, false);
    lane(
        &mut work.current.runtime.revocation.json.cells_remaining,
        false,
    );
    lane(
        &mut work.current.runtime.revocation.json.json_nodes_remaining,
        false,
    );
    lane(&mut work.current.runtime.proposals.cells_remaining, false);
    lane(
        &mut work.current.runtime.proposals.json.cells_remaining,
        false,
    );
    lane(
        &mut work.current.runtime.proposals.json.json_nodes_remaining,
        false,
    );
    lane(&mut work.current.runtime.governance.cells_remaining, false);
    lane(&mut work.current.runtime.rollback.cells_remaining, false);
    lane(
        &mut work.current.runtime.pending.index.steps_remaining,
        false,
    );
    lane(
        &mut work.current.runtime.pending.typed.json.cells_remaining,
        false,
    );
    lane(
        &mut work.current.runtime.pending.typed.json.json_nodes_remaining,
        false,
    );
    lane(
        &mut work.current.runtime.pending.original.objects_remaining,
        false,
    );
    lane(
        &mut work
            .current
            .runtime
            .pending
            .original
            .original_bytes_remaining,
        true,
    );
    lane(
        &mut work
            .current
            .runtime
            .pending
            .original
            .canonical
            .visits_remaining,
        false,
    );
    lane(
        &mut work
            .current
            .runtime
            .pending
            .original
            .canonical
            .bytes_remaining,
        true,
    );
    lane(
        &mut work.current.runtime.pending.selected_scan.nodes_remaining,
        false,
    );
    lane(
        &mut work
            .current
            .runtime
            .pending
            .selected_scan
            .read_bytes_remaining,
        true,
    );
    lane(
        &mut work.current.runtime.pending.selected_scan.leaves_remaining,
        false,
    );
    lane(&mut work.current.raw.state_scan.nodes_remaining, false);
    lane(&mut work.current.raw.state_scan.read_bytes_remaining, true);
    lane(&mut work.current.raw.state_scan.leaves_remaining, false);
    lane(&mut work.current.raw.agents.cells_remaining, false);
    lane(&mut work.current.raw.agent_changes.cells_remaining, false);
    lane(&mut work.current.raw.module_states.states_remaining, false);
    lane(
        &mut work.current.raw.module_states.logical_bytes_remaining,
        true,
    );
    lane(&mut work.current.raw.instance_relations_remaining, false);
    lane(&mut work.current.raw.allocators.records_remaining, false);
    lane(&mut work.current.raw.schedule.cells_remaining, false);
    lane(&mut work.current.raw.schedule.index.nodes_remaining, false);
    lane(
        &mut work.current.raw.schedule.index.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.current.raw.schedule.index.buckets_remaining,
        false,
    );
    lane(
        &mut work.current.raw.schedule.index.instances_remaining,
        false,
    );
    lane(&mut work.current.raw.mailbox_occurrences_remaining, false);
    lane(&mut work.current.raw.originals.objects_remaining, false);
    lane(
        &mut work.current.raw.originals.original_bytes_remaining,
        true,
    );
    lane(
        &mut work.current.raw.originals.canonical.visits_remaining,
        false,
    );
    lane(
        &mut work.current.raw.originals.canonical.bytes_remaining,
        true,
    );
    lane(&mut work.current.cognition.scan.nodes_remaining, false);
    lane(&mut work.current.cognition.scan.read_bytes_remaining, true);
    lane(&mut work.current.cognition.scan.leaves_remaining, false);
    lane(&mut work.current.cognition.json.json.cells_remaining, false);
    lane(
        &mut work.current.cognition.json.json.json_nodes_remaining,
        false,
    );
    lane(&mut work.current.cognition.json.paths_remaining, false);
    lane(&mut work.current.cognition.outer_cells_remaining, false);
    lane(&mut work.admission.key_scan.nodes_remaining, false);
    lane(&mut work.admission.key_scan.read_bytes_remaining, true);
    lane(&mut work.admission.key_scan.leaves_remaining, false);
    lane(&mut work.admission.native_outer_scan.nodes_remaining, false);
    lane(
        &mut work.admission.native_outer_scan.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.admission.native_outer_scan.leaves_remaining,
        false,
    );
    lane(&mut work.admission.native_inner_scan.nodes_remaining, false);
    lane(
        &mut work.admission.native_inner_scan.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.admission.native_inner_scan.leaves_remaining,
        false,
    );
    lane(&mut work.admission.key_changes_remaining, false);
    lane(&mut work.admission.native.legacy_cells_remaining, false);
    lane(
        &mut work.admission.native.primitive.records_remaining,
        false,
    );
    lane(&mut work.admission.native.native.records_remaining, false);
    lane(
        &mut work.admission.native.native.typed.json.cells_remaining,
        false,
    );
    lane(
        &mut work.admission.native.native.typed.json.json_nodes_remaining,
        false,
    );
    lane(
        &mut work.admission.native.inventory.records_remaining,
        false,
    );
    lane(
        &mut work.admission.native.inventory.index_steps_remaining,
        false,
    );
    lane(&mut work.admission.native.factory.cells_remaining, false);
    lane(
        &mut work.admission.native.factory.scan.nodes_remaining,
        false,
    );
    lane(
        &mut work.admission.native.factory.scan.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.admission.native.factory.scan.leaves_remaining,
        false,
    );
    lane(&mut work.admission.native.industry.cells_remaining, false);
    lane(
        &mut work.admission.native.industry.scan.nodes_remaining,
        false,
    );
    lane(
        &mut work.admission.native.industry.scan.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.admission.native.industry.scan.leaves_remaining,
        false,
    );
    lane(
        &mut work.admission.native.market.outer_rows_remaining,
        false,
    );
    lane(
        &mut work.admission.native.market.changed_cells_remaining,
        false,
    );
    lane(
        &mut work.admission.native.market.membership_cells_remaining,
        false,
    );
    lane(&mut work.admission.native.logistics.cells_remaining, false);
    lane(
        &mut work.admission.native.logistics.scan.nodes_remaining,
        false,
    );
    lane(
        &mut work.admission.native.logistics.scan.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.admission.native.logistics.scan.leaves_remaining,
        false,
    );
    lane(
        &mut work.admission.native.rules.outer_records_remaining,
        false,
    );
    lane(
        &mut work.admission.native.rules.legacy_cells_remaining,
        false,
    );
    lane(
        &mut work.admission.native.rules.composite.json.cells_remaining,
        false,
    );
    lane(
        &mut work
            .admission
            .native
            .rules
            .composite
            .json
            .json_nodes_remaining,
        false,
    );
    lane(
        &mut work.admission.native.rules.remainder.cells_remaining,
        false,
    );
    lane(&mut work.admission.native.controls.records_remaining, false);
    lane(
        &mut work.admission.native.controls.index_steps_remaining,
        false,
    );
    lane(
        &mut work.admission.runtime.outer_scan.nodes_remaining,
        false,
    );
    lane(
        &mut work.admission.runtime.outer_scan.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.admission.runtime.outer_scan.leaves_remaining,
        false,
    );
    lane(&mut work.admission.runtime.body_scan.nodes_remaining, false);
    lane(
        &mut work.admission.runtime.body_scan.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.admission.runtime.body_scan.leaves_remaining,
        false,
    );
    for scan in &mut work.admission.runtime.resource_scans {
        lane(&mut scan.nodes_remaining, false);
        lane(&mut scan.read_bytes_remaining, true);
        lane(&mut scan.leaves_remaining, false);
    }
    lane(&mut work.admission.runtime.cells_remaining, false);
    lane(&mut work.admission.runtime.config.cells_remaining, false);
    lane(
        &mut work.admission.runtime.config.json.cells_remaining,
        false,
    );
    lane(
        &mut work.admission.runtime.config.json.json_nodes_remaining,
        false,
    );
    lane(&mut work.admission.runtime.resources.cells_remaining, false);
    lane(&mut work.admission.runtime.content.objects_remaining, false);
    lane(&mut work.admission.runtime.content.bytes_remaining, true);
    lane(&mut work.admission.runtime.effects.cells_remaining, false);
    lane(
        &mut work.admission.runtime.effects.json_nodes_remaining,
        false,
    );
    lane(
        &mut work.admission.runtime.module_authorization.cells_remaining,
        false,
    );
    lane(
        &mut work
            .admission
            .runtime
            .module_authorization
            .comparison_bytes_remaining,
        true,
    );
    lane(&mut work.admission.runtime.json.cells_remaining, false);
    lane(&mut work.admission.runtime.json.json_nodes_remaining, false);
    lane(
        &mut work.admission.runtime.revocation.json.cells_remaining,
        false,
    );
    lane(
        &mut work.admission.runtime.revocation.json.json_nodes_remaining,
        false,
    );
    lane(&mut work.admission.runtime.proposals.cells_remaining, false);
    lane(
        &mut work.admission.runtime.proposals.json.cells_remaining,
        false,
    );
    lane(
        &mut work.admission.runtime.proposals.json.json_nodes_remaining,
        false,
    );
    lane(
        &mut work.admission.runtime.governance.cells_remaining,
        false,
    );
    lane(&mut work.admission.runtime.rollback.cells_remaining, false);
    lane(
        &mut work.admission.runtime.pending.index.steps_remaining,
        false,
    );
    lane(
        &mut work.admission.runtime.pending.typed.json.cells_remaining,
        false,
    );
    lane(
        &mut work
            .admission
            .runtime
            .pending
            .typed
            .json
            .json_nodes_remaining,
        false,
    );
    lane(
        &mut work.admission.runtime.pending.original.objects_remaining,
        false,
    );
    lane(
        &mut work
            .admission
            .runtime
            .pending
            .original
            .original_bytes_remaining,
        true,
    );
    lane(
        &mut work
            .admission
            .runtime
            .pending
            .original
            .canonical
            .visits_remaining,
        false,
    );
    lane(
        &mut work
            .admission
            .runtime
            .pending
            .original
            .canonical
            .bytes_remaining,
        true,
    );
    lane(
        &mut work.admission.runtime.pending.selected_scan.nodes_remaining,
        false,
    );
    lane(
        &mut work
            .admission
            .runtime
            .pending
            .selected_scan
            .read_bytes_remaining,
        true,
    );
    lane(
        &mut work
            .admission
            .runtime
            .pending
            .selected_scan
            .leaves_remaining,
        false,
    );
    lane(&mut work.admission.raw.state_scan.nodes_remaining, false);
    lane(
        &mut work.admission.raw.state_scan.read_bytes_remaining,
        true,
    );
    lane(&mut work.admission.raw.state_scan.leaves_remaining, false);
    lane(&mut work.admission.raw.agents.cells_remaining, false);
    lane(&mut work.admission.raw.agent_changes.cells_remaining, false);
    lane(
        &mut work.admission.raw.module_states.states_remaining,
        false,
    );
    lane(
        &mut work.admission.raw.module_states.logical_bytes_remaining,
        true,
    );
    lane(&mut work.admission.raw.instance_relations_remaining, false);
    lane(&mut work.admission.raw.allocators.records_remaining, false);
    lane(&mut work.admission.raw.schedule.cells_remaining, false);
    lane(
        &mut work.admission.raw.schedule.index.nodes_remaining,
        false,
    );
    lane(
        &mut work.admission.raw.schedule.index.read_bytes_remaining,
        true,
    );
    lane(
        &mut work.admission.raw.schedule.index.buckets_remaining,
        false,
    );
    lane(
        &mut work.admission.raw.schedule.index.instances_remaining,
        false,
    );
    lane(&mut work.admission.raw.mailbox_occurrences_remaining, false);
    lane(&mut work.admission.raw.originals.objects_remaining, false);
    lane(
        &mut work.admission.raw.originals.original_bytes_remaining,
        true,
    );
    lane(
        &mut work.admission.raw.originals.canonical.visits_remaining,
        false,
    );
    lane(
        &mut work.admission.raw.originals.canonical.bytes_remaining,
        true,
    );
    lane(&mut work.admission.cognition.scan.nodes_remaining, false);
    lane(
        &mut work.admission.cognition.scan.read_bytes_remaining,
        true,
    );
    lane(&mut work.admission.cognition.scan.leaves_remaining, false);
    lane(
        &mut work.admission.cognition.json.json.cells_remaining,
        false,
    );
    lane(
        &mut work.admission.cognition.json.json.json_nodes_remaining,
        false,
    );
    lane(&mut work.admission.cognition.json.paths_remaining, false);
    lane(&mut work.admission.cognition.outer_cells_remaining, false);
    lane(&mut work.runtime_index.cells_remaining, false);
    lane(&mut work.cognition_index.cells_remaining, false);
    lane(&mut work.hashes.input_bytes_remaining, true);
    lane(&mut work.hashes.hash_nodes_remaining, false);
    lane(&mut work.mailbox_decode.structure.bytes_remaining, true);
    lane(&mut work.mailbox_decode.structure.values_remaining, false);
    lane(&mut work.mailbox_decode.canonical.visits_remaining, false);
    lane(&mut work.mailbox_decode.canonical.bytes_remaining, true);
    lane(&mut work.local_records_remaining, false);
    lane(&mut work.local_original_bytes_remaining, true);
}
#[derive(Clone, Copy)]
enum LaneKind {
    Steps,
    IoBytes,
    WorkBytes,
}
impl LaneKind {
    fn io(bytes: bool) -> Self {
        if bytes { Self::IoBytes } else { Self::Steps }
    }
    fn is_bytes(self) -> bool {
        !matches!(self, Self::Steps)
    }
    fn step_weight(self) -> u64 {
        match self {
            Self::Steps => 1,
            Self::IoBytes => 16,
            Self::WorkBytes => 0,
        }
    }
    fn limit(self, steps: u64, bytes: u64) -> u64 {
        match self {
            Self::Steps => steps,
            Self::IoBytes => (steps / 16).min(bytes),
            Self::WorkBytes => bytes,
        }
    }
}

/// Two temporary projections admitted by the original exclusive Common child.
/// They start at zero, cannot escape the material operation, and settle once
/// through that same child's inventory. There is no second Common or refill.
pub(super) struct CatalogWorkLoanV1 {
    steps: u64,
    bytes: u64,
}
impl CatalogWorkLoanV1 {
    pub(super) fn charge(&mut self, steps: u64, bytes: u64) -> Result<(), FormalError> {
        let enough_steps = self.steps >= steps;
        let enough_bytes = self.bytes >= bytes;
        self.steps = self.steps.saturating_sub(steps);
        self.bytes = self.bytes.saturating_sub(bytes);
        if !enough_steps {
            Err(FormalBoundedChargeErrorV1::StepLimit.into())
        } else if !enough_bytes {
            Err(FormalBoundedChargeErrorV1::ByteLimit.into())
        } else {
            Ok(())
        }
    }
}

trait Inventory {
    fn lanes(&mut self, visit: &mut dyn FnMut(&mut u64, LaneKind));
    fn admit_loans(&mut self, _steps: u64, _bytes: u64) {}
}
impl Inventory for IwsColdParentWorkV1 {
    fn lanes(&mut self, visit: &mut dyn FnMut(&mut u64, LaneKind)) {
        cold_lanes(self, |value, bytes| visit(value, LaneKind::io(bytes)));
    }
}
struct MaterialLoans<'a> {
    cold: &'a mut IwsColdParentWorkV1,
    source: &'a mut super::work::IwsCandidateSourceWorkV1,
    catalog: CatalogWorkLoanV1,
}
impl Inventory for MaterialLoans<'_> {
    fn lanes(&mut self, visit: &mut dyn FnMut(&mut u64, LaneKind)) {
        {
            let mut visit = |value: &mut u64, bytes| visit(value, LaneKind::io(bytes));
            cold_lanes(self.cold, &mut visit);
            let w = &mut self.source;
            visit(&mut w.artifacts.structural.objects_remaining, false);
            visit(&mut w.artifacts.structural.original_bytes_remaining, true);
            visit(&mut w.artifacts.structural.new_pages_remaining, false);
            visit(&mut w.artifacts.structural.merkle_reads_remaining, false);
            visit(&mut w.artifacts.originals.objects_remaining, false);
            visit(&mut w.artifacts.originals.bytes_remaining, true);
            visit(&mut w.native_chunk_occurrences, false);
            visit(&mut w.native_chunk_bytes, true);
            visit(&mut w.frozen_wire_bytes, true);
            visit(&mut w.frozen_material.metadata_entries_remaining, false);
            visit(&mut w.frozen_material.comparison_bytes_remaining, true);
            visit(&mut w.frozen_material.hash_bytes_remaining, true);
            visit(&mut w.frozen_material.frozen_wire_bytes_remaining, true);
            visit(&mut w.frozen_material.original_calls_remaining, false);
            visit(&mut w.frozen_material.original_bytes_remaining, true);
        }
        visit(&mut self.catalog.steps, LaneKind::Steps);
        visit(&mut self.catalog.bytes, LaneKind::WorkBytes);
    }
    fn admit_loans(&mut self, steps: u64, bytes: u64) {
        // Derived only after the original child is locked and all lanes counted.
        self.catalog.steps = steps;
        self.catalog.bytes = bytes;
    }
}
pub(super) fn run<T, E: From<FormalError>>(
    work: &mut IwsColdParentWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    operation: impl FnOnce(&mut IwsColdParentWorkV1, &mut dyn FnMut() -> bool) -> Result<T, E>,
) -> Result<T, E> {
    run_inventory(work, common, operation)
}
pub(super) fn run_material<T, E: From<FormalError>>(
    work: &mut IwsColdParentWorkV1,
    source: &mut super::work::IwsCandidateSourceWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    operation: impl FnOnce(
        &mut IwsColdParentWorkV1,
        &mut super::work::IwsCandidateSourceWorkV1,
        &mut CatalogWorkLoanV1,
        &mut dyn FnMut() -> bool,
    ) -> Result<T, E>,
) -> Result<T, E> {
    run_inventory(
        &mut MaterialLoans {
            cold: work,
            source,
            catalog: CatalogWorkLoanV1 { steps: 0, bytes: 0 },
        },
        common,
        |loans, stop| operation(loans.cold, loans.source, &mut loans.catalog, stop),
    )
}
fn run_inventory<W: Inventory, T, E: From<FormalError>>(
    work: &mut W,
    common: &mut FormalBoundedContextV1<'_>,
    operation: impl FnOnce(&mut W, &mut dyn FnMut() -> bool) -> Result<T, E>,
) -> Result<T, E> {
    common.consume_steps(1)?;
    let _fee = common
        .memory()
        .reserve(
            3 * std::mem::size_of::<[u64; CAPACITY]>()
                + std::mem::size_of::<[LaneKind; CAPACITY]>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| FormalError::Invalid("cold IO inventory admission".into()))?;
    let mut originals = [0u64; CAPACITY];
    let mut before = [0u64; CAPACITY];
    let mut kinds = [LaneKind::Steps; CAPACITY];
    let mut count = 0usize;
    let mut overflow = false;
    work.lanes(&mut |value, kind| {
        if count < CAPACITY {
            originals[count] = *value;
            kinds[count] = kind;
        } else {
            overflow = true;
        }
        count += 1;
    });
    // The array is a layout bound only: it never silently truncates a lane.
    if overflow || count == 0 {
        return Err(FormalError::Invalid("cold IO inventory layout".into()).into());
    }
    let byte_count = kinds[..count].iter().filter(|kind| kind.is_bytes()).count() as u64;
    let (_, meter, stop) = common.parts();
    let child = meter.begin_child()?;
    let step_limit = child.remaining_steps() / count as u64;
    let byte_limit = child
        .remaining_read_bytes()
        .checked_div(byte_count)
        .unwrap_or(0);
    work.admit_loans(step_limit, byte_limit);
    let mut i = 0;
    work.lanes(&mut |value, kind| {
        *value = (*value).min(kind.limit(step_limit, byte_limit));
        before[i] = *value;
        i += 1;
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(work, stop)));
    let mut delta = [0u64; CAPACITY];
    let mut increased = false;
    i = 0;
    work.lanes(&mut |value, _| {
        match before[i].checked_sub(*value) {
            Some(spent) => {
                delta[i] = spent;
                *value = originals[i].saturating_sub(spent);
            }
            None => {
                increased = true;
                *value = 0;
            }
        }
        i += 1;
    });
    let accounting = (|| {
        if increased {
            return Err(FormalError::Invalid("cold IO counters increased".into()));
        }
        let mut steps = 0u64;
        let mut bytes = 0u64;
        for i in 0..count {
            let weight = kinds[i].step_weight();
            steps = steps
                .checked_add(
                    delta[i]
                        .checked_mul(weight)
                        .ok_or_else(|| FormalError::Invalid("cold IO step overflow".into()))?,
                )
                .ok_or_else(|| FormalError::Invalid("cold IO step overflow".into()))?;
            if kinds[i].is_bytes() {
                bytes = bytes
                    .checked_add(delta[i])
                    .ok_or_else(|| FormalError::Invalid("cold IO byte overflow".into()))?;
            }
        }
        child.consume(steps, bytes)
    })();
    match result {
        Err(payload) => {
            drop(child);
            std::panic::resume_unwind(payload)
        }
        Ok(result) => {
            if let Err(error) = accounting {
                drop(child);
                return match result {
                    Err(first) => Err(first),
                    Ok(_) => Err(error.into()),
                };
            }
            child.finish();
            // Preserve the first real source/decoder error over accounting or
            // trailing cancellation. Err and success both debit actual work.
            let value = result?;
            common.consume_steps(0)?;
            Ok(value)
        }
    }
}

#[cfg(test)]
#[path = "endpoint_incremental_cold_io_tests.rs"]
mod tests;
