use super::fixtures::{
    sample_render_state_with_receipt_target, sample_render_state_with_recommended_target,
};
use super::*;
use std::collections::BTreeSet;

fn selected_cue_raster_app(kind: &str, id: &str) -> App {
    let mut state = sample_render_state_with_beacon_candidates(kind, id);
    // Keep the world layers, but give the selected cue an unobscured, integer
    // pixel center at (480, 270). This is a fixed raster fixture, not a readback
    // of the cue's own geometry that would accept the same placement bug.
    state.agents[0].pos = Some(if kind == "agent" {
        sample_position(1_500_000.0, 1_000_000.0)
    } else {
        sample_position(1_850_000.0, 1_150_000.0)
    });
    let mut app = render_test_app(state);
    app.world_mut()
        .resource_mut::<BevyRuntimeState>()
        .reduced_motion = true;
    app.update();
    app
}

fn selected_cue_amber_mask(image: &RgbaImage, exclude_agent_body: bool) -> BTreeSet<(i32, i32)> {
    let mut pixels = BTreeSet::new();
    for y in -20_i32..20 {
        for x in -20_i32..20 {
            // The selected Agent body is also amber. Its pixels cannot serve
            // as evidence for the separate corner frame around it.
            if exclude_agent_body && x.abs() <= 8 && (-15..=1).contains(&y) {
                continue;
            }
            if image.get_pixel((480 + x) as u32, (270 + y) as u32).0 == [251, 191, 36, 255] {
                pixels.insert((x, y));
            }
        }
    }
    pixels
}

fn selected_location_ring_mask() -> BTreeSet<(i32, i32)> {
    (-11..11)
        .flat_map(|y| (-11..11).map(move |x| (x, y)))
        .filter(|(x, y)| !(-9..9).contains(x) || !(-9..9).contains(y))
        .collect()
}

fn selected_agent_corner_mask() -> BTreeSet<(i32, i32)> {
    // Independent integer-pixel contract for the 16px fixture marker:
    // four 6x2 / 2x6 L corners, with open gaps between them.
    [
        (-12, -20, 6, 2),
        (-13, -19, 2, 6),
        (6, -20, 6, 2),
        (11, -19, 2, 6),
        (-12, 4, 6, 2),
        (-13, -1, 2, 6),
        (6, 4, 6, 2),
        (11, -1, 2, 6),
    ]
    .into_iter()
    .flat_map(|(left, top, width, height)| {
        (top..top + height).flat_map(move |y| (left..left + width).map(move |x| (x, y)))
    })
    .collect()
}

#[test]
fn reduced_motion_freezes_pixels_across_reconcile_and_resumes_on_preference_change() {
    let mut app = render_test_app(sample_render_state(12_000.0));
    app.world_mut()
        .resource_mut::<BevyRuntimeState>()
        .reduced_motion = true;
    app.update();
    let (first, _) = rasterize_pixel_regression(&mut app);
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(std::time::Duration::from_millis(350));
    app.update();
    let (frozen, _) = rasterize_pixel_regression(&mut app);
    assert_eq!(first, frozen, "static reconcile must not restart motion");
    app.world_mut()
        .resource_mut::<BevyRuntimeState>()
        .reduced_motion = false;
    app.update();
    let (resumed, _) = rasterize_pixel_regression(&mut app);
    assert_eq!(
        frozen, resumed,
        "static shared Agent pixels must remain stable when motion preference changes"
    );
}

#[test]
fn bevy_pixel_regression_exports_selected_location_ring_with_world_layers() {
    let mut app = selected_cue_raster_app("location", "loc-0");
    let (image, summary) = rasterize_pixel_regression(&mut app);

    assert!(summary.grid_pixels > 0);
    assert!(summary.fragment_pixels > 0);
    assert!(summary.location_pixels > 0);
    assert_eq!(summary.selected_location_cue_pixels, 160);
    assert!(summary.agent_pixels > 0);
    assert!(summary.agent_pixels > summary.selected_location_cue_pixels);
    assert_eq!(
        selected_cue_amber_mask(&image, false),
        selected_location_ring_mask(),
        "selected location must retain the complete 2px ring and its hollow center"
    );

    write_pixel_probe_if_requested(&image, &summary);
}

