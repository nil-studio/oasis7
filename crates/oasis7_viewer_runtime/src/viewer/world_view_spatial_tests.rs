use super::*;
use crate::{
    geometry::GeoPos,
    simulator::{
        Agent, Factory, Location, ModuleVisualAnchor, ModuleVisualEntity, ResourceOwner,
        WorldKernel,
    },
};
use std::collections::BTreeSet;
fn fixture() -> WorldSnapshot {
    let mut snapshot = WorldKernel::new().snapshot();
    snapshot.model.agents.clear();
    snapshot.model.locations.clear();
    snapshot.model.factories.clear();
    snapshot
        .model
        .agents
        .insert("a".into(), Agent::new("a", "l", GeoPos::new(0, 0, 0)));
    snapshot.model.locations.insert(
        "l".into(),
        Location::new("l", "site", GeoPos::new(200, 200, 0)),
    );
    snapshot.model.factories.insert(
        "f".into(),
        Factory {
            id: "f".into(),
            owner: ResourceOwner::Agent {
                agent_id: "a".into(),
            },
            location_id: "l".into(),
            kind: "smelter".into(),
        },
    );
    let mut runtime =
        crate::runtime::World::new_with_state(crate::runtime::WorldState::default()).snapshot();
    runtime.state.module_visual_entities.insert(
        "f".into(),
        ModuleVisualEntity {
            entity_id: "f".into(),
            module_id: "industry".into(),
            kind: "factory".into(),
            label: None,
            anchor: ModuleVisualAnchor::Absolute {
                pos: GeoPos::new(1000, 1000, 0),
            },
        },
    );
    runtime.state.logistics_routes.insert(
        "r".into(),
        crate::runtime::LogisticsRouteV1 {
            route_id: "r".into(),
            from_ledger: crate::runtime::MaterialLedgerId::Agent("a".into()),
            to_ledger: crate::runtime::MaterialLedgerId::Factory("f".into()),
            kind: "road".into(),
            distance_km: 1,
            priority: Default::default(),
            owner_agent_id: "private-owner-marker".into(),
            available: true,
            capacity_units: 20,
            reserved_capacity_units: 4,
            tariff_electricity_per_unit: 2,
        },
    );
    runtime.state.agents.insert(
        "a".into(),
        crate::runtime::AgentCell::new(
            crate::models::AgentState::new("a", GeoPos::new(0, 0, 0)),
            0,
        ),
    );
    snapshot.time = runtime.state.time;
    snapshot.model.agents.get_mut("a").unwrap().resources =
        runtime.state.agents["a"].state.resources.clone();
    snapshot.runtime_snapshot = Some(runtime);
    snapshot
}
fn query() -> Value {
    json!({"request_generation":"q","page_size":200})
}
#[test]
fn industrial_levels_count_only_authorized_trusted_members_and_preserve_pins() {
    let mut snapshot = fixture();
    let second = snapshot.model.factories["f"].clone();
    let mut second = second;
    second.id = "g".into();
    snapshot.model.factories.insert("g".into(), second);
    let mut visual = snapshot
        .runtime_snapshot
        .as_ref()
        .unwrap()
        .state
        .module_visual_entities["f"]
        .clone();
    visual.entity_id = "g".into();
    snapshot
        .runtime_snapshot
        .as_mut()
        .unwrap()
        .state
        .module_visual_entities
        .insert("g".into(), visual);
    let source = SourceAuthority::local_runtime();
    let prepared = prepare(&snapshot, "w", 0, "public", &source).unwrap();
    let overview = projection::query_objects(&prepared, "overview", None, &[], None, false);
    let aggregate = overview
        .iter()
        .find(|o| o.value.get("aggregate_id").is_some())
        .unwrap();
    assert_eq!(aggregate.value["member_count"], "2");
    assert_eq!(aggregate.value["counts_by_kind"]["smelter"], "2");
    assert!(!overview.iter().any(|o| o.value["kind"] == "factory"));
    let relation = overview.iter().find(|o| o.relation).unwrap();
    assert_eq!(relation.value["to"]["kind"], "aggregate");
    assert_eq!(relation.value["route_count"], "1");
    let pinned = projection::query_objects(
        &prepared,
        "overview",
        None,
        &[("factory".into(), "f".into())],
        None,
        false,
    );
    assert!(!pinned.iter().any(|o| o.value.get("aggregate_id").is_some()));
    assert_eq!(
        pinned
            .iter()
            .filter(|o| o.value["kind"] == "factory")
            .count(),
        2
    );
    let detail = projection::query_objects(&prepared, "entity", None, &[], None, false);
    assert!(!detail.iter().any(|o| o.value.get("aggregate_id").is_some()));
    let searched = projection::query_objects(&prepared, "overview", None, &[], Some("g"), false);
    assert!(
        searched
            .iter()
            .all(|o| o.value.get("aggregate_id").is_none())
    );
    // A hidden member is absent before aggregation; neither its count nor ID survives.
    snapshot.model.factories.remove("g");
    let prepared = prepare(&snapshot, "w", 0, "public", &source).unwrap();
    assert!(
        !projection::query_objects(&prepared, "overview", None, &[], None, false)
            .iter()
            .any(|o| o.value.get("aggregate_id").is_some())
    );
}

