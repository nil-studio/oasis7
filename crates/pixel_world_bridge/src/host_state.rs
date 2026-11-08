use serde_json::{Map, Value, json};

#[path = "host_action_receipt_support.rs"]
mod action_receipt_support;
#[path = "host_intent_projection.rs"]
mod intent_projection;
#[path = "host_micro_depot_projection.rs"]
mod micro_depot_projection;
#[path = "host_relation_projection.rs"]
mod relation_projection;
#[path = "host_resource_summary.rs"]
mod resource_summary_projection;
#[path = "host_social_links.rs"]
mod social_links;
#[path = "host_spatial_overlays.rs"]
mod spatial_overlays;
#[path = "host_spatial_routes.rs"]
mod spatial_routes;
#[path = "host_world_bounds.rs"]
mod world_bounds;
#[path = "host_world_view.rs"]
mod world_view;

use action_receipt_support::{
    action_receipt_title, has_enabled_first_agent_claim, is_authoritative_world_delta_stage,
    is_non_advancing_receipt_stage, is_pending_receipt_stage, is_rejected_receipt_stage,
    rejected_receipt_detail,
};
use resource_summary_projection::{count_resource_entries, resource_summary};
use world_bounds::build_world_bounds;

const FRAGMENT_TERRAIN_PALETTE: &[(&str, [u8; 3])] = &[
    ("silicate_matrix", [126, 144, 99]),
    ("iron_nickel_alloy", [176, 184, 196]),
    ("water_ice", [125, 211, 252]),
    ("hydrated_mineral", [96, 165, 250]),
    ("carbonaceous_organic", [120, 113, 108]),
    ("sulfide_ore", [202, 138, 4]),
    ("rare_earth_oxide", [167, 139, 250]),
    ("uranium_bearing_ore", [132, 204, 22]),
    ("thorium_bearing_ore", [244, 114, 182]),
    ("unknown", [148, 163, 184]),
];

fn tr(locale: &str, zh: &str, en: &str) -> String {
    if locale.to_ascii_lowercase().starts_with("zh") {
        zh.to_string()
    } else {
        en.to_string()
    }
}

fn is_zh_locale(locale: &str) -> bool {
    locale.to_ascii_lowercase().starts_with("zh")
}

fn contains_cjk(value: &str) -> bool {
    value
        .chars()
        .any(|ch| ('\u{3400}'..='\u{9fff}').contains(&ch))
}

fn normalize_gameplay_token(value: Option<&str>) -> String {
    value
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '_' && *ch != '-')
        .collect()
}

fn zh_or_published(
    locale: &str,
    published: Option<&str>,
    zh_fallback: &str,
    en_fallback: &str,
) -> String {
    if !is_zh_locale(locale) {
        return published.unwrap_or(en_fallback).to_string();
    }
    if let Some(published) = published
        && contains_cjk(published)
    {
        return published.to_string();
    }
    zh_fallback.to_string()
}

fn obj<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}

fn arr<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    obj(value, key).as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn str_value(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.is_empty())
}

fn str_key<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    str_value(obj(value, key))
}

fn string_key(value: &Value, key: &str) -> Option<String> {
    str_key(value, key).map(ToString::to_string)
}

fn number(value: &Value, fallback: f64) -> f64 {
    value.as_f64().filter(|n| n.is_finite()).unwrap_or(fallback)
}

fn number_key(value: &Value, key: &str, fallback: f64) -> f64 {
    number(obj(value, key), fallback)
}

fn normalize_position(value: &Value) -> Option<Value> {
    let x = number_key(value, "x_cm", f64::NAN);
    let y = number_key(value, "y_cm", f64::NAN);
    let z = number_key(value, "z_cm", f64::NAN);
    if x.is_finite() && y.is_finite() && z.is_finite() {
        Some(json!({ "x_cm": x, "y_cm": y, "z_cm": z }))
    } else {
        None
    }
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.min(max).max(min)
}

fn clamp_world_position(pos: &Value, world_bounds: &Value) -> Option<Value> {
    if !pos.is_object() || !world_bounds.is_object() {
        return None;
    }
    Some(json!({
        "x_cm": clamp(number_key(pos, "x_cm", 0.0), 0.0, number_key(world_bounds, "width_cm", 0.0)),
        "y_cm": clamp(number_key(pos, "y_cm", 0.0), 0.0, number_key(world_bounds, "depth_cm", 0.0)),
        "z_cm": clamp(number_key(pos, "z_cm", 0.0), 0.0, number_key(world_bounds, "height_cm", 0.0)),
    }))
}

fn world_center_position(world_bounds: &Value) -> Option<Value> {
    if !world_bounds.is_object() {
        return None;
    }
    Some(json!({
        "x_cm": number_key(world_bounds, "width_cm", 0.0) / 2.0,
        "y_cm": number_key(world_bounds, "depth_cm", 0.0) / 2.0,
        "z_cm": number_key(world_bounds, "height_cm", 0.0) / 2.0,
    }))
}

fn agent_power_state(agent: &Value) -> Option<&'static str> {
    match str_key(obj(agent, "power"), "state") {
        Some("normal") => Some("normal"),
        Some("low_power") => Some("low_power"),
        Some("critical") => Some("critical"),
        Some("shutdown") => Some("shutdown"),
        _ => None,
    }
}

