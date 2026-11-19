use super::*;
use crate::world::participant_tests::{fixture, policy, request};

fn participant() -> (World, oasis7_agent_api::game::GameParticipantReceiptV1) {
    let mut world = fixture();
    world
        .install_private_participant_fixture_policy(policy())
        .unwrap();
    let (owner, admission) = request("ordinary-account", "participant-registration");
    let receipt = world
        .apply_participant_registration(&owner, &admission, "main", 0)
        .unwrap();
    (world, receipt)
}

fn claim_event(
    world: &World,
    receipt: &oasis7_agent_api::game::GameParticipantReceiptV1,
) -> DomainEvent {
    DomainEvent::WorldRuleCommitted {
        action_id: 1,
        actor_id: receipt.agent_id.clone(),
        request_id: "ordinary-claim".into(),
        command: WorldRuleCommandV1::OrdinaryStarter {
            command: Box::new(OrdinaryStarterCommandV1::Claim {
                offer_id: "finite-offer".into(),
                offer_revision: 1,
                slot_id: "slot".into(),
                offer_digest: "digest".into(),
                owner_binding: receipt.owner_binding.clone(),
                owner_generation: receipt.owner_generation.0,
            }),
        },
        governance_epoch: 0,
        world_binding: world.world_rule_binding(),
    }
}

#[test]
fn ordinary_starter_runtime_requires_canonical_participant_without_starter_oc() {
    let (world, receipt) = participant();
    assert!(world.state.starter_oc_claims.is_empty());
    assert_eq!(
        world.ordinary_starter_owner(&receipt.agent_id).unwrap(),
        (receipt.owner_binding.clone(), 1)
    );
    assert!(
        world
            .validate_ordinary_starter_event(&claim_event(&world, &receipt))
            .is_ok()
    );
    let status = world.ordinary_starter_status(Some(&receipt.agent_id));
    assert_eq!(status.owner_binding, Some(receipt.owner_binding));
    assert!(!status.eligible);
    assert_eq!(
        status.blocker.as_deref(),
        Some("ordinary_starter_supply_unavailable")
    );
    assert!(world.ordinary_starter_owner("unregistered-agent").is_err());
    assert!(world.ordinary_starter_status(None).owner_binding.is_none());
}

#[test]
fn ordinary_starter_runtime_rejects_forged_owner_generation_and_foreign_genesis() {
    let (world, receipt) = participant();
    let original = claim_event(&world, &receipt);
    for case in ["owner", "generation", "world", "system"] {
        let mut event = original.clone();
        let DomainEvent::WorldRuleCommitted {
            world_binding,
            action_id,
            command: WorldRuleCommandV1::OrdinaryStarter { command },
            ..
        } = &mut event
        else {
            unreachable!()
        };
        let OrdinaryStarterCommandV1::Claim {
            owner_binding,
            owner_generation,
            ..
        } = command.as_mut()
        else {
            unreachable!()
        };
        match case {
            "owner" => *owner_binding = "forged-owner".into(),
            "generation" => *owner_generation = 2,
            "world" => *world_binding = Some("other-genesis".into()),
            "system" => *action_id = 0,
            _ => unreachable!(),
        }
        assert!(
            world.validate_ordinary_starter_event(&event).is_err(),
            "{case}"
        );
        assert!(
            world
                .prepare_world_rule_event(&event, world.state.time)
                .is_err(),
            "{case}"
        );
    }
}

#[test]
fn ordinary_starter_runtime_changed_identity_cannot_read_or_reuse_participant_eligibility() {
    let (mut world, receipt) = participant();
    let event = claim_event(&world, &receipt);
    world
        .install_capability_agent_identity(&receipt.agent_id, &receipt.owner_binding, 2)
        .unwrap();
    assert!(world.validate_ordinary_starter_event(&event).is_err());
    let status = world.ordinary_starter_status(Some(&receipt.agent_id));
    assert!(!status.eligible);
    assert!(status.owner_binding.is_none());
    assert!(status.claim.is_none());
}
