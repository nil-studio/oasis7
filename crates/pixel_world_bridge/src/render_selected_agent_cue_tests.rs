use super::fixtures::sample_render_state_with_receipt_target;
use super::*;

const SELECTED_AGENT_CUE_COLOR: Color = Color::srgb_u8(251, 191, 36);
const RECEIPT_BADGE_BACKING_COLOR: Color = Color::srgba_u8(15, 23, 42, 240);
const RECEIPT_BADGE_STROKE_COLOR: Color = Color::srgba_u8(248, 250, 252, 255);
const NARROW_RECEIPT_BADGE_BACKING_COLOR: Color = Color::srgba_u8(15, 23, 42, 245);
const NARROW_RECEIPT_BADGE_OUTLINE_COLOR: Color = Color::srgba_u8(203, 213, 225, 184);

#[derive(Clone, Copy, Debug, PartialEq)]
struct CueSegmentProbe {
    x: f32,
    y: f32,
    z: f32,
    width: f32,
    height: f32,
    rotation: f32,
    color: Color,
}

fn selected_agent_cue_segments(app: &mut App) -> Vec<CueSegmentProbe> {
    let world = app.world_mut();
    let mut query = world.query::<(&Sprite, &Transform)>();
    let mut segments = query
        .iter(world)
        .filter_map(|(sprite, transform)| {
            let size = sprite.custom_size?;
            (sprite.color == SELECTED_AGENT_CUE_COLOR
                && transform.translation.z > AGENT_LAYER_Z + SELECTED_ENTITY_LAYER_Z_OFFSET)
                .then_some(CueSegmentProbe {
                    x: transform.translation.x,
                    y: transform.translation.y,
                    z: transform.translation.z,
                    width: size.x,
                    height: size.y,
                    rotation: transform.rotation.to_euler(EulerRot::XYZ).2,
                    color: sprite.color,
                })
        })
        .collect::<Vec<_>>();
    segments.sort_by(|left, right| {
        left.x
            .total_cmp(&right.x)
            .then(left.y.total_cmp(&right.y))
            .then(left.width.total_cmp(&right.width))
            .then(left.height.total_cmp(&right.height))
    });
    segments
}

fn normalized_hit_regions(app: &mut App) -> Vec<(String, String, i64, i64, i64, i64)> {
    let mut regions = hit_regions(app)
        .into_iter()
        .map(|region| {
            (
                region.kind.to_string(),
                region.id,
                (region.left * 1_000.0).round() as i64,
                (region.top * 1_000.0).round() as i64,
                (region.right * 1_000.0).round() as i64,
                (region.bottom * 1_000.0).round() as i64,
            )
        })
        .collect::<Vec<_>>();
    regions.sort();
    regions
}

fn receipt_cue_geometry(app: &mut App) -> Vec<CueSegmentProbe> {
    let summary = visual_probe_summary(app);
    let agent = summary
        .agents
        .iter()
        .find(|agent| agent.id == "agent-0")
        .expect("receipt fixture target Agent");
    let core = summary
        .agent_cores
        .iter()
        .find(|core| core.id == "agent-0")
        .expect("receipt fixture target Agent core");
    let world = app.world_mut();
    // Grounding rims also sit above the body; only receipt-tagged geometry
    // can demonstrate receipt cleanup.
    let mut query =
        world.query_filtered::<(&Sprite, &Transform), With<PixelWorldReceiptTargetCue>>();
    let mut cues = query
        .iter(world)
        .filter_map(|(sprite, transform)| {
            let size = sprite.custom_size?;
            (transform.translation.z > core.z
                // The narrow blocked-receipt badge deliberately sits on the
                // target's left shoulder to avoid the upper-right tooltip lane.
                && (transform.translation.x - agent.x).abs() <= agent.size_px + 40.0
                && transform.translation.y > agent.y)
                .then_some(CueSegmentProbe {
                    x: transform.translation.x,
                    y: transform.translation.y,
                    z: transform.translation.z,
                    width: size.x,
                    height: size.y,
                    rotation: transform.rotation.to_euler(EulerRot::XYZ).2,
                    color: sprite.color,
                })
        })
        .collect::<Vec<_>>();
    cues.sort_by(|left, right| {
        left.x
            .total_cmp(&right.x)
            .then(left.y.total_cmp(&right.y))
            .then(left.width.total_cmp(&right.width))
            .then(left.height.total_cmp(&right.height))
    });
    cues
}

fn render_receipt_test_app_at_viewport(render_state: RenderState, width: f32, height: f32) -> App {
    let mut app = render_test_app(render_state);
    let world = app.world_mut();
    let mut windows = world.query_filtered::<&mut Window, With<PrimaryWindow>>();
    windows
        .single_mut(world)
        .expect("receipt test primary window")
        .resolution
        .set(width, height);
    app.update();
    app
}

