use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1, incremental_semantic_key::SemanticKeyBudget,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
type PageMap = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
struct Pages {
    memory: MemoryBudget,
    nodes: RefCell<PageMap>,
    _metadata: MemoryReservation,
}
impl Pages {
    fn new(memory: &MemoryBudget) -> Self {
        Self {
            memory: memory.clone(),
            nodes: RefCell::new(BTreeMap::new()),
            _metadata: memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap(),
        }
    }
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(self.nodes.borrow().len() < 16_384);
        self.nodes.borrow_mut().insert(id, Arc::new(value));
        Ok(())
    }
}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let value = self
            .nodes
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("governance test missing page"))?;
        if !value.uses_budget(memory) || class != MemoryClass::Ordinary || value.len() > max {
            return Err(invalid());
        }
        Ok(value)
    }
}
struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl MerkleNodeReader for Pages {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        self.nodes
            .borrow()
            .get(&id)
            .map(|n| {
                if n.len() > max {
                    Err("control state bound".into())
                } else {
                    Ok(Node(n.clone()))
                }
            })
            .transpose()
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    }
}

impl StatePageWriter for Pages {}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("governance test")
}
fn context<'a>(
    pages: &'a Pages,
    roots: &'a RootSetV1,
    memory: &'a MemoryBudget,
    io: &'a mut ValueIoBudget,
    key_work: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError> + use<>,
    impl FnMut() -> bool + use<>,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work,
        semantic_bytes: 4096,
        record_bytes: 1 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100_000,
    }
}
#[test]
fn governance_records_recover_exact_original_fields_without_source_dto() {
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::new(&memory);
    let roots = RootSetV1::empty("governance-test".into(), [71; 32]).unwrap();
    let policy = GovernanceExecutionPolicy {
        emergency_brake_guardian_threshold: u16::MAX,
        epoch_length_ticks: 0,
        ..Default::default()
    };
    let snapshot = GovernanceFinalityEpochSnapshot {
        epoch_id: 9,
        threshold: u16::MAX,
        signer_node_ids: vec!["same".into(), "same".into(), String::new()],
        validator_stakes: BTreeMap::from([("count".into(), 0), (String::new(), u64::MAX)]),
        ..Default::default()
    };
    let penalty = GovernanceIdentityPenaltyRecord {
        penalty_id: 33,
        detection_risk_score: i64::MIN,
        appeal_evidence_hash: Some(String::new()),
        appellant: Some(" 原身份 ".into()),
        resolved_at_tick: Some(0),
        status: GovernanceIdentityPenaltyStatus::AppealRejected,
        identity_status_before: GovernanceIdentityStatus::Expelled,
        ..Default::default()
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut vi = io();
    let mut work = RuntimeGovernanceWorkV1 {
        cells_remaining: 1000,
        max_pending_node_bytes: 8 << 20,
    };
    let mut prepared = Vec::new();
    for record in [
        RuntimeGovernanceRecordV1::Policy(&policy),
        RuntimeGovernanceRecordV1::EpochSnapshot(&snapshot),
        RuntimeGovernanceRecordV1::IdentityPenalty(&penalty),
    ] {
        let kind = record.kind();
        let root = prepare_runtime_governance_record_v1(
            record,
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut vi,
                key_work: &mut keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 1 << 20,
                    record_bytes: 1 << 20,
                },
                cancelled: || false,
            },
            &mut work,
        )
        .unwrap();
        prepared.push((kind, root));
    }
    // Recovery receives only authenticated roots and pages; originals are comparison oracles.
    let mut vi = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut vi, &mut keys);
    for (kind, root) in &prepared {
        let recovered =
            recover_runtime_governance_record_v1(*kind, root, &mut ctx, &mut scan(), 2 << 20)
                .unwrap();
        match recovered.value() {
            RecoveredRuntimeGovernanceRecordV1::Policy(v) => assert_eq!(v, &policy),
            RecoveredRuntimeGovernanceRecordV1::EpochSnapshot(v) => assert_eq!(v, &snapshot),
            RecoveredRuntimeGovernanceRecordV1::IdentityPenalty(v) => assert_eq!(v, &penalty),
        }
        assert!(
            recover_runtime_governance_record_v1(*kind, root, &mut ctx, &mut scan(), 0).is_err()
        );
    }
}
#[test]
fn governance_genesis_family_keeps_map_key_separate_from_inner_id() {
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::new(&memory);
    let mut roots = RootSetV1::empty("governance-family".into(), [72; 32]).unwrap();
    let policy = GovernanceExecutionPolicy::default();
    let snapshots = BTreeMap::from([(
        101,
        GovernanceFinalityEpochSnapshot {
            epoch_id: 9,
            ..Default::default()
        },
    )]);
    let penalties = BTreeMap::from([(
        202,
        GovernanceIdentityPenaltyRecord {
            penalty_id: 33,
            ..Default::default()
        },
    )]);
    let mut rows = Vec::with_capacity(6);
    let mut vi = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    prepare_runtime_governance_genesis_v1(
        &RuntimeGovernanceControlsRefV1 {
            policy: &policy,
            snapshots: &snapshots,
            penalties: &penalties,
        },
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut vi,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 1 << 20,
            },
            cancelled: || false,
        },
        &mut RuntimeGovernanceWorkV1 {
            cells_remaining: 1000,
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
    let charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let delta = charge.retain(
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
    let mut vi = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut vi, &mut keys);
    let proof = qualify_runtime_governance_v1(&roots, &mut ctx, &mut scan(), 2 << 20).unwrap();
    assert_eq!(proof.record_counts(), [1, 1, 1]);
    let wrong = RootSetV1::empty("other-world".into(), [72; 32]).unwrap();
    assert!(proof.validate_binding(&wrong).is_err());
}

#[path = "incremental_runtime_governance_overlay_tests.rs"]
mod overlay;
