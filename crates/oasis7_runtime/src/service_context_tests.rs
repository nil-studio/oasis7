use super::*;
use crate::Action;
use oasis7_agent_api::world_action::GeoPos;
fn world() -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent".into(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world
        .bind_cognition_runtime("provider-observation", "main", 0, None, "pending", 0)
        .unwrap();
    world
        .install_capability_agent_identity("agent", "owner", 1)
        .unwrap();
    world
}
#[test]
fn absent_provider_is_optional_only_for_ordinary_observation() {
    let world = world();
    assert!(
        world
            .optional_canonical_agent_service_context("agent")
            .unwrap()
            .is_none()
    );
    assert!(world.canonical_agent_service_context("agent").is_err());
}
#[test]
fn existing_valid_provider_projection_is_unchanged() {
    let mut world = world();
    world
        .install_test_provider_capability_fixture("agent")
        .unwrap();
    assert_eq!(
        serde_json::to_value(
            world
                .optional_canonical_agent_service_context("agent")
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(world.canonical_agent_service_context("agent").unwrap()).unwrap()
    );
}
#[test]
fn existing_provider_history_and_selected_catalog_errors_are_not_absence() {
    let mut source = world();
    let context = source
        .install_test_provider_capability_fixture("agent")
        .unwrap();
    for other_history in [false, true] {
        let mut world = world();
        let mut existing = context.clone();
        if other_history {
            existing.audience.branch_id = "another-branch".into();
        }
        // A real persisted record in a fresh independent fixture, deliberately
        // without its grant/catalog authority, must remain an error, not None.
        world
            .install_capability_invocation_context(existing)
            .unwrap();
        let strict = world.canonical_agent_service_context("agent").unwrap_err();
        assert_eq!(
            world
                .optional_canonical_agent_service_context("agent")
                .unwrap_err(),
            strict
        );
        if other_history {
            assert!(strict.contains("invocation context is unavailable"));
        } else {
            assert!(strict.contains("Runtime capability context unavailable"));
        }
    }
}