#[test]
fn receipt_target_cue_is_target_gated_reused_and_removed_without_hit_regions() {
    let mut visible = render_test_app(sample_render_state_with_receipt_target(
        Some("blocked"),
        Some("agent-0"),
    ));
    let baseline_regions = normalized_hit_regions(&mut visible);
    let initial = receipt_cue_geometry(&mut visible);
    assert!(
        !initial.is_empty(),
        "a present blocked receipt targeting the rendered Agent needs a visible cap above that Agent"
    );
    let backing = initial
        .iter()
        .find(|cue| cue.width == 20.0 && cue.height == 16.0)
        .expect("blocked receipt needs a 20x16 badge backing");
    assert_eq!(backing.color, RECEIPT_BADGE_BACKING_COLOR);
    assert!(
        initial
            .iter()
            .filter(|cue| cue.width == 12.0 && cue.height == 2.0)
            .count()
            == 2
            && initial
                .iter()
                .filter(|cue| cue.width == 12.0 && cue.height == 2.0)
                .all(|cue| cue.color == RECEIPT_BADGE_STROKE_COLOR),
        "blocked receipt X must remain pale above its dark backing"
    );
    assert_eq!(
        normalized_hit_regions(&mut visible),
        baseline_regions,
        "the display-only receipt cue must not add or move hit regions"
    );

    let mut selected_state =
        sample_render_state_with_receipt_target(Some("blocked"), Some("agent-0"));
    selected_state.selection = Some(Selection {
        kind: "agent".to_string(),
        id: "agent-0".to_string(),
    });
    let mut selected = render_test_app(selected_state);
    let selected_backing = receipt_cue_geometry(&mut selected)
        .into_iter()
        .find(|cue| cue.width == 20.0 && cue.height == 16.0)
        .expect("selected target keeps its receipt backing");
    let selected_frame_outer_top = selected_agent_cue_segments(&mut selected)
        .into_iter()
        .map(|segment| segment.y + (segment.height / 2.0))
        .max_by(f32::total_cmp)
        .expect("selected target keeps its corner frame");
    assert!(
        selected_backing.y - (selected_backing.height / 2.0) - selected_frame_outer_top >= 3.99,
        "selected receipt badge must retain a four-pixel gap above the corner frame"
    );

    visible.update();
    assert_eq!(
        receipt_cue_geometry(&mut visible),
        initial,
        "an unchanged receipt reconcile must deterministically reuse its cue geometry"
    );

    {
        let mut runtime = visible.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = Some(sample_render_state_with_receipt_target(None, None));
        runtime.render_version += 1;
    }
    visible.update();
    assert!(
        receipt_cue_geometry(&mut visible).is_empty(),
        "removing the receipt must despawn all receipt-target cue geometry"
    );
    assert_eq!(
        normalized_hit_regions(&mut visible),
        baseline_regions,
        "receipt removal must leave agent interaction regions unchanged"
    );

    for (state, target) in [
        (None, None),
        (Some("completed"), Some("agent-not-rendered")),
    ] {
        let mut suppressed =
            render_test_app(sample_render_state_with_receipt_target(state, target));
        assert!(
            receipt_cue_geometry(&mut suppressed).is_empty(),
            "receipt state {state:?} targeting {target:?} must not render a cue"
        );
    }
}

#[test]
fn rejected_receipt_badge_uses_one_same_scale_slash_without_changing_hit_regions() {
    let mut blocked = render_test_app(sample_render_state_with_receipt_target(
        Some("blocked"),
        Some("agent-0"),
    ));
    let blocked_cues = receipt_cue_geometry(&mut blocked);
    let blocked_backing = blocked_cues
        .iter()
        .find(|cue| cue.width == 20.0 && cue.height == 16.0)
        .expect("blocked receipt backing");

    let mut rejected = render_test_app(sample_render_state_with_receipt_target(
        Some("rejected"),
        Some("agent-0"),
    ));
    let baseline_regions = normalized_hit_regions(&mut rejected);
    let initial = receipt_cue_geometry(&mut rejected);
    let backing = initial
        .iter()
        .find(|cue| cue.width == 20.0 && cue.height == 16.0)
        .expect("rejected receipt needs the same 20x16 badge backing");
    assert_eq!(backing.color, RECEIPT_BADGE_BACKING_COLOR);
    assert_eq!(
        (
            backing.x,
            backing.y,
            backing.z,
            backing.width,
            backing.height
        ),
        (
            blocked_backing.x,
            blocked_backing.y,
            blocked_backing.z,
            blocked_backing.width,
            blocked_backing.height
        ),
        "rejected receipt keeps the blocked badge's placement, scale, and layer"
    );
    let slashes = initial
        .iter()
        .filter(|cue| {
            cue.width == 12.0 && cue.height == 2.0 && cue.color == RECEIPT_BADGE_STROKE_COLOR
        })
        .collect::<Vec<_>>();
    assert_eq!(
        slashes.len(),
        1,
        "rejected receipt must be distinct from blocked's two-stroke X"
    );
    assert!(
        (slashes[0].rotation - std::f32::consts::FRAC_PI_4).abs() < f32::EPSILON,
        "rejected receipt uses a stable ascending rejection slash"
    );
    assert_eq!(
        normalized_hit_regions(&mut rejected),
        baseline_regions,
        "the display-only rejection slash must not add or move hit regions"
    );

    rejected.update();
    assert_eq!(
        receipt_cue_geometry(&mut rejected),
        initial,
        "unchanged rejected receipt reconcile must deterministically reuse its cue geometry"
    );
}

