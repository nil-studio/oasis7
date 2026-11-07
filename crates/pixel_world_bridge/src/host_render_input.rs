//! Bounded, versioned display input. The world model never crosses this ABI.
use super::display_profile::DisplayProfile;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[path = "host_display_budget.rs"]
mod budget;

#[cfg(test)]
const MAX_BYTES: usize = 256 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RenderInputV1 {
    #[serde(rename = "schema_version")]
    schema_version: u32,
    #[serde(default, rename = "display_profile")]
    display_profile: DisplayProfile,
    #[serde(default)]
    locale: Option<String>,
    snapshot: DisplaySnapshot,
    lists: DisplayLists,
    gameplay: Value,
    #[serde(
        default,
        rename = "display_budget",
        skip_serializing_if = "Option::is_none"
    )]
    display_budget: Option<Value>,
    #[serde(default)]
    selected: Option<Value>,
    #[serde(default)]
    selected_kind: Option<String>,
    #[serde(default)]
    selected_id: Option<String>,
    #[serde(default)]
    presentation: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    world_view: Option<Value>,
    #[serde(default)]
    world_view_stale: Option<bool>,
    #[serde(default)]
    navigation: Option<Value>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DisplaySnapshot {
    #[serde(default)]
    time: Option<Value>,
    #[serde(default)]
    config: Option<Value>,
    model: DisplayModel,
    #[serde(default)]
    player_gameplay: Option<Value>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DisplayModel {
    #[serde(default)]
    module_visual_entities: Option<Value>,
    #[serde(default)]
    social_edges: Option<Value>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DisplayLists {
    agents: Vec<Value>,
    locations: Vec<Value>,
}

pub(super) fn derive(raw: &str) -> Result<Value, String> {
    if raw.len() > DisplayProfile::WorldView.bytes() {
        return Err("pixel world display capacity exceeded".into());
    }
    let dto: RenderInputV1 =
        serde_json::from_str(raw).map_err(|error| format!("invalid render input: {error}"))?;
    if dto.schema_version != 1 {
        return Err("unsupported pixel world render input version".into());
    }
    let profile = dto.display_profile;
    if raw.len() > profile.bytes()
        || (profile == DisplayProfile::WorldView
            && !dto.world_view.as_ref().is_some_and(Value::is_object))
    {
        return Err("pixel world display profile mismatch or capacity exceeded".into());
    }
    if dto.lists.agents.len() + dto.lists.locations.len() > profile.markers() {
        return Err("pixel world display capacity exceeded".into());
    }
    if let Some(view) = dto.world_view.as_ref() {
        let mut items = 0;
        for field in ["entities", "aggregates", "events", "relations"] {
            if let Some(values) = view.get(field) {
                items += values
                    .as_array()
                    .ok_or("invalid world view collection")?
                    .len();
            }
        }
        if items > 1000 {
            return Err("pixel world world-view capacity exceeded".into());
        }
    }
    let input = serde_json::to_value(dto).map_err(|error| error.to_string())?;
    profile.validate_structure(&input)?;
    let mut state = super::host_state::build_render_state(&input);
    if profile == DisplayProfile::WorldView {
        state["display_profile"] =
            serde_json::to_value(profile).map_err(|error| error.to_string())?;
    }
    if let Some(cut) = input["worldView"]["source_cut"].as_str() {
        state["display_source_cut"] = Value::String(cut.to_owned());
    }
    budget::fit(&mut state, &input)?;
    profile.validate_structure(&state)?;
    let encoded = serde_json::to_vec(&state).map_err(|error| error.to_string())?;
    if encoded.len() > profile.bytes() {
        return Err("pixel world display output capacity exceeded".into());
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input() -> Value {
        json!({"schema_version":1,"locale":"en","snapshot":{"time":5,"model":{},
            "config":{"space":{"width_cm":100,"depth_cm":100,"height_cm":10}}},
            "lists":{"agents":[],"locations":[]},"gameplay":{}})
    }
    fn world_input(count: usize) -> Value {
        let mut input = input();
        input["worldView"] = json!({"schema_version":1,"world_id":"W","scope_id":"S","source_cut":"C",
            "entities":(0..count).map(|i|json!({"kind":"agent","entity_id":format!("A{i:04}"),
                "render_record":{"name":"界".repeat(128),"pos":{"x_cm":i,"y_cm":0,"z_cm":0}},
                "provenance":{"geometry_source":"authoritative","freshness":"current","valid_at_source_cut":"C"}})).collect::<Vec<_>>()});
        input
    }
    #[test]
    fn absent_goal_facts_never_create_a_spatial_goal_from_onboarding_copy() {
        let mut source = world_input(1000);
        source["display_profile"] = json!("world_view_v1");
        for gameplay in [
            Value::Null,
            json!({}),
            json!({"receiptTargetAgentId":null}),
            json!({"goalTitle":"  "}),
            json!({"goalKind":"unknown"}),
        ] {
            source["gameplay"] = gameplay;
            let state = derive(&source.to_string()).unwrap();
            assert!(state["goal_highlight"].is_null());
            assert!(
                !state["visual_hotspots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|hotspot| hotspot["kind"] == "goal")
            );
            assert_eq!(state["agents"].as_array().unwrap().len(), 1000);
            assert!(
                state["commercial_surface"]["objective"]["title"].is_string(),
                "ordinary onboarding guidance remains available"
            );
        }
        for gameplay in [
            json!({"goalTitle":"Published goal"}),
            json!({"goalKind":"RecoverCapability"}),
            json!({"goalKind":"future-kind","goalTitle":"Published future goal"}),
        ] {
            source["gameplay"] = gameplay;
            let state = derive(&source.to_string()).unwrap();
            assert!(state["goal_highlight"]["title"].is_string());
            assert!(
                state["visual_hotspots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|hotspot| hotspot["kind"] == "goal")
            );
        }
    }
    #[test]
    fn explicit_world_profile_keeps_1000_records_and_old_world_inputs_keep_legacy_budget() {
        let mut input = world_input(1000);
        input["display_profile"] = json!("world_view_v1");
        assert!(input.to_string().len() > 256 * 1024);
        let state = derive(&input.to_string()).unwrap();
        assert_eq!(state["agents"].as_array().unwrap().len(), 1000);
        assert_eq!(state["display_profile"], "world_view_v1");
        assert!(state.to_string().len() <= 2 * 1024 * 1024);
        let mut old = world_input(500);
        // Older V1 JS sent worldView without any display_profile field.
        for entity in old["worldView"]["entities"].as_array_mut().unwrap() {
            entity["render_record"]["name"] = json!("Actor");
        }
        let state = derive(&old.to_string()).expect("old worldView V1 stays compatible");
        assert!(state["agents"].as_array().unwrap().len() <= 500);
        assert!(state.to_string().len() <= 256 * 1024);
        assert!(state.get("display_profile").is_none());
    }
    #[test]
    fn rejects_unknown_profile_missing_view_and_combined_query_overflow() {
        for invalid in [Value::Null, json!("arbitrary"), json!(1000)] {
            let mut source = input();
            source["display_profile"] = invalid;
            assert!(derive(&source.to_string()).is_err());
        }
        let mut source = input();
        source["display_profile"] = json!("world_view_v1");
        assert!(derive(&source.to_string()).is_err());
        source["worldView"] = json!([]);
        assert!(derive(&source.to_string()).is_err());
        source = world_input(1000);
        source["display_profile"] = json!("world_view_v1");
        source["worldView"]["relations"] = json!([{"relation_id":"R"}]);
        assert!(derive(&source.to_string()).is_err());
    }
    #[test]
    fn derived_output_is_independently_checked_after_input_passes() {
        let mut source = input();
        // The published kind fits the input string boundary, but its derived
        // "kind=" status badge does not. Byte limits alone cannot catch this.
        source["lists"]["agents"] = json!([{"id":"A","kind":"x".repeat(8192),
            "pos":{"x_cm":1,"y_cm":1,"z_cm":0}}]);
        source["selectedKind"] = json!("agent");
        source["selectedId"] = json!("A");
        DisplayProfile::Legacy.validate_structure(&source).unwrap();
        let raw_state = super::super::host_state::build_render_state(&source);
        assert!(serde_json::to_vec(&raw_state).unwrap().len() < 256 * 1024);
        assert!(
            DisplayProfile::Legacy
                .validate_structure(&raw_state)
                .is_err()
        );
        assert!(derive(&source.to_string()).is_err());
        source["selectedKind"] = Value::Null;
        source["selectedId"] = Value::Null;
        let output = derive(&source.to_string()).expect("invalid optional marker is omitted");
        assert!(output["agents"].as_array().unwrap().is_empty());
        assert_eq!(output["display_budget"]["output_omitted_markers"], 1);
        DisplayProfile::Legacy.validate_structure(&output).unwrap();
    }
    #[test]
    fn bounded_json_preserves_the_existing_render_projection() {
        let input = input();
        assert_eq!(
            derive(&input.to_string()).unwrap(),
            super::super::host_state::build_render_state(&input)
        );
    }
    #[test]
    fn output_budget_keeps_selected_actor_and_bounds_rich_labels() {
        let mut source = input();
        source["lists"]["agents"] = json!((0..350).map(|i|json!({"id":format!("A{i:04}"),"name":"😀".repeat(128),"pos":{"x_cm":i,"y_cm":0,"z_cm":0}})).collect::<Vec<_>>());
        source["selectedKind"] = json!("agent");
        source["selectedId"] = json!("A0349");
        let state = derive(&source.to_string())
            .expect("optional markers must shrink before required semantics fail");
        assert!(
            state["agents"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == "A0349")
        );
        assert!(
            state["display_budget"]["output_omitted_markers"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(serde_json::to_vec(&state).unwrap().len() <= MAX_BYTES);
    }
    #[test]
    fn accepts_javascript_budget_field_and_preserves_authorized_source_count() {
        let mut source = input();
        source["display_budget"] =
            json!({"source_counts":{"agents":9998},"omitted_markers":9500,"required_entities":[]});
        let state = derive(&source.to_string()).unwrap();
        assert_eq!(state["display_budget"]["source_counts"]["agents"], 9998);
        assert_eq!(state["commercial_surface"]["world_read"]["agents"], 9998);
    }
    #[test]
    fn rejects_unknown_versions_private_model_fields_and_capacity_overflow() {
        let mut input = input();
        input["schema_version"] = json!(2);
        assert!(derive(&input.to_string()).is_err());
        input["schema_version"] = json!(1);
        input["snapshot"]["model"]["agent_genomes"] = json!({});
        assert!(derive(&input.to_string()).is_err());
        assert!(derive(&" ".repeat(MAX_BYTES + 1)).is_err());
    }
}
