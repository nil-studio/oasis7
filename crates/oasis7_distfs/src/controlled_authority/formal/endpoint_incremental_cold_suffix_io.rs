//! The suffix uses the original Cold and candidate-source allowances. Every
//! scalar it can spend joins the same exclusive Common child on Err and unwind.
use super::super::work::IwsCandidateSourceWorkV1;
use super::*;

fn source_lanes(work: &mut IwsCandidateSourceWorkV1, lane: &mut dyn FnMut(&mut u64, LaneKind)) {
    macro_rules! steps {
        ($($field:expr),* $(,)?) => { $(lane($field, LaneKind::Steps);)* };
    }
    macro_rules! bytes {
        ($($field:expr),* $(,)?) => { $(lane($field, LaneKind::IoBytes);)* };
    }
    steps!(
        &mut work.router.facts.facts_remaining,
        &mut work.router.envelope.values_remaining,
        &mut work.router.runtime.structure.values_remaining,
        &mut work.router.runtime.canonical.visits_remaining,
        &mut work.router.cognition.nodes_remaining,
        &mut work.router.cognition.source_reads_remaining,
        &mut work.proof_ledger_metadata_steps,
        &mut work.proof_ledger_cold.cells_remaining,
        &mut work.proof_ledger_cold.json_nodes_remaining,
        &mut work.proof_ledger_changed.json.cells_remaining,
        &mut work.proof_ledger_changed.json.json_nodes_remaining,
        &mut work.proof_ledger_changed.paths_remaining,
        &mut work.runtime.rows_remaining,
        &mut work.runtime.index.cells_remaining,
        &mut work.runtime.originals.facts_remaining,
        &mut work.runtime.decode.structure.values_remaining,
        &mut work.runtime.decode.canonical.visits_remaining,
        &mut work.cognition_index.cells_remaining,
        &mut work.cognition_cbor.nodes_remaining,
        &mut work.cognition_cbor.source_reads_remaining,
        &mut work.cognition_hash.hash_nodes_remaining,
        &mut work.cognition_lifecycle.cells_remaining,
        &mut work.history_occurrences,
    );
    bytes!(
        &mut work.router.facts.original_bytes_remaining,
        &mut work.router.envelope.bytes_remaining,
        &mut work.router.runtime.structure.bytes_remaining,
        &mut work.router.runtime.canonical.bytes_remaining,
        &mut work.router.cognition.bytes_remaining,
        &mut work.router.cognition.key_bytes_remaining,
        &mut work.runtime.originals.original_bytes_remaining,
        &mut work.runtime.decode.structure.bytes_remaining,
        &mut work.runtime.decode.canonical.bytes_remaining,
        &mut work.cognition_cbor.bytes_remaining,
        &mut work.cognition_cbor.key_bytes_remaining,
        &mut work.cognition_hash.input_bytes_remaining,
        &mut work.cognition_lifecycle.key_bytes_remaining,
        &mut work.history_hash_bytes,
        &mut work.write_keys.bytes_remaining,
    );
    // This is already paired with its explicit visit allowance above.
    lane(&mut work.proof_ledger_metadata_bytes, LaneKind::WorkBytes);
    for io in [&mut work.history_physical, &mut work.write_physical] {
        steps!(&mut io.reads_remaining, &mut io.writes_remaining);
        bytes!(&mut io.read_bytes_remaining, &mut io.write_bytes_remaining);
    }
    for io in [&mut work.history_values, &mut work.write_values] {
        steps!(&mut io.nodes_remaining);
        bytes!(&mut io.read_bytes_remaining, &mut io.write_bytes_remaining);
    }
}

struct SuffixLoans<'a> {
    cold: &'a mut IwsColdParentWorkV1,
    source: &'a mut IwsCandidateSourceWorkV1,
    metadata: CatalogWorkLoanV1,
}
impl Inventory for SuffixLoans<'_> {
    fn lanes(&mut self, visit: &mut dyn FnMut(&mut u64, LaneKind)) {
        cold_lanes(self.cold, |value, bytes| visit(value, LaneKind::io(bytes)));
        source_lanes(self.source, visit);
        visit(&mut self.metadata.steps, LaneKind::Steps);
        visit(&mut self.metadata.bytes, LaneKind::WorkBytes);
    }
    fn admit_loans(&mut self, steps: u64, bytes: u64) {
        self.metadata.steps = steps;
        self.metadata.bytes = bytes;
    }
}

pub(in super::super) fn run_suffix<T, E: From<FormalError>>(
    work: &mut IwsColdParentWorkV1,
    source: &mut IwsCandidateSourceWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    operation: impl FnOnce(
        &mut IwsColdParentWorkV1,
        &mut IwsCandidateSourceWorkV1,
        &mut CatalogWorkLoanV1,
        &mut dyn FnMut() -> bool,
    ) -> Result<T, E>,
) -> Result<T, E> {
    run_inventory(
        &mut SuffixLoans {
            cold: work,
            source,
            metadata: CatalogWorkLoanV1 { steps: 0, bytes: 0 },
        },
        common,
        |loans, stop| operation(loans.cold, loans.source, &mut loans.metadata, stop),
    )
}
