use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayContextV1;
use oasis7_formal_protocol::{incremental_execution::*, incremental_state::StateScanContext};
struct Sink<'a>(&'a Pages);
impl ValueNodeWriter for Sink<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.nodes.borrow_mut().insert(id, Arc::new(value));
        Ok(())
    }
}
impl ValueNodeReader for Sink<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        ValueNodeReader::read_node(self.0, id, max, memory, class)
    }
}
impl StatePageWriter for Sink<'_> {}
fn seed(memory: &MemoryBudget) -> (Pages, RootSetV1) {
    let mut pages = Pages::new(memory);
    let mut rows = Vec::with_capacity(2);
    prepare_runtime_revocation_genesis_v1(
        &crate::CapabilityRevocationState::default(),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 4096,
                record_bytes: 4096,
            },
            cancelled: || false,
        },
        &mut write_work(),
        &mut |row| {
            assert!(rows.len() < 2);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let mut writes: Vec<_> = rows.iter().map(|r| r.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
        writes,
        &pages,
        4 << 20,
    )
    .unwrap();
    let root = save_delta(delta, &mut pages, memory);
    let mut roots = RootSetV1::empty("revocation-test".into(), [1; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Authorization)
        .unwrap() = root;
    (pages, roots)
}
#[test]
fn qualified_root_replacement_independent_receiver_and_failure_poison() {
    let noop_polls = std::cell::Cell::new(0);
    for case in [5, 0, 1, 2, 3, 4] {
        let polls = std::cell::Cell::new(0);
        let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
        let (pages, roots) = seed(&memory);
        let mut ri = io();
        let mut rk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let proof = qualify_runtime_revocation_v1(
            &roots,
            &mut context(&pages, &roots, &memory, &mut ri, &mut rk),
            &mut scan(),
            &mut read_work(),
        )
        .unwrap();
        proof.validate_binding(&roots).unwrap();
        let mut wrong = roots.clone();
        wrong.height += 1;
        assert!(proof.validate_binding(&wrong).is_err());
        assert!(readmit_runtime_revocation_ordinary_v1(&proof, &roots, &memory, || false).is_err());
        let mut sink = Sink(&pages);
        let mut after_dto = crate::CapabilityRevocationState::default();
        after_dto.epoch = 77;
        let after = prepare_runtime_revocation_body_v1(
            &after_dto,
            &mut MemberRecordWriteContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut io(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 4 << 20,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 4096,
                    record_bytes: 4096,
                },
                cancelled: || false,
            },
            &mut write_work(),
        )
        .unwrap();
        let parent = ExecutionParent {
            commit: [2; 32],
            roots: roots.clone(),
            authority: AuthorityBinding {
                token: [3; 32],
                epoch: 4,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [4; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 16,
                max_facts: 8,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        tx.write(StateNamespaceV1::Scheduler, b"outer-prefix", Some([9; 32]))
            .unwrap();
        let guard = tx.guard();
        let mut read_io = io();
        let mut read_keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut overlay_io = io();
        let mut overlay_keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut write_io = io();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut wrong_expected = proof.body().clone();
        wrong_expected.leaf_count += 1;
        let result = change_runtime_revocation_overlay_v1(
            &proof,
            if case == 1 {
                &wrong_expected
            } else {
                proof.body()
            },
            if case >= 3 { proof.body() } else { &after },
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut overlay_io,
                },
                key_work: &mut overlay_keys,
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || None,
                cancelled: || {
                    polls.set(polls.get() + 1);
                    case == 2 || (case == 3 && polls.get() >= noop_polls.get())
                },
            },
            &mut WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Authorization),
                state: &pages,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut read_io,
                },
                key_work: &mut read_keys,
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || {
                    if case == 4 {
                        Some(ValueTreeError::ResourceLimited("pending source failure"))
                    } else {
                        None
                    }
                },
                cancelled: || false,
            },
            &mut scan(),
            &mut read_work(),
            &mut MemberRecordWriteContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 4096,
                    record_bytes: 4096,
                },
                cancelled: || false,
            },
            &guard,
            &live,
        );
        if case == 5 {
            assert!(!result.unwrap());
            assert!(polls.get() > 1);
            noop_polls.set(polls.get());
            continue;
        }
        if case != 0 {
            assert!(result.is_err(), "case {case}");
            let guard = tx.guard();
            assert!(tx.seal(&guard, &live, 1).is_err());
            continue;
        }
        assert!(result.unwrap());
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        for node in &sealed.new_nodes {
            let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
            let charge = memory
                .reserve(node.bytes.len(), MemoryClass::Ordinary)
                .unwrap();
            sink.put_node(id, charge.retain(node.bytes.clone()))
                .unwrap();
        }
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [4; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let mut vi = io();
        let mut vk = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let next = accept_runtime_revocation_transition_v1(
            &proof,
            &verifier,
            &sealed.record,
            &mut context(&pages, &sealed.record.after, &memory, &mut vi, &mut vk),
            &mut scan(),
            &mut read_work(),
        )
        .unwrap();
        next.validate_binding(&sealed.record.after).unwrap();
        assert_eq!(next.body(), &after);
    }
}

