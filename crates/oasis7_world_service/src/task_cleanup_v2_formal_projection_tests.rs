use super::*;
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
    let (closed, closed_anchor, _) =
        crate::formal_read_fixtures::actual_world_successor_fixture(&active_anchor, world);
    crate::formal_read_service::FormalReadPublication::new(closed_anchor, closed.clone()).unwrap();
    assert!(crate::game_task::current_control(&closed, "agent").is_err());
}