fn fragment_blocks(location: &Value) -> &[Value] {
    obj(obj(obj(location, "fragment_profile"), "blocks"), "blocks")
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn dominant_compound(block: &Value) -> String {
    let Some(ppm) = obj(obj(block, "compounds"), "ppm").as_object() else {
        return "unknown".to_string();
    };
    let mut ranked: Vec<(&String, f64)> = ppm
        .iter()
        .map(|(kind, value)| (kind, number(value, 0.0)))
        .filter(|(_, value)| *value > 0.0)
        .collect();
    ranked.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(right.0))
    });
    ranked
        .first()
        .map(|(kind, _)| (*kind).clone())
        .unwrap_or_else(|| "unknown".to_string())
}

fn fragment_terrain_color(compound: &str) -> [u8; 3] {
    FRAGMENT_TERRAIN_PALETTE
        .iter()
        .find(|(kind, _)| *kind == compound)
        .map(|(_, color)| *color)
        .unwrap_or([148, 163, 184])
}

fn estimate_fragment_half_extent_cm(location: &Value, blocks: &[Value]) -> f64 {
    let explicit_radius = number_key(obj(location, "profile"), "radius_cm", 0.0);
    if explicit_radius > 0.0 {
        return explicit_radius;
    }
    let max_extent = blocks.iter().fold(0.0_f64, |value, block| {
        let origin = obj(block, "origin_cm");
        let size = obj(block, "size_cm");
        let origin_x = number_key(origin, "x_cm", 0.0);
        let origin_z = number_key(origin, "z_cm", number_key(origin, "y_cm", 0.0));
        let size_x = number_key(size, "x_cm", 0.0);
        let size_z = number_key(size, "z_cm", number_key(size, "y_cm", 0.0));
        value.max(origin_x + size_x).max(origin_z + size_z)
    });
    max_extent.max(2.0) / 2.0
}

fn build_fragment_terrain_for_location(location: &Value, world_bounds: &Value) -> Vec<Value> {
    let Some(pos) = normalize_position(obj(location, "pos")) else {
        return Vec::new();
    };
    let blocks = fragment_blocks(location);
    if !world_bounds.is_object() || blocks.is_empty() {
        return Vec::new();
    }
    let half_extent_cm = estimate_fragment_half_extent_cm(location, blocks);
    blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            let size = obj(block, "size_cm");
            let origin = obj(block, "origin_cm");
            let size_x = number_key(size, "x_cm", 0.0);
            let size_z = number_key(size, "z_cm", number_key(size, "y_cm", 0.0));
            if size_x <= 0.0 || size_z <= 0.0 {
                return None;
            }
            let origin_x = number_key(origin, "x_cm", 0.0);
            let origin_z = number_key(origin, "z_cm", number_key(origin, "y_cm", 0.0));
            let dominant = dominant_compound(block);
            let local_x = origin_x + (size_x / 2.0) - half_extent_cm;
            let local_y = origin_z + (size_z / 2.0) - half_extent_cm;
            let terrain_pos = clamp_world_position(
                &json!({
                    "x_cm": number_key(&pos, "x_cm", 0.0) + local_x,
                    "y_cm": number_key(&pos, "y_cm", 0.0) + local_y,
                    "z_cm": number_key(&pos, "z_cm", 0.0),
                }),
                world_bounds,
            )?;
            Some(json!({
                "id": format!("fragment:{}:{index}", str_key(location, "id").unwrap_or("")),
                "location_id": str_key(location, "id").unwrap_or(""),
                "pos": terrain_pos,
                "footprint_cm": size_x.max(size_z).max(1.0),
                "dominant_compound": dominant,
                "color": fragment_terrain_color(&dominant),
                "emphasis": 0.58,
            }))
        })
        .collect()
}

fn deterministic_hash(input: &str) -> u32 {
    input.chars().fold(2_166_136_261_u32, |hash, ch| {
        hash.wrapping_mul(31).wrapping_add(ch as u32)
    })
}

fn build_micro_depot_facilities(
    gameplay: &Value,
    location_by_id: &Map<String, Value>,
    world_bounds: &Value,
) -> Vec<Value> {
    // The raw runtime snapshot is snake_case, while `buildGameplaySummary`
    // normalizes it for the viewer as camelCase. An explicitly published snake
    // array, including `[]`, remains authoritative over the compatibility form.
    let facilities = match gameplay.get("micro_depot_facilities") {
        Some(Value::Array(facilities)) => facilities.as_slice(),
        _ => arr(gameplay, "microDepotFacilities"),
    };
    let mut facilities: Vec<_> = facilities
        .iter()
        .filter_map(|facility| {
            let facility_id =
                str_key(facility, "facility_id").or_else(|| str_key(facility, "facilityId"))?;
            let location_id =
                str_key(facility, "location_id").or_else(|| str_key(facility, "locationId"))?;
            let status = str_key(facility, "status")?;
            let location = location_by_id.get(location_id)?;
            let anchor = normalize_position(obj(location, "pos"))?;
            let hash = deterministic_hash(&format!("{facility_id}:{location_id}"));
            let angle = ((hash % 360) as f64).to_radians();
            let radius_hint = number_key(location, "radius_cm", 0.0);
            let offset_cm = radius_hint.clamp(12_000.0, 60_000.0);
            let pos = clamp_world_position(
                &json!({
                    "x_cm": number_key(&anchor, "x_cm", 0.0) + angle.cos() * offset_cm,
                    "y_cm": number_key(&anchor, "y_cm", 0.0) + angle.sin() * offset_cm,
                    "z_cm": number_key(&anchor, "z_cm", 0.0),
                }),
                world_bounds,
            )?;
            let mut projection = json!({
                "id": format!("micro_depot:{facility_id}"),
                "facility_id": facility_id,
                "location_id": location_id,
                "status": status,
                "pos": pos,
                "service_radius_cm": number_key(
                    facility,
                    "service_radius_cm",
                    number_key(facility, "serviceRadiusCm", 0.0),
                ),
            });
            projection
                .as_object_mut()
                .expect("micro-depot projection object")
                .extend(micro_depot_projection::fields(facility));
            Some(projection)
        })
        .collect();
    facilities.sort_by(|left, right| str_key(left, "id").cmp(&str_key(right, "id")));
    facilities
}

