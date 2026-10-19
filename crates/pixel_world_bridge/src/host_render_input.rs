//! Bounded, versioned display input. The world model never crosses this ABI.
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    let state = super::host_state::build_render_state(&input);
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
