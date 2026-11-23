use super::*;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
use std::{cell::Cell, sync::Arc};
fn seed_sized(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    source: &WorldState,
    text: usize,
) -> RootSetV1 {
    let _caller = memory.reserve(256 << 10, MemoryClass::Ordinary).unwrap();
    let mut writer = store.value_session(physical());
    let mut rows = Vec::with_capacity(16);
    let mut work = RewardRecordsWorkV1 {
        cells_remaining: 55,
        max_pending_node_bytes: 2 << 20,
    };
    prepare_reward_records_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: text.max(1024) as u64,
                record_bytes: (text + 8192) as u64,
            },
            cancelled: || false,
        },
        &mut work,
        &mut |row| {
            assert!(rows.len() < 16);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(work.cells_remaining, 0);
    assert_eq!(rows.len(), 10);
    let mut roots = RootSetV1::empty("reward-budget-test".into(), [0x38; 32]).unwrap();
    for ns in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut writes = rows
            .iter()
            .filter(|row| row.namespace == ns)
            .map(|row| StateWriteV1 {
                key: row.mutation.key.clone(),
                before: None,
                after: row.mutation.after,
            })
            .collect::<Vec<_>>();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let delta = memory
            .reserve(8 << 20, MemoryClass::Ordinary)
            .unwrap()
            .retain(
                StateDeltaV1::build(StateRootV1::empty(ns).unwrap(), writes, &state, 2 << 20)
                    .unwrap(),
            );
        writer.put_state_pages(&delta, memory).unwrap();
        *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() = delta.after.clone();
    }
    roots
}

