//! Real World producer and bounded physical six-namespace GEN assembly.
use super::*;
use oasis7_distfs::incremental_page_store::genesis::GenesisIndexWorkV1;
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_formal_protocol::incremental_history::GenesisConfigV1;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::incremental_genesis::RuntimeGenesisSummaryV1;
use oasis7_runtime::{BodyMutationError, World};

pub(super) fn prepare(
    world: &World,
    store: &IncrementalPageStore,
    config: &GenesisConfigV1,
    memory: &MemoryBudget,
    cancelled: &AtomicBool,
) -> (ReservedValue<RootSetV1>, RuntimeGenesisSummaryV1) {
    let mut index = store
        .genesis_state_index_builder(
            config,
            GenesisIndexWorkV1 {
                rows_remaining: 100_000,
                new_nodes_remaining: 1_000_000,
                new_bytes_remaining: 256 << 20,
                max_pending_node_bytes: 16 << 20,
            },
            physical(),
            physical(),
            cancelled,
        )
        .unwrap();
    let mut pages = store.value_session_with_cancelled(physical(), cancelled);
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut summary = None;
    // This fixture intentionally owns an actual empty fresh World. Unexpected
    // originals fail closed; no callback manufactures IDs for unavailable bytes.
    index
        .contribute::<oasis7_runtime::incremental_genesis::RuntimeGenesisErrorV1>(|sink| {
            let actual = world.prepare_iws_genesis_v1(
                &mut MemberRecordWriteContext {
                    sink: &mut pages,
                    memory,
                    io: &mut io,
                    key_work: &mut keys,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 8 << 20,
                        record_bytes: 9 << 20,
                    },
                    cancelled: || cancelled.load(Ordering::Acquire),
                },
                &mut genesis_work::work(),
                &mut |_, _| Err(ValueTreeError::Invalid("unexpected fresh GEN artifact")),
                &mut |_, _| {
                    Err(ValueTreeError::Invalid(
                        "unexpected fresh GEN pending action",
                    ))
                },
                &mut |_, _, _| Err(BodyMutationError::Invalid("unexpected fresh GEN mailbox")),
                &mut |row| sink(row),
            )?;
            summary = Some(actual);
            Ok(())
        })
        .unwrap();
    (index.finish().unwrap(), summary.unwrap())
}