fn derive_agent_position(
    agent: &Value,
    location_by_id: &Map<String, Value>,
    world_bounds: &Value,
) -> Option<Value> {
    let location_id = str_key(agent, "location_id")?;
    let location = location_by_id.get(location_id)?;
    let location_pos = obj(location, "pos");
    if !location_pos.is_object() || !world_bounds.is_object() {
        return None;
    }
    let hash = deterministic_hash(&format!(
        "{}:{location_id}",
        str_key(agent, "id").unwrap_or("")
    ));
    let angle = ((hash % 360) as f64).to_radians();
    let max_dim =
        number_key(world_bounds, "width_cm", 0.0).max(number_key(world_bounds, "depth_cm", 0.0));
    let explicit_radius = number_key(location, "radius_cm", 0.0);
    let radius_hint = if explicit_radius > 0.0 {
        explicit_radius
    } else {
        35_000.0
    };
    let radius_cm = 10_000.0_f64.max((max_dim * 0.015).min(radius_hint));
    clamp_world_position(
        &json!({
            "x_cm": number_key(location_pos, "x_cm", 0.0) + angle.cos() * radius_cm,
            "y_cm": number_key(location_pos, "y_cm", 0.0) + angle.sin() * radius_cm,
            "z_cm": number_key(location_pos, "z_cm", 0.0),
        }),
        world_bounds,
    )
}

fn resolve_agent_position(
    agent: &Value,
    selected: &Value,
    location_by_id: &Map<String, Value>,
    world_bounds: &Value,
) -> (Value, &'static str) {
    let selected_pos = if str_key(selected, "id") == str_key(agent, "id") {
        obj(selected, "pos")
    } else {
        &Value::Null
    };
    if let Some(pos) =
        normalize_position(obj(agent, "pos")).or_else(|| normalize_position(selected_pos))
    {
        return (pos, "snapshot");
    }
    if let Some(pos) = derive_agent_position(agent, location_by_id, world_bounds) {
        return (pos, "location_derived");
    }
    (Value::Null, "missing")
}

fn resolve_selection_position(
    selection: &Value,
    agents: &[Value],
    locations: &[Value],
    module_visual_entities: &[Value],
) -> Option<Value> {
    let kind = str_key(selection, "kind")?;
    let id = str_key(selection, "id")?;
    match kind {
        "agent" => agents
            .iter()
            .find(|agent| str_key(agent, "id") == Some(id))
            .and_then(|agent| normalize_position(obj(agent, "pos"))),
        "location" => locations
            .iter()
            .find(|location| str_key(location, "id") == Some(id))
            .and_then(|location| normalize_position(obj(location, "pos"))),
        "module_visual" => module_visual_entities
            .iter()
            .find(|entity| str_key(entity, "id") == Some(id))
            .and_then(|entity| normalize_position(obj(entity, "pos"))),
        _ => None,
    }
}

fn build_module_visual_entities(
    input: &Value,
    locations: &[Value],
    agents: &[Value],
) -> Vec<Value> {
    let Some(entities) = obj(obj(input, "snapshot"), "model")
        .get("module_visual_entities")
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };

    let mut projected = entities
        .values()
        .filter_map(|entity| {
            let id = str_key(entity, "entity_id")?.to_string();
            let anchor = obj(entity, "anchor");
            let anchor_data = obj(anchor, "data");
            let pos = match str_key(anchor, "type")? {
                "absolute" => normalize_position(obj(anchor_data, "pos")),
                "location" => {
                    let location_id = str_key(anchor_data, "location_id")?;
                    locations
                        .iter()
                        .find(|location| str_key(location, "id") == Some(location_id))
                        .and_then(|location| normalize_position(obj(location, "pos")))
                }
                "agent" => {
                    let agent_id = str_key(anchor_data, "agent_id")?;
                    agents
                        .iter()
                        .find(|agent| str_key(agent, "id") == Some(agent_id))
                        .and_then(|agent| normalize_position(obj(agent, "pos")))
                }
                _ => None,
            }?;
            Some(json!({
                "id": id,
                "module_id": str_key(entity, "module_id").unwrap_or(""),
                "kind": str_key(entity, "kind").unwrap_or(""),
                "factory_kind": string_key(entity, "factory_kind"),
                "label": string_key(entity, "label"),
                "anchor": anchor.clone(),
                "pos": pos,
            }))
        })
        .collect::<Vec<_>>();
    projected.sort_by(|left, right| str_key(left, "id").cmp(&str_key(right, "id")));
    projected
}

