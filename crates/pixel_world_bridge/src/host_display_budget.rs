//! Bound the serialized display result without changing the source world.
use serde_json::{Value, json};
use std::collections::BTreeSet;
const MAX_BYTES: usize = 256 * 1024;
const MARKERS: [(&str, &str); 6] = [
    ("agents", "agent"),
    ("locations", "location"),
    ("module_visual_entities", "module_visual"),
    ("micro_depot_facilities", "facility"),
    ("industry_aggregates", "aggregate"),
    ("world_events", "event"),
];
const DECORATIONS: [&str; 3] = ["links", "social_links", "fragment_terrain"];

pub(super) fn fit(state: &mut Value, input: &Value) -> Result<(), String> {
    if let Some(budget) = input.get("display_budget").filter(|v| v.is_object()) {
        state["display_budget"] = budget.clone();
        if let Some(count) = budget["source_counts"]["agents"]
            .as_u64()
            .filter(|_| budget["omitted_markers"].as_u64().unwrap_or(0) > 0)
        {
            state["commercial_surface"]["world_read"]["agents"] = json!(count);
        }
    }
    let count: usize = MARKERS
        .iter()
        .map(|(field, _)| state[field].as_array().map_or(0, Vec::len))
        .sum();
    if count <= 500 && serde_json::to_vec(state).map_err(|e| e.to_string())?.len() <= MAX_BYTES {
        return Ok(());
    }
    let mut essential: BTreeSet<String> = input["display_budget"]["required_entities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|key| {
            key.strip_prefix("factory:")
                .map_or_else(|| key.to_owned(), |id| format!("module_visual:{id}"))
        })
        .collect();
    if let (Some(kind), Some(id)) = (
        state["selection"]["kind"].as_str(),
        state["selection"]["id"].as_str(),
    ) {
        essential.insert(format!("{kind}:{id}"));
    }
    for field in [
        "receipt_target",
        "recommended_target",
        "active_intent_target",
    ] {
        if let Some(id) = state[field]["agent_id"].as_str() {
            essential.insert(format!("agent:{id}"));
        }
    }
    let mut entries = Vec::new();
    for (field, kind) in MARKERS {
        let values = state[field]
            .as_array_mut()
            .map(std::mem::take)
            .unwrap_or_default();
        for value in values {
            let id = value["id"].as_str().unwrap_or("");
            entries.push((
                !essential.contains(&format!("{kind}:{id}")),
                field,
                id.to_string(),
                value,
            ));
        }
    }
    // Routes and terrain are decorative. Retaining orphan route geometry after
    // removing endpoints would be misleading, so omit this layer as a whole.
    let mut omitted_decorations = 0;
    for field in DECORATIONS {
        if let Some(values) = state[field].as_array_mut() {
            omitted_decorations += values.len();
            values.clear();
        }
    }
    state["display_budget"]["output_omitted_markers"] = json!(count);
    state["display_budget"]["output_omitted_decorations"] = json!(omitted_decorations);
    state["display_budget"]["output_reason"] = json!("byte_or_marker_budget");
    let base = serde_json::to_vec(state).map_err(|e| e.to_string())?.len();
    let mut remaining = MAX_BYTES
        .checked_sub(base + 128)
        .ok_or("pixel world required display capacity exceeded")?;
    entries.sort_by(|a, b| (&a.0, a.1, &a.2).cmp(&(&b.0, b.1, &b.2)));
    let mut retained = 0;
    for (optional, field, _, value) in entries {
        let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() + 1;
        if retained < 500 && bytes <= remaining {
            state[field]
                .as_array_mut()
                .ok_or("invalid display marker array")?
                .push(value);
            remaining -= bytes;
            retained += 1;
        } else if !optional {
            return Err("pixel world required display capacity exceeded".into());
        }
    }
    state["display_budget"]["output_omitted_markers"] = json!(count - retained);
    state["display_budget"]["output_projected_markers"] = json!(retained);
    if serde_json::to_vec(state).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("pixel world display output capacity exceeded".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_clipping_preserves_navigation_factory_with_adapted_kind() {
        let mut state = json!({"agents":[],"locations":[],"module_visual_entities":[],
            "micro_depot_facilities":[],"industry_aggregates":[],"world_events":[]});
        state["module_visual_entities"] = json!(
            (0..500)
                .map(|i| json!({"id":format!("F{i:04}"),"label":"😀".repeat(128)}))
                .collect::<Vec<_>>()
        );
        let input = json!({"display_budget":{"required_entities":["factory:F0499"]}});
        fit(&mut state, &input).unwrap();
        assert!(
            state["module_visual_entities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["id"] == "F0499")
        );
        assert!(
            state["display_budget"]["output_omitted_markers"]
                .as_u64()
                .unwrap()
                > 0
        );
    }
}
