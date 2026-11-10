use super::*;

const EMPTY_RESOURCE_READOUT: &str = "No resources reported";
const RESOURCE_READOUT_LAYER_Z: f32 = 5.0;
const RESOURCE_READOUT_ABOVE_SELECTION_PX: f64 = 36.0;
const RESOURCE_READOUT_MAX_CHARS: usize = 48;
const RESOURCE_READOUT_TOP_MARGIN_PX: f64 = 12.0;
const RESOURCE_READOUT_BOTTOM_HUD_CLEARANCE_PX: f64 = 72.0;
const RESOURCE_READOUT_HORIZONTAL_MARGIN_PX: f64 = 12.0;
const RESOURCE_READOUT_GLYPH_ADVANCE_PX: f64 = 6.6;
const RESOURCE_READOUT_NARROW_VIEWPORT_MAX_WIDTH_PX: f64 = 390.0;
const RESOURCE_READOUT_PRIORITY_CUE_LANE_OFFSET_PX: f64 = 28.0;
const RESOURCE_READOUT_PRIORITY_CUE_LEFT_BAND_WIDTH: f64 = 0.44;

/// The selected entity's resource text, kept separate from status badges and
/// interaction entities so player-facing state cannot alter world input.
#[derive(Component)]
pub(crate) struct PixelWorldSelectedResourceReadout {
    #[cfg(test)]
    pub(crate) target_kind: String,
    #[cfg(test)]
    pub(crate) target_id: String,
    pub(crate) display: String,
}

pub(super) fn reconcile_selected_resource_readout(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    existing_readouts: &Query<(Entity, &PixelWorldSelectedResourceReadout)>,
    width: f64,
    height: f64,
) {
    let reserve_priority_cue_lane = width <= RESOURCE_READOUT_NARROW_VIEWPORT_MAX_WIDTH_PX
        && runtime.render_state.as_ref().is_some_and(|render_state| {
            render_state
                .visual_hotspots
                .iter()
                .any(|hotspot| matches!(hotspot.kind.as_str(), "blocker" | "goal"))
        });
    let selected = runtime.render_state.as_ref().and_then(|render_state| {
        let selection = render_state.selection.as_ref()?;
        match selection.kind.as_str() {
            "agent" => render_state
                .agents
                .iter()
                .find(|agent| agent.id == selection.id)
                .map(|agent| {
                    let (canvas_x, canvas_y) = render_state
                        .world_bounds
                        .as_ref()
                        .and_then(|world_bounds| {
                            agent.pos.as_ref().and_then(|pos| {
                                to_canvas_point(pos, world_bounds, width, height, &runtime.camera)
                            })
                        })
                        .unwrap_or_else(|| {
                            fallback_point_for_entity(&agent.id, width, height, &runtime.camera)
                        });
                    (
                        "agent",
                        agent.id.as_str(),
                        agent.resource_summary.as_str(),
                        canvas_x,
                        canvas_y,
                    )
                }),
            "location" => render_state
                .locations
                .iter()
                .find(|location| location.id == selection.id)
                .map(|location| {
                    let (canvas_x, canvas_y) = render_state
                        .world_bounds
                        .as_ref()
                        .and_then(|world_bounds| {
                            to_canvas_point(
                                &location.pos,
                                world_bounds,
                                width,
                                height,
                                &runtime.camera,
                            )
                        })
                        .unwrap_or_else(|| {
                            fallback_point_for_entity(&location.id, width, height, &runtime.camera)
                        });
                    (
                        "location",
                        location.id.as_str(),
                        location.resource_summary.as_str(),
                        canvas_x,
                        canvas_y,
                    )
                }),
            _ => None,
        }
    });

    let Some((_target_kind, _target_id, resource_summary, canvas_x, canvas_y)) = selected else {
        for (entity, _) in existing_readouts.iter() {
            commands.entity(entity).despawn();
        }
        return;
    };
    let display = player_facing_resource_summary(resource_summary);
    let readout = PixelWorldSelectedResourceReadout {
        #[cfg(test)]
        target_kind: _target_kind.to_string(),
        #[cfg(test)]
        target_id: _target_id.to_string(),
        display,
    };
    let visuals = resource_readout_visuals(
        &readout.display,
        canvas_x,
        resource_readout_canvas_y(
            resource_readout_anchor_y(
                canvas_y,
                _target_kind,
                f64::from(agent_pixel_scale(width, height).y),
            ),
            height,
            reserve_priority_cue_lane,
        ),
        width,
        height,
        reserve_priority_cue_lane,
    );

    let mut existing = existing_readouts.iter();
    if let Some((entity, _)) = existing.next() {
        commands.entity(entity).insert((readout, visuals));
    } else {
        commands.spawn((readout, visuals));
    }
    for (entity, _) in existing {
        commands.entity(entity).despawn();
    }
}