fn offset_world_position(
    anchor: Option<Value>,
    world_bounds: &Value,
    x_ratio: f64,
    y_ratio: f64,
) -> Option<Value> {
    if !world_bounds.is_object() {
        return None;
    }
    let base = anchor.or_else(|| world_center_position(world_bounds))?;
    clamp_world_position(
        &json!({
            "x_cm": number_key(&base, "x_cm", 0.0) + number_key(world_bounds, "width_cm", 0.0) * x_ratio,
            "y_cm": number_key(&base, "y_cm", 0.0) + number_key(world_bounds, "depth_cm", 0.0) * y_ratio,
            "z_cm": number_key(&base, "z_cm", 0.0),
        }),
        world_bounds,
    )
}

fn build_visual_hotspots(
    world_bounds: &Value,
    anchor: Option<Value>,
    goal_highlight: &Value,
    blocker_highlight: &Value,
    recent_event_hotspots: &[Value],
) -> Vec<Value> {
    if !world_bounds.is_object() {
        return Vec::new();
    }
    let offsets = [
        (-0.18, -0.14),
        (0.18, -0.12),
        (0.22, 0.14),
        (-0.2, 0.16),
        (0.0, -0.22),
        (0.0, 0.22),
    ];
    let mut staged = Vec::new();
    if let Some(title) = str_key(goal_highlight, "title") {
        staged.push(json!({
            "id": "goal-highlight",
            "label": title,
            "kind": "goal",
            "emphasis": 1.0,
            "size_hint_px": 14.0,
        }));
    }
    if let Some(kind) = str_key(blocker_highlight, "kind") {
        staged.push(json!({
            "id": "blocker-highlight",
            "label": str_key(blocker_highlight, "label").unwrap_or(kind),
            "kind": "blocker",
            "emphasis": 1.0,
            "size_hint_px": 16.0,
        }));
    }
    for hotspot in recent_event_hotspots.iter().take(4) {
        staged.push(json!({
            "id": format!("recent:{}", str_key(hotspot, "id").unwrap_or("")),
            "label": str_key(hotspot, "title").unwrap_or(""),
            "kind": str_key(hotspot, "kind").unwrap_or("recent_event"),
            "emphasis": 0.72,
            "size_hint_px": 10.0,
        }));
    }
    staged
        .into_iter()
        .enumerate()
        .filter_map(|(index, mut entry)| {
            let (x_ratio, y_ratio) = offsets[index % offsets.len()];
            let pos = offset_world_position(anchor.clone(), world_bounds, x_ratio, y_ratio)?;
            entry.as_object_mut()?.insert("pos".to_string(), pos);
            Some(entry)
        })
        .collect()
}

fn pick_known_agent_id(candidate_ids: Vec<Option<String>>, agents: &[Value]) -> Option<String> {
    candidate_ids.into_iter().flatten().find(|candidate| {
        agents
            .iter()
            .any(|agent| str_key(agent, "id") == Some(candidate.as_str()))
    })
}

// A published null is a player-level or unknown binding, never a request
// to borrow the recommendation or current observation target.
fn receipt_target_agent_id(gameplay: &Value) -> Option<String> {
    if gameplay.get("receiptTargetAgentId").is_some() {
        return string_key(gameplay, "receiptTargetAgentId");
    }
    let feedback = obj(gameplay, "recentFeedback");
    if feedback.get("agentId").is_some() {
        return string_key(feedback, "agentId");
    }
    string_key(gameplay, "acceptedIntentTarget")
}

