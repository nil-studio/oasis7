use super::*;
use crate::{
    WorldState,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer, io},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, ExecutionWriteV1,
        TransactionLimits,
    },
    incremental_state::StateDeltaV1,
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 100000,
    }
}
fn work() -> WorldCollectionControlsWorkV1 {
    WorldCollectionControlsWorkV1 {
        records_remaining: 100000,
        index_steps_remaining: 1_000_000,
        max_record_bytes: 2 << 20,
    }
}
fn source() -> WorldState {
    let mut s = WorldState::default();
    s.data_access_permissions = [(" group/界 ".into(), [" member/界 ".into()].into())].into();
    s.module_release_role_bindings = [(" group/界 ".into(), [" member/界 ".into()].into())].into();
    s.authenticated_collect_data_last_nonces = [(
        " group/界 ".into(),
        [(" member/界 ".into(), u64::MAX)].into(),
    )]
    .into();
    s
}
fn install(memory: &MemoryBudget, pages: &Pages) -> RootSetV1 {
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::new();
    let mut sink = |w| {
        rows.push(w);
        Ok(())
    };
    let mut writer = Writer(pages);
    let mut io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut ctx = MemberRecordWriteContext {
        sink: &mut writer,
        memory,
        io: &mut io,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 16,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let mut records = 10000;
    let source = source();
    crate::incremental_authorization_mapsets::prepare_authorization_mapsets_genesis_v1(
        &source,
        &mut ctx,
        &mut records,
        &mut sink,
    )
    .unwrap();
    crate::incremental_authenticated_nonces::prepare_authenticated_nonces_genesis_v1(
        &source,
        &mut ctx,
        &mut records,
        &mut sink,
    )
    .unwrap();
    crate::incremental_delivery_cursor::prepare_delivery_cursor_genesis_v1(
        &source.product_validation_delivery_cursor,
        &mut ctx,
        &mut records,
        &mut sink,
    )
    .unwrap();
    let mut roots = RootSetV1::empty("collection-controls".into(), [8; 32]).unwrap();
    for ns in [StateNamespaceV1::Authorization, StateNamespaceV1::Scheduler] {
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == ns)
            .map(|r| r.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let delta =
            StateDeltaV1::build(roots.state_root(ns).clone(), writes, pages, 8 << 20).unwrap();
        for n in &delta.new_nodes {
            Writer(pages)
                .put_node(
                    *blake3::Hash::from_hex(&n.digest).unwrap().as_bytes(),
                    memory
                        .reserve(n.bytes.len() + 128, MemoryClass::Ordinary)
                        .unwrap()
                        .retain(n.bytes.clone()),
                )
                .unwrap();
        }
        *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() = delta.after;
    }
    roots
}
fn context<'a>(
    memory: &'a MemoryBudget,
    pages: &'a Pages,
    roots: &'a RootSetV1,
    quota: &'a mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}

#[test]
fn collection_controls_full_qualification_and_general_delta_close_unchanged_ledger_members() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 16384);
    let roots = install(&memory, &pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let proof = qualify_world_collection_controls_v1(
        &roots,
        &mut context(&memory, &pages, &roots, &mut quota, &mut keys),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.group_member_counts(), [[1, 1]; 3]);
    let field = descriptor(0).unwrap();
    let group = crate::incremental_semantic_key::hash_semantic_key_v1(
        &" group/界 ",
        &memory,
        MemoryClass::Ordinary,
        &mut keys,
        || false,
    )
    .unwrap();
    let group =
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Group, Some(group.digest)).unwrap();
    let entry = crate::incremental_semantic_key::hash_semantic_key_v1(
        &(" group/界 ", " member/界 "),
        &memory,
        MemoryClass::Ordinary,
        &mut keys,
        || false,
    )
    .unwrap();
    let entry =
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Entry, Some(entry.digest)).unwrap();
    for (remove_group, remove_member, valid) in [
        (false, false, true),
        (true, false, false),
        (true, true, true),
        (false, true, true),
    ] {
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [1; 32],
            authority: AuthorityBinding {
                token: [2; 32],
                epoch: 3,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [3; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 16,
                max_writes: 16,
                max_facts: 8,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        if remove_group {
            tx.write(StateNamespaceV1::Authorization, group.as_bytes(), None)
                .unwrap();
        }
        if remove_member {
            tx.write(StateNamespaceV1::Authorization, entry.as_bytes(), None)
                .unwrap();
        }
        let guard = tx.guard();
        let prepared = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [3; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        let result = accept_world_collection_controls_transition_v1(
            &proof,
            &verifier,
            &prepared.record,
            &mut context(
                &memory,
                &pages,
                &prepared.record.after,
                &mut quota,
                &mut keys,
            ),
            &mut work(),
        );
        assert_eq!(
            result.is_ok(),
            valid,
            "group={remove_group} member={remove_member}"
        );
        if let Ok(after) = result {
            assert_eq!(
                after.group_member_counts(),
                [
                    [u64::from(!remove_group), u64::from(!remove_member)],
                    [1, 1],
                    [1, 1]
                ]
            );
        }
    }
}
#[test]
fn collection_controls_qualification_rejects_missing_required_metadata_and_cancelled_work() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 16384);
    let roots = RootSetV1::empty("missing-inventory".into(), [8; 32]).unwrap();
    assert!(
        qualify_world_collection_controls_v1(
            &roots,
            &mut context(
                &memory,
                &pages,
                &roots,
                &mut io(),
                &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20
                }
            ),
            &mut scan(),
            &mut work()
        )
        .is_err()
    );
    let roots = install(&memory, &pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = context(&memory, &pages, &roots, &mut quota, &mut keys);
    let mut limited = work();
    limited.index_steps_remaining = 0;
    assert!(matches!(
        qualify_world_collection_controls_v1(&roots, &mut ctx, &mut scan(), &mut limited),
        Err(ValueTreeError::ResourceLimited(_))
    ));
}

