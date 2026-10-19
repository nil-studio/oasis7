//! Bounded, versioned display input. The world model never crosses this ABI.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[path = "host_display_budget.rs"]
mod budget;

const MAX_BYTES: usize = 256 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RenderInputV1 {
    #[serde(rename = "schema_version")]
    schema_version: u32,
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
    if raw.len() > MAX_BYTES {
        return Err("pixel world display capacity exceeded".into());
    }
    let dto: RenderInputV1 =
        serde_json::from_str(raw).map_err(|error| format!("invalid render input: {error}"))?;
    if dto.schema_version != 1 {
        return Err("unsupported pixel world render input version".into());
    }
    if dto.lists.agents.len() + dto.lists.locations.len() > 500 {
        return Err("pixel world display capacity exceeded".into());
    }
    let input = serde_json::to_value(dto).map_err(|error| error.to_string())?;
    let mut state = super::host_state::build_render_state(&input);
    budget::fit(&mut state, &input)?;
    let encoded = serde_json::to_vec(&state).map_err(|error| error.to_string())?;
    if encoded.len() > MAX_BYTES {
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
