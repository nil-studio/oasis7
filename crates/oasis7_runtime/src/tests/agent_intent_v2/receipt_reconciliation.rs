use super::*;
use serde_json::json;
#[test]
fn public_receipt_reconciliation_selects_own_committed_effect_and_rejects_forged_identity() {
    let mut state = WorldState::default();
    state.agents.insert(AGENT_ID.into(), legacy_agent_cell());
    let mut world = World::new_with_state(state);
    let snapshot = world.snapshot();
    world
        .record_agent_chat_intent_with_authority(
            "player",
            AGENT_ID,
            1,
            "complete only my effect",
            AgentIntentAuthorityContext {
                intent_tick: Some(7),
                world_id: Some("runtime-world".into()),
                reorg_epoch: Some(2),
                authority_scope: Some("player_agent_chat".into()),
                replaces_intent_id: None,
            },
        )
        .unwrap();
    let mut journal = world.journal().clone();
    for event in &mut journal.events {
        match &mut event.body {
            WorldEventBody::Domain(DomainEvent::AgentIntentProposed { intent })
            | WorldEventBody::Domain(DomainEvent::AgentIntentSubmitted { intent })
            | WorldEventBody::Domain(DomainEvent::AgentIntentAccepted { intent }) => {
                intent.effect_intent_id = Some("own-effect".into())
            }
            _ => {}
        }
    }
    for (id, effect) in [(4, "own-effect"), (5, "foreign-effect")] {
        journal.events.push(WorldEvent {
            id,
            time: 7,
            caused_by: None,
            body: WorldEventBody::EffectQueued(EffectIntent {
                birth_era: None,
                intent_id: effect.into(),
                kind: "test_effect".into(),
                params: json!({}),
                cap_ref: "test".into(),
                origin: EffectOrigin::System,
            }),
        });
    }
    let receipt = |id, effect: &str| WorldEvent {
        id,
        time: 7,
        caused_by: None,
        body: WorldEventBody::ReceiptAppended(EffectReceipt {
            birth_era: None,
            world: None,
            intent_payload_digest: None,
            intent_id: effect.into(),
            status: "ok".into(),
            payload: json!({}),
            cost_cents: None,
            signature: None,
        }),
    };
    journal.events.push(receipt(6, "foreign-effect"));
    let mut restored = World::from_snapshot(snapshot.clone(), journal.clone()).unwrap();
    let intent = restored.state().agents[AGENT_ID]
        .intent
        .as_ref()
        .unwrap()
        .clone();
    let identity = AgentIntentReference {
        agent_id: AGENT_ID,
        intent_id: &intent.intent_id,
        request_digest: &intent.request_digest,
    };
    let before = serde_json::to_vec(&restored).unwrap();
    assert!(
        restored
            .reconcile_agent_intent_receipt(identity)
            .unwrap()
            .is_none()
    );
    assert_eq!(serde_json::to_vec(&restored).unwrap(), before);
    journal.events.push(receipt(7, "own-effect"));
    let mut restored = World::from_snapshot(snapshot, journal).unwrap();
    assert!(
        restored
            .reconcile_agent_intent_receipt(AgentIntentReference {
                request_digest: "forged",
                ..identity
            })
            .unwrap()
            .is_none()
    );
    let completed = restored
        .reconcile_agent_intent_receipt(identity)
        .unwrap()
        .unwrap();
    assert_eq!(completed.status, "completed");
    assert_eq!(completed.receipt_ref.as_deref(), Some("world-event:7"));
}
