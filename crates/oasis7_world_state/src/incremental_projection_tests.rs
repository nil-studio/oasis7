use super::*;
use crate::{AgentCell, AgentState, MaterialLedgerId, WorldState};
use oasis7_agent_api::world_action::GeoPos;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
struct Capture {
    fields: BTreeMap<&'static str, FieldDescriptor>,
    values: BTreeMap<&'static str, Vec<(Value, Value)>>,
    bytes: BTreeMap<&'static str, (usize, usize)>,
    groups: BTreeMap<&'static str, Vec<(Value, usize)>>,
    fail_group: Option<&'static str>,
    fail_on: Option<&'static str>,
}
impl GenesisFieldVisitor for Capture {
    type Error = &'static str;
    fn begin(&mut self, field: FieldDescriptor, _: Option<usize>) -> Result<(), Self::Error> {
        if self.fail_on == Some(field.name) {
            return Err("admission denied");
        }
        assert!(self.fields.insert(field.name, field).is_none());
        Ok(())
    }
    fn group<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        entries: usize,
    ) -> Result<(), Self::Error> {
        if self.fail_group == Some(field.name) {
            return Err("group admission denied");
        }
        crate::incremental_semantic_key::write_semantic_key_v1(key, &mut std::io::sink())
            .map_err(|_| "semantic key encoding")?;
        self.groups
            .entry(field.name)
            .or_default()
            .push((serde_json::to_value(key).unwrap(), entries));
        Ok(())
    }
    fn entry<K: SemanticKeyV1 + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error> {
        crate::incremental_semantic_key::write_semantic_key_v1(key, &mut std::io::sink())
            .map_err(|_| "semantic key encoding")?;
        self.values.entry(field.name).or_default().push((
            serde_json::to_value(key).unwrap(),
            serde_json::to_value(value).unwrap(),
        ));
        Ok(())
    }
    fn bytes<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        key: &K,
        value: &[u8],
    ) -> Result<(), Self::Error> {
        crate::incremental_semantic_key::write_semantic_key_v1(key, &mut std::io::sink())
            .map_err(|_| "semantic key encoding")?;
        self.bytes
            .insert(field.name, (value.as_ptr() as usize, value.len()));
        Ok(())
    }
    fn side<T: Serialize + ?Sized>(
        &mut self,
        field: FieldDescriptor,
        _: &T,
        _: Option<u64>,
    ) -> Result<(), Self::Error> {
        assert!(self.fields.insert(field.name, field).is_none());
        Ok(())
    }
    fn process(&mut self, field: FieldDescriptor) -> Result<(), Self::Error> {
        assert!(self.fields.insert(field.name, field).is_none());
        Ok(())
    }
}

#[test]
fn genesis_projection_borrows_binary_and_nested_entries_without_mutating_sources() {
    let mut state = WorldState::default();
    state.agents.insert(
        "a".into(),
        AgentCell::new(AgentState::new("a", GeoPos::new(1, 2, 3)), 7),
    );
    state
        .module_states
        .insert("large-instance".into(), vec![42; 1 << 20]);
    state
        .material_ledgers
        .entry(MaterialLedgerId::world())
        .or_default()
        .insert("ore".into(), 17);
    let pointer = state.module_states["large-instance"].as_ptr() as usize;
    let mut sink = Capture::default();
    state.visit_iws_genesis_fields(&mut sink).unwrap();
    assert_eq!(sink.bytes["module_states"], (pointer, 1 << 20));
    assert_eq!(
        sink.values["material_ledgers"],
        vec![(json!(["world", "ore"]), json!(17))]
    );
    assert_eq!(sink.groups["material_ledgers"], vec![(json!("world"), 1)]);
    for (name, namespace) in [
        ("time", StateNamespaceV1::Allocator),
        ("world_rules", StateNamespaceV1::Authorization),
        ("module_states", StateNamespaceV1::Business),
    ] {
        assert_eq!(
            sink.fields[name].destination,
            FieldDestination::State(namespace)
        );
    }
    assert!(!sink.fields.contains_key("agents"));
    assert!(sink.fields.contains_key("agents.body"));
    assert!(sink.fields.contains_key("agents.mailbox_frontier"));
    assert!(sink.fields.contains_key("starter_oc_claims"));
    assert_eq!(
        sink.fields["retained_history_root"].destination,
        FieldDestination::ProofLedger
    );
    assert_eq!(
        state.module_states["large-instance"].as_ptr() as usize,
        pointer
    );
    assert_eq!(state.agents["a"].last_active, 7);
}

