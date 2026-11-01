use super::*;
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueTreeError};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::incremental_domain_event_proof::{
    DomainEventProofRequestV1, test_support::provenance, verify_domain_event_occurrence_v1,
};
use oasis7_world_state::{AgentCell, AgentState, DomainEvent, WorldEvent, WorldEventBody};

struct Discard;
impl ValueNodeWriter for Discard {
    fn put_node(&mut self, _: [u8; 32], _: ReservedValue<Vec<u8>>) -> Result<(), ValueTreeError> {
        Ok(())
    }
}
fn work() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 1_000_000,
        bytes_remaining: 1 << 20,
        max_depth: 64,
        max_map_keys: 32,
        max_key_bytes: 128,
    }
}
#[test]
fn verified_genesis_uses_original_domain_witness_and_rejects_same_length_substitution() {
    for mode in 0..5 {
        let wrong = mode == 1;
        let late_cancel = std::cell::Cell::new(false);
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let domain = DomainEvent::ActionAccepted {
            action_id: 23,
            action_kind: "raw".into(),
            actor_id: "actor".into(),
            eta_ticks: u64::MAX,
            notes: vec!["".into(), "same".into(), "same".into()],
        };
        let event = WorldEvent {
            id: 5,
            time: 7,
            caused_by: None,
            body: WorldEventBody::Domain(domain.clone()),
        };
        let raw = provenance(&event, &memory);
        let mut world = World::new();
        let mut cell = AgentCell::new(
            AgentState::new(
                "alice",
                oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
            ),
            3,
        );
        let mut stored = domain;
        if wrong && let DomainEvent::ActionAccepted { actor_id, .. } = &mut stored {
            *actor_id = "Actor".into();
        }
        cell.mailbox.push_back(stored);
        world.state.agents.insert("alice".into(), cell);
        let mut validation =
            DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || {
                mode == 2 && late_cancel.get()
            });
        let mut source_validation =
            DomainEventProofContextV1::new(&memory, MemoryClass::Ordinary, work(), || false);
        let mut calls = 0;
        let mut mailbox_emitted = 0;
        let mailbox_key =
            oasis7_world_state::incremental_event_fields::MailboxKeyV1::new("alice", Some(0))
                .unwrap();
        let result = world.write_iws_agent_genesis_fields_verified(
            "alice",
            &mut VerifiedAgentGenesisContextV1 {
                mutation: BodyMutationContext {
                    sink: &mut Discard,
                    memory: &memory,
                    io: &mut ValueIoBudget {
                        nodes_remaining: 1000,
                        read_bytes_remaining: 1 << 20,
                        write_bytes_remaining: 1 << 20,
                    },
                    cancelled: || mode == 3 && late_cancel.get(),
                },
                emit: |_: Ns, key: &[u8], _| {
                    if key == mailbox_key.as_bytes() {
                        mailbox_emitted += 1;
                        if mode == 2 || mode == 3 {
                            late_cancel.set(true);
                        }
                    }
                    Ok(())
                },
                mailbox_original: |ordinal| {
                    assert_eq!(ordinal, 0);
                    calls += 1;
                    if mode == 4 {
                        return Err(BodyMutationError::Invalid(
                            "injected original source failure",
                        ));
                    }
                    verify_domain_event_occurrence_v1(
                        &DomainEventProofRequestV1 {
                            event: &event,
                            occurrence: &raw,
                            expected_head_record: raw.head_record_id(),
                            expected_roots: raw.roots_id(),
                        },
                        &mut source_validation,
                    )
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))
                },
                validation: &mut validation,
                expected_head_record: raw.head_record_id(),
                expected_roots: raw.roots_id(),
                max_scalar_bytes: 1 << 20,
            },
        );
        assert_eq!(calls, 1);
        if mode != 0 {
            assert!(result.is_err());
            assert!(validation.is_poisoned());
            assert_eq!(mailbox_emitted, if mode == 2 || mode == 3 { 1 } else { 0 });
        } else {
            assert!(result.unwrap() > 20);
            assert!(!validation.is_poisoned());
            assert_eq!(mailbox_emitted, 1);
        }
    }
}
