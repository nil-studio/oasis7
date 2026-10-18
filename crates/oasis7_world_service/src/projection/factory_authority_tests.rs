use super::*;
use oasis7_runtime::{AgentLocationAuthorityV1, FactorySiteAuthorityV1, LocationAnchorV1};

#[test]
fn construction_projection_scopes_authorities_and_preserves_public_anchor_registry() {
    let mut state = WorldState::default();
    for id in ["agent-a", "agent-b"] {
        state.agent_location_authorities.insert(
            id.into(),
            AgentLocationAuthorityV1 {
                agent_id: id.into(),
                location_id: format!("location-{id}"),
                active: true,
                authority_revision: 1,
                effective_at: 0,
            },
        );
        state.location_anchors.insert(
            format!("location-{id}"),
            LocationAnchorV1 {
                location_id: format!("location-{id}"),
                active: true,
                authority_revision: 1,
                effective_at: 0,
            },
        );
    }
    for (site, owner, members) in [
        ("own", "agent-a", vec!["agent-b"]),
        ("shared", "private-owner", vec!["agent-a", "private-member"]),
        ("foreign", "agent-b", vec!["private-member"]),
    ] {
        state.factory_site_authorities.insert(
            site.into(),
            FactorySiteAuthorityV1 {
                site_id: site.into(),
                location_id: "location-agent-a".into(),
                owner_agent_id: owner.into(),
                authorized_agent_ids: members.into_iter().map(Into::into).collect(),
                active: true,
                chunk_ready: true,
                authority_revision: 1,
                registered_at: 0,
            },
        );
    }
    for (job_id, builder) in [(1, "agent-a"), (2, "agent-b")] {
        state.pending_factory_builds.insert(job_id, serde_json::from_value(serde_json::json!({
            "job_id": job_id, "builder_agent_id": builder, "site_id": "own", "ready_at": 10,
            "spec": {
                "factory_id": format!("factory-{builder}"), "display_name": "Factory", "tier": 1,
                "tags": [], "build_cost": [], "build_time_ticks": 1, "base_power_draw": 0,
                "recipe_slots": 1, "throughput_bps": 10000, "maintenance_per_tick": 0
            }
        })).unwrap());
    }
    let world = oasis7_runtime::World::new_with_state(state);
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let scoped = WorldServiceProjection::from_world(&world, Some("agent-a")).unwrap();
    assert_eq!(scoped.state.agent_location_authorities.len(), 1);
    assert!(
        scoped
            .state
            .agent_location_authorities
            .contains_key("agent-a")
    );
    assert_eq!(scoped.state.factory_site_authorities.len(), 2);
    let shared = &scoped.state.factory_site_authorities["shared"];
    assert!(shared.owner_agent_id.is_empty());
    assert_eq!(shared.authorized_agent_ids, ["agent-a"]);
    assert_eq!(
        scoped.state.factory_site_authorities["own"].owner_agent_id,
        "agent-a"
    );
    assert!(
        scoped.state.factory_site_authorities["own"]
            .authorized_agent_ids
            .is_empty()
    );
    assert_eq!(scoped.state.pending_factory_builds.len(), 1);
    assert_eq!(
        scoped.state.pending_factory_builds[&1].builder_agent_id,
        "agent-a"
    );
    assert_eq!(scoped.state.location_anchors.len(), 2);
    let generic = WorldServiceProjection::from_world(&world, None).unwrap();
    assert!(generic.state.agent_location_authorities.is_empty());
    assert!(generic.state.factory_site_authorities.is_empty());
    assert!(generic.state.pending_factory_builds.is_empty());
    assert_eq!(generic.state.location_anchors.len(), 2);
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
}
