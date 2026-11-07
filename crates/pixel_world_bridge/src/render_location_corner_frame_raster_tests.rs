use super::*;

fn raster_corner_frame_state() -> RenderState {
    let mut state = sample_render_state(12_000.0);
    state.selection = None;
    state.agents[0].pos = Some(sample_position(2_500_000.0, 1_500_000.0)); // Isolate the location frame from the Agent texture.
    state.locations[0].id = "loc-corner-raster".to_string();
    state.locations[0].marker_role = None;
    state.locations[0].marker_alpha = Some(0.72);
    // The minimum arm clamp keeps this deterministic raster at three pixels per arm.
    state.locations[0].size_hint_px = Some(10.0);
    state
}

fn normalized_raster_location_hits(app: &mut App) -> Vec<(String, i64, i64, i64, i64)> {
    let mut hits = hit_regions(app)
        .into_iter()
        .filter(|region| region.kind == "location")
        .map(|region| {
            (
                region.id,
                (region.left * 1_000.0).round() as i64,
                (region.top * 1_000.0).round() as i64,
                (region.right * 1_000.0).round() as i64,
                (region.bottom * 1_000.0).round() as i64,
            )
        })
        .collect::<Vec<_>>();
    hits.sort();
    hits
}

#[test]
fn location_corner_frame_raster_exports_mint_pixels_without_changing_location_base_or_hits() {
    let mut app = render_test_app(raster_corner_frame_state());
    let baseline_hits = normalized_raster_location_hits(&mut app);
    let (image, summary) = rasterize_pixel_regression(&mut app);

    assert_eq!(
        summary.location_corner_frame_pixels, 23,
        "two 2px-thick, three-pixel L brackets must contribute twenty-three dedicated corner-frame pixels"
    );
    assert_eq!(
        summary.location_sample_rgba,
        [82, 170, 138, 255],
        "the display-only frame must not obscure the terrain-composited mint base-location sample"
    );
    assert_eq!(
        normalized_raster_location_hits(&mut app),
        baseline_hits,
        "corner-frame raster decoration must not alter location hit semantics"
    );
    assert!(
        image
            .pixels()
            .any(|pixel| pixel.0[1] > pixel.0[0] && pixel.0[1] > pixel.0[2]),
        "the frame layer must leave visible mint-dominant pixels in the final raster"
    );

    println!(
        "location_corner_frame_raster pixels={} location_pixels={} location_sample={:?} hash={}",
        summary.location_corner_frame_pixels,
        summary.location_pixels,
        summary.location_sample_rgba,
        summary.raw_rgba_fnv1a64,
    );
    write_pixel_probe_if_requested(&image, &summary);
}