#[test]
fn explicit_event_position_stays_historical_and_never_follows_current_entity() {
    let snapshot = fixture();
    let event = crate::runtime::WorldEvent {
        id: 9007199254740993,
        time: 0,
        caused_by: None,
        body: crate::runtime::WorldEventBody::Domain(
            crate::runtime::DomainEvent::AgentRegistered {
                agent_id: "a".into(),
                pos: GeoPos::new(12, 34, 56),
            },
        ),
    };
    let authority =
        SourceAuthority::local_runtime().with_events(std::slice::from_ref(&event), "w", 7);
    let prepared = prepare(&snapshot, "w", 7, "public", &authority).unwrap();
    let objects = projection::query_objects(&prepared, "entity", None, &[], None, true);
    let anchor = objects
        .iter()
        .find(|o| o.value.get("event_id").is_some())
        .unwrap();
    assert_eq!(anchor.value["position"]["x_cm"], "12");
    assert_eq!(anchor.value["identity"]["event_seq"], "9007199254740993");
    assert_eq!(anchor.value["identity"]["reorg_epoch"], "7");
    assert_eq!(anchor.value["freshness"], "last_known");
    assert_eq!(anchor.value["source_cut"], prepared.cut);
    assert_eq!(anchor.value["feed_ref"], anchor.value["identity"]);
    assert!(anchor.value.get("visibility_ref").is_none());
    assert!(
        !projection::query_objects(&prepared, "entity", None, &[], None, false)
            .iter()
            .any(|o| o.value.get("event_id").is_some())
    );
    let mut hidden = snapshot.clone();
    hidden.model.agents.clear();
    let hidden = prepare(&hidden, "w", 7, "public", &authority).unwrap();
    assert!(
        !hidden
            .objects
            .iter()
            .any(|o| o.value.get("event_id").is_some())
    );
    let mut conflict = event.clone();
    conflict.time += 1;
    let conflict = SourceAuthority::local_runtime().with_events(&[event, conflict], "w", 7);
    assert!(conflict.events().is_empty());
}

