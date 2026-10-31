#[path = "task_cleanup_v2_formal_fixture.rs"]
mod formal_fixture;
#[test]
fn cleanup_v2_actual_closed_world_preserves_versioned_formal_publication() {
    let (world, request, _) = formal_fixture::fixture();
    assert!(
        crate::game_task::current_control(&world, "agent")
            .unwrap()
            .is_some()
    );
    let active_root = crate::formal_projection::commitments_for_world(&world)
        .unwrap()
        .root()
        .unwrap();
    let owner = request.subject_public_key.clone();
    let delegate = world.capability_revocation_state().agent_signer_delegations["agent"]
        .delegate_public_key
        .clone();
    let audiences = [
        oasis7_client_api::formal_projection::ProjectionAudienceV1::public(),
        oasis7_client_api::formal_projection::ProjectionAudienceV1::agent("agent", owner).unwrap(),
        oasis7_client_api::formal_projection::ProjectionAudienceV1::agent("agent", delegate)
            .unwrap(),
    ];
    let leaves = audiences
        .into_iter()
        .map(|audience| {
            let (agent, reader) = if audience.scope_id == "public" {
                (None, "")
            } else {
                (
                    Some("agent"),
                    audience.reader_public_key.as_deref().unwrap(),
                )
            };
            let projection =
                crate::projection::from_formal_committed_world(&world, agent, reader).unwrap();
            let leaf =
                oasis7_world_projection::codec::projection_leaf(&audience, &projection).unwrap();
            (audience, leaf)
        })
        .collect();
    assert_eq!(
        active_root,
        oasis7_client_api::formal_projection::ProjectionCommitmentsV1::from_leaves(leaves)
            .unwrap()
            .root()
            .unwrap()
    );
    let (mut world, _, active_anchor, _) =
        crate::formal_read_fixtures::actual_world_initial_fixture_from_world(
            world,
            crate::formal_read_fixtures::h(50),
        );
    crate::formal_read_service::FormalReadPublication::new(active_anchor.clone(), world.clone())
        .unwrap();
    // Actual Runtime private begun context: UTC is committed context data and
    // parent time is hash-bound by the existing local test qualification seam.
    let context = formal_fixture::begin(&mut world, 1000);
    world.apply_task_revoke_and_close_v2(&request).unwrap();
    world.finish_formal_administrative_tick(&context).unwrap();
    assert!(crate::game_task::current_control(&world, "agent").is_err());
    assert!(matches!(
        world.current_game_task_state_v2("agent").unwrap(),
        Some(oasis7_agent_api::game::GameTaskStateV2::Closed(_))
    ));
    // Target: do not make the formal writer impossible to commit simply
    // because the intentionally unsupported legacy task projection now errs.
    let terminal_root = crate::formal_projection::commitments_for_world(&world)
        .expect("versioned Closed formal commitment must remain constructible")
        .root()
        .unwrap();
    assert_ne!(terminal_root, active_root);
    let owner = request.subject_public_key.clone();
    let audience =
        oasis7_client_api::formal_projection::ProjectionAudienceV1::agent("agent", owner.clone())
            .unwrap();
    let Some(oasis7_agent_api::game::GameTaskStateV2::Closed(closed)) =
        world.current_game_task_state_v2("agent").unwrap()
    else {
        panic!("actual terminal state missing");
    };
    let typed = oasis7_client_api::formal_projection::ClosedTaskFormalProjectionV2 {
        api_revision: 2,
        closed,
    };
    typed.validate(&audience).unwrap();
    let proof = crate::formal_projection::commitments_for_world(&world)
        .unwrap()
        .inclusion(&audience)
        .unwrap();
    proof.verify(&audience, &typed, &terminal_root).unwrap();
    assert!(proof.verify(&audience, &(), &terminal_root).is_err());
    assert!(crate::projection::from_formal_committed_world(&world, Some("agent"), &owner).is_err());
    let value = serde_json::to_value(&typed).unwrap();
    let restored: oasis7_client_api::formal_projection::ClosedTaskFormalProjectionV2 =
        serde_json::from_value(value.clone()).unwrap();
    assert_eq!(restored, typed);
    let mut extra = value;
    extra["unrecognized"] = serde_json::json!(true);
    assert!(serde_json::from_value::<oasis7_client_api::formal_projection::ClosedTaskFormalProjectionV2>(extra).is_err());
    let (closed, closed_anchor, _) =
        crate::formal_read_fixtures::actual_world_successor_fixture(&active_anchor, world);
    crate::formal_read_service::FormalReadPublication::new(closed_anchor, closed.clone()).unwrap();
    assert!(crate::game_task::current_control(&closed, "agent").is_err());
}
