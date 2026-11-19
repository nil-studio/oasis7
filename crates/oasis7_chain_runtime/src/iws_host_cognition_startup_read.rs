//! Full source verification at the two allowed admission boundaries. These
//! explicit legacy counters are exclusively projected from the same Common;
//! no parent operation can run while the child loan is active.
use super::*;
use oasis7_distfs::controlled_authority::formal::endpoint::HostAcceptedParentReadViewV1;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionFactKindV1,
    incremental_material::{CoreBudget, FactBudget, FactReadContextV1, FactRequestV1},
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueIoBudget, ValueReadContext},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_cognition_proof_ledger::{
        AcceptedCognitionProofLedgerV1, recover_cognition_proof_ledger_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::JsonReadWorkV1,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_result_router::WorldResultRouterWorkV1,
    incremental_world_result_router_accepted::{
        AcceptedWorldResultRouterRequestV1, route_accepted_world_results_v1,
    },
};

/// Finite independent remaining source accounts, supplied once by the caller.
/// This carrier contains no accepted state, Core identity or refill operation.
pub struct HostCognitionStartupWorkV1 {
    pub core: CoreBudget,
    pub facts: FactBudget,
    pub router: WorldResultRouterWorkV1,
    pub fields: ValueIoBudget,
    pub keys: SemanticKeyBudget,
    pub json: JsonReadWorkV1,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
}

const LANES: usize = 39;
fn lanes(
    io: &mut HostColdIoWorkV1,
    work: &mut HostCognitionStartupWorkV1,
    mut lane: impl FnMut(&mut u64, bool),
) {
    for physical in [&mut io.state, &mut io.values, &mut io.originals] {
        lane(&mut physical.reads_remaining, false);
        lane(&mut physical.writes_remaining, false);
        lane(&mut physical.read_bytes_remaining, true);
        lane(&mut physical.write_bytes_remaining, true);
    }
    lane(&mut io.original_values.nodes_remaining, false);
    lane(&mut io.original_values.read_bytes_remaining, true);
    lane(&mut io.original_values.write_bytes_remaining, true);
    lane(&mut work.core.objects_remaining, false);
    lane(&mut work.core.new_pages_remaining, false);
    lane(&mut work.core.merkle_reads_remaining, false);
    lane(&mut work.core.original_bytes_remaining, true);
    lane(&mut work.facts.facts_remaining, false);
    lane(&mut work.facts.original_bytes_remaining, true);
    lane(&mut work.router.facts.facts_remaining, false);
    lane(&mut work.router.facts.original_bytes_remaining, true);
    lane(&mut work.router.envelope.values_remaining, false);
    lane(&mut work.router.envelope.bytes_remaining, true);
    lane(&mut work.router.runtime.structure.values_remaining, false);
    lane(&mut work.router.runtime.structure.bytes_remaining, true);
    lane(&mut work.router.runtime.canonical.visits_remaining, false);
    lane(&mut work.router.runtime.canonical.bytes_remaining, true);
    lane(&mut work.router.cognition.nodes_remaining, false);
    lane(&mut work.router.cognition.source_reads_remaining, false);
    lane(&mut work.router.cognition.bytes_remaining, true);
    lane(&mut work.router.cognition.key_bytes_remaining, true);
    lane(&mut work.json.cells_remaining, false);
    lane(&mut work.json.json_nodes_remaining, false);
    lane(&mut work.keys.bytes_remaining, true);
    lane(&mut work.fields.nodes_remaining, false);
    lane(&mut work.fields.read_bytes_remaining, true);
    lane(&mut work.fields.write_bytes_remaining, true);
}

fn metered<T>(
    io: &mut HostColdIoWorkV1,
    work: &mut HostCognitionStartupWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    run: impl FnOnce(
        &mut HostColdIoWorkV1,
        &mut HostCognitionStartupWorkV1,
    ) -> Result<T, HostCognitionStartupErrorV1>,
) -> Result<T, HostCognitionStartupErrorV1> {
    common.charge(4 * LANES as u64, 8192)?;
    let _fee = common
        .memory()
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let (mut original, mut before, mut bytes) = ([0; LANES], [0; LANES], [false; LANES]);
    let mut count = 0;
    lanes(io, work, |value, is_bytes| {
        if count < LANES {
            original[count] = *value;
            bytes[count] = is_bytes;
        }
        count += 1;
    });
    if count != LANES {
        return Err(invalid("Cognition startup complete counter inventory"));
    }
    let child = common.work().begin_child()?;
    // Every content byte also pays one bounded-work step. Dividing the single
    // remaining allowance makes all concurrent private lanes jointly bounded.
    let step_limit = child.remaining_steps() / LANES as u64;
    let byte_limit = child.remaining_read_bytes() / bytes.iter().filter(|v| **v).count() as u64;
    let mut index = 0;
    lanes(io, work, |value, is_bytes| {
        *value = (*value).min(if is_bytes {
            step_limit.min(byte_limit)
        } else {
            step_limit
        });
        before[index] = *value;
        index += 1;
    });
    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(io, work))) {
        Ok(result) => result,
        Err(payload) => {
            lanes(io, work, |value, _| *value = 0);
            drop(child);
            std::panic::resume_unwind(payload);
        }
    };
    let mut used = [0; LANES];
    let mut increased = false;
    index = 0;
    lanes(io, work, |value, _| {
        match before[index].checked_sub(*value) {
            Some(delta) => {
                used[index] = delta;
                *value = original[index] - delta;
            }
            None => {
                increased = true;
                *value = 0;
            }
        }
        index += 1;
    });
    let debit = if increased {
        Err(FormalError::Invalid(
            "Cognition startup counters increased".into(),
        ))
    } else {
        child.consume(
            used.iter().sum(),
            used.iter()
                .zip(bytes)
                .filter_map(|(used, is_bytes)| is_bytes.then_some(*used))
                .sum(),
        )
    };
    if let Err(error) = debit {
        lanes(io, work, |value, _| *value = 0);
        drop(child);
        return match result {
            Err(first) => Err(first),
            Ok(_) => Err(error.into()),
        };
    }
    child.finish();
    let value = result?;
    common.consume_steps(0)?;
    Ok(value)
}