#[test]
fn sink_admission_failure_stops_before_binary_entries_and_later_fields() {
    let mut state = WorldState::default();
    state
        .module_states
        .insert("large-instance".into(), vec![7; 1 << 20]);
    let pointer = state.module_states["large-instance"].as_ptr() as usize;
    let mut sink = Capture {
        fail_on: Some("module_states"),
        ..Default::default()
    };
    assert!(matches!(
        state.visit_iws_genesis_fields(&mut sink),
        Err(ProjectionError::Sink("admission denied"))
    ));
    assert!(!sink.bytes.contains_key("module_states"));
    assert!(!sink.fields.contains_key("module_visual_entities"));
    assert_eq!(
        state.module_states["large-instance"].as_ptr() as usize,
        pointer
    );
}

#[test]
fn historical_side_borrow_never_serializes_payload_in_projection() {
    struct HistoricalPayload;
    impl Serialize for HistoricalPayload {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            panic!("projection must not hydrate history");
        }
    }
    let mut sink = Capture::default();
    let field = FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: "historical-original",
        destination: FieldDestination::History,
        shape: FieldShape::History,
    };
    crate::visit_iws_field!(&mut sink, field, &HistoricalPayload, side).unwrap();
    assert_eq!(sink.fields["historical-original"], field);
}

#[test]
fn empty_nested_owners_and_sets_remain_distinct_from_absent_owners() {
    let mut state = WorldState::default();
    state
        .material_ledgers
        .insert(MaterialLedgerId::world(), BTreeMap::new());
    state
        .data_access_permissions
        .entry("empty-permission".into())
        .or_default();
    state
        .authenticated_collect_data_last_nonces
        .entry("empty-nonces".into())
        .or_default();
    state
        .module_release_role_bindings
        .entry("empty-role".into())
        .or_default();
    let mut sink = Capture::default();
    state.visit_iws_genesis_fields(&mut sink).unwrap();
    for (field, key) in [
        ("material_ledgers", "world"),
        ("data_access_permissions", "empty-permission"),
        ("authenticated_collect_data_last_nonces", "empty-nonces"),
        ("module_release_role_bindings", "empty-role"),
    ] {
        assert_eq!(
            sink.groups.get(field),
            Some(&vec![(json!(key), 0)]),
            "{field}"
        );
        assert!(sink.values.get(field).is_none_or(Vec::is_empty));
    }
    assert!(
        state
            .data_access_permissions
            .contains_key("empty-permission")
    );
    assert!(
        !state
            .data_access_permissions
            .contains_key("absent-permission")
    );
}

#[test]
fn outer_group_admission_failure_precedes_members_and_preserves_source() {
    let mut state = WorldState::default();
    state
        .material_ledgers
        .entry(MaterialLedgerId::world())
        .or_default()
        .insert("ore".into(), 17);
    state
        .data_access_permissions
        .entry("owner".into())
        .or_default()
        .insert("ore".into());
    for (field, later) in [
        ("material_ledgers", "material_profiles"),
        ("data_access_permissions", "economic_contracts"),
    ] {
        let mut sink = Capture {
            fail_group: Some(field),
            ..Default::default()
        };
        assert!(matches!(
            state.visit_iws_genesis_fields(&mut sink),
            Err(ProjectionError::Sink("group admission denied"))
        ));
        assert!(!sink.values.contains_key(field));
        assert!(!sink.fields.contains_key(later));
    }
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::world()]["ore"],
        17
    );
    assert!(state.data_access_permissions["owner"].contains("ore"));
}