fn build_action_receipt(locale: &str, gameplay: &Value) -> Value {
    let recent_feedback = obj(gameplay, "recentFeedback");
    let recent_feedback_action = str_key(recent_feedback, "action");
    let execution_stage = str_key(gameplay, "executionState");
    let feedback_stage = str_key(recent_feedback, "stage");
    let rejected =
        is_rejected_receipt_stage(execution_stage) || is_rejected_receipt_stage(feedback_stage);
    // Either canonical field may be omitted at compatibility boundaries, but an
    // explicit rejection must win over a stale accepted/executing state.
    let receipt_stage = rejected
        .then_some("rejected")
        .or(execution_stage)
        .or(feedback_stage);
    let has_non_advancing_stage = is_non_advancing_receipt_stage(execution_stage)
        || is_non_advancing_receipt_stage(feedback_stage);
    let has_world_delta = !rejected
        && !has_non_advancing_stage
        && !is_pending_receipt_stage(receipt_stage)
        && (is_authoritative_world_delta_stage(execution_stage)
            || is_authoritative_world_delta_stage(feedback_stage));
    // Keep the established zero-delta shape for an in-flight receipt, while
    // withholding deltas from rejected/non-advancing outcomes altogether.
    let exposes_receipt_delta =
        !rejected && (has_world_delta || is_pending_receipt_stage(receipt_stage));
    let has_player_intent = str_key(gameplay, "acceptedIntentId").is_some()
        || str_key(gameplay, "acceptedIntentScope").is_some()
        || str_key(gameplay, "acceptedIntentTarget").is_some()
        || recent_feedback_action.is_some_and(|action| action != "chain_sync");
    if str_key(gameplay, "blockerKind") == Some("runtime_snapshot_empty_entities")
        && has_enabled_first_agent_claim(gameplay)
        && !has_player_intent
    {
        return json!({
            "present": false,
            "state": "waiting_for_intent",
            "confidence": "none",
            "title": action_receipt_title(locale, "waiting_for_intent", false),
            "summary": tr(
                locale,
                "当前是新用户空世界，先认领第一个 Agent。",
                "This is a new-user empty world; claim the first Agent first.",
            ),
            "detail": tr(
                locale,
                "链同步回执不会作为阻塞主因显示；认领提交并同步后，Agent 会出现在世界里。",
                "Chain-sync feedback is not the primary blocker here; after the claim is submitted and synced, the Agent will appear in the world.",
            ),
            "target_agent_id": Value::Null,
            "effect_kind": Value::Null,
            "delta_logical_time": Value::Null,
            "delta_event_seq": Value::Null,
        });
    }
    let present = rejected
        || has_world_delta
        || has_player_intent
        || str_key(recent_feedback, "reason").is_some();
    let raw_state = receipt_stage.unwrap_or("waiting_for_intent");
    let state = if present {
        raw_state
    } else {
        "waiting_for_intent"
    };
    let confidence = if has_world_delta {
        "world_delta"
    } else if has_player_intent && !rejected {
        "accepted_intent"
    } else {
        "none"
    };
    let summary = if present {
        if rejected {
            tr(
                locale,
                "请求的玩法动作在执行前被拒绝。",
                "The requested gameplay action was rejected before execution.",
            )
        } else if has_world_delta {
            str_key(gameplay, "lastWorldChange")
                .or_else(|| str_key(recent_feedback, "effect"))
                .or_else(|| str_key(gameplay, "acceptedIntentSummary"))
                .or_else(|| str_key(recent_feedback, "action"))
                .or_else(|| str_key(gameplay, "executionSummary"))
                .unwrap_or("")
                .to_string()
        } else {
            str_key(recent_feedback, "effect")
                .or_else(|| str_key(gameplay, "acceptedIntentSummary"))
                .or_else(|| str_key(recent_feedback, "action"))
                .or_else(|| str_key(gameplay, "executionSummary"))
                .unwrap_or("")
                .to_string()
        }
    } else {
        tr(
            locale,
            "还没有一条玩家行动产生可确认的世界变化。",
            "No player-caused world change has been confirmed yet.",
        )
    };
    let detail = if present {
        if rejected {
            Some(rejected_receipt_detail(locale, gameplay, recent_feedback))
        } else {
            str_key(gameplay, "executionCauseDetail")
                .or_else(|| str_key(recent_feedback, "reason"))
                .or_else(|| str_key(recent_feedback, "hint"))
                .or_else(|| str_key(gameplay, "acceptedIntentDetail"))
                .or_else(|| str_key(gameplay, "progressDetail"))
                .map(ToString::to_string)
        }
    } else {
        Some(tr(
            locale,
            "先提交玩法动作或推进世界，再查看系统确认、阻塞或完成的回执。",
            "Submit a gameplay action or advance the world, then read whether the system accepted, blocked, or completed it.",
        ))
    };
    json!({
        "present": present,
        "state": state,
        "confidence": confidence,
        "title": action_receipt_title(locale, state, present),
        "summary": summary,
        "detail": detail,
        "target_agent_id": if present {
            receipt_target_agent_id(gameplay)
                .map(Value::String)
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        },
        "effect_kind": if present {
            string_key(gameplay, "executionCauseKind")
                .or_else(|| string_key(recent_feedback, "stage"))
                .map(Value::String)
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        },
        "delta_logical_time": if present && exposes_receipt_delta { obj(recent_feedback, "deltaLogicalTime").clone() } else { Value::Null },
        "delta_event_seq": if present && exposes_receipt_delta { obj(recent_feedback, "deltaEventSeq").clone() } else { Value::Null },
    })
}

fn localized_goal_title(locale: &str, gameplay: &Value) -> String {
    let published = str_key(gameplay, "goalTitle");
    match normalize_gameplay_token(str_key(gameplay, "goalKind")).as_str() {
        "recovercapability" => zh_or_published(
            locale,
            published,
            "恢复可持续能力",
            "Recover sustainable capability",
        ),
        "stabilizefirstline" | "establishfirstcapability" => zh_or_published(
            locale,
            published,
            "稳定第一条生产线",
            "Stabilize the first production line",
        ),
        "choosefirstexpansiontradeoff" | "choosemidlooppath" => zh_or_published(
            locale,
            published,
            "选择下一条扩张路径",
            "Choose the next expansion path",
        ),
        "createfirstworldfeedback" => zh_or_published(
            locale,
            published,
            "确认第一条世界反馈",
            "Confirm the first world feedback",
        ),
        _ => zh_or_published(
            locale,
            published,
            "进入世界，建立第一条能力链",
            "Enter the world and build the first capability chain",
        ),
    }
}

fn has_published_goal(gameplay: &Value) -> bool {
    str_key(gameplay, "goalTitle").is_some_and(|title| !title.trim().is_empty())
        || matches!(
            normalize_gameplay_token(str_key(gameplay, "goalKind")).as_str(),
            "recovercapability"
                | "stabilizefirstline"
                | "establishfirstcapability"
                | "choosefirstexpansiontradeoff"
                | "choosemidlooppath"
                | "createfirstworldfeedback"
        )
}

