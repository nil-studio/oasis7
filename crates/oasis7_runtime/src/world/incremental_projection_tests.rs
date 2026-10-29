use super::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Capture {
    fields: BTreeMap<(FieldOwner, &'static str), FieldDescriptor>,
    values: BTreeMap<(FieldOwner, &'static str), Vec<(Value, Value)>>,
    bytes: BTreeMap<&'static str, (usize, usize)>,
    side_counts: BTreeMap<(FieldOwner, &'static str), Option<u64>>,
    groups: BTreeMap<(FieldOwner, &'static str), Vec<(Value, usize)>>,
    fail_on: Option<&'static str>,
}
impl GenesisFieldVisitor for Capture {
    type Error = &'static str;
    fn begin(&mut self, field: FieldDescriptor, _count: Option<usize>) -> Result<(), Self::Error> {
        if self.fail_on == Some(field.name) {
            return Err("admission denied");
        }
        assert!(matches!(field.destination, FieldDestination::State(_)));
        assert!(
            self.fields
                .insert((field.owner, field.name), field)
                .is_none()
        );
        Ok(())
    }
    fn group<K: Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        entries: usize,
    ) -> Result<(), Self::Error> {
        self.groups
            .entry((field.owner, field.name))
            .or_default()
            .push((serde_json::to_value(key).unwrap(), entries));
        Ok(())
    }
    fn entry<K: Serialize + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error> {
        self.values
            .entry((field.owner, field.name))
            .or_default()
            .push((
                serde_json::to_value(key).unwrap(),
                serde_json::to_value(value).unwrap(),
            ));
        Ok(())
    }
    fn bytes<K: Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        _key: &K,
        value: &[u8],
    ) -> Result<(), Self::Error> {
        self.bytes
            .insert(field.name, (value.as_ptr() as usize, value.len()));
        Ok(())
    }
    fn side<T: Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        _value: &T,
        count: Option<u64>,
    ) -> Result<(), Self::Error> {
        assert!(!matches!(field.destination, FieldDestination::State(_)));
        assert!(
            self.fields
                .insert((field.owner, field.name), field)
                .is_none()
        );
        self.side_counts.insert((field.owner, field.name), count);
        // Deliberately do not serialize historical/receipt payloads.
        Ok(())
    }
    fn process(&mut self, field: FieldDescriptor) -> Result<(), Self::Error> {
        assert_eq!(field.destination, FieldDestination::Process);
        assert!(
            self.fields
                .insert((field.owner, field.name), field)
                .is_none()
        );
        Ok(())
    }
}

#[test]
fn field_catalogs_cover_runtime_state_and_snapshot_without_building_a_snapshot() {
    let mut world = World::new();
    world
        .state
        .data_access_permissions
        .entry("empty-permission".into())
        .or_default();
    let mut sink = Capture::default();
    world.visit_iws_genesis_fields(&mut sink).unwrap();
    assert_eq!(
        sink.groups[&(FieldOwner::WorldState, "data_access_permissions")],
        vec![(json!("empty-permission"), 0)]
    );
    for field in oasis7_world_state::incremental_projection::WORLD_STATE_FIELDS {
        if field.destination != FieldDestination::Children {
            assert_eq!(sink.fields.get(&(field.owner, field.name)), Some(field));
        }
    }
    assert!(
        !sink
            .fields
            .contains_key(&(FieldOwner::WorldState, "agents"))
    );
    for field in oasis7_world_state::incremental_agent_fields::AgentFieldV1::ALL {
        assert_eq!(
            sink.fields[&(FieldOwner::WorldState, field.name())].destination,
            FieldDestination::State(field.namespace())
        );
    }
    for name in ["agents.body", "agents.mailbox_frontier", "agents.mailbox"] {
        assert!(sink.fields.contains_key(&(FieldOwner::WorldState, name)));
    }
    let runtime_fields = World::iws_runtime_fields();
    let names: BTreeSet<_> = runtime_fields.iter().map(|field| field.name).collect();
    assert_eq!(names.len(), runtime_fields.len());
    for field in runtime_fields {
        if field.destination != FieldDestination::Children {
            assert_eq!(sink.fields.get(&(field.owner, field.name)), Some(field));
        }
    }
    for link in oasis7_world_state::incremental_projection::SNAPSHOT_FIELDS {
        if link.kind
            != oasis7_world_state::incremental_projection::SnapshotFieldLinkKind::HistoryView
        {
            assert!(names.contains(link.runtime_field), "{}", link.name);
        }
    }
    for (field, expected) in [
        ("next_event_id", Namespace::Allocator),
        ("module_tick_schedule", Namespace::Scheduler),
        ("capability_nonce_records", Namespace::Authorization),
        ("pending_effects", Namespace::Effects),
        ("module_artifact_bytes", Namespace::Artifacts),
        ("threat_heatmap", Namespace::Business),
    ] {
        assert_eq!(
            sink.fields[&(FieldOwner::Runtime, field)].destination,
            FieldDestination::State(expected)
        );
    }
}

#[test]
fn module_wakes_use_fixed_bytes_and_reject_unencodable_instance_keys() {
    let mut world = World::new();
    world.module_tick_schedule.insert("m#1".into(), 0);
    world.module_tick_schedule.insert("m#2".into(), u64::MAX);
    let mut sink = Capture::default();
    world.visit_iws_genesis_fields(&mut sink).unwrap();
    let owner = (FieldOwner::Runtime, "module_tick_schedule");
    assert_eq!(sink.fields[&owner].shape, FieldShape::ByteMap);
    assert_eq!(
        sink.bytes["module_tick_schedule"].1,
        oasis7_world_state::incremental_scheduler_fields::MODULE_WAKE_BYTES
    );
    assert!(!sink.values.contains_key(&owner));
    world.module_tick_schedule.insert("x".repeat(1024), 7);
    let before = world.module_tick_schedule.clone();
    assert!(matches!(
        world.visit_iws_genesis_fields(&mut Capture::default()),
        Err(ProjectionError::Invalid("module wake key length"))
    ));
    assert_eq!(world.module_tick_schedule, before);
}

#[test]
fn maps_and_binary_values_are_borrowed_per_entry_including_skipped_empty_fields() {
    use oasis7_world_state::MaterialLedgerId;
    let mut world = World::new();
    world
        .state
        .material_ledgers
        .entry(MaterialLedgerId::world())
        .or_default()
        .insert("ore".into(), 17);
    world
        .state
        .module_states
        .insert("instance".into(), vec![1, 2, 3, 4]);
    world
        .module_artifact_bytes
        .insert("artifact".into(), std::sync::Arc::from([5u8, 6, 7]));
    let module_ptr = world.state.module_states["instance"].as_ptr() as usize;
    let artifact_ptr = world.module_artifact_bytes["artifact"].as_ptr() as usize;
    let mut sink = Capture::default();
    world.visit_iws_genesis_fields(&mut sink).unwrap();
    assert_eq!(sink.bytes["module_states"], (module_ptr, 4));
    assert_eq!(sink.bytes["module_artifact_bytes"], (artifact_ptr, 3));
    assert!(
        sink.values[&(FieldOwner::WorldState, "material_ledgers")]
            .iter()
            .any(|(key, value)| key == &json!(["world", "ore"]) && value == &json!(17))
    );
    assert!(
        sink.fields
            .contains_key(&(FieldOwner::WorldState, "starter_oc_claims"))
    );
}

#[test]
fn proofs_and_full_history_stay_outside_current_state_and_wall_clock_is_excluded() {
    let mut world = World::new();
    world.module_tick_routing_metrics.last_route_duration_ms = 12345;
    world.module_tick_routing_metrics.routing_count = 77;
    world.cognition["cognition_journal"]["events"] =
        json!([{"original_event":"preserved"}, {"original_event":"second"}]);
    world.cognition["responses"] = json!([{"response_artifact":"original"}]);
    world.cognition["retention_state"] =
        json!({"records":{"request":{"commit_record_id":"current-commit"}}});
    let before = serde_json::to_vec(&world).unwrap();
    let mut sink = Capture::default();
    world.visit_iws_genesis_fields(&mut sink).unwrap();
    assert_eq!(
        sink.side_counts[&(FieldOwner::Cognition, "journal.events")],
        Some(2)
    );
    assert_eq!(
        sink.side_counts[&(FieldOwner::Cognition, "responses")],
        Some(1)
    );
    assert_eq!(
        sink.fields[&(FieldOwner::Cognition, "retention_state")].destination,
        FieldDestination::RetentionLedger
    );
    for field in [
        "commit_records",
        "receipt_registry",
        "receipt_lineage_registry",
        "recovery",
    ] {
        assert_eq!(
            sink.fields[&(FieldOwner::Cognition, field)].destination,
            FieldDestination::ProofLedger
        );
    }
    let deterministic = &sink.values[&(FieldOwner::Runtime, "module_tick_routing_metrics")][0].1;
    assert_eq!(deterministic["routing_count"], 77);
    assert!(deterministic.get("last_route_duration_ms").is_none());
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
}

#[test]
fn unknown_json_members_invalid_shapes_and_sink_errors_reject_projection_without_mutation() {
    let mut world = World::new();
    world.cognition["new_unclassified_control"] = json!({"nonce":1});
    assert!(matches!(
        world.visit_iws_genesis_fields(&mut Capture::default()),
        Err(ProjectionError::Invalid(
            "unclassified or missing cognition field"
        ))
    ));
    world
        .cognition
        .as_object_mut()
        .unwrap()
        .remove("new_unclassified_control");
    world.cognition["cognition_journal"]["new_cursor"] = json!(3);
    assert!(matches!(
        world.visit_iws_genesis_fields(&mut Capture::default()),
        Err(ProjectionError::Invalid(_))
    ));
    world.cognition["cognition_journal"]
        .as_object_mut()
        .unwrap()
        .remove("new_cursor");
    world.cognition["feedback_outbox"] = json!([]);
    assert!(matches!(
        world.visit_iws_genesis_fields(&mut Capture::default()),
        Err(ProjectionError::Invalid("cognition state map shape"))
    ));
    world.cognition["feedback_outbox"] = json!({});
    let before = serde_json::to_vec(&world).unwrap();
    let mut sink = Capture {
        fail_on: Some("pending_actions"),
        ..Capture::default()
    };
    assert!(matches!(
        world.visit_iws_genesis_fields(&mut sink),
        Err(ProjectionError::Sink("admission denied"))
    ));
    assert_eq!(serde_json::to_vec(&world).unwrap(), before);
}

#[test]
fn hot_history_limits_and_wall_clock_do_not_change_deterministic_fields() {
    let mut world = World::new();
    let mut original = Capture::default();
    world.visit_iws_genesis_fields(&mut original).unwrap();
    world.runtime_memory_limits.max_journal_events = 7;
    world.runtime_backpressure_stats.journal_events_evicted = 17;
    world
        .runtime_backpressure_stats
        .inflight_effect_dispatch_blocked = 23;
    world.module_tick_routing_metrics.last_route_duration_ms = 123;
    world.module_tick_routing_metrics.max_route_duration_ms = 456;
    let mut changed_cache = Capture::default();
    world.visit_iws_genesis_fields(&mut changed_cache).unwrap();
    assert_eq!(original.values, changed_cache.values);
    for name in [
        "runtime_memory_limits.max_journal_events",
        "runtime_backpressure_stats.journal_events_evicted",
        "runtime_backpressure_stats.inflight_effect_dispatch_blocked",
        "module_tick_routing_metrics.wall_clock",
    ] {
        assert_eq!(
            changed_cache.fields[&(FieldOwner::Runtime, name)].destination,
            FieldDestination::Process
        );
    }
    world.runtime_memory_limits.max_pending_actions = 31;
    world.runtime_backpressure_stats.pending_actions_evicted = 5;
    let mut changed_control = Capture::default();
    world
        .visit_iws_genesis_fields(&mut changed_control)
        .unwrap();
    for name in [
        "runtime_memory_limits.max_pending_actions",
        "runtime_backpressure_stats.pending_actions_evicted",
    ] {
        assert_ne!(
            original.values[&(FieldOwner::Runtime, name)],
            changed_control.values[&(FieldOwner::Runtime, name)]
        );
    }
}

#[test]
fn cognition_economy_projection_preserves_live_lease_and_separates_original_journal() {
    let world = economy_world_for_projection();
    let before = world.cognition.clone();
    let object = before["cognition_economy"].as_object().unwrap();
    let mut sink = Capture::default();
    world.visit_iws_genesis_fields(&mut sink).unwrap();
    for key in object.keys() {
        assert!(
            sink.fields
                .keys()
                .any(|(owner, name)| *owner == FieldOwner::Cognition
                    && *name == format!("economy.{key}")),
            "missing economy field {key}"
        );
    }
    let balances = &sink.values[&(FieldOwner::Cognition, "economy.balances")];
    assert_eq!(balances.len(), 1);
    assert_eq!(balances[0].0, json!(["account-agent-a", "cognition_units"]));
    assert_eq!(balances[0].1["available"], 12);
    assert_eq!(balances[0].1["reserved"], 5);
    assert_eq!(
        sink.values[&(FieldOwner::Cognition, "economy.leases")].len(),
        1
    );
    assert_eq!(
        sink.side_counts[&(FieldOwner::Cognition, "economy.journal")],
        Some(1)
    );
    assert!(
        !sink
            .values
            .contains_key(&(FieldOwner::Cognition, "economy.journal"))
    );
    for (name, namespace) in [
        ("economy.balances", Namespace::Business),
        ("economy.leases", Namespace::Effects),
        ("economy.idempotency", Namespace::Authorization),
        ("economy.head_seq", Namespace::Allocator),
    ] {
        assert_eq!(
            sink.fields[&(FieldOwner::Cognition, name)].destination,
            FieldDestination::State(namespace)
        );
    }
    assert_eq!(world.cognition, before);
}

fn economy_world_for_projection() -> World {
    let mut world = World::new();
    world
        .set_cognition_resource_balance("account-agent-a", "cognition_units", 17)
        .unwrap();
    world
        .reserve_cognition_lease(crate::CognitionLeaseRequestV1::new(
            "original-reservation",
            "account-agent-a",
            "agent-a",
            "session-a",
            "turn-a",
            "request-a",
            "request-digest-a",
            crate::CognitionLeaseQuoteV1::new("original-quote", "cognition_units", 5),
        ))
        .unwrap();
    world
}

#[test]
fn economy_projection_rejects_unknown_missing_or_invalid_field_shapes_without_mutation() {
    let mut world = economy_world_for_projection();
    let valid = world.cognition["cognition_economy"].clone();
    for mode in 0..6 {
        world.cognition["cognition_economy"] = valid.clone();
        let economy = &mut world.cognition["cognition_economy"];
        match mode {
            0 => economy["new_control"] = json!(1),
            1 => economy["balances"]["account-agent-a"] = json!([]),
            2 => economy["journal"] = json!({}),
            3 => economy["head_seq"] = json!(-1),
            4 => economy["schema_version"] = json!("cognition-economy.future"),
            _ => {
                economy.as_object_mut().unwrap().remove("operations");
            }
        }
        let before = world.cognition.clone();
        assert!(
            matches!(
                world.visit_iws_genesis_fields(&mut Capture::default()),
                Err(ProjectionError::Invalid(_))
            ),
            "invalid economy mode {mode}"
        );
        assert_eq!(world.cognition, before);
    }
}