#[test]
fn bevy_pixel_regression_exports_selected_agent_corner_cue_with_world_layers() {
    let mut app = selected_cue_raster_app("agent", "agent-0");
    let (image, summary) = rasterize_pixel_regression(&mut app);

    assert!(summary.grid_pixels > 0);
    assert!(summary.agent_pixels > 0);
    assert!(summary.agent_core_pixels > 0);
    assert_eq!(summary.selected_agent_cue_pixels, 92);
    let expected = selected_agent_corner_mask();
    assert_eq!(
        selected_cue_amber_mask(&image, true),
        expected,
        "all eight amber arms must surround the body with open gaps between corners"
    );

    write_pixel_probe_if_requested(&image, &summary);

    let world = app.world_mut();
    let missing_arm = world
        .query::<(Entity, &PixelWorldSelectedAgentCue)>()
        .iter(world)
        .next()
        .map(|(entity, _)| entity)
        .expect("selected Agent has corner arms");
    world.despawn(missing_arm);
    let (incomplete, _) = rasterize_pixel_regression(&mut app);
    assert!(
        incomplete
            .pixels()
            .any(|pixel| pixel.0 == [251, 191, 36, 255]),
        "negative control retains amber body pixels that satisfied the old presence check"
    );
    assert_ne!(
        selected_cue_amber_mask(&incomplete, true),
        expected,
        "a missing arm must fail the shape contract even when amber pixels remain"
    );
}

#[test]
fn bevy_pixel_regression_exports_only_location_derived_position_cues() {
    let mut derived_state = sample_render_state(12_000.0);
    derived_state.selection = None;
    let mut derived = render_test_app(derived_state);
    let (image, derived_summary) = rasterize_pixel_regression(&mut derived);
    let mut snapshot_state = sample_render_state(12_000.0);
    snapshot_state.selection = None;
    snapshot_state.agents[0].position_source = AgentPositionSource::Snapshot;
    let mut snapshot = render_test_app(snapshot_state);
    let (_, snapshot_summary) = rasterize_pixel_regression(&mut snapshot);

    assert!(
        derived_summary.derived_position_cue_pixels > 0,
        "a location-derived Agent position must contribute a visible hollow cue to the raster"
    );
    assert_eq!(
        snapshot_summary.derived_position_cue_pixels, 0,
        "snapshot positions must not contribute provenance cue pixels"
    );
    write_pixel_probe_if_requested(&image, &derived_summary);
}

#[test]
fn bevy_pixel_regression_exports_visible_receipt_target_pixels() {
    let mut blocked = render_test_app(sample_render_state_with_receipt_target(
        Some("blocked"),
        Some("agent-0"),
    ));
    let (_, blocked_summary) = rasterize_pixel_regression(&mut blocked);
    let mut rejected = render_test_app(sample_render_state_with_receipt_target(
        Some("rejected"),
        Some("agent-0"),
    ));
    let (rejected_image, rejected_summary) = rasterize_pixel_regression(&mut rejected);
    let mut absent = render_test_app(sample_render_state_with_receipt_target(None, None));
    let (_, absent_summary) = rasterize_pixel_regression(&mut absent);

    assert!(
        rejected_summary.non_background_pixels > absent_summary.non_background_pixels,
        "a rejected receipt cue must contribute visible raster pixels above its target Agent"
    );
    assert_ne!(
        rejected_summary.raw_rgba_fnv1a64, blocked_summary.raw_rgba_fnv1a64,
        "rejected receipt raster must remain visibly distinct from blocked's X badge"
    );
    write_pixel_probe_if_requested(&rejected_image, &rejected_summary);
}

#[test]
fn bevy_pixel_regression_exports_recommended_target_wayfinder_pixels() {
    let mut visible = render_test_app(sample_render_state_with_recommended_target(Some("agent-0")));
    let (_, visible_summary) = rasterize_pixel_regression(&mut visible);
    let mut absent = render_test_app(sample_render_state_with_recommended_target(None));
    let (_, absent_summary) = rasterize_pixel_regression(&mut absent);

    assert!(
        visible_summary.non_background_pixels > absent_summary.non_background_pixels,
        "the recommended target bracket must contribute visible pixels above its target Agent"
    );
}
