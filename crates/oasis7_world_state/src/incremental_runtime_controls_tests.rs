use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1, incremental_semantic_key::SemanticKeyBudget,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
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
            .ok_or(ValueTreeError::Invalid("control test missing page"))?;
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
fn seed(memory: &MemoryBudget, authority: &str, brake: Option<u64>) -> (Pages, RootSetV1) {
    let mut pages = Pages::new(memory);
    let _rows = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows = Vec::with_capacity(6);
    let mut quota = 6;
    prepare_runtime_controls_genesis_v1(
        &RuntimeControlsRefV1 {
            tick_consensus_authority_source: authority,
            next_governance_identity_penalty_id: u64::MAX,
            governance_emergency_brake_until_tick: brake,
        },
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut quota,
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(quota, 0);
    assert_eq!(rows.len(), 6);
    let mut roots = RootSetV1::empty("runtime-control-test".into(), [29; 32]).unwrap();
    for namespace in [StateNamespaceV1::Allocator, StateNamespaceV1::Authorization] {
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == namespace)
            .map(|r| r.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let charge = memory
            .reserve((writes.len() + 1) * (512 << 10), MemoryClass::Ordinary)
            .unwrap();
        let delta = charge.retain(
            StateDeltaV1::build(
                StateRootV1::empty(namespace).unwrap(),
                writes,
                &pages,
                8 << 20,
            )
            .unwrap(),
        );
        for node in &delta.new_nodes {
            let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
            let charge = memory
                .reserve(node.bytes.len(), MemoryClass::Ordinary)
                .unwrap();
            pages
                .put_node(id, charge.retain(node.bytes.clone()))
                .unwrap();
        }
        *roots
            .state
            .iter_mut()
            .find(|r| r.namespace == namespace)
            .unwrap() = delta.after.clone();
    }
    (pages, roots)
}
#[test]
fn real_genesis_root_qualification_preserves_control_values_and_option_distinctions() {
    for (authority, brake) in [
        ("", None),
        ("原 authority", Some(0)),
        (" 界\0 authority ", Some(u64::MAX)),
    ] {
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let (pages, roots) = seed(&memory, authority, brake);
        let mut reads = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let qualified = qualify_runtime_controls_v1(
            &roots,
            &mut ctx,
            &mut StateScanBudget {
                nodes_remaining: 100_000,
                leaves_remaining: 1000,
                read_bytes_remaining: 64 << 20,
            },
            1 << 20,
        )
        .unwrap();
        qualified.validate_binding(&roots).unwrap();
        let mut other = roots.clone();
        other.height = 1;
        assert!(qualified.validate_binding(&other).is_err());
        for field in RuntimeControlFieldV1::ALL {
            ctx.root = roots.state_root(field.namespace());
            let _output = memory.reserve(1024, MemoryClass::Ordinary).unwrap();
            let mut bytes = Vec::with_capacity(1024);
            let value = read_runtime_control_v1(field, &mut ctx, 1024, &mut bytes).unwrap();
            let expected = match field {
                RuntimeControlFieldV1::NextGovernanceIdentityPenaltyId => {
                    RuntimeControlReadV1::Unsigned(u64::MAX)
                }
                RuntimeControlFieldV1::TickConsensusAuthoritySource => RuntimeControlReadV1::Text {
                    utf8_bytes: authority.len() as u64,
                },
                RuntimeControlFieldV1::GovernanceEmergencyBrakeUntilTick => {
                    brake.map_or(RuntimeControlReadV1::Unit, RuntimeControlReadV1::Unsigned)
                }
            };
            assert_eq!(value, expected);
            if field == RuntimeControlFieldV1::TickConsensusAuthoritySource {
                assert_eq!(bytes, authority.as_bytes());
            }
        }
    }
}

struct Combined<'a> {
    base: &'a Pages,
    extra: &'a RefCell<Pages>,
}
impl ValueNodeReader for Combined<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if self.extra.borrow().nodes.borrow().contains_key(id) {
            ValueNodeReader::read_node(&*self.extra.borrow(), id, max, memory, class)
        } else {
            ValueNodeReader::read_node(self.base, id, max, memory, class)
        }
    }
}
impl MerkleNodeReader for Combined<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.base.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        if let Some(value) = MerkleNodeReader::read_node(&*self.extra.borrow(), id, max)? {
            Ok(Some(value))
        } else {
            MerkleNodeReader::read_node(self.base, id, max)
        }
    }
}
struct Sink<'a>(&'a RefCell<Pages>);
impl ValueNodeWriter for Sink<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.borrow_mut().put_node(id, bytes)
    }
}
#[test]
fn same_transaction_controls_coalesce_and_pass_independent_replay_and_raw_qualification() {
    use crate::incremental_field_overlay::WorldFieldOverlayContextV1;
    use RuntimeControlFieldV1 as F;
    use RuntimeControlValueRefV1 as V;
    use oasis7_formal_protocol::incremental_execution::*;
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (pages, roots) = seed(&memory, "original authority", None);
    let extra = RefCell::new(Pages::new(&memory));
    let values = Combined {
        base: &pages,
        extra: &extra,
    };
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut root_read = WorldFieldReadContextV1 {
        root: &roots.state[0],
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let qualification = qualify_runtime_controls_v1(
        &roots,
        &mut root_read,
        &mut StateScanBudget {
            nodes_remaining: 100_000,
            leaves_remaining: 1000,
            read_bytes_remaining: 64 << 20,
        },
        1024,
    )
    .unwrap();
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [6; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let mut reads = io();
    let mut writes = io();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut sink = Sink(&extra);
    {
        let mut read = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut read_keys,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || false,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut sink,
            memory: &memory,
            io: &mut writes,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        };
        macro_rules! change {
            ($field:expr,$expected:expr,$after:expr) => {{
                let guard = read.transaction.guard();
                change_runtime_control_overlay_v1(
                    RuntimeControlChangeV1 {
                        field: $field,
                        expected: $expected,
                        after: $after,
                        max_text_bytes: 1024,
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live,
                )
                .unwrap()
            }};
        }
        assert!(change!(
            F::NextGovernanceIdentityPenaltyId,
            V::Unsigned(u64::MAX),
            V::Unsigned(0)
        ));
        assert!(change!(
            F::NextGovernanceIdentityPenaltyId,
            V::Unsigned(0),
            V::Unsigned(7)
        ));
        assert!(!change!(
            F::NextGovernanceIdentityPenaltyId,
            V::Unsigned(7),
            V::Unsigned(7)
        ));
        assert!(change!(
            F::TickConsensusAuthoritySource,
            V::Text("original authority"),
            V::Text("")
        ));
        assert!(change!(
            F::TickConsensusAuthoritySource,
            V::Text(""),
            V::Text(" 界\0 cursor ")
        ));
        assert!(!change!(
            F::TickConsensusAuthoritySource,
            V::Text(" 界\0 cursor "),
            V::Text(" 界\0 cursor ")
        ));
        assert!(change!(
            F::GovernanceEmergencyBrakeUntilTick,
            V::Absent,
            V::Unsigned(0)
        ));
        assert!(change!(
            F::GovernanceEmergencyBrakeUntilTick,
            V::Unsigned(0),
            V::Unsigned(u64::MAX)
        ));
        let _output = memory.reserve(1024, MemoryClass::Ordinary).unwrap();
        let mut bytes = Vec::with_capacity(1024);
        assert_eq!(
            read_runtime_control_overlay_v1(
                F::TickConsensusAuthoritySource,
                &mut read,
                1024,
                &mut bytes
            )
            .unwrap(),
            RuntimeControlReadV1::Text { utf8_bytes: 13 }
        );
        assert_eq!(bytes, " 界\0 cursor ".as_bytes());
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(sealed.record.writes.len(), 3);
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let verified = verifier.verify(&sealed.record).unwrap();
    let after = &verified.record.after;
    let mut reads = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: &after.state[0],
        state: &pages,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut reads,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    accept_runtime_controls_transition_v1(
        &qualification,
        &verifier,
        &sealed.record,
        &mut read,
        1024,
    )
    .unwrap()
    .validate_binding(after)
    .unwrap();
    for node in &verified.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        extra
            .borrow_mut()
            .put_node(id, charge.retain(node.bytes.clone()))
            .unwrap();
    }
    let mut reads = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: &after.state[0],
        state: &values,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut reads,
        },
        key_work: &mut keys,
        semantic_bytes: 1024,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    qualify_runtime_controls_v1(
        after,
        &mut read,
        &mut StateScanBudget {
            nodes_remaining: 100_000,
            leaves_remaining: 1000,
            read_bytes_remaining: 64 << 20,
        },
        1024,
    )
    .unwrap();
    read.root = after.state_root(F::NextGovernanceIdentityPenaltyId.namespace());
    assert_eq!(
        read_runtime_control_v1(
            F::NextGovernanceIdentityPenaltyId,
            &mut read,
            1024,
            &mut std::io::sink()
        )
        .unwrap(),
        RuntimeControlReadV1::Unsigned(7)
    );
    read.root = after.state_root(F::GovernanceEmergencyBrakeUntilTick.namespace());
    assert_eq!(
        read_runtime_control_v1(
            F::GovernanceEmergencyBrakeUntilTick,
            &mut read,
            1024,
            &mut std::io::sink()
        )
        .unwrap(),
        RuntimeControlReadV1::Unsigned(u64::MAX)
    );
}

#[path = "incremental_runtime_controls_pages_tests.rs"]
mod page_tests;

#[path = "incremental_runtime_controls_fault_tests.rs"]
mod fault_tests;

#[path = "incremental_runtime_controls_malformed_tests.rs"]
mod malformed_tests;

#[path = "incremental_runtime_controls_class_tests.rs"]
mod class_tests;
