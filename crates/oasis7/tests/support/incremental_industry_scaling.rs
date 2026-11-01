use super::*;
use std::{cell::Cell, sync::Arc};
#[derive(Default, Debug, Clone, Copy)]
struct Stats {
    state_calls: u64,
    state_bytes: u64,
    value_calls: u64,
    value_bytes: u64,
}
impl Stats {
    fn calls(self) -> u64 {
        self.state_calls + self.value_calls
    }
    fn bytes(self) -> u64 {
        self.state_bytes + self.value_bytes
    }
}
struct Counted<'a, R> {
    source: &'a R,
    stats: Cell<Stats>,
}
impl<R: ValueNodeReader> ValueNodeReader for Counted<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let page = self.source.read_node(id, max, memory, class)?;
        let mut stats = self.stats.get();
        if page.starts_with(b"oasis7.iws.state-page.v1\0") {
            stats.state_calls += 1;
            stats.state_bytes += page.len() as u64;
        } else {
            stats.value_calls += 1;
            stats.value_bytes += page.len() as u64;
        }
        self.stats.set(stats);
        Ok(page)
    }
}
pub(super) fn fixed_scalar_receiver_cost_excludes_unchanged_text_and_vectors() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let mut observed = Vec::new();
        for (text_bytes, tags) in [(8_000, 4), (420_000, 64)] {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let dir = Directory::new();
            let store =
                IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), compression)
                    .unwrap();
            let mut source = source();
            source.pending_material_transits.clear();
            source.recipe_completion_receipts.clear();
            source.factory_production_failure_dispositions.clear();
            source.logistics_settlement_receipts.clear();
            let recipe = source.pending_recipe_jobs.get_mut(&u64::MAX).unwrap();
            recipe.requester_agent_id = format!("原/{}", "x".repeat(text_bytes));
            recipe.bottleneck_tags = vec!["same-original-tag".to_owned(); tags];
            let roots = seed(&store, &memory, &source);
            let qualification = boundaries::qualify_full(&store, &memory, &roots);
            assert_eq!(qualification.record_counts(), [1, 0, 0, 0, 0]);
            let parent = parent(roots);
            let live = parent.fence().unwrap();
            let state = store
                .state_session(physical_scope(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical_scope());
            let mut writer = store.value_session(physical_scope());
            let mut tx = ExecutionTransaction::begin(
                parent.clone(),
                [0x94; 32],
                &state,
                &memory,
                tx_limits(),
            )
            .unwrap();
            let mut read_io = logical();
            let read_before = read_io.clone();
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut tx,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut read_io,
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 2 << 20,
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("industry scalar state"))
                    },
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 165,
                        record_bytes: 8192,
                    },
                    cancelled: || false,
                };
                let guard = read.transaction.guard();
                assert!(
                    change_industry_cell_overlay_v1(
                        IndustryCellChangeV1 {
                            key: IndustryRecordKeyV1 {
                                kind: IndustryRecordKindV1::Recipe,
                                id: u64::MAX
                            },
                            cell: IndustryCellV1::Column(IndustryColumnV1::RecordReadyAt),
                            expected: Some(IndustryValueRefV1::Unsigned(u64::MAX)),
                            after: Some(IndustryValueRefV1::Unsigned(u64::MAX - 1)),
                            max_current_payload_bytes: 8
                        },
                        &mut read,
                        &mut write,
                        &mut work(),
                        &guard,
                        &live
                    )
                    .unwrap()
                );
            }
            let guard = tx.guard();
            let sealed = tx.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.writes.len(), 1);
            let provider = store.value_session(physical_scope());
            let counted = Counted {
                source: &provider,
                stats: Cell::new(Stats::default()),
            };
            let mut receiver_io = logical();
            let before = receiver_io.clone();
            let verified = verify_industry_transition_v1(
                &parent,
                [0x94; 32],
                &sealed.record,
                &qualification,
                &mut IndustryTransitionReadContextV1 {
                    values: ValueReadContext {
                        source: &counted,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut receiver_io,
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 2 << 20,
                    max_payload_bytes: 8,
                    cancelled: || false,
                },
                &mut work(),
            )
            .unwrap();
            assert_eq!(verified.qualification().record_counts(), [1, 0, 0, 0, 0]);
            let stats = counted.stats.get();
            assert!(stats.calls() > 0);
            assert_eq!(
                before.nodes_remaining - receiver_io.nodes_remaining,
                stats.calls()
            );
            assert_eq!(
                before.read_bytes_remaining - receiver_io.read_bytes_remaining,
                stats.bytes()
            );
            eprintln!(
                "industry scalar codec={compression:?} text={text_bytes} tags={tags} provider={stats:?} logical={}/{} sender={}/{} native_remaining={:?}",
                before.nodes_remaining - receiver_io.nodes_remaining,
                before.read_bytes_remaining - receiver_io.read_bytes_remaining,
                read_before.nodes_remaining - read_io.nodes_remaining,
                read_before.read_bytes_remaining - read_io.read_bytes_remaining,
                provider.remaining_io().unwrap()
            );
            observed.push(stats);
        }
        let [small, large] = observed.as_slice() else {
            unreachable!()
        };
        assert!(
            large.calls() <= small.calls() + 64,
            "receiver traversed unchanged Vec members: {observed:?}"
        );
        assert!(
            large.bytes() <= small.bytes() + (64 << 10),
            "receiver traversed unchanged text: {observed:?}"
        );
    }
}