#[test]
fn narrow_receipt_target_badge_stays_visible_at_actual_canvas_size_with_selected_frame_clearance() {
    let mut render_state =
        sample_render_state_with_receipt_target(Some("blocked"), Some("agent-0"));
    render_state.selection = Some(Selection {
        kind: "agent".to_string(),
        id: "agent-0".to_string(),
    });
    let viewport_width = 318.0;
    let viewport_height = 179.0;
    let mut app =
        render_receipt_test_app_at_viewport(render_state, viewport_width, viewport_height);
    let cues = receipt_cue_geometry(&mut app);
    let backing = cues
        .iter()
        .find(|cue| cue.width == 28.0 && cue.height == 22.0)
        .expect("narrow receipt badge backing");
    assert_eq!(backing.color, NARROW_RECEIPT_BADGE_BACKING_COLOR);
    assert_eq!(
        cues.iter()
            .filter(|cue| cue.width == 18.0
                && cue.height == 3.0
                && cue.color == RECEIPT_BADGE_STROKE_COLOR)
            .count(),
        2,
        "narrow receipt badge keeps two enlarged pale X strokes"
    );
    assert_eq!(
        cues.iter()
            .filter(|cue| cue.color == NARROW_RECEIPT_BADGE_OUTLINE_COLOR)
            .count(),
        4,
        "narrow receipt badge needs four neutral outline segments"
    );
    let frame_outer_top = selected_agent_cue_segments(&mut app)
        .into_iter()
        .map(|segment| segment.y + (segment.height / 2.0))
        .max_by(f32::total_cmp)
        .expect("selected Agent corner frame");
    assert!(
        backing.y - (backing.height / 2.0) - frame_outer_top >= 7.99,
        "narrow receipt backing keeps an eight-pixel gap above the selected frame"
    );
    let agent = visual_probe_summary(&mut app)
        .agents
        .into_iter()
        .find(|agent| agent.id == "agent-0")
        .expect("selected receipt target Agent");
    assert!(
        backing.x + (backing.width / 2.0) < agent.x,
        "narrow selected receipt badge must use the target's upper-left shoulder, clear of the upper-right tooltip lane"
    );
    let backing_canvas_x = backing.x + (viewport_width / 2.0);
    let backing_canvas_y = (viewport_height / 2.0) - backing.y;
    assert!(
        agent.x + (agent.size_px / 2.0) <= viewport_width / 2.0
            && agent.x - (agent.size_px / 2.0) >= -(viewport_width / 2.0)
            && agent.y + (agent.size_px / 2.0) <= viewport_height / 2.0
            && agent.y - (agent.size_px / 2.0) >= -(viewport_height / 2.0),
        "selected target Agent must remain inside the actual narrow canvas"
    );
    assert!(
        backing_canvas_x - (backing.width / 2.0) >= 0.0
            && backing_canvas_x + (backing.width / 2.0) <= viewport_width
            && backing_canvas_y - (backing.height / 2.0) >= 0.0
            && backing_canvas_y + (backing.height / 2.0) <= viewport_height,
        "narrow receipt backing must remain fully inside the actual canvas"
    );
}

#[test]
fn narrow_rejected_receipt_badge_keeps_one_enlarged_slash() {
    let mut render_state =
        sample_render_state_with_receipt_target(Some("rejected"), Some("agent-0"));
    render_state.selection = Some(Selection {
        kind: "agent".to_string(),
        id: "agent-0".to_string(),
    });
    let mut app = render_receipt_test_app_at_viewport(render_state, 318.0, 179.0);
    let cues = receipt_cue_geometry(&mut app);
    assert_eq!(
        cues.iter()
            .filter(|cue| cue.width == 18.0
                && cue.height == 3.0
                && cue.color == RECEIPT_BADGE_STROKE_COLOR)
            .count(),
        1,
        "narrow rejected badge keeps one enlarged pale rejection slash"
    );
}