pub(super) fn despawn_selected_resource_readouts(
    commands: &mut Commands,
    existing_readouts: &Query<(Entity, &PixelWorldSelectedResourceReadout)>,
) {
    for (entity, _) in existing_readouts.iter() {
        commands.entity(entity).despawn();
    }
}

fn player_facing_resource_summary(resource_summary: &str) -> String {
    let trimmed = resource_summary.trim();
    if trimmed.is_empty() || trimmed == "-" || is_empty_amounts_container(trimmed) {
        EMPTY_RESOURCE_READOUT.to_string()
    } else if let Some(formatted) = format_structured_resource_summary(trimmed) {
        formatted
    } else {
        resource_summary.to_string()
    }
}

/// Convert the structured host projection (`kind:{"amount":..,"unit":..}`)
/// into a compact player label. Invalid or incomplete projections deliberately
/// return `None` so the existing bounded text fallback remains unchanged.
fn format_structured_resource_summary(resource_summary: &str) -> Option<String> {
    resource_summary
        .split(" · ")
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (kind, payload) = entry.split_once(':')?;
            let kind = title_case_resource_kind(kind.trim())?;
            let object = serde_json::from_str::<serde_json::Value>(payload.trim())
                .ok()?
                .as_object()?
                .clone();
            let amount = scalar_resource_value(object.get("amount")?)?;
            let unit = object.get("unit")?.as_str()?.trim();
            if unit.is_empty() {
                return None;
            }

            let suffix = if unit == "%" {
                format!("{amount}{unit}")
            } else {
                format!("{amount} {unit}")
            };
            Some(format!("{kind} {suffix}"))
        })
        .collect::<Option<Vec<_>>>()
        .filter(|entries| !entries.is_empty())
        .map(|entries| entries.join(" · "))
}

fn scalar_resource_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::String(value) if !value.trim().is_empty() => {
            Some(value.trim().to_string())
        }
        _ => None,
    }
}

