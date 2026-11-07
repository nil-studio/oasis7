use std::collections::{HashMap, HashSet};

use super::map_labels::map_label_obstacles;
use super::*;

const AGENT_LABEL_COLOR: Color = Color::srgba_u8(226, 232, 240, 220);
const AGENT_LABEL_LAYER_Z: f32 = AGENT_LAYER_Z + 0.005;
const AGENT_LABEL_MIN_ZOOM: f64 = 1.75;
const AGENT_LABEL_ABOVE_MARKER_PX: f64 = 12.0;
const AGENT_LABEL_FONT_SIZE_PX: f32 = 10.0;
const AGENT_LABEL_GLYPH_ADVANCE_PX: f64 = 5.8;
const AGENT_LABEL_MAX_CHARS: usize = 24;
const AGENT_LABEL_HEIGHT_PX: f64 = 12.0;

/// A zoom-gated, noninteractive identity label for an existing Agent marker.
#[derive(Component)]
pub(super) struct PixelWorldAgentLabel {
    pub(super) id: String,
}

#[derive(SystemParam)]
pub(super) struct AgentLabelQueries<'w, 's> {
    labels: Query<'w, 's, (Entity, &'static PixelWorldAgentLabel)>,
}

pub(super) fn despawn_agent_labels(commands: &mut Commands, queries: &AgentLabelQueries) {
    for (entity, _) in queries.labels.iter() {
        commands.entity(entity).despawn();
    }
}

pub(super) fn reconcile_agent_labels(
    commands: &mut Commands,
    runtime: &BevyRuntimeState,
    queries: &AgentLabelQueries,
    width: f64,
    height: f64,
) {
    let existing = queries
        .labels
        .iter()
        .map(|(entity, label)| (label.id.clone(), entity))
        .collect::<HashMap<_, _>>();
    let Some(render_state) = runtime.render_state.as_ref() else {
        despawn_agent_labels(commands, queries);
        return;
    };
    let selected_id = render_state
        .selection
        .as_ref()
        .and_then(|selection| (selection.kind == "agent").then_some(selection.id.as_str()));
    let map_obstacles = map_label_obstacles(render_state, width, height, &runtime.camera);
    let mut agents = render_state.agents.iter().collect::<Vec<_>>();
    agents.sort_by(|left, right| {
        agent_label_priority(render_state, left.id.as_str(), selected_id)
            .cmp(&agent_label_priority(
                render_state,
                right.id.as_str(),
                selected_id,
            ))
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut active_ids = HashSet::new();
    let mut accepted_rects: Vec<AgentLabelRect> = Vec::new();
    for agent in agents {
        let (canvas_x, canvas_y) = render_state
            .world_bounds
            .as_ref()
            .and_then(|bounds| {
                agent
                    .pos
                    .as_ref()
                    .and_then(|pos| to_canvas_point(pos, bounds, width, height, &runtime.camera))
            })
            .unwrap_or_else(|| {
                fallback_point_for_entity(&agent.id, width, height, &runtime.camera)
            });
        let display = agent_label_display(agent);
        let label_x = canvas_x;
        let selected = selected_id == Some(agent.id.as_str());
        let body_size = agent_visual_style(agent, selected, 0.0, 0).size_px;
        let label_y = canvas_y
            - body_size * 15.0 / 16.0 * agent_pixel_scale(width, height).y as f64
            - AGENT_LABEL_ABOVE_MARKER_PX;
        let rect = AgentLabelRect::above_marker(label_x, label_y, &display);
        let priority = agent_label_priority(render_state, agent.id.as_str(), selected_id);
        if runtime.camera.zoom < AGENT_LABEL_MIN_ZOOM && priority > 1 {
            continue;
        }
        if accepted_rects
            .iter()
            .any(|accepted| accepted.overlaps(rect))
        {
            continue;
        }
        if priority > 1
            && map_obstacles.iter().any(|obstacle| {
                obstacle.overlaps_bounds(rect.left, rect.right, rect.top, rect.bottom)
            })
        {
            continue;
        }
        accepted_rects.push(rect);
        active_ids.insert(agent.id.clone());
        let visuals = (
            Text2d::new(display),
            TextFont {
                font_size: FontSize::Px(AGENT_LABEL_FONT_SIZE_PX),
                ..default()
            },
            TextColor(AGENT_LABEL_COLOR),
            Transform::from_translation(to_bevy_translation(
                label_x,
                label_y,
                width,
                height,
                AGENT_LABEL_LAYER_Z,
            )),
        );
        let label = PixelWorldAgentLabel {
            id: agent.id.clone(),
        };
        if let Some(entity) = existing.get(&agent.id) {
            commands.entity(*entity).insert((label, visuals));
        } else {
            commands.spawn((label, visuals));
        }
    }
    for (id, entity) in existing {
        if !active_ids.contains(&id) {
            commands.entity(entity).despawn();
        }
    }
}

fn agent_label_priority(render_state: &RenderState, id: &str, selected_id: Option<&str>) -> u8 {
    if selected_id == Some(id) {
        return 0;
    }
    if render_state
        .active_intent_target
        .as_ref()
        .is_some_and(|target| {
            target.agent_id == id
                && matches!(target.status.as_str(), "submitted" | "accepted" | "blocked")
        })
    {
        return 1;
    }
    if render_state
        .recommended_target
        .as_ref()
        .is_some_and(|target| target.agent_id == id)
        || render_state
            .receipt_target
            .as_ref()
            .is_some_and(|target| target.agent_id == id)
    {
        return 2;
    }
    3
}

fn agent_label_display(agent: &Agent) -> String {
    let label = agent.label.trim();
    truncate_agent_label(if label.is_empty() { &agent.id } else { label })
}

pub(super) fn truncate_agent_label(label: &str) -> String {
    let mut characters = label.chars();
    let visible = characters
        .by_ref()
        .take(AGENT_LABEL_MAX_CHARS)
        .collect::<String>();
    if characters.next().is_some() {
        format!("{visible}…")
    } else {
        visible
    }
}

#[derive(Clone, Copy)]
struct AgentLabelRect {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

impl AgentLabelRect {
    fn above_marker(canvas_x: f64, canvas_y: f64, display: &str) -> Self {
        let half_width = display.chars().count() as f64 * AGENT_LABEL_GLYPH_ADVANCE_PX / 2.0;
        Self {
            left: canvas_x - half_width,
            right: canvas_x + half_width,
            top: canvas_y - AGENT_LABEL_HEIGHT_PX,
            bottom: canvas_y,
        }
    }

    fn overlaps(self, other: Self) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }
}