fn localized_objective_detail(locale: &str, gameplay: &Value) -> String {
    let published = str_key(gameplay, "objective").or_else(|| str_key(gameplay, "progressDetail"));
    match normalize_gameplay_token(str_key(gameplay, "goalKind")).as_str() {
        "recovercapability" => zh_or_published(
            locale,
            published,
            "先恢复阻塞点，再确认生产线重新具备可经营能力。",
            "Recover the blocker first, then confirm the line is operable again.",
        ),
        "stabilizefirstline" | "establishfirstcapability" => zh_or_published(
            locale,
            published,
            "先稳定第一条产线，再决定扩张、恢复或分工。",
            "Stabilize the first line before choosing expansion, recovery, or specialization.",
        ),
        "choosefirstexpansiontradeoff" | "choosemidlooppath" => zh_or_published(
            locale,
            published,
            "比较下一步带来的用途、弹性和分支价值，再推进。",
            "Compare the next move's use, resilience, and branch value before advancing.",
        ),
        "createfirstworldfeedback" => zh_or_published(
            locale,
            published,
            "先拿到一条明确世界反馈，再继续后续工业选择。",
            "Get one clear world feedback signal before continuing industrial choices.",
        ),
        _ => zh_or_published(
            locale,
            published,
            "先让 Agent、路线和资源关系变得可读，再推进下一步。",
            "Read the agent, route, and resource relationship before pushing the next move.",
        ),
    }
}

fn localized_next_action_label(locale: &str, gameplay: &Value) -> String {
    let recommended_action = obj(gameplay, "recommendedAction");
    let published = str_key(recommended_action, "label")
        .or_else(|| str_key(gameplay, "nextStepHint"))
        .or_else(|| str_key(gameplay, "narrativeNextStep"));
    if is_zh_locale(locale) && published.map(contains_cjk).unwrap_or(false) {
        return published.unwrap_or_default().to_string();
    }
    if !is_zh_locale(locale)
        && let Some(published) = published
    {
        return published.to_string();
    }
    let action_id = normalize_gameplay_token(str_key(recommended_action, "actionId"));
    let label_token = normalize_gameplay_token(str_key(recommended_action, "label"));
    if action_id == "buildfactorysmeltermk1" || label_token.contains("smeltermk1") {
        return tr(
            locale,
            "排队建造一型冶炼炉",
            "Queue Smelter MK1 construction",
        );
    }
    match str_key(recommended_action, "executeKind") {
        Some("gameplay_action") => tr(
            locale,
            "提交推荐玩法动作",
            "Submit recommended gameplay action",
        ),
        Some("step") => tr(locale, "推进世界一步", "Advance the world one step"),
        Some("play") => tr(locale, "继续运行世界", "Keep the world running"),
        Some("request_snapshot") => tr(locale, "刷新世界快照", "Refresh world snapshot"),
        Some("agent_chat") => tr(
            locale,
            "向选中 Agent 发送消息",
            "Message the selected agent",
        ),
        _ => tr(
            locale,
            "选择一个 Agent 或推进世界一步",
            "Select an agent or advance the world one step",
        ),
    }
}

fn localized_blocker_label(locale: &str, gameplay: &Value) -> Option<String> {
    if str_key(gameplay, "blockerKind") == Some("runtime_snapshot_empty_entities") {
        return Some(tr(locale, "认领第一个 Agent", "Claim the first Agent"));
    }
    if let Some(published) = str_key(gameplay, "blockerLabel")
        && (!is_zh_locale(locale) || contains_cjk(published))
    {
        return Some(published.to_string());
    }
    match str_key(gameplay, "blockerKind") {
        Some("material_shortage") => Some(tr(locale, "物料不足", "Missing Material")),
        Some("power_shortage") => Some(tr(locale, "供电不足", "Power Shortage")),
        Some("governance_gate") => Some(tr(locale, "治理门槛", "Governance Gate")),
        Some("llm_required") => Some(tr(locale, "需要 LLM", "LLM Required")),
        Some(kind) => Some(kind.to_string()),
        None => None,
    }
}

fn localized_optional_detail(locale: &str, published: Option<&str>) -> Option<String> {
    let published = published?;
    if !is_zh_locale(locale) || contains_cjk(published) {
        return Some(published.to_string());
    }
    let token = normalize_gameplay_token(Some(published));
    if token.contains("requestasnapshot")
        || token.contains("advance1step")
        || token.contains("inspectthenewdelta")
    {
        return Some("先请求一次快照，推进 1 步，再检查新的世界变化和事件。".to_string());
    }
    Some(tr(
        locale,
        "查看当前回执和阻塞原因，再决定下一步。",
        "Read the current receipt and blocker before choosing the next move.",
    ))
}