#[test]
fn explicit_factory_retirement_evidence_is_local_and_changes_the_cut() {
    let mut snapshot = fixture();
    let source = SourceAuthority::local_runtime()
        .with_retirement_candidates(BTreeSet::from(["retired".into()]));
    let before = prepare(&snapshot, "w", 0, "public", &source).unwrap();
    snapshot
        .runtime_snapshot
        .as_mut()
        .unwrap()
        .state
        .retired_factory_ids
        .insert("retired".into());
    snapshot
        .runtime_snapshot
        .as_mut()
        .unwrap()
        .state
        .retired_factory_ids
        .insert("unseen-private-retirement".into());
    let after = prepare(&snapshot, "w", 0, "public", &source).unwrap();
    assert!(after.deleted_factories.contains("retired"));
    assert_eq!(after.deleted_factories.len(), 1);
    assert_ne!(before.cut, after.cut);
    let remote = SourceAuthority::remote(json!({}), json!({}), String::new());
    assert!(
        prepare(&snapshot, "w", 0, "public", &remote)
            .unwrap()
            .deleted_factories
            .is_empty()
    );
}
#[test]
fn module_event_anchors_require_typed_absolute_payload_and_visible_visual_identity() {
    let mut snapshot = fixture();
    let mut visual = snapshot
        .runtime_snapshot
        .as_ref()
        .unwrap()
        .state
        .module_visual_entities["f"]
        .clone();
    visual.anchor = ModuleVisualAnchor::Absolute {
        pos: GeoPos::new(12, 34, 0),
    };
    let event = |id, kind: &str, payload| crate::runtime::WorldEvent {
        id,
        time: 0,
        caused_by: None,
        body: crate::runtime::WorldEventBody::ModuleEmitted(oasis7_wasm_abi::ModuleEmitEvent {
            module_id: "industry".into(),
            trace_id: "private-trace-not-a-display-field".into(),
            kind: kind.into(),
            payload,
        }),
    };
    let absolute = event(1, "module_visual_entity_upserted", json!({"entity":visual}));
    visual.anchor = ModuleVisualAnchor::Agent {
        agent_id: "a".into(),
    };
    let relative = event(2, "module_visual_entity_upserted", json!({"entity":visual}));
    let removed = event(3, "module_visual_entity_removed", json!({"entity_id":"f"}));
    let source =
        SourceAuthority::local_runtime().with_events(&[absolute, relative, removed], "w", 0);
    let prepared = prepare(&snapshot, "w", 0, "public", &source).unwrap();
    let anchors: Vec<_> = prepared
        .objects
        .iter()
        .filter(|o| o.value.get("event_id").is_some())
        .collect();
    assert_eq!(anchors.len(), 1);
    assert_eq!(anchors[0].value["position"]["x_cm"], "12");
    assert!(!anchors[0].value.to_string().contains("private-trace"));
    snapshot
        .runtime_snapshot
        .as_mut()
        .unwrap()
        .state
        .module_visual_entities
        .clear();
    assert!(
        !prepare(&snapshot, "w", 0, "public", &source)
            .unwrap()
            .objects
            .iter()
            .any(|o| o.value.get("event_id").is_some())
    );
}
#[test]
fn factories_require_explicit_fixed_geometry_and_routes_require_visible_endpoints() {
    let mut source = fixture();
    let result = build_with_authority(&source, "w", 0, "public", query(), true);
    let f = result["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "factory")
        .unwrap();
    assert_eq!(f["position"]["x_cm"], "1000");
    assert_eq!(f["provenance"]["freshness"], "current");
    assert_eq!(
        result["relations"][0]["from"],
        json!({"kind":"agent","id":"a"})
    );
    assert!(!result.to_string().contains("private-owner-marker"));
    source
        .runtime_snapshot
        .as_mut()
        .unwrap()
        .state
        .module_visual_entities
        .clear();
    let without_anchor = build_with_authority(&source, "w", 0, "public", query(), true);
    let f = without_anchor["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "factory")
        .unwrap();
    assert_eq!(f["provenance"]["freshness"], "unknown");
    assert_eq!(without_anchor["relations"][0]["freshness"], "unknown");
    source.model.agents.clear();
    assert!(
        build_with_authority(&source, "w", 0, "public", query(), true)["relations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn viewport_route_intersection_includes_both_endpoints_and_authorized_pins() {
    let source = fixture();
    let mut q = query();
    q["bounds"] = json!({"min_x_cm":"400","max_x_cm":"600","min_y_cm":"400","max_y_cm":"600"});
    q["pinned_entities"] = json!([{"kind":"location","id":"l"},{"kind":"agent","id":"hidden"}]);
    let result = build_with_authority(&source, "w", 0, "public", q, true);
    assert_eq!(result["relations"].as_array().unwrap().len(), 1);
    let ids: Vec<_> = result["entities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["entity_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["l", "a", "f"]);
    assert!(!result.to_string().contains("hidden"));
}
#[test]
fn joint_paging_never_duplicates_or_mixes_entity_and_relation_budgets() {
    let source = fixture();
    let mut q = query();
    q["page_size"] = json!(1);
    let mut objects = BTreeSet::new();
    loop {
        let result = build_with_authority(&source, "w", 0, "public", q.clone(), true);
        let entities = result["entities"].as_array().unwrap();
        let relations = result["relations"].as_array().unwrap();
        assert_eq!(entities.len() + relations.len(), 1);
        for e in entities {
            assert!(objects.insert(format!("{}:{}", e["kind"], e["entity_id"])));
        }
        for r in relations {
            assert!(objects.insert(format!("relation:{}", r["relation_id"])));
        }
        if result["next_cursor"].is_null() {
            break;
        }
        q["cursor"] = result["next_cursor"].clone();
    }
    assert_eq!(objects.len(), 4);
}
#[test]
fn stable_source_reuses_index_without_snapshot_and_permission_change_resets() {
    let source = fixture();
    let mut cache = None;
    // Local wrapper permits a synthetic simulator fixture; production uses a bound Runtime digest.
    let a = SourceAuthority::local_runtime();
    let first = build_session_bound(&mut cache, || source, "w", 0, "public", query(), a.clone());
    let mut q = query();
    q["bounds"] = json!({"min_x_cm":"0","max_x_cm":"20","min_y_cm":"0","max_y_cm":"20"});
    let next = build_session_bound(
        &mut cache,
        || panic!("same source viewport must reuse the indexed projection"),
        "w",
        0,
        "public",
        q,
        a,
    );
    assert_eq!(first["source_cut"], next["source_cut"]);
    let mut page = query();
    page["page_size"] = json!(1);
    let source = fixture();
    let first = build_session(&mut cache, || source, "w", 0, "public", page.clone(), true);
    page["cursor"] = first["next_cursor"].clone();
    let denied = build_session_bound(
        &mut cache,
        || panic!("invalid scope must fail before source access"),
        "w",
        0,
        "public",
        page,
        SourceAuthority::unknown(),
    );
    assert_eq!(denied["unavailable_reasons"][0], "cursor_reset_required");
}

#[test]
fn stable_bound_pagination_stays_current_but_advanced_source_freezes_old_cut() {
    use crate::viewer::world_view_authority::spatial_state_digest;
    let source = fixture();
    let authority = SourceAuthority::local_bound(
        spatial_state_digest(&source.runtime_snapshot.as_ref().unwrap().state).unwrap(),
    );
    let mut cache = None;
    let mut q = query();
    q["page_size"] = json!(1);
    let first = build_session_bound(
        &mut cache,
        || source.clone(),
        "w",
        0,
        "public",
        q.clone(),
        authority.clone(),
    );
    q["cursor"] = first["next_cursor"].clone();
    let second = build_session_bound(
        &mut cache,
        || panic!("continuation must not rebuild"),
        "w",
        0,
        "public",
        q.clone(),
        authority,
    );
    assert_eq!(second["source_frozen"], false);
    assert_eq!(second["entities"][0]["provenance"]["freshness"], "current");
    let mut advanced = source.runtime_snapshot.unwrap().state;
    advanced.time += 1;
    let later = SourceAuthority::local_bound(spatial_state_digest(&advanced).unwrap());
    let frozen = build_session_bound(
        &mut cache,
        || panic!("use frozen old cut"),
        "w",
        0,
        "public",
        q,
        later,
    );
    assert_eq!(frozen["source_cut"], first["source_cut"]);
    assert_eq!(frozen["source_frozen"], true);
    assert_eq!(
        frozen["entities"][0]["provenance"]["freshness"],
        "last_known"
    );
}
#[test]
fn pinned_target_is_on_first_page_even_when_lexically_last_and_outside_viewport() {
    let mut source = fixture();
    for i in 0..1001 {
        let id = format!("agent-{i:04}");
        source
            .model
            .agents
            .insert(id.clone(), Agent::new(id, "l", GeoPos::new(0, 0, 0)));
    }
    source.model.agents.insert(
        "z-target".into(),
        Agent::new("z-target", "l", GeoPos::new(999, 999, 0)),
    );
    let mut q = query();
    q["bounds"] = json!({"min_x_cm":"0","max_x_cm":"10","min_y_cm":"0","max_y_cm":"10"});
    q["pinned_entities"] = json!([{"kind":"agent","id":"z-target"}]);
    let view = build_with_authority(&source, "w", 0, "public", q, true);
    assert_eq!(view["entities"][0]["entity_id"], "z-target");
    assert_eq!(
        view["entities"].as_array().unwrap().len() + view["relations"].as_array().unwrap().len(),
        200
    );
    assert_eq!(view["completeness"], "partial");
}
#[test]
fn site_ledger_resolves_distinct_location_identity_and_inactive_site_does_not_project() {
    let mut source = fixture();
    let state = &mut source.runtime_snapshot.as_mut().unwrap().state;
    state.factory_site_authorities.insert(
        "site-S".into(),
        crate::runtime::FactorySiteAuthorityV1 {
            site_id: "site-S".into(),
            location_id: "l".into(),
            active: true,
            ..Default::default()
        },
    );
    state.logistics_routes.get_mut("r").unwrap().from_ledger =
        crate::runtime::MaterialLedgerId::Site("site-S".into());
    let view = build_with_authority(&source, "w", 0, "public", query(), true);
    assert_eq!(
        view["relations"][0]["from"],
        json!({"kind":"location","id":"l"})
    );
    source
        .runtime_snapshot
        .as_mut()
        .unwrap()
        .state
        .factory_site_authorities
        .get_mut("site-S")
        .unwrap()
        .active = false;
    assert!(
        build_with_authority(&source, "w", 0, "public", query(), true)["relations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