struct Recovery<'a>(&'a Pages);
impl MerkleNodeReader for Recovery<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.0.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let Some(node) = MerkleNodeReader::read_node(self.0, id, max)? else {
            return Ok(None);
        };
        let charge = self
            .0
            .memory
            .reserve(node.as_ref().len(), MemoryClass::Recovery)
            .map_err(|e| e.to_string())?;
        Ok(Some(Node(Arc::new(charge.retain(node.as_ref().to_vec())))))
    }
}
#[test]
fn recovery_qualification_needs_explicit_same_pool_ordinary_readmission() {
    let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
    let (pages, roots) = seed(&memory);
    let recovery = Recovery(&pages);
    let proof = qualify_runtime_revocation_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Authorization),
            state: &recovery,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4 << 20,
            },
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    assert_eq!(proof.allocation_class(), MemoryClass::Recovery);
    let ordinary =
        readmit_runtime_revocation_ordinary_v1(&proof, &roots, &memory, || false).unwrap();
    assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
    ordinary.validate_binding(&roots).unwrap();
    let foreign = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
    assert!(readmit_runtime_revocation_ordinary_v1(&proof, &roots, &foreign, || false).is_err());
    let polls = std::cell::Cell::new(0);
    assert!(
        readmit_runtime_revocation_ordinary_v1(&proof, &roots, &memory, || {
            polls.set(polls.get() + 1);
            polls.get() > 1
        })
        .is_err()
    );
}

#[test]
fn misplaced_original_metadata_or_unit_and_missing_outer_records_never_qualify() {
    for case in 0..4 {
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let (mut pages, mut roots) = seed(&memory);
        let kind = if case % 2 == 0 {
            WorldFieldRecordKindV1::Metadata
        } else {
            WorldFieldRecordKindV1::Entry
        };
        let mut original = None;
        StateScanContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut scan(),
        }
        .visit_entries(
            roots.state_root(StateNamespaceV1::Authorization),
            |entry| {
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if parsed.kind == kind {
                    original = Some(entry.clone());
                }
                Ok::<(), ValueTreeError>(())
            },
            || false,
        )
        .unwrap();
        let original = original.unwrap();
        let namespace = if case < 2 {
            StateNamespaceV1::Business
        } else {
            StateNamespaceV1::Authorization
        };
        let delta = StateDeltaV1::build(
            roots.state_root(namespace).clone(),
            vec![StateWriteV1 {
                key: original.key,
                before: if case < 2 { None } else { Some(original.value) },
                after: if case < 2 { Some(original.value) } else { None },
            }],
            &pages,
            4 << 20,
        )
        .unwrap();
        let bad = save_delta(delta, &mut pages, &memory);
        *roots
            .state
            .iter_mut()
            .find(|r| r.namespace == namespace)
            .unwrap() = bad;
        let mut read_io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 4 << 20,
        };
        assert!(
            qualify_runtime_revocation_v1(
                &roots,
                &mut context(&pages, &roots, &memory, &mut read_io, &mut keys),
                &mut scan(),
                &mut read_work()
            )
            .is_err(),
            "case {case}"
        );
    }
}