fn build_commercial_surface(
    locale: &str,
    gameplay: &Value,
    agents: &[Value],
    links: &[Value],
    fragment_terrain: &[Value],
    visual_hotspots: &[Value],
    selection: &Value,
) -> Value {
    // Observation, recommendation and original operation each retain their own identity.
    let active_agent_id = pick_known_agent_id(
        vec![if str_key(selection, "kind") == Some("agent") {
            string_key(selection, "id")
        } else {
            None
        }],
        agents,
    );
    let objective_title = localized_goal_title(locale, gameplay);
    let objective_detail = localized_objective_detail(locale, gameplay);
    let next_action_label = localized_next_action_label(locale, gameplay);
    let next_action_detail = localized_optional_detail(
        locale,
        str_key(obj(gameplay, "recommendedAction"), "disabledReason")
            .or_else(|| str_key(gameplay, "nextStepHint"))
            .or_else(|| str_key(gameplay, "executionSummary")),
    );
    let leverage_summary = str_key(gameplay, "acceptedIntentSummary")
        .or_else(|| str_key(gameplay, "lastWorldChange"))
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            tr(
                locale,
                "还没有一条被正式接受的玩家意图",
                "No player-facing accepted intent yet",
            )
        });
    let leverage_detail = str_key(gameplay, "lastWorldChange")
        .or_else(|| str_key(gameplay, "executionCauseDetail"))
        .or_else(|| str_key(gameplay, "acceptedIntentDetail"))
        .or_else(|| str_key(gameplay, "progressDetail"))
        .map(ToString::to_string);
    let action_receipt = build_action_receipt(locale, gameplay);
    let blocker_label = localized_blocker_label(locale, gameplay);

    json!({
        "objective": {
            "title": objective_title,
            "detail": objective_detail,
            "progress_percent": obj(gameplay, "progressPercent").clone(),
        },
        "next_action": {
            "label": next_action_label,
            "detail": next_action_detail,
            "target_agent_id": string_key(obj(gameplay, "recommendedAction"), "targetAgentId")
                .map(Value::String)
                .unwrap_or(Value::Null),
            "execute_kind": string_key(obj(gameplay, "recommendedAction"), "executeKind")
                .map(Value::String)
                .unwrap_or(Value::Null),
        },
        "active_agent_id": active_agent_id,
        "player_leverage": {
            "state": str_key(gameplay, "executionState").unwrap_or("waiting_for_intent"),
            "label": str_key(gameplay, "executionStateLabel")
                .map(ToString::to_string)
                .unwrap_or_else(|| tr(locale, "等待玩家意图", "Waiting for Intent")),
            "summary": leverage_summary,
            "detail": leverage_detail,
        },
        "action_receipt": action_receipt,
        "blocker": {
            "label": blocker_label,
            "detail": string_key(gameplay, "narrativeBlockerDetail")
                .or_else(|| string_key(gameplay, "blockerDetail")),
        },
        "world_read": {
            "agents": agents.len(),
            "routes": links.len(),
            "fragments": fragment_terrain.len(),
            "hotspots": visual_hotspots.len(),
        },
    })
}