struct Forbid<'a, R> {
    source: &'a R,
    id: [u8; 32],
    hits: &'a Cell<usize>,
    calls: &'a Cell<usize>,
}
impl<R: ValueNodeReader> ValueNodeReader for Forbid<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.calls.set(self.calls.get() + 1);
        if id == &self.id {
            self.hits.set(self.hits.get() + 1);
            return Err(ValueTreeError::Invalid(
                "unchanged mint signature was scanned",
            ));
        }
        self.source.read_node(id, max, memory, class)
    }
}
#[test]
fn append_and_receiver_skip_unchanged_signature_through_thirty_two_mib() {
    let mut samples = Vec::new();
    for text in [64usize, 1 << 20, 32 << 20] {
        let directory = Directory::new();
        let path = directory.0.join("scale");
        let roots = {
            let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
            let _source = memory
                .reserve(text + (256 << 10), MemoryClass::Ordinary)
                .unwrap();
            let mut original = source();
            original.reward_mint_records[0].signature = "x".repeat(text);
            let store =
                IncrementalPageStore::create(&path, memory.clone(), PageCompression::Zstd).unwrap();
            seed_sized(&store, &memory, &original, text)
        };
        // Original World, source store and producing pool are gone; complete bootstrap
        // consumes Recovery4MiB before any ordinary operation or inherited closure.
        let memory = MemoryBudget::new(24 << 20, 4 << 20).unwrap();
        let _roots = memory.reserve(8192, MemoryClass::Recovery).unwrap();
        let store = IncrementalPageStore::open(
            &path,
            memory.clone(),
            PageCompression::Zstd,
            MemoryClass::Recovery,
        )
        .unwrap();
        let (family, forbidden) = {
            let state = store
                .state_session(physical(), MemoryClass::Recovery)
                .unwrap();
            let values = store.value_session(physical());
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut context = ctx!(
                &roots.state[0],
                &state,
                &values,
                &memory,
                MemoryClass::Recovery,
                &mut io,
                &mut keys
            );
            context.record_bytes = (text + 8192) as u64;
            let family = qualify_reward_family_v1(
                &roots,
                &mut context,
                &mut scan(),
                &mut scan(),
                &mut 8192,
                text.max(165) as u64,
            )
            .unwrap();
            assert_eq!(family.mint_length(), 3);
            context.root = roots
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .unwrap();
            let key = RewardRecordKeyV1 {
                kind: RewardRecordKindV1::Mint,
                index: 0,
            };
            let record = open_qualified_reward_record_v1(key, &roots, &family, &mut context)
                .unwrap()
                .unwrap();
            let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
            semantic.extend_from_slice(&[5, 0, 1]);
            semantic.extend_from_slice(&9u64.to_be_bytes());
            semantic.extend_from_slice(b"signature");
            let field = FieldDescriptor {
                owner: FieldOwner::WorldState,
                name: "reward_mint_records",
                shape: FieldShape::Ordered,
                destination: FieldDestination::State(StateNamespaceV1::Authorization),
            };
            let signature = WorldFieldKeyV1::new(
                field,
                WorldFieldRecordKindV1::Entry,
                Some(*blake3::hash(&semantic).as_bytes()),
            )
            .unwrap();
            let id = record
                .root()
                .get(signature.as_bytes(), &state)
                .unwrap()
                .unwrap();
            (family, id)
        };
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let hits = Cell::new(0usize);
        let calls = Cell::new(0usize);
        let source = Forbid {
            source: &values,
            id: forbidden,
            hits: &hits,
            calls: &calls,
        };
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 32,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut read_io = logical();
        read_io.nodes_remaining = 4096;
        read_io.read_bytes_remaining = 512 << 10;
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut writer = store.value_session(physical());
        let mut write_io = logical();
        write_io.nodes_remaining = 8192;
        write_io.read_bytes_remaining = 512 << 10;
        write_io.write_bytes_remaining = 1 << 20;
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 165,
                record_bytes: 8192,
            },
            cancelled: || false,
        };
        let minted = NodeRewardMintRecord {
            epoch_index: 7,
            node_id: "append node".into(),
            source_awarded_points: 1,
            minted_power_credits: 2,
            settlement_hash: "append hash".into(),
            signer_node_id: "append signer".into(),
            signature: "tiny append signature".into(),
        };
        let guard = overlay.transaction.guard();
        let _appended = append_qualified_reward_mint_v1(
            RewardMintAppendV1 {
                expected_length: 3,
                source: &minted,
                family: &family,
            },
            &mut overlay,
            &mut write,
            &mut RewardRecordsWorkV1 {
                cells_remaining: 100,
                max_pending_node_bytes: 2 << 20,
            },
            &guard,
            &live,
        )
        .unwrap();
        let sender = (
            4096 - overlay.values.io.nodes_remaining,
            (512 << 10) - overlay.values.io.read_bytes_remaining,
            8192 - write.io.nodes_remaining,
            (512 << 10) - write.io.read_bytes_remaining,
            (1 << 20) - write.io.write_bytes_remaining,
        );
        let sender_calls = calls.get();
        let guard = overlay.transaction.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.writes.len(), 2);
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let mut io = logical();
        io.nodes_remaining = 4096;
        io.read_bytes_remaining = 512 << 10;
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut context = ctx!(
            &sealed.record.after.state[0],
            &state,
            &source,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        context.record_bytes = 8192;
        let mut inner_scan = StateScanBudget {
            nodes_remaining: 8192,
            leaves_remaining: 9,
            read_bytes_remaining: 1 << 20,
        };
        let mut changed_cells = 11;
        let after = accept_reward_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut context,
            &mut inner_scan,
            &mut changed_cells,
            165,
        )
        .unwrap();
        assert_eq!(changed_cells, 0);
        assert_eq!(inner_scan.leaves_remaining, 0);
        eprintln!(
            "reward new child state scan nodes={} bytes={}",
            8192 - inner_scan.nodes_remaining,
            (1 << 20) - inner_scan.read_bytes_remaining
        );
        assert_eq!(after.mint_length(), 4);
        assert_eq!(after.record_counts(), [2, 4]);
        let receiver = (
            4096 - context.values.io.nodes_remaining,
            (512 << 10) - context.values.io.read_bytes_remaining,
        );
        assert_eq!(hits.get(), 0);
        eprintln!(
            "reward signature={text}: sender(contextnodes,contextbytes,producer_nodes,producer_read,producer_write)={sender:?}; receiver owner-valuepass={receiver:?}; valuecalls sender={sender_calls} after={}; generic/native state={:?}, values={:?}, producer={:?}",
            calls.get(),
            state.remaining_io(),
            values.remaining_io(),
            write.sink.remaining_io()
        );
        samples.push((sender, receiver, sender_calls, calls.get()));
    }
    assert!(
        samples.windows(2).all(|pair| pair[0] == pair[1]),
        "unchanged signature growth changed append/owner-pass work: {samples:?}"
    );
}
