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
    let world = World::new();
    let mut sink = Capture::default();
    world.visit_iws_genesis_fields(&mut sink).unwrap();
    for field in oasis7_world_state::incremental_projection::WORLD_STATE_FIELDS {
        assert_eq!(sink.fields.get(&(field.owner, field.name)), Some(field));
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
