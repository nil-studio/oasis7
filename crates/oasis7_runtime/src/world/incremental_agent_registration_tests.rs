use super::*;
use oasis7_formal_protocol::incremental_execution::{AuthorityBinding, RootSetV1};
use oasis7_formal_protocol::incremental_state::StateScanBudget;
use oasis7_world_state::incremental_agent_subtree::{
    AgentSubtreeScanWorkV1, qualify_agent_subtree_structure_v1,
};
use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1;
use oasis7_world_state::incremental_field_read::WorldFieldReadContextV1;
use oasis7_world_state::incremental_semantic_key::SemanticKeyBudget;

struct ChargedNode(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for ChargedNode {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
struct ChargedState<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
}
impl MerkleNodeReader for ChargedState<'_> {
    type Node = ChargedNode;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let Some(bytes) = self.pages.0.get(id) else {
            return Ok(None);
        };
        if bytes.len() > max {
            return Err("fixture state bound".into());
        }
        let charge = self
            .memory
            .reserve(bytes.len(), MemoryClass::Ordinary)
            .map_err(|e| e.to_string())?;
        Ok(Some(ChargedNode(charge.retain(bytes.clone()))))
    }
}

#[test]
fn registration_is_one_transaction_and_failed_original_poisoned() {
    for case in 0..8 {
        let fail_original = case == 1;
        let expect_failure = case != 0;
        let memory = MemoryBudget::new(192 << 20, 8 << 20).unwrap();
        let mut state = Pages::default();
        let mut values = Values::default();
        let mut parent = ExecutionParent {
            commit: [1; 32],
            roots: RootSetV1::empty("registration".into(), [2; 32]).unwrap(),
            authority: AuthorityBinding {
                token: [3; 32],
                epoch: 0,
            },
        };
        // Seed only the original allocator, leaving the new agent genuinely absent.
        let sequence = write_field_bytes(
            &EventSequenceV1 { next: 1, era: 0 }.canonical_bytes(),
            &mut BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
        )
        .unwrap();
        let index = Ns::Allocator as usize - 1;
        let delta = StateDeltaV1::build(
            parent.roots.state[index].clone(),
            vec![StateWriteV1 {
                key: EVENT_SEQUENCE_KEY_V1.to_vec(),
                before: None,
                after: Some(sequence),
            }],
            &state,
            4 << 20,
        )
        .unwrap();
        parent.roots.state[index] = delta.after;
        for page in delta.new_nodes {
            state.0.insert(page.digest, page.bytes);
        }
        let mut clock_world = World::new();
        clock_world.state.time = 11;
        let mut clock_rows = Vec::new();
        clock_world
            .write_iws_clock_genesis(
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
                |key, id| {
                    clock_rows.push(StateWriteV1 {
                        key: key.to_vec(),
                        before: None,
                        after: Some(id),
                    });
                    Ok(())
                },
            )
            .unwrap();
        let delta = StateDeltaV1::build(parent.roots.state[1].clone(), clock_rows, &state, 4 << 20)
            .unwrap();
        parent.roots.state[1] = delta.after;
        for page in delta.new_nodes {
            state.0.insert(page.digest, page.bytes);
        }
        // Qualification must match the exact parent, including the allocator root.
        let charged = ChargedState {
            pages: &state,
            memory: &memory,
        };
        let proof = qualify_agent_subtree_structure_v1(
            &parent.roots,
            &mut WorldFieldReadContextV1 {
                root: &parent.roots.state[0],
                state: &charged,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 1 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
            &mut StateScanBudget {
                nodes_remaining: 10000,
                leaves_remaining: 10000,
                read_bytes_remaining: 64 << 20,
            },
            &mut AgentSubtreeScanWorkV1 {
                max_agents: 2,
                cells_remaining: 10000,
                max_scalar_bytes: 1 << 20,
            },
            &mut |_| Err(ValueTreeError::Invalid("unexpected original")),
        )
        .unwrap();
        if case == 7 {
            parent.roots.height = 1;
        }
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent,
            [9; 32],
            &charged,
            &memory,
            oasis7_formal_protocol::incremental_execution::TransactionLimits {
                max_reads: 256,
                max_writes: 256,
                ..limits()
            },
        )
        .unwrap();
        if case == 6 {
            use oasis7_world_state::incremental_agent_fields::{AgentFieldKeyV1, AgentFieldV1};
            let key = AgentFieldKeyV1::new("new", AgentFieldV1::ResourceAmount, 0).unwrap();
            tx.write(Ns::Business, key.as_bytes(), Some(sequence))
                .unwrap();
        }
        let guard = tx.guard();
        let mut passed_live = live;
        if case == 5 {
            passed_live.authority.epoch += 1;
        }
        let event = WorldEvent {
            id: 1,
            time: 11,
            caused_by: Some(CausedBy::Action(1)),
            body: WorldEventBody::Domain(DomainEvent::AgentRegistered {
                agent_id: if case == 3 {
                    "industrial:forbidden".into()
                } else {
                    "new".into()
                },
                pos: oasis7_agent_api::world_action::GeoPos::new(1, 2, 3),
                registration_contract_version: if case == 2 { 2 } else { 1 },
            }),
        };
        let mut sink = values.clone();
        let mut write_io = io();
        write_io.nodes_remaining = 10000;
        write_io.write_bytes_remaining = 16 << 20;
        let result = World::contribute_iws_agent_registration_at(
            11,
            &event,
            &proof,
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 1 << 20,
                take_state_error: || None,
                cancelled: || case == 4,
            },
            &mut BodyMutationContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut write_io,
                cancelled: || case == 4,
            },
            &guard,
            &passed_live,
            &mut original_work(),
            1 << 20,
            |original| {
                if fail_original {
                    return Err(BodyMutationError::Invalid("original failure"));
                }
                let _ = collect_original(original, &memory)?;
                Ok(())
            },
        );
        if expect_failure {
            assert!(result.is_err(), "case {case}");
            let guard = tx.guard();
            assert!(tx.seal(&guard, &live, 12).is_err());
        } else {
            result.unwrap();
            let moved = WorldEvent {
                id: 2,
                time: 11,
                caused_by: None,
                body: WorldEventBody::Domain(DomainEvent::AgentMoved {
                    agent_id: "new".into(),
                    from: oasis7_agent_api::world_action::GeoPos::new(1, 2, 3),
                    to: oasis7_agent_api::world_action::GeoPos::new(4, 5, 6),
                }),
            };
            World::contribute_iws_agent_event_at(
                11,
                &moved,
                &mut tx,
                &mut AgentEventContext {
                    values: &values,
                    mutation: BodyMutationContext {
                        sink: &mut sink,
                        memory: &memory,
                        io: &mut write_io,
                        cancelled: || false,
                    },
                    persist_original: |original: &mut dyn AgentEventOriginalV1| {
                        let _ = collect_original(original, &memory)?;
                        Ok(())
                    },
                    max_event_bytes: 1 << 20,
                    original_work: original_work(),
                },
            )
            .unwrap();
            let guard = tx.guard();
            let sealed = tx.seal(&guard, &live, 12).unwrap();
            assert_eq!(sealed.record.facts.len(), 2);
        }
    }
}