#[test]
fn selected_agent_corner_frame_has_exactly_four_amber_l_cues_above_the_core() {
    let mut app = render_test_app(sample_render_state_with_beacon_candidates(
        "agent", "agent-0",
    ));
    let summary = visual_probe_summary(&mut app);
    let selected = summary
        .agents
        .iter()
        .find(|agent| agent.id == "agent-0")
        .expect("selected agent");
    let core = summary
        .agent_cores
        .iter()
        .find(|agent| agent.id == "agent-0")
        .expect("selected agent core");
    let segments = selected_agent_cue_segments(&mut app);

    assert_eq!(
        segments.len(),
        8,
        "four L-shaped corner cues require one horizontal and one vertical segment per corner"
    );
    for x_sign in [-1.0_f32, 1.0] {
        for y_sign in [-1.0_f32, 1.0] {
            let corner = segments
                .iter()
                .filter(|segment| {
                    (segment.x - selected.x).signum() == x_sign
                        && (segment.y - selected.y).signum() == y_sign
                })
                .collect::<Vec<_>>();
            assert_eq!(corner.len(), 2, "each of the four corners has one L cue");
            assert!(
                corner.iter().any(|segment| segment.width > segment.height),
                "each L cue has a horizontal tick"
            );
            assert!(
                corner.iter().any(|segment| segment.height > segment.width),
                "each L cue has a vertical tick"
            );
        }
    }
    assert!(
        segments.iter().all(|segment| segment.z > core.z),
        "corner cues render above the selected agent core"
    );
    let selected_half_size = selected.size_px / 2.0;
    assert!(
        segments.iter().all(|segment| {
            (segment.x - selected.x).abs() > selected_half_size
                || (segment.y - selected.y).abs() > selected_half_size
        }),
        "corner cues remain outside the selected agent body"
    );
}

#[test]
fn selected_agent_corner_frame_is_absent_for_unselected_or_non_agent_selection() {
    for (kind, id) in [("location", "loc-0"), ("agent", "missing-agent")] {
        let mut app = render_test_app(sample_render_state_with_beacon_candidates(kind, id));
        assert!(
            selected_agent_cue_segments(&mut app).is_empty(),
            "{kind}:{id} must not receive selected-agent corner cues"
        );
    }
}

#[test]
fn selected_agent_corner_frame_reuses_geometry_across_repeat_and_agent_reorder() {
    let mut app = render_test_app(sample_render_state_with_beacon_candidates(
        "agent", "agent-0",
    ));
    let initial = selected_agent_cue_segments(&mut app);
    assert_eq!(initial.len(), 8);

    app.update();
    assert_eq!(
        selected_agent_cue_segments(&mut app),
        initial,
        "unchanged reconcile keeps stable cue geometry"
    );

    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        let render_state = runtime.render_state.as_mut().expect("render state");
        render_state.agents.reverse();
        runtime.render_version += 1;
    }
    app.update();
    assert_eq!(
        selected_agent_cue_segments(&mut app),
        initial,
        "DTO reorder must not move or duplicate the selected agent cue"
    );
}

#[test]
fn selected_agent_corner_frame_cleans_up_without_changing_hit_regions() {
    let mut selected_app = render_test_app(sample_render_state_with_beacon_candidates(
        "agent", "agent-0",
    ));
    let selected_regions = normalized_hit_regions(&mut selected_app);
    assert_eq!(selected_agent_cue_segments(&mut selected_app).len(), 8);

    let mut unselected_app = render_test_app({
        let mut state = sample_render_state_with_beacon_candidates("agent", "agent-0");
        state.selection = None;
        state
    });
    assert_eq!(
        normalized_hit_regions(&mut unselected_app),
        selected_regions,
        "visual selection cues must not change interaction hit regions"
    );

    {
        let mut runtime = selected_app.world_mut().resource_mut::<BevyRuntimeState>();
        let render_state = runtime.render_state.as_mut().expect("render state");
        render_state.selection = None;
        runtime.render_version += 1;
    }
    selected_app.update();
    assert!(
        selected_agent_cue_segments(&mut selected_app).is_empty(),
        "deselection removes every corner cue"
    );

    {
        let mut runtime = selected_app.world_mut().resource_mut::<BevyRuntimeState>();
        let render_state = runtime.render_state.as_mut().expect("render state");
        render_state.selection = Some(Selection {
            kind: "agent".to_string(),
            id: "agent-0".to_string(),
        });
        render_state.agents.retain(|agent| agent.id != "agent-0");
        runtime.render_version += 1;
        runtime.hit_regions_dirty = true;
    }
    selected_app.update();
    assert!(
        selected_agent_cue_segments(&mut selected_app).is_empty(),
        "removing the selected agent leaves no stale corner cue"
    );
}

#[path = "render_recommended_target_cue_tests.rs"]
mod recommended_target_cue_tests;
