use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayContextV1;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits, TransitionVerifier,
};
struct Writer<'a>(&'a Pages);
impl ValueNodeWriter for Writer<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(self.0.nodes.borrow().len() < 32768);
        self.0.nodes.borrow_mut().insert(id, Arc::new(value));
        Ok(())
    }
}
impl ValueNodeReader for Writer<'_> {
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
impl StatePageWriter for Writer<'_> {}
struct Fixture {
    memory: MemoryBudget,
    pages: Pages,
    roots: RootSetV1,
    _source: MemoryReservation,
    snapshots: BTreeMap<u64, GovernanceFinalityEpochSnapshot>,
    penalties: BTreeMap<u64, GovernanceIdentityPenaltyRecord>,
}
fn seed(large: bool, empty: bool) -> Fixture {
    let memory = MemoryBudget::new(256 << 20, 4 << 20).unwrap();
    let source_charge = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::new(&memory);
    let mut roots = RootSetV1::empty("governance-delta".into(), [83; 32]).unwrap();
    let policy = GovernanceExecutionPolicy::default();
    let snapshots = if empty {
        BTreeMap::new()
    } else {
        BTreeMap::from([(
            101,
            GovernanceFinalityEpochSnapshot {
                epoch_id: 9,
                threshold: 1,
                signer_node_ids: if large {
                    vec![
                        "a".repeat(256 << 10),
                        "b".repeat(256 << 10),
                        "c".repeat(256 << 10),
                    ]
                } else {
                    vec!["first".into(), "second".into()]
                },
                validator_stakes: BTreeMap::from([("count".into(), 1), (String::new(), u64::MAX)]),
                ..Default::default()
            },
        )])
    };
    let penalties = if empty {
        BTreeMap::new()
    } else {
        BTreeMap::from([(
            202,
            GovernanceIdentityPenaltyRecord {
                penalty_id: 33,
                reason: if large {
                    "r".repeat(512 << 10)
                } else {
                    "reason".into()
                },
                status: GovernanceIdentityPenaltyStatus::Applied,
                appeal_evidence_hash: None,
                ..Default::default()
            },
        )])
    };
    let _rows = memory.reserve(65536, MemoryClass::Ordinary).unwrap();
    let mut rows = Vec::with_capacity(6);
    prepare_runtime_governance_genesis_v1(
        &RuntimeGovernanceControlsRefV1 {
            policy: &policy,
            snapshots: &snapshots,
            penalties: &penalties,
        },
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 1 << 20,
            },
            cancelled: || false,
        },
        &mut RuntimeGovernanceWorkV1 {
            cells_remaining: 2000,
            max_pending_node_bytes: 8 << 20,
        },
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let mut writes: Vec<_> = rows.iter().map(|r| r.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta_charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let delta = delta_charge.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
            writes,
            &pages,
            8 << 20,
        )
        .unwrap(),
    );
    pages.put_state_pages(&delta, &memory).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Authorization)
        .unwrap() = delta.after.clone();
    drop(delta);
    Fixture {
        memory,
        pages,
        roots,
        _source: source_charge,
        snapshots,
        penalties,
    }
}
fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots: roots.clone(),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 64,
        max_writes: 16,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    }
}
fn work() -> RuntimeGovernanceWorkV1 {
    RuntimeGovernanceWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 8 << 20,
    }
}
#[test]
fn governance_scalar_batches_and_changed_receiver_do_not_hydrate_large_siblings() {
    let fixture = seed(true, false);
    let Fixture {
        memory,
        pages,
        roots,
        ..
    } = &fixture;
    let mut cold_io = io();
    let mut cold_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut cold = context(pages, roots, memory, &mut cold_io, &mut cold_keys);
    let family = qualify_runtime_governance_v1(roots, &mut cold, &mut scan(), 2 << 20).unwrap();
    let penalty = open_runtime_governance_record_v1(
        &family,
        roots,
        RuntimeGovernanceKeyV1::Penalty(202),
        &mut cold,
    )
    .unwrap()
    .unwrap();
    let snapshot = open_runtime_governance_record_v1(
        &family,
        roots,
        RuntimeGovernanceKeyV1::Epoch(101),
        &mut cold,
    )
    .unwrap()
    .unwrap();
    let parent = parent(roots);
    let live = parent.fence().unwrap();
    let mut transaction =
        ExecutionTransaction::begin(parent.clone(), [6; 32], pages, memory, limits()).unwrap();
    let mut read_io = io();
    let before = read_io.read_bytes_remaining;
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write_io = io();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut writer = Writer(pages);
    let mut output = MemberRecordWriteContext {
        sink: &mut writer,
        memory,
        io: &mut write_io,
        key_work: &mut write_keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 1 << 20,
            record_bytes: 1 << 20,
        },
        cancelled: || false,
    };
    let guard = transaction.guard();
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut transaction,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut read_keys,
        semantic_bytes: 4096,
        record_bytes: 1 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let changed = change_runtime_governance_columns_v1(
        &family,
        roots,
        &penalty,
        &[RuntimeGovernanceColumnChangeV1 {
            column: RuntimeGovernanceColumnV1::Penalty(PenaltyColumnV1::Status),
            expected: Some(RuntimeGovernanceValueV1::Unsigned(0)),
            after: Some(RuntimeGovernanceValueV1::Unsigned(1)),
        }],
        &mut overlay,
        &mut output,
        &mut work(),
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    let guard = overlay.transaction.guard();
    assert!(
        change_runtime_governance_columns_v1(
            &family,
            roots,
            &changed,
            &[RuntimeGovernanceColumnChangeV1 {
                column: RuntimeGovernanceColumnV1::Penalty(PenaltyColumnV1::Status),
                expected: Some(RuntimeGovernanceValueV1::Unsigned(1)),
                after: Some(RuntimeGovernanceValueV1::Unsigned(1))
            }],
            &mut overlay,
            &mut output,
            &mut work(),
            &guard,
            &live
        )
        .unwrap()
        .is_none()
    );
    let guard = overlay.transaction.guard();
    change_runtime_governance_columns_v1(
        &family,
        roots,
        &snapshot,
        &[RuntimeGovernanceColumnChangeV1 {
            column: RuntimeGovernanceColumnV1::Epoch(EpochColumnV1::Threshold),
            expected: Some(RuntimeGovernanceValueV1::Unsigned(1)),
            after: Some(RuntimeGovernanceValueV1::Unsigned(2)),
        }],
        &mut overlay,
        &mut output,
        &mut work(),
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    let used = before - overlay.values.io.read_bytes_remaining;
    assert!(
        used < 768 << 10,
        "changed scalar paths read unchanged payloads: {used}"
    );
    let guard = transaction.guard();
    let transition = transaction.seal(&guard, &live, 1).unwrap();
    assert_eq!(transition.record.writes.len(), 2);
    for node in &transition.new_nodes {
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        ValueNodeWriter::put_node(
            &mut Writer(pages),
            *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
            charge.retain(node.bytes.clone()),
        )
        .unwrap();
    }
    let mut receive_io = io();
    let before = receive_io.read_bytes_remaining;
    let mut receive_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut receive = context(
        pages,
        &transition.record.after,
        memory,
        &mut receive_io,
        &mut receive_keys,
    );
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: pages,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let accepted = accept_runtime_governance_transition_v1(
        &family,
        &verifier,
        &transition.record,
        &mut receive,
        &mut scan(),
        &mut work(),
        2 << 20,
    )
    .unwrap();
    assert_eq!(accepted.record_counts(), [1, 1, 1]);
    let used = before - receive.values.io.read_bytes_remaining;
    assert!(
        used < 512 << 10,
        "receiver hydrated unchanged graph: {used}"
    );
    let penalty_body = recover_runtime_governance_record_v1(
        RuntimeGovernanceKindV1::IdentityPenalty,
        changed.root(),
        &mut receive,
        &mut scan(),
        2 << 20,
    )
    .unwrap();
    let RecoveredRuntimeGovernanceRecordV1::IdentityPenalty(penalty) = penalty_body.value() else {
        panic!("wrong variant")
    };
    assert_eq!(penalty.status, GovernanceIdentityPenaltyStatus::Appealed);
    assert_eq!(penalty.reason, fixture.penalties[&202].reason);
    assert_eq!(fixture.snapshots[&101].epoch_id, 9);
}

#[path = "incremental_runtime_governance_overlay_failures.rs"]
mod failures;

#[path = "incremental_runtime_governance_guards_tests.rs"]
mod guards;