pub(crate) fn build_render_state(input: &Value) -> Value {
    let adapted = world_view::adapt_input(input);
    let input = adapted.as_ref().unwrap_or(input);
    let locale = str_key(input, "locale").unwrap_or("en");
    let world_bounds = build_world_bounds(input);
    let world_scale_base = number_key(&world_bounds, "width_cm", 1.0)
        .min(number_key(&world_bounds, "depth_cm", 1.0))
        .max(1.0);
    let selected = obj(input, "selected");
    let mut fragment_terrain = Vec::new();
    let mut locations = Vec::new();

    for location in arr(obj(input, "lists"), "locations") {
        let terrain = build_fragment_terrain_for_location(location, &world_bounds);
        fragment_terrain.extend(terrain.iter().cloned());
        let resource_summary = resource_summary(obj(location, "resources"));
        let resource_score = count_resource_entries(&resource_summary);
        let has_terrain = !terrain.is_empty();
        if let Some(pos) = normalize_position(obj(location, "pos")) {
            let radius_cm = number_key(obj(location, "profile"), "radius_cm", 0.0);
            let size_hint_px = if has_terrain {
                10.0
            } else {
                16.0 + 18.0_f64
                    .min((radius_cm / world_scale_base) * 420.0 + (resource_score * 2) as f64)
            };
            locations.push(json!({
                "id": str_key(location, "id").unwrap_or(""),
                "label": str_key(location, "name").or_else(|| str_key(location, "id")).unwrap_or(""),
                "pos": pos,
                "radius_cm": radius_cm,
                "resource_summary": resource_summary,
                "resource_score": resource_score,
                "fragment_terrain_count": terrain.len(),
                "marker_role": if has_terrain { "logic_anchor" } else { "primary_marker" },
                "marker_alpha": if has_terrain { 0.32 } else { 0.72 },
                "size_hint_px": size_hint_px,
            }));
        }
    }
    let location_by_id: Map<String, Value> = locations
        .iter()
        .filter_map(|location| Some((str_key(location, "id")?.to_string(), location.clone())))
        .collect();
    // This is a display-only projection of published facility state. A missing
    // location is intentionally suppressed rather than given a fallback anchor.
    let gameplay = obj(input, "gameplay");
    let micro_depot_facilities =
        build_micro_depot_facilities(gameplay, &location_by_id, &world_bounds);

    let agents: Vec<Value> = arr(obj(input, "lists"), "agents")
        .iter()
        .map(|agent| {
            let (pos, position_source) =
                resolve_agent_position(agent, selected, &location_by_id, &world_bounds);
            let resource_summary = resource_summary(obj(agent, "resources"));
            let resource_score = count_resource_entries(&resource_summary);
            let power_state = agent_power_state(agent);
            let mut status_badges = Vec::new();
            if let Some(location_id) = str_key(agent, "location_id") {
                status_badges.push(Value::String(format!("location={location_id}")));
            }
            if let Some(kind) = str_key(agent, "kind") {
                status_badges.push(Value::String(format!("kind={kind}")));
            }
            if position_source == "location_derived" {
                status_badges.push(Value::String("position=location_derived".to_string()));
            }
            json!({
                "id": str_key(agent, "id").unwrap_or(""),
                "label": str_key(agent, "name").or_else(|| str_key(agent, "id")).unwrap_or(""),
                "location_id": string_key(agent, "location_id"),
                "pos": pos,
                "position_source": position_source,
                // Preserve only the explicit authority envelopes. The link
                // projector still validates every semantic field and never
                // treats location/geometry as assignment authority.
                "relation": obj(agent, "relation"),
                "assignment": obj(agent, "assignment"),
                "resource_summary": resource_summary,
                "resource_score": resource_score,
                "status_badges": status_badges,
                "power_state": power_state,
                "size_hint_px": 12.0 + 10.0_f64.min(
                    (resource_score * 2) as f64
                    + if str_key(agent, "location_id").is_some() { 2.0 } else { 0.0 }
                    + if str_key(agent, "kind").is_some() { 1.0 } else { 0.0 }
                ),
            })
        })
        .collect();
    // Persisted module visuals are display-only: keep their anchor resolution
    // faithful to the already-projected world and suppress unresolved anchors.
    let module_visual_entities = build_module_visual_entities(input, &locations, &agents);

    let selection = match (str_key(input, "selectedKind"), str_key(input, "selectedId")) {
        (Some(kind), Some(id)) => json!({ "kind": kind, "id": id }),
        _ => Value::Null,
    };
    let (industry_aggregates, world_events) = spatial_overlays::project(input, &world_bounds);
    let links = if input.get("worldView").is_some() {
        spatial_routes::project(
            input,
            &agents,
            &locations,
            &module_visual_entities,
            &industry_aggregates,
        )
    } else {
        relation_projection::build_pixel_world_links(&agents, &location_by_id)
    };
    let social_links = social_links::build_pixel_world_social_links(input, &agents, &locations);
    let anchor =
        resolve_selection_position(&selection, &agents, &locations, &module_visual_entities)
            .or_else(|| {
                agents
                    .iter()
                    .find(|agent| obj(agent, "pos").is_object())
                    .and_then(|agent| normalize_position(obj(agent, "pos")))
            })
            .or_else(|| {
                locations
                    .first()
                    .and_then(|location| normalize_position(obj(location, "pos")))
            })
            .or_else(|| world_center_position(&world_bounds));
    // Generic onboarding copy belongs in the ordinary guidance surface. It is
    // not a published business goal and cannot acquire a spatial Agent anchor.
    let goal_highlight = if has_published_goal(gameplay) {
        json!({
            "title": localized_goal_title(locale, gameplay),
            "objective": localized_objective_detail(locale, gameplay),
        })
    } else {
        Value::Null
    };
    let blocker_highlight = if str_key(gameplay, "blockerKind").is_some()
        || str_key(gameplay, "blockerDetail").is_some()
    {
        let blocker_label = localized_blocker_label(locale, gameplay);
        json!({
            "kind": str_key(gameplay, "blockerKind").unwrap_or("blocked"),
            "label": blocker_label,
            "detail": string_key(gameplay, "blockerDetail"),
        })
    } else {
        Value::Null
    };
    let recent_event_hotspots: Vec<Value> = Vec::new();
    let visual_hotspots = build_visual_hotspots(
        &world_bounds,
        anchor,
        &goal_highlight,
        &blocker_highlight,
        &recent_event_hotspots,
    );
    let commercial_surface = build_commercial_surface(
        locale,
        gameplay,
        &agents,
        &links,
        &fragment_terrain,
        &visual_hotspots,
        &selection,
    );
    // Keep this target-only projection separate from the player-facing receipt text.
    let receipt_target = obj(&commercial_surface, "action_receipt")
        .get("present")
        .and_then(Value::as_bool)
        .filter(|present| *present)
        .and_then(|_| {
            let receipt = obj(&commercial_surface, "action_receipt");
            let agent_id = str_key(receipt, "target_agent_id")?;
            let state = str_key(receipt, "state")?;
            agents
                .iter()
                .any(|agent| str_key(agent, "id") == Some(agent_id))
                .then(|| json!({ "agent_id": agent_id, "state": state }))
        })
        .unwrap_or(Value::Null);
    // This is a display-only projection of the runtime-gated recommendation.
    // Do not rank, replace, or otherwise interpret actions in the renderer.
    let recommended_target = {
        let recommended_action = obj(gameplay, "recommendedAction");
        str_key(recommended_action, "targetAgentId")
            .filter(|_| str_key(recommended_action, "disabledReason").is_none())
            .filter(|agent_id| {
                agents
                    .iter()
                    .any(|agent| str_key(agent, "id") == Some(agent_id))
            })
            .map(|agent_id| json!({ "agent_id": agent_id }))
            .unwrap_or(Value::Null)
    };
    let active_intent_target =
        intent_projection::project_active_intent_target(input, &world_bounds, &agents);
    let presentation = obj(input, "presentation");

    json!({
        "locale": locale,
        "exploration_mode": input.get("worldView").is_some(),
        "navigation": obj(input, "navigation").clone(),
        "world_bounds": world_bounds,
        "locations": locations,
        "fragment_terrain": fragment_terrain,
        "micro_depot_facilities": micro_depot_facilities,
        "module_visual_entities": module_visual_entities,
        "agents": agents,
        "links": links,
        "social_links": social_links,
        "selection": selection,
        "goal_highlight": goal_highlight,
        "blocker_highlight": blocker_highlight,
        "recent_event_hotspots": recent_event_hotspots,
        "industry_aggregates": industry_aggregates,
        "world_events": world_events,
        "visual_hotspots": visual_hotspots,
        "receipt_target": receipt_target,
        "recommended_target": recommended_target,
        "active_intent_target": active_intent_target,
        "commercial_surface": commercial_surface,
        "presentation": {
            "world_bounds_label": obj(presentation, "world_bounds_label").clone(),
            "marker_truth_note": obj(presentation, "marker_truth_note").clone(),
        },
    })
}

#[cfg(test)]
#[path = "host_state_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "host_identity_tests.rs"]
mod identity_tests;