fn title_case_resource_kind(kind: &str) -> Option<String> {
    let kind = kind.trim();
    if kind.is_empty()
        || !kind.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ' ')
        })
    {
        return None;
    }

    let words = kind
        .split(['-', '_', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut characters = word.chars();
            let first = characters.next()?.to_ascii_uppercase();
            Some(format!(
                "{first}{}",
                characters.as_str().to_ascii_lowercase()
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
}

fn is_empty_amounts_container(resource_summary: &str) -> bool {
    resource_summary
        .chars()
        .filter(|character| !character.is_whitespace())
        .eq("amounts:{}".chars())
}

fn resource_readout_canvas_y(
    selection_canvas_y: f64,
    canvas_height: f64,
    reserve_priority_cue_lane: bool,
) -> f64 {
    let maximum_y = (canvas_height - RESOURCE_READOUT_BOTTOM_HUD_CLEARANCE_PX)
        .max(RESOURCE_READOUT_TOP_MARGIN_PX);
    let lane_offset = RESOURCE_READOUT_ABOVE_SELECTION_PX
        + if reserve_priority_cue_lane {
            RESOURCE_READOUT_PRIORITY_CUE_LANE_OFFSET_PX
        } else {
            0.0
        };
    (selection_canvas_y - lane_offset).clamp(RESOURCE_READOUT_TOP_MARGIN_PX, maximum_y)
}

fn resource_readout_visuals(
    display: &str,
    canvas_x: f64,
    canvas_y: f64,
    width: f64,
    height: f64,
    reserve_priority_cue_lane: bool,
) -> (Text2d, TextFont, TextColor, Transform) {
    let (left, right) = resource_readout_horizontal_bounds(width, reserve_priority_cue_lane);
    let visible_display = visible_resource_summary(display, right - left);
    (
        Text2d::new(visible_display.clone()),
        TextFont {
            font_size: FontSize::Px(11.0),
            ..default()
        },
        TextColor(Color::srgb_u8(226, 232, 240)),
        Transform::from_translation(to_bevy_translation(
            resource_readout_canvas_x(canvas_x, &visible_display, left, right),
            canvas_y,
            width,
            height,
            RESOURCE_READOUT_LAYER_Z,
        )),
    )
}

fn resource_readout_canvas_x(
    selection_canvas_x: f64,
    visible_display: &str,
    left: f64,
    right: f64,
) -> f64 {
    let half_width = resource_readout_text_width(visible_display) / 2.0;
    let minimum_x = left + half_width;
    let maximum_x = right - half_width;
    if minimum_x <= maximum_x {
        selection_canvas_x.clamp(minimum_x, maximum_x)
    } else {
        (left + right) / 2.0
    }
}

fn resource_readout_horizontal_bounds(
    canvas_width: f64,
    reserve_priority_cue_lane: bool,
) -> (f64, f64) {
    let left = RESOURCE_READOUT_HORIZONTAL_MARGIN_PX.min(canvas_width / 2.0);
    let canvas_right = (canvas_width - RESOURCE_READOUT_HORIZONTAL_MARGIN_PX).max(left);
    let right = if reserve_priority_cue_lane {
        (canvas_width * RESOURCE_READOUT_PRIORITY_CUE_LEFT_BAND_WIDTH).clamp(left, canvas_right)
    } else {
        canvas_right
    };
    (left, right)
}

fn resource_readout_text_width(display: &str) -> f64 {
    display.chars().count() as f64 * RESOURCE_READOUT_GLYPH_ADVANCE_PX
}

fn visible_resource_summary(display: &str, max_text_width: f64) -> String {
    let characters = display.chars().collect::<Vec<_>>();
    let max_visible_chars = (max_text_width / RESOURCE_READOUT_GLYPH_ADVANCE_PX)
        .floor()
        .max(1.0) as usize;
    let payload_limit = RESOURCE_READOUT_MAX_CHARS.min(max_visible_chars);
    if characters.len() <= payload_limit {
        return display.to_string();
    }

    let abbreviated = characters
        .iter()
        .take(RESOURCE_READOUT_MAX_CHARS.min(max_visible_chars.saturating_sub(1)))
        .collect::<String>();
    format!("{abbreviated}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_readout_visual_uses_text_layer_and_bounds_long_content() {
        let (text, font, color, transform) = resource_readout_visuals(
            "water=12, ore=5, carbon=9, oxygen=4, hydrogen=7, silicon=3",
            480.0,
            resource_readout_canvas_y(290.0, 540.0, false),
            960.0,
            540.0,
            false,
        );

        assert!(text.0.ends_with('…'));
        assert!(text.0.chars().count() <= RESOURCE_READOUT_MAX_CHARS + 1);
        assert_eq!(font.font_size, FontSize::Px(11.0));
        assert_eq!(color.0, Color::srgb_u8(226, 232, 240));
        assert_eq!(transform.translation.z, RESOURCE_READOUT_LAYER_Z);
        assert_eq!(transform.translation.x, 0.0);
        assert_eq!(transform.translation.y, 16.0);
    }

    #[test]
    fn resource_readout_normalizes_empty_amounts_and_clamps_above_the_selection() {
        assert_eq!(
            player_facing_resource_summary(" amounts : { } "),
            EMPTY_RESOURCE_READOUT
        );
        assert_eq!(resource_readout_canvas_y(20.0, 390.0, false), 12.0);
        assert_eq!(resource_readout_canvas_y(380.0, 390.0, false), 318.0);
        assert_eq!(resource_readout_canvas_y(220.0, 390.0, false), 184.0);
    }

    #[test]
    fn resource_readout_formats_structured_energy_without_exposing_raw_json() {
        let display = player_facing_resource_summary(r#"energy:{"amount":72,"unit":"%"}"#);

        assert_eq!(display, "Energy 72%");
        assert!(!display.contains('{'));
        assert!(!display.contains('"'));
    }

    #[test]
    fn resource_readout_formats_each_structured_entry_without_raw_json() {
        let display = player_facing_resource_summary(
            r#"energy:{"amount":72,"unit":"%"} · ore:{"amount":5,"unit":"kg"}"#,
        );

        assert_eq!(display, "Energy 72% · Ore 5 kg");
        assert!(!display.contains('{'));
        assert!(!display.contains('}'));
        assert!(!display.contains('"'));
    }

    #[test]
    fn resource_readout_reserves_a_distinct_390px_lane_for_priority_cues() {
        let ordinary_lane = resource_readout_canvas_y(220.0, 390.0, false);
        let priority_cue_lane = resource_readout_canvas_y(220.0, 390.0, true);

        assert_eq!(ordinary_lane, 184.0);
        assert!(
            priority_cue_lane < ordinary_lane,
            "a narrow blocker or goal cue must reserve a distinct lane above the resource readout"
        );
        assert_eq!(priority_cue_lane, 156.0);
        assert!(priority_cue_lane >= RESOURCE_READOUT_TOP_MARGIN_PX);
    }

    #[test]
    fn priority_cue_readout_uses_a_left_safe_band_with_width_aware_text() {
        let (left, right) = resource_readout_horizontal_bounds(318.0, true);
        let visible = visible_resource_summary(
            "water=12, ore=5, carbon=9, oxygen=4, hydrogen=7, silicon=3",
            right - left,
        );
        let center = resource_readout_canvas_x(318.0, &visible, left, right);
        let half_width = resource_readout_text_width(&visible) / 2.0;

        assert_eq!(left, RESOURCE_READOUT_HORIZONTAL_MARGIN_PX);
        assert!(right < 318.0 - RESOURCE_READOUT_HORIZONTAL_MARGIN_PX);
        assert!(visible.ends_with('…'));
        assert!(center - half_width >= left);
        assert!(center + half_width <= right);
    }

    #[test]
    fn resource_readout_keeps_long_text_inside_390px_left_and_right_edges() {
        let display = "water=12, ore=5, carbon=9, oxygen=4, hydrogen=7, silicon=3";
        let visible = visible_resource_summary(display, 390.0);
        let half_width = resource_readout_text_width(&visible) / 2.0;
        let (left, right) = resource_readout_horizontal_bounds(390.0, false);

        for selection_x in [0.0, 390.0] {
            let safe_x = resource_readout_canvas_x(selection_x, &visible, left, right);
            assert!(safe_x - half_width >= RESOURCE_READOUT_HORIZONTAL_MARGIN_PX);
            assert!(safe_x + half_width <= 390.0 - RESOURCE_READOUT_HORIZONTAL_MARGIN_PX);
        }
        assert_eq!(visible.chars().count(), RESOURCE_READOUT_MAX_CHARS + 1);
    }
}

fn resource_readout_anchor_y(foot_y: f64, kind: &str, pixel_y: f64) -> f64 {
    // Agent top is fifteen authored logical pixels above its shared foot.
    // Preserve the original native-text lane gap as the glyph grows.
    foot_y
        - if kind == "agent" {
            15.0 * (pixel_y - 1.0).max(0.0)
        } else {
            0.0
        }
}

#[test]
fn agent_resource_readout_lane_follows_scaled_sprite_top() {
    assert_eq!(resource_readout_anchor_y(1080.0, "agent", 6.0), 1005.0);
    assert_eq!(resource_readout_anchor_y(1080.0, "location", 6.0), 1080.0);
    assert_eq!(resource_readout_anchor_y(1080.0, "agent", 0.5), 1080.0);
}
