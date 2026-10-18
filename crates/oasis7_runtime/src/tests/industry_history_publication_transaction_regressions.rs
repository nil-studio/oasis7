use super::pos;
use crate::*;
use oasis7_wasm_abi::MaterialStack;

fn register(world: &mut World, id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: id.into(),
        pos: pos(0, 0),
    });
    world.step().unwrap();
}

fn anchored_world() -> World {
    let mut world = World::new();
    register(&mut world, "builder");
    let mut state = world.state().clone();
    state.location_anchors.insert(
        "location".into(),
        LocationAnchorV1 {
            location_id: "location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        },
    );
    World::new_with_state(state)
}

fn bodies() -> Vec<(&'static str, World, WorldEventBody)> {
    let anchor = DomainEvent::LocationAnchorUpdated {
        anchor: LocationAnchorV1 {
            location_id: "location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        },
    };
    let authority = DomainEvent::AgentLocationAuthorityUpdated {
        authority: AgentLocationAuthorityV1 {
            agent_id: "builder".into(),
            location_id: "location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        },
    };
    let site = DomainEvent::FactorySiteAuthorityUpdated {
        authority: FactorySiteAuthorityV1 {
            site_id: "site".into(),
            location_id: "location".into(),
            owner_agent_id: "builder".into(),
            authorized_agent_ids: vec!["builder".into()],
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 0,
        },
    };
    let power = DomainEvent::FactoryConstructionPowerProfileUpdated {
        profile: FactoryConstructionPowerProfileV1 {
            factory_id: "factory".into(),
            factory_kind: "assembler".into(),
            source_module_id: Some("m4.power".into()),
            electricity_amount: 3,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        },
    };
    let receipt = ProductValidationReceiptV1 {
        job_id: 41,
        validation_index: Some(0),
        requester_agent_id: "builder".into(),
        module_id: "m4.product".into(),
        stack: MaterialStack::new("widget", 1),
        decision: ProductValidationDecision::accepted("widget", 8, true, vec!["q1".into()]),
        failure_detail: None,
    };
    let attempt = ProductValidationAttemptV1 {
        job_id: 42,
        validation_index: Some(0),
        requester_agent_id: "builder".into(),
        module_id: "m4.product".into(),
        stack: MaterialStack::new("widget", 1),
    };
    let mut validation_world = World::new();
    register(&mut validation_world, "builder");
    vec![
        ("anchor", World::new(), WorldEventBody::Domain(anchor)),
        (
            "agent authority",
            anchored_world(),
            WorldEventBody::Domain(authority),
        ),
        (
            "site authority",
            anchored_world(),
            WorldEventBody::Domain(site),
        ),
        ("power profile", World::new(), WorldEventBody::Domain(power)),
        (
            "validation receipt",
            validation_world.clone(),
            WorldEventBody::Domain(DomainEvent::ProductValidationRecorded { receipt }),
        ),
        (
            "validation attempt",
            validation_world,
            WorldEventBody::Domain(DomainEvent::ProductValidationAttemptStarted { attempt }),
        ),
        (
            "delivery cursor",
            World::new(),
            WorldEventBody::ProductValidationDeliveryCursorUpdated(
                ProductValidationDeliveryCursor {
                    routed_through_event_id: 1,
                    event_id_era: 0,
                },
            ),
        ),
    ]
}

#[test]
fn industry_history_events_honor_postprepare_failure_and_same_world_retry() {
    let mut unprepared = Vec::new();
    for (index, (name, mut world, body)) in bodies().into_iter().enumerate() {
        let snapshot = world.snapshot();
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        world.fail_next_append_after_publication_prepare_for_test();
        match world
            .append_event_for_test(body.clone(), Some(CausedBy::Action(9_000 + index as u64)))
        {
            Err(error) if format!("{error:?}").contains("publication preparation") => {
                assert_eq!(world.snapshot(), before.snapshot());
                assert_eq!(world.journal(), before.journal());
                assert_eq!(
                    world.runtime_backpressure_stats(),
                    before.runtime_backpressure_stats()
                );
                assert_eq!(
                    world.tick_consensus_records(),
                    before.tick_consensus_records()
                );
                assert_eq!(world.current_state_root_hash().unwrap(), root);
                let cause = Some(CausedBy::Action(9_100 + index as u64));
                world.append_event_for_test(body, cause.clone()).unwrap();
                assert_eq!(world.journal().events.last().unwrap().caused_by, cause);
                assert_eq!(
                    world
                        .tick_consensus_records()
                        .last()
                        .unwrap()
                        .block
                        .header
                        .state_root,
                    world.current_state_root_hash().unwrap(),
                    "published root mismatch: {name}"
                );
                let replay = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
                assert_eq!(
                    replay.snapshot(),
                    world.snapshot(),
                    "replay mismatch: {name}"
                );
                assert_eq!(
                    replay.current_state_root_hash().unwrap(),
                    world.current_state_root_hash().unwrap()
                );
            }
            result => unprepared.push((name, format!("{result:?}"))),
        }
    }
    assert!(
        unprepared.is_empty(),
        "events lacked typed preparation: {unprepared:?}"
    );
}