pub(super) fn recover(
    parent: &HostAcceptedParentReadViewV1<'_>,
    io: &mut HostColdIoWorkV1,
    work: &mut HostCognitionStartupWorkV1,
    memory: &MemoryBudget,
    cancelled: &AtomicBool,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<AcceptedCognitionProofLedgerV1>, HostCognitionStartupErrorV1> {
    common.charge(8, 8192)?;
    let selected = parent
        .storage()
        .results()
        .proof_ledger()
        .ok_or_else(|| invalid("Cognition actual ProofLedger role"))?
        .occurrence()
        .location();
    let pages = parent.pages();
    let objects = super::super::normal::io::open_originals_metered(
        pages,
        &mut io.originals,
        cancelled,
        common,
    )?;
    metered(io, work, common, |io, work| {
        let budget = take(&mut io.state);
        let ceiling = amounts(&budget);
        let state = Loan {
            value: pages.state_session_with_cancelled(budget, MemoryClass::Ordinary, cancelled)?,
            slot: &mut io.state,
            ceiling,
        };
        let budget = take(&mut io.values);
        let ceiling = amounts(&budget);
        let values = Loan {
            value: pages.value_session_with_cancelled(budget, cancelled),
            slot: &mut io.values,
            ceiling,
        };
        let holder = super::super::normal::originals::HostNormalOriginalsV1::new(
            &objects,
            ObjectIo {
                physical: &mut io.originals,
                logical: &mut io.original_values,
                class: MemoryClass::Ordinary,
                cancelled,
            },
            memory,
        )?;
        let mut originals = holder.reader();
        // Fresh GEN's checked router and Cold's accepted router are different
        // types. Re-read the actual closed accepted FactSet using its already
        // accepted Runtime anchor; never relabel the checked router or clone Q.
        let routing = route_accepted_world_results_v1(
            &AcceptedWorldResultRouterRequestV1 {
                head: parent.head(),
                roots: parent.roots(),
                anchor: parent.runtime().occurrence(),
            },
            &mut originals,
            &state.value,
            memory,
            MemoryClass::Ordinary,
            &mut work.core,
            &mut work.router,
            || cancelled.load(Ordering::Acquire),
        );
        if let Some(error) = state.value.take_read_error() {
            return Err(error.into());
        }
        let routing = routing.map_err(HostCognitionStartupErrorV1::Router)?;
        let routed = routing
            .proof_ledger()
            .ok_or_else(|| invalid("Cognition recovered ProofLedger role"))?;
        if routed.occurrence().location() != selected {
            return Err(invalid("Cognition exact routed ProofLedger occurrence"));
        }
        let mut fields = WorldFieldReadContextV1 {
            root: parent.roots().state_root(StateNamespaceV1::Allocator),
            state: &state.value,
            values: ValueReadContext {
                source: &values.value,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut work.fields,
            },
            key_work: &mut work.keys,
            semantic_bytes: work.semantic_bytes,
            record_bytes: work.record_bytes,
            take_state_error: || {
                state
                    .value
                    .take_read_error()
                    .map(|error| ValueTreeError::Io(std::io::Error::other(error)))
            },
            cancelled: || cancelled.load(Ordering::Acquire),
        };
        let ledger = recover_cognition_proof_ledger_v1(
            &FactRequestV1 {
                kind: ExecutionFactKindV1::Result,
                head: parent.head(),
                roots: parent.roots(),
                location: selected,
            },
            &routing,
            &mut FactReadContextV1 {
                originals: &mut originals,
                catalog_pages: &state.value,
                memory,
                class: MemoryClass::Ordinary,
                core_budget: &mut work.core,
                fact_budget: &mut work.facts,
                cancelled: || cancelled.load(Ordering::Acquire),
            },
            &mut fields,
            &mut work.json,
        );
        // Return the first typed state IO failure even if a generic catalog
        // implementation swallowed its String boundary before returning Ok.
        if let Some(error) = state.value.take_read_error() {
            return Err(error.into());
        }
        ledger.map_err(HostCognitionStartupErrorV1::Ledger)
    })
}