use std::sync::Arc;
struct Recovery<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
}
struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl ValueNodeReader for Recovery<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Recovery || !memory.same_pool(self.memory) {
            return Err(ValueTreeError::Invalid("phase recovery source"));
        }
        let bytes = ValueNodeReader::read_node(self.pages, id, max, memory, MemoryClass::Ordinary)?;
        let charge = memory.reserve(bytes.len() + 128, class)?;
        Ok(Arc::new(charge.retain(bytes.to_vec())))
    }
}
impl MerkleNodeReader for Recovery<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        ValueNodeReader::read_node(self, &id, max, self.memory, MemoryClass::Recovery)
            .map(|n| Some(Node(n)))
            .map_err(|e| e.to_string())
    }
}

#[test]
fn collection_controls_recovery_class_requires_finite_explicit_deep_readmission() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 16384);
    let roots = install(&memory, &pages);
    let recovery = Recovery {
        pages: &pages,
        memory: &memory,
    };
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let proof = qualify_world_collection_controls_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Authorization),
            state: &recovery,
            values: ValueReadContext {
                source: &recovery,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.allocation_class(), MemoryClass::Recovery);
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [1; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let tx = ExecutionTransaction::begin(
        parent.clone(),
        [3; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [3; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    assert!(
        accept_world_collection_controls_transition_v1(
            &proof,
            &verifier,
            &prepared.record,
            &mut context(
                &memory,
                &pages,
                &prepared.record.after,
                &mut quota,
                &mut keys
            ),
            &mut work()
        )
        .is_err()
    );
    let promoted =
        readmit_world_collection_controls_ordinary_v1(&proof, &roots, &memory, &mut work(), || {
            false
        })
        .unwrap();
    assert_eq!(promoted.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(promoted.group_member_counts(), proof.group_member_counts());
    accept_world_collection_controls_transition_v1(
        &promoted,
        &verifier,
        &prepared.record,
        &mut context(
            &memory,
            &pages,
            &prepared.record.after,
            &mut quota,
            &mut keys,
        ),
        &mut work(),
    )
    .unwrap();
    assert!(
        readmit_world_collection_controls_ordinary_v1(
            &promoted,
            &roots,
            &memory,
            &mut work(),
            || false
        )
        .is_err()
    );
    let foreign = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    assert!(
        readmit_world_collection_controls_ordinary_v1(
            &proof,
            &roots,
            &foreign,
            &mut work(),
            || false
        )
        .is_err()
    );
    let mut wrong = roots.clone();
    wrong.simulation_tick += 1;
    assert!(
        readmit_world_collection_controls_ordinary_v1(&proof, &wrong, &memory, &mut work(), || {
            false
        })
        .is_err()
    );
    let mut polls = 0;
    assert!(matches!(
        readmit_world_collection_controls_ordinary_v1(&proof, &roots, &memory, &mut work(), || {
            polls += 1;
            polls > 2
        }),
        Err(ValueTreeError::Cancelled)
    ));
    let mut finite = work();
    finite.index_steps_remaining = 0;
    assert!(matches!(
        readmit_world_collection_controls_ordinary_v1(&proof, &roots, &memory, &mut finite, || {
            false
        }),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    let held = memory
        .reserve((112 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        readmit_world_collection_controls_ordinary_v1(&proof, &roots, &memory, &mut work(), || {
            false
        }),
        Err(ValueTreeError::Memory(_))
    ));
    drop(held);
}

fn delta_ok(
    memory: &MemoryBudget,
    pages: &Pages,
    roots: &RootSetV1,
    proof: &ReservedValue<WorldCollectionControlsQualificationV1>,
    writes: &[(StateNamespaceV1, WorldFieldKeyV1, Option<[u8; 32]>)],
) -> bool {
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [1; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [3; 32],
        pages,
        memory,
        TransactionLimits {
            max_reads: 32,
            max_writes: 32,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    for (ns, key, value) in writes {
        tx.write(*ns, key.as_bytes(), *value).unwrap();
    }
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [3; 32],
        source: pages,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    accept_world_collection_controls_transition_v1(
        proof,
        &verifier,
        &prepared.record,
        &mut context(
            memory,
            pages,
            &prepared.record.after,
            &mut io(),
            &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
        ),
        &mut work(),
    )
    .is_ok()
}
#[test]
fn collection_controls_nonce_cursor_delta_and_required_singletons() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 16384);
    let roots = install(&memory, &pages);
    let proof = qualify_world_collection_controls_v1(
        &roots,
        &mut context(
            &memory,
            &pages,
            &roots,
            &mut io(),
            &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
        ),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.field_count(), 4);
    assert_eq!(proof.cursor_entries(), 1);
    let mut writer = Writer(&pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut ctx = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut quota,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 256,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let nonce = write_member_record_v1(
        descriptor(2).unwrap(),
        WorldFieldRecordKindV1::Entry,
        &(" group/界 ", " member/界 "),
        MemberPayloadRefV1::U64(0),
        &mut ctx,
    )
    .unwrap();
    let cursor_value = crate::state::ProductValidationDeliveryCursor {
        event_id_era: u64::MAX,
        routed_through_event_id: 7,
    };
    let cursor = write_member_record_v1(
        descriptor(3).unwrap(),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::DeliveryCursor(&cursor_value),
        &mut ctx,
    )
    .unwrap();
    assert!(delta_ok(
        &memory,
        &pages,
        &roots,
        &proof,
        &[
            (
                StateNamespaceV1::Authorization,
                nonce.field_key.clone(),
                Some(nonce.value_root)
            ),
            (
                StateNamespaceV1::Scheduler,
                cursor.field_key.clone(),
                Some(cursor.value_root)
            )
        ]
    ));
    assert!(!delta_ok(
        &memory,
        &pages,
        &roots,
        &proof,
        &[(StateNamespaceV1::Scheduler, cursor.field_key.clone(), None)]
    ));
    let malformed = write_member_record_v1(
        descriptor(3).unwrap(),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::Text("not sixteen cursor bytes"),
        &mut ctx,
    )
    .unwrap();
    assert!(!delta_ok(
        &memory,
        &pages,
        &roots,
        &proof,
        &[(
            StateNamespaceV1::Scheduler,
            malformed.field_key.clone(),
            Some(malformed.value_root)
        )]
    ));
    for i in 0..4 {
        let meta = WorldFieldKeyV1::new(
            descriptor(i).unwrap(),
            WorldFieldRecordKindV1::Metadata,
            None,
        )
        .unwrap();
        assert!(!delta_ok(
            &memory,
            &pages,
            &roots,
            &proof,
            &[(namespace(i), meta, None)]
        ));
    }
    // A typed descriptor in the wrong root is not an additional legitimate collection.
    assert!(!delta_ok(
        &memory,
        &pages,
        &roots,
        &proof,
        &[(
            StateNamespaceV1::Business,
            nonce.field_key.clone(),
            Some(nonce.value_root)
        )]
    ));
    let wrong_key = write_member_record_v1(
        descriptor(0).unwrap(),
        WorldFieldRecordKindV1::Entry,
        &(" group/界 ", 7u64),
        MemberPayloadRefV1::Unit,
        &mut ctx,
    )
    .unwrap();
    assert!(!delta_ok(
        &memory,
        &pages,
        &roots,
        &proof,
        &[(
            StateNamespaceV1::Authorization,
            wrong_key.field_key.clone(),
            Some(wrong_key.value_root)
        )]
    ));
}
#[test]
fn collection_controls_streamed_semantics_preserve_original_group_digest() {
    use crate::incremental_semantic_key::write_semantic_key_v1;
    use std::io::Write;
    for group in ["", " group/界 ", "_length"] {
        let member = "界".repeat(32768);
        let mut original_group = Vec::new();
        write_semantic_key_v1(&group, &mut original_group).unwrap();
        let expected = *blake3::hash(&original_group).as_bytes();
        let mut bytes = Vec::new();
        write_semantic_key_v1(&(group, member.as_str()), &mut bytes).unwrap();
        let mut probe = semantic::Probe::new(semantic::Shape::Member);
        for chunk in bytes.chunks(7) {
            probe.write_all(chunk).unwrap();
        }
        assert_eq!(probe.finish().unwrap(), expected);
        bytes.push(0);
        let mut probe = semantic::Probe::new(semantic::Shape::Member);
        assert!(probe.write_all(&bytes).is_err());
    }
    // Correct Text framing still must contain complete UTF-8, not just a matching hash.
    let mut bytes = SEMANTIC_KEY_DOMAIN.to_vec();
    bytes.push(1);
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.push(0xff);
    let mut probe = semantic::Probe::new(semantic::Shape::Group);
    assert!(probe.write_all(&bytes).is_err());
}
