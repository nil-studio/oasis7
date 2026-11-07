//! Adapt a bounded authorized projection, never a replacement Runtime model.
use serde_json::{Value, json};

fn exact_display(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| {
                    if item.is_object() || item.is_array() {
                        exact_display(item)
                    } else {
                        item.clone()
                    }
                })
                .collect(),
        ),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| {
                    let numeric = matches!(
                        k.as_str(),
                        "x_cm"
                            | "y_cm"
                            | "z_cm"
                            | "width_cm"
                            | "depth_cm"
                            | "height_cm"
                            | "radius_cm"
                    );
                    let converted = if numeric {
                        exact_display(v)
                    } else if v.is_object() || v.is_array() {
                        exact_display(v)
                    } else {
                        v.clone()
                    };
                    (k.clone(), converted)
                })
                .collect(),
        ),
        Value::String(text)
            if !text.is_empty() && text.bytes().all(|c| c.is_ascii_digit() || c == b'-') =>
        {
            text.parse::<i64>()
                .ok()
                .filter(|n| n.unsigned_abs() <= 9_007_199_254_740_991)
                .map(Value::from)
                .unwrap_or_else(|| value.clone())
        }
        _ => value.clone(),
    }
}

pub(super) fn adapt_input(input: &Value) -> Option<Value> {
    let view = input.get("worldView")?.as_object()?;
    let mut adapted = input.clone();
    let entities = view.get("entities").and_then(Value::as_array);
    let mut agents = Vec::new();
    let mut locations = Vec::new();
    let mut facilities = serde_json::Map::new();
    for entity in entities.into_iter().flatten() {
        let Some(record) = entity.get("render_record").filter(|r| r.is_object()) else {
            continue;
        };
        let mut record = exact_display(record);
        // Identity is an opaque string, including numeric-looking IDs.
        record["id"] = entity.get("entity_id").cloned().unwrap_or(Value::Null);
        let provenance = entity.get("provenance").unwrap_or(&Value::Null);
        let current = matches!(
            provenance.get("geometry_source").and_then(Value::as_str),
            Some("authoritative" | "location_derived")
        ) && provenance.get("freshness").and_then(Value::as_str) == Some("current")
            && provenance.get("valid_at_source_cut") == view.get("source_cut")
            && !input
                .get("worldViewStale")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        if !current {
            record["pos"] = Value::Null;
            record["location_id"] = Value::Null;
            record["fragment_profile"] = Value::Null;
        }
        match entity.get("kind").and_then(Value::as_str) {
            Some("agent") => agents.push(record),
            Some("location") => locations.push(record),
            Some("factory") if current => {
                let id = entity.get("entity_id").and_then(Value::as_str);
                if let Some(id) = id {
                    let anchor = if record["pos"].is_object() {
                        json!({"type":"absolute","data":{"pos":record["pos"]}})
                    } else {
                        json!({"type":"location","data":{"location_id":record["location_id"]}})
                    };
                    facilities.insert(id.into(), json!({"entity_id":id,"module_id":"runtime_factory",
                        "kind":"factory","factory_kind":record.get("kind").and_then(Value::as_str),"label":record.get("name").or_else(||record.get("label")),"anchor":anchor}));
                }
            }
            _ => (),
        }
    }
    adapted["lists"] = json!({"agents":agents,"locations":locations});
    // A selected object's old snapshot position cannot override the projection.
    adapted["selected"] = Value::Null;
    if adapted["selectedKind"] == "factory" {
        adapted["selectedKind"] = json!("module_visual");
    }
    if adapted["navigation"]["target"]["kind"] == "factory" {
        adapted["navigation"]["target"]["kind"] = json!("module_visual");
        // A fixed site anchor supports locating a facility, not live tracking.
        if adapted["navigation"]["mode"] == "follow" {
            adapted["navigation"]["mode"] = json!("stop");
        }
    }
    // Module/facility/event overlays do not inherit the spatial projection's grant.
    adapted["snapshot"]["model"]["module_visual_entities"] = Value::Object(facilities);
    adapted["recentEvents"] = json!([]);
    if let Some(bounds) = view
        .get("world_bounds")
        .filter(|v| v.get("width_cm").is_some())
    {
        adapted["snapshot"]["config"]["space"] = exact_display(bounds);
    }
    Some(adapted)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_preserves_receipt_and_world_basis_but_suppresses_unknown_positions() {
        let input = json!({"snapshot":{"config":{"space":{"width_cm":100,"depth_cm":200,"height_cm":20}},"model":{}},"gameplay":{"actionReceipt":{"id":"original"}},"selected":{"id":"A","pos":{"x_cm":10,"y_cm":20,"z_cm":0}},"worldView":{"source_cut":"cut","entities":[{"entity_id":"A","kind":"agent","render_record":{"id":"A","pos":{"x_cm":"10","y_cm":"20","z_cm":"0"}},"provenance":{"freshness":"unknown"}}]}});
        let result = adapt_input(&input).unwrap();
        assert_eq!(result["gameplay"], input["gameplay"]);
        assert_eq!(result["snapshot"]["config"], input["snapshot"]["config"]);
        assert!(result["lists"]["agents"][0]["pos"].is_null());
        assert!(result["selected"].is_null());
    }
    #[test]
    fn authorized_factory_maps_to_existing_marker_and_navigation_without_snapshot_overlay_leaks() {
        let mut input = json!({"selectedKind":"factory","selectedId":"F","navigation":{"request_id":"1","mode":"locate","target":{"kind":"factory","id":"F"}},"snapshot":{"model":{"module_visual_entities":{"private":{}}}},"worldView":{"source_cut":"C","entities":[{"entity_id":"F","kind":"factory","render_record":{"name":"Forge","pos":{"x_cm":"10","y_cm":"20","z_cm":"0"}},"provenance":{"geometry_source":"authoritative","freshness":"current","valid_at_source_cut":"C"}}]}});
        let result = adapt_input(&input).unwrap();
        assert_eq!(result["selectedKind"], "module_visual");
        assert_eq!(result["navigation"]["target"]["kind"], "module_visual");
        let facilities = &result["snapshot"]["model"]["module_visual_entities"];
        assert_eq!(facilities["F"]["anchor"]["data"]["pos"]["x_cm"], 10);
        assert!(facilities.get("private").is_none());
        input["navigation"]["mode"] = json!("follow");
        assert_eq!(adapt_input(&input).unwrap()["navigation"]["mode"], "stop");
        input["worldView"]["entities"][0]["provenance"]["freshness"] = json!("expired");
        assert!(
            adapt_input(&input).unwrap()["snapshot"]["model"]["module_visual_entities"]
                .as_object()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn factory_type_is_explicit_and_never_inferred_from_label() {
        let mut input = json!({"worldView":{"source_cut":"C","entities":[{"entity_id":"F","kind":"factory","render_record":{"name":"Smelter Mk1","kind":"factory.smelter.mk1","pos":{"x_cm":10,"y_cm":20,"z_cm":0}},"provenance":{"geometry_source":"authoritative","freshness":"current","valid_at_source_cut":"C"}}]}});
        let adapted = adapt_input(&input).unwrap();
        let factory = &adapted["snapshot"]["model"]["module_visual_entities"]["F"];
        assert_eq!(factory["kind"], "factory");
        assert_eq!(factory["factory_kind"], "factory.smelter.mk1");
        let state = super::super::build_render_state(&input);
        let entities: Vec<crate::ModuleVisualEntity> =
            serde_json::from_value(state["module_visual_entities"].clone()).unwrap();
        assert_eq!(
            entities[0].factory_kind.as_deref(),
            Some("factory.smelter.mk1")
        );
        assert_eq!(entities[0].kind, "factory");

        input["worldView"]["entities"][0]["render_record"]
            .as_object_mut()
            .unwrap()
            .remove("kind");
        assert!(adapt_input(&input).unwrap()["snapshot"]["model"]["module_visual_entities"]["F"]["factory_kind"].is_null());
        input["worldView"]["entities"][0]["render_record"]["kind"] = json!("unsupported");
        assert_eq!(
            adapt_input(&input).unwrap()["snapshot"]["model"]["module_visual_entities"]["F"]["factory_kind"],
            "unsupported"
        );
        input["worldView"]["entities"][0]["provenance"]["freshness"] = json!("expired");
        assert!(
            adapt_input(&input).unwrap()["snapshot"]["model"]["module_visual_entities"]
                .as_object()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn current_positions_require_the_same_cut() {
        let input = json!({"snapshot":{"config":{},"model":{}},"worldView":{"source_cut":"cut","entities":[{"entity_id":"001","kind":"agent","render_record":{"id":"001","pos":{"x_cm":"10","y_cm":"20","z_cm":"0"}},"provenance":{"geometry_source":"authoritative","freshness":"current","valid_at_source_cut":"cut"}}]}});
        let result = adapt_input(&input).unwrap();
        assert_eq!(result["lists"]["agents"][0]["id"], "001");
        assert_eq!(result["lists"]["agents"][0]["pos"]["x_cm"], 10);
    }
}
