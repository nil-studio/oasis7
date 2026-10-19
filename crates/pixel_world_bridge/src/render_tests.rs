use super::*;
use bevy::app::{App, Update};
use bevy::color::Alpha;
use bevy::window::Window;
use image::{Rgba, RgbaImage};
use serde::Serialize;
use std::fs;
use std::path::Path;
#[path = "render_test_fixtures.rs"]
mod fixtures;
#[path = "render_raster_readback.rs"]
mod raster_readback;
use fixtures::{
    sample_render_state_with_beacon_candidates, sample_render_state_with_hotspot_candidates,
    sample_render_state_with_selection, sample_render_state_with_unoccluded_detail_fleck,
    test_runtime,
};
use raster_readback::PixelRegressionSummary;
const VIEWPORT_WIDTH: u32 = 960;
const VIEWPORT_HEIGHT: u32 = 540;
const PIXEL_BACKGROUND: [u8; 4] = [8, 12, 20, 255];
#[derive(Clone, Debug, Serialize)]
struct VisualProbeRow {
    id: String,
    size_px: f32,
    alpha: f32,
    z: f32,
    x: f32,
    y: f32,
}
#[derive(Clone, Debug, Serialize)]
struct VisualProbeSummary {
    fragments: Vec<VisualProbeRow>,
    fragment_shadows: Vec<VisualProbeRow>,
    fragment_insets: Vec<VisualProbeRow>,
    fragment_flecks: Vec<VisualProbeRow>,
    locations: Vec<VisualProbeRow>,
    agents: Vec<VisualProbeRow>,
    agent_cores: Vec<VisualProbeRow>,
    hotspots: Vec<VisualProbeRow>,
    hotspot_cores: Vec<VisualProbeRow>,
    selected_location_cues: Vec<VisualProbeRow>,
    hit_regions: usize,
    fragment_entity_cache_size: usize,
    fragment_shadow_entity_count: usize,
    fragment_inset_entity_count: usize,
    fragment_fleck_entity_count: usize,
    location_entity_cache_size: usize,
    agent_entity_cache_size: usize,
    agent_core_entity_count: usize,
    hotspot_entity_cache_size: usize,
    hotspot_core_entity_count: usize,
}
#[derive(Clone, Debug)]
struct PixelLayer {
    kind: &'static str,
    center_x: f32,
    center_y: f32,
    size: Vec2,
    rotation: f32,
    z: f32,
    rgba: [f32; 4],
}
fn sample_position(x_cm: f64, y_cm: f64) -> Position {
    Position {
        x_cm,
        y_cm,
        z_cm: 0.0,
    }
}
fn sample_render_state(fragment_footprint_cm: f64) -> RenderState {
    RenderState {
        exploration_mode: false,
        navigation: None,
        locale: "en".to_string(),
        world_bounds: Some(WorldBounds {
            width_cm: 3_000_000.0,
            depth_cm: 2_000_000.0,
            height_cm: 500_000.0,
        }),
        locations: vec![Location {
            id: "loc-0".to_string(),
            label: "Fragment Field Anchor".to_string(),
            pos: sample_position(1_500_000.0, 1_000_000.0),
            radius_cm: 30_000.0,
            resource_summary: "-".to_string(),
            size_hint_px: Some(10.0),
            marker_role: Some("logic_anchor".to_string()),
            marker_alpha: Some(0.32),
        }],
        fragment_terrain: vec![FragmentTerrainPatch {
            id: "fragment:loc-0:0".to_string(),
            location_id: "loc-0".to_string(),
            pos: sample_position(1_503_000.0, 1_006_000.0),
            footprint_cm: fragment_footprint_cm,
            dominant_compound: "silicate_matrix".to_string(),
            color: [141, 199, 170],
            emphasis: Some(0.58),
        }],
        micro_depot_facilities: vec![],
        module_visual_entities: vec![],
        agents: vec![Agent {
            id: "agent-0".to_string(),
            label: "Survey Agent".to_string(),
            pos: Some(sample_position(1_520_000.0, 1_015_000.0)),
            location_id: Some("loc-0".to_string()),
            resource_summary: "-".to_string(),
            status_badges: vec!["position=location_derived".to_string()],
            position_source: AgentPositionSource::LocationDerived,
            size_hint_px: Some(16.0),
            power_state: None,
        }],
        links: vec![],
        social_links: vec![],
        visual_hotspots: vec![],
        industry_aggregates: vec![],
        world_events: vec![],
        selection: Some(Selection {
            kind: "agent".to_string(),
            id: "agent-0".to_string(),
        }),
        receipt_target: None,
        recommended_target: None,
        active_intent_target: None,
    }
}
fn render_test_app(render_state: RenderState) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(test_runtime(render_state));
    app.world_mut().spawn((
        Window {
            resolution: (VIEWPORT_WIDTH, VIEWPORT_HEIGHT).into(),
            ..Default::default()
        },
        PrimaryWindow,
    ));
    app.add_systems(
        Update,
        (render_scene, label_budget::enforce_label_budget).chain(),
    );
    app.update();
    app
}
fn row(id: &str, sprite: &Sprite, transform: &Transform) -> VisualProbeRow {
    let size = sprite
        .custom_size
        .map(|custom_size| custom_size.x.max(custom_size.y))
        .unwrap_or(0.0);
    VisualProbeRow {
        id: id.to_string(),
        size_px: size,
        alpha: sprite.color.alpha(),
        z: transform.translation.z,
        x: transform.translation.x,
        y: transform.translation.y,
    }
}
fn visual_probe_summary(app: &mut App) -> VisualProbeSummary {
    let world = app.world_mut();
    let mut fragment_query = world.query::<(&PixelWorldFragmentVisual, &Sprite, &Transform)>();
    let fragments = fragment_query
        .iter(world)
        .map(|(visual, sprite, transform)| row(&visual.id, sprite, transform))
        .collect();
    let mut fragment_shadow_query =
        world.query::<(&PixelWorldFragmentShadowVisual, &Sprite, &Transform)>();
    let fragment_shadows = fragment_shadow_query
        .iter(world)
        .map(|(visual, sprite, transform)| row(&visual.id, sprite, transform))
        .collect::<Vec<_>>();
    let mut fragment_inset_query =
        world.query::<(&PixelWorldFragmentInsetVisual, &Sprite, &Transform)>();
    let fragment_insets = fragment_inset_query
        .iter(world)
        .map(|(visual, sprite, transform)| row(&visual.id, sprite, transform))
        .collect::<Vec<_>>();
    let mut fragment_fleck_query =
        world.query::<(&PixelWorldFragmentFleckVisual, &Sprite, &Transform)>();
    let fragment_flecks = fragment_fleck_query
        .iter(world)
        .map(|(visual, sprite, transform)| row(&visual.id, sprite, transform))
        .collect::<Vec<_>>();
    let mut location_query = world.query::<(&PixelWorldLocationVisual, &Sprite, &Transform)>();
    let locations = location_query
        .iter(world)
        .map(|(visual, sprite, transform)| row(&visual.id, sprite, transform))
        .collect();
    let mut agent_query = world.query::<(&PixelWorldAgentVisual, &Sprite, &Transform)>();
    let agents = agent_query
        .iter(world)
        .map(|(visual, sprite, transform)| row(&visual.id, sprite, transform))
        .collect();
    let mut agent_core_query = world.query::<(&PixelWorldAgentCoreVisual, &Sprite, &Transform)>();
    let agent_cores = agent_core_query
        .iter(world)
        .map(|(visual, sprite, transform)| row(&visual.id, sprite, transform))
        .collect::<Vec<_>>();
    let mut hotspot_query = world.query::<(&PixelWorldHotspotVisual, &Sprite, &Transform)>();
    let hotspots = hotspot_query
        .iter(world)
        .map(|(visual, sprite, transform)| row(&visual.id, sprite, transform))
        .collect::<Vec<_>>();
    let mut hotspot_core_query =
        world.query::<(&PixelWorldHotspotCoreVisual, &Sprite, &Transform)>();
    let hotspot_cores = hotspot_core_query
        .iter(world)
        .map(|(visual, sprite, transform)| row(&visual.id, sprite, transform))
        .collect::<Vec<_>>();
    let mut selected_location_cue_query =
        world.query::<(&PixelWorldSelectedLocationCue, &Sprite, &Transform)>();
    let selected_location_cues = selected_location_cue_query
        .iter(world)
        .map(|(cue, sprite, transform)| row(&cue.location_id, sprite, transform))
        .collect();
    let runtime = world.resource::<BevyRuntimeState>();
    VisualProbeSummary {
        fragments,
        fragment_shadows,
        fragment_insets,
        fragment_flecks,
        locations,
        agents,
        agent_cores,
        hotspots,
        hotspot_cores,
        selected_location_cues,
        hit_regions: runtime.hit_regions.len(),
        fragment_entity_cache_size: runtime.fragment_entities.len(),
        fragment_shadow_entity_count: fragment_shadow_query.iter(world).count(),
        fragment_inset_entity_count: fragment_inset_query.iter(world).count(),
        fragment_fleck_entity_count: fragment_fleck_query.iter(world).count(),
        location_entity_cache_size: runtime.location_entities.len(),
        agent_entity_cache_size: runtime.agent_entities.len(),
        agent_core_entity_count: agent_core_query.iter(world).count(),
        hotspot_entity_cache_size: runtime.hotspot_entities.len(),
        hotspot_core_entity_count: hotspot_core_query.iter(world).count(),
    }
}
fn hit_regions(app: &mut App) -> Vec<HitRegion> {
    app.world_mut()
        .resource::<BevyRuntimeState>()
        .hit_regions
        .clone()
}

#[path = "render_navigation_tests.rs"]
mod navigation_tests;

fn pixel_layer(kind: &'static str, sprite: &Sprite, transform: &Transform) -> PixelLayer {
    let size_px = sprite
        .custom_size
        .map(|size| size.x.max(size.y))
        .unwrap_or(0.0);
    let color = sprite.color.to_srgba();
    PixelLayer {
        kind,
        center_x: (VIEWPORT_WIDTH as f32 / 2.0) + transform.translation.x,
        center_y: (VIEWPORT_HEIGHT as f32 / 2.0) - transform.translation.y,
        size: sprite.custom_size.unwrap_or(Vec2::splat(size_px)),
        // Canvas Y points down; both position and rotation must change basis.
        rotation: -transform.rotation.to_euler(EulerRot::XYZ).2,
        z: transform.translation.z,
        rgba: [color.red, color.green, color.blue, color.alpha],
    }
}
#[path = "render_material_pixel_layers.rs"]
mod material_pixel_layers;
fn collect_pixel_layers(app: &mut App) -> Vec<PixelLayer> {
    let world = app.world_mut();
    let mut layers = Vec::new();
    material_pixel_layers::collect(world, &mut layers);
    let mut grid_query = world.query::<(&PixelWorldGridVisual, &Sprite, &Transform)>();
    layers.extend(
        grid_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("grid", sprite, transform)),
    );
    let mut selected_agent_cue_query =
        world.query::<(&PixelWorldSelectedAgentCue, &Sprite, &Transform)>();
    layers.extend(
        selected_agent_cue_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("selected_agent_cue", sprite, transform)),
    );
    let mut active_intent_cue_query =
        world.query::<(&PixelWorldActiveIntentCue, &Sprite, &Transform)>();
    layers.extend(
        active_intent_cue_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("active_intent_cue", sprite, transform)),
    );
    let mut derived_position_cue_query =
        world.query::<(&PixelWorldDerivedPositionCue, &Sprite, &Transform)>();
    layers.extend(
        derived_position_cue_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("derived_position_cue", sprite, transform)),
    );
    let mut missing_position_cue_query =
        world.query::<(&PixelWorldMissingPositionCue, &Sprite, &Transform)>();
    layers.extend(
        missing_position_cue_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("missing_position_cue", sprite, transform)),
    );
    let mut receipt_target_cue_query =
        world.query::<(&PixelWorldReceiptTargetCue, &Sprite, &Transform)>();
    layers.extend(
        receipt_target_cue_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("receipt_target_cue", sprite, transform)),
    );
    let mut recommended_target_cue_query =
        world.query::<(&PixelWorldRecommendedTargetCue, &Sprite, &Transform)>();
    layers.extend(
        recommended_target_cue_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("recommended_target_cue", sprite, transform)),
    );
    let mut fragment_shadow_query =
        world.query::<(&PixelWorldFragmentShadowVisual, &Sprite, &Transform)>();
    layers.extend(
        fragment_shadow_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("fragment_shadow", sprite, transform)),
    );
    let mut fragment_query = world.query::<(&PixelWorldFragmentVisual, &Sprite, &Transform)>();
    layers.extend(
        fragment_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("fragment", sprite, transform)),
    );
    let mut fragment_inset_query =
        world.query::<(&PixelWorldFragmentInsetVisual, &Sprite, &Transform)>();
    layers.extend(
        fragment_inset_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("fragment_inset", sprite, transform)),
    );
    let mut fragment_fleck_query =
        world.query::<(&PixelWorldFragmentFleckVisual, &Sprite, &Transform)>();
    layers.extend(
        fragment_fleck_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("fragment_fleck", sprite, transform)),
    );
    let mut selected_location_cue_query =
        world.query::<(&PixelWorldSelectedLocationCue, &Sprite, &Transform)>();
    layers.extend(
        selected_location_cue_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("selected_location_cue", sprite, transform)),
    );
    let mut location_corner_frame_query =
        world.query::<(&PixelWorldLocationCornerFrame, &Sprite, &Transform)>();
    layers.extend(
        location_corner_frame_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("location_corner_frame", sprite, transform)),
    );
    let mut location_resource_cue_query =
        world.query::<(&PixelWorldLocationResourceCue, &Sprite, &Transform)>();
    layers.extend(
        location_resource_cue_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("location_resource_cue", sprite, transform)),
    );
    let mut location_query = world.query::<(&PixelWorldLocationVisual, &Sprite, &Transform)>();
    layers.extend(
        location_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("location", sprite, transform)),
    );
    let mut agent_query = world.query::<(&PixelWorldAgentVisual, &Sprite, &Transform)>();
    layers.extend(
        agent_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("agent", sprite, transform)),
    );
    let mut agent_core_query = world.query::<(&PixelWorldAgentCoreVisual, &Sprite, &Transform)>();
    layers.extend(
        agent_core_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("agent_core", sprite, transform)),
    );
    let mut agent_power_query = world.query::<(&PixelWorldAgentPowerCue, &Sprite, &Transform)>();
    layers.extend(
        agent_power_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("agent_power_cue", sprite, transform)),
    );
    let mut hotspot_query = world.query::<(&PixelWorldHotspotVisual, &Sprite, &Transform)>();
    layers.extend(
        hotspot_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("hotspot", sprite, transform)),
    );
    let mut hotspot_core_query =
        world.query::<(&PixelWorldHotspotCoreVisual, &Sprite, &Transform)>();
    layers.extend(
        hotspot_core_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("hotspot_core", sprite, transform)),
    );
    let mut assignment_cue_query =
        world.query::<(&PixelWorldAssignmentCueVisual, &Sprite, &Transform)>();
    layers.extend(
        assignment_cue_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("assignment_cue", sprite, transform)),
    );
    let mut social_link_query = world.query::<(&PixelWorldSocialLinkVisual, &Sprite, &Transform)>();
    layers.extend(
        social_link_query
            .iter(world)
            .map(|(_, sprite, transform)| pixel_layer("social_link", sprite, transform)),
    );
    layers.sort_by(|left, right| {
        left.z
            .partial_cmp(&right.z)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    layers
}
fn blend_src_over(src_rgba: [f32; 4], dst: [u8; 4]) -> [u8; 4] {
    let src_alpha = src_rgba[3].clamp(0.0, 1.0);
    let dst_alpha = (dst[3] as f32 / 255.0).clamp(0.0, 1.0);
    let out_alpha = src_alpha + (dst_alpha * (1.0 - src_alpha));
    let mut out = [0u8; 4];
    for channel in 0..3 {
        let src = src_rgba[channel].clamp(0.0, 1.0);
        let dst = (dst[channel] as f32 / 255.0).clamp(0.0, 1.0);
        let blended = if out_alpha <= f32::EPSILON {
            0.0
        } else {
            ((src * src_alpha) + (dst * dst_alpha * (1.0 - src_alpha))) / out_alpha
        };
        out[channel] = (blended * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
    out
}
fn layer_kind_id(kind: &str) -> u8 {
    match kind {
        "grid" => 1,
        "fragment" => 2,
        "fragment_shadow" => 7,
        "fragment_inset" => 6,
        "fragment_fleck" => 8,
        "location" => 3,
        "selected_location_cue" => 4,
        "location_resource_cue" => 15,
        "agent" => 5,
        "agent_core" => 9,
        "hotspot" => 10,
        "hotspot_core" => 11,
        "selected_agent_cue" => 12,
        "derived_position_cue" => 13,
        "missing_position_cue" => 16,
        "agent_power_cue" => 18,
        "active_intent_cue" => 19,
        "location_corner_frame" => 14,
        "social_link" => 17,
        _ => 0,
    }
}
fn fnv1a64(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
fn sample_pixel(image: &RgbaImage, layer: &PixelLayer) -> [u8; 4] {
    let x = layer
        .center_x
        .round()
        .clamp(0.0, (VIEWPORT_WIDTH - 1) as f32) as u32;
    let y = layer
        .center_y
        .round()
        .clamp(0.0, (VIEWPORT_HEIGHT - 1) as f32) as u32;
    image.get_pixel(x, y).0
}
fn rasterize_pixel_regression(app: &mut App) -> (RgbaImage, PixelRegressionSummary) {
    let layers = collect_pixel_layers(app);
    let mut image = RgbaImage::from_pixel(VIEWPORT_WIDTH, VIEWPORT_HEIGHT, Rgba(PIXEL_BACKGROUND));
    let mut kind_buffer = vec![0u8; (VIEWPORT_WIDTH * VIEWPORT_HEIGHT) as usize];
    for layer in &layers {
        let width = layer.size.x.round().max(1.0) as i32;
        let height = layer.size.y.round().max(1.0) as i32;
        let is_rotated = layer.rotation.abs() > f32::EPSILON;
        let (sin, cos) = layer.rotation.sin_cos();
        let (left, top, right, bottom) = if is_rotated {
            let half_width = (layer.size.x * cos.abs()) + (layer.size.y * sin.abs());
            let half_height = (layer.size.x * sin.abs()) + (layer.size.y * cos.abs());
            let left = (layer.center_x - (half_width / 2.0)).floor() as i32;
            let top = (layer.center_y - (half_height / 2.0)).floor() as i32;
            let right = (layer.center_x + (half_width / 2.0)).ceil() as i32;
            let bottom = (layer.center_y + (half_height / 2.0)).ceil() as i32;
            (left, top, right, bottom)
        } else {
            let left = layer.center_x.round() as i32 - (width / 2);
            let top = layer.center_y.round() as i32 - (height / 2);
            (left, top, left + width, top + height)
        };
        for y in top.max(0)..bottom.min(VIEWPORT_HEIGHT as i32) {
            for x in left.max(0)..right.min(VIEWPORT_WIDTH as i32) {
                if is_rotated {
                    let relative_x = (x as f32 + 0.5) - layer.center_x;
                    let relative_y = (y as f32 + 0.5) - layer.center_y;
                    let local_x = (cos * relative_x) + (sin * relative_y);
                    let local_y = (-sin * relative_x) + (cos * relative_y);
                    if local_x.abs() > layer.size.x / 2.0 || local_y.abs() > layer.size.y / 2.0 {
                        continue;
                    }
                }
                let pixel = image.get_pixel_mut(x as u32, y as u32);
                let next = blend_src_over(layer.rgba, pixel.0);
                pixel.0 = next;
                kind_buffer[(y as u32 * VIEWPORT_WIDTH + x as u32) as usize] =
                    layer_kind_id(layer.kind);
            }
        }
    }
    let non_background_pixels = image
        .pixels()
        .filter(|pixel| pixel.0 != PIXEL_BACKGROUND)
        .count();
    let grid_pixels = kind_buffer.iter().filter(|kind| **kind == 1).count();
    let fragment_pixels = kind_buffer.iter().filter(|kind| **kind == 2).count();
    let fragment_fleck_pixels = kind_buffer.iter().filter(|kind| **kind == 8).count();
    let location_pixels = kind_buffer.iter().filter(|kind| **kind == 3).count();
    let location_corner_frame_pixels = kind_buffer.iter().filter(|kind| **kind == 14).count();
    let location_resource_cue_pixels = kind_buffer.iter().filter(|kind| **kind == 15).count();
    let selected_location_cue_pixels = kind_buffer.iter().filter(|kind| **kind == 4).count();
    let agent_pixels = kind_buffer.iter().filter(|kind| **kind == 5).count();
    let agent_core_pixels = kind_buffer.iter().filter(|kind| **kind == 9).count();
    let selected_agent_cue_pixels = kind_buffer.iter().filter(|kind| **kind == 12).count();
    let derived_position_cue_pixels = kind_buffer.iter().filter(|kind| **kind == 13).count();
    let missing_position_cue_pixels = kind_buffer.iter().filter(|kind| **kind == 16).count();
    let agent_power_cue_pixels = kind_buffer.iter().filter(|kind| **kind == 18).count();
    let hotspot_pixels = kind_buffer.iter().filter(|kind| **kind == 10).count();
    let hotspot_core_pixels = kind_buffer.iter().filter(|kind| **kind == 11).count();
    let fragment_layer = layers
        .iter()
        .find(|layer| layer.kind == "fragment")
        .expect("pixel regression fragment layer");
    let location_layer = layers
        .iter()
        .find(|layer| layer.kind == "location")
        .expect("pixel regression location layer");
    let fragment_fleck_layer = layers.iter().find(|layer| layer.kind == "fragment_fleck");
    let agent_layer = layers
        .iter()
        .find(|layer| layer.kind == "agent")
        .expect("pixel regression agent layer");
    let agent_core_layer = layers
        .iter()
        .find(|layer| layer.kind == "agent_core")
        .expect("pixel regression agent core layer");
    let hotspot_core_layer = layers.iter().find(|layer| layer.kind == "hotspot_core");
    let summary = PixelRegressionSummary {
        width: VIEWPORT_WIDTH,
        height: VIEWPORT_HEIGHT,
        raw_rgba_fnv1a64: fnv1a64(image.as_raw()),
        non_background_pixels,
        grid_pixels,
        fragment_pixels,
        fragment_fleck_pixels,
        location_pixels,
        location_resource_cue_pixels,
        location_corner_frame_pixels,
        selected_location_cue_pixels,
        selected_agent_cue_pixels,
        derived_position_cue_pixels,
        missing_position_cue_pixels,
        agent_power_cue_pixels,
        agent_pixels,
        agent_core_pixels,
        hotspot_pixels,
        hotspot_core_pixels,
        fragment_sample_rgba: sample_pixel(&image, fragment_layer),
        fragment_fleck_sample_rgba: fragment_fleck_layer
            .map(|layer| sample_pixel(&image, layer))
            .unwrap_or(PIXEL_BACKGROUND),
        location_sample_rgba: sample_pixel(&image, location_layer),
        agent_sample_rgba: sample_pixel(&image, agent_layer),
        agent_core_sample_rgba: sample_pixel(&image, agent_core_layer),
        hotspot_core_sample_rgba: hotspot_core_layer
            .map(|layer| sample_pixel(&image, layer))
            .unwrap_or(PIXEL_BACKGROUND),
    };
    (image, summary)
}
fn write_probe_summary_if_requested(summary: &VisualProbeSummary) {
    let Ok(out_dir) = std::env::var("PIXEL_WORLD_BEVY_RENDER_PROBE_OUT_DIR") else {
        return;
    };
    let out_dir = Path::new(&out_dir);
    fs::create_dir_all(out_dir).expect("create bevy render probe output directory");
    let summary_json =
        serde_json::to_string_pretty(summary).expect("serialize bevy render probe summary");
    fs::write(out_dir.join("summary.json"), summary_json).expect("write bevy render probe summary");
}
fn write_pixel_probe_if_requested(image: &RgbaImage, summary: &PixelRegressionSummary) {
    let Ok(out_dir) = std::env::var("PIXEL_WORLD_BEVY_PIXEL_PROBE_OUT_DIR") else {
        return;
    };
    let out_dir = Path::new(&out_dir);
    fs::create_dir_all(out_dir).expect("create bevy pixel probe output directory");
    let summary_json =
        serde_json::to_string_pretty(summary).expect("serialize bevy pixel probe summary");
    fs::write(out_dir.join("pixel-summary.json"), summary_json)
        .expect("write bevy pixel probe summary");
    image
        .save(out_dir.join("pixel-regression.png"))
        .expect("write bevy pixel probe png");
    zoom_non_background_pixels(image, 12)
        .save(out_dir.join("pixel-regression-crop.png"))
        .expect("write bevy pixel probe crop png");
    if summary.fragment_fleck_pixels > 0 {
        zoom_pixels_matching(image, summary.fragment_fleck_sample_rgba, 48)
            .expect("find visible fragment fleck pixels")
            .save(out_dir.join("pixel-regression-fleck-crop.png"))
            .expect("write bevy pixel fleck crop png");
    }
}
fn zoom_non_background_pixels(image: &RgbaImage, scale: u32) -> RgbaImage {
    zoom_matching_pixels(image, |pixel| pixel != PIXEL_BACKGROUND, scale)
        .expect("find non-background pixels")
}
fn zoom_pixels_matching(image: &RgbaImage, target: [u8; 4], scale: u32) -> Option<RgbaImage> {
    zoom_matching_pixels(image, |pixel| pixel == target, scale)
}
fn zoom_matching_pixels(
    image: &RgbaImage,
    matches: impl Fn([u8; 4]) -> bool,
    scale: u32,
) -> Option<RgbaImage> {
    let mut min_x = VIEWPORT_WIDTH;
    let mut min_y = VIEWPORT_HEIGHT;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut found = false;
    for (x, y, pixel) in image.enumerate_pixels() {
        if !matches(pixel.0) {
            continue;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
        found = true;
    }
    if !found {
        return None;
    }
    let padding = 12u32;
    min_x = min_x.saturating_sub(padding);
    min_y = min_y.saturating_sub(padding);
    max_x = (max_x + padding).min(VIEWPORT_WIDTH - 1);
    max_y = (max_y + padding).min(VIEWPORT_HEIGHT - 1);
    let crop_width = max_x - min_x + 1;
    let crop_height = max_y - min_y + 1;
    let scale = scale.min((2048 / crop_width.max(crop_height)).max(1));
    let mut zoomed = RgbaImage::from_pixel(
        crop_width * scale,
        crop_height * scale,
        Rgba(PIXEL_BACKGROUND),
    );
    for y in 0..crop_height {
        for x in 0..crop_width {
            let source = *image.get_pixel(min_x + x, min_y + y);
            for yy in 0..scale {
                for xx in 0..scale {
                    zoomed.put_pixel((x * scale) + xx, (y * scale) + yy, source);
                }
            }
        }
    }
    Some(zoomed)
}
#[test]
fn bevy_ecs_reconciles_fragment_location_agent_visuals_from_render_state() {
    let mut app = render_test_app(sample_render_state(12_000.0));
    let summary = visual_probe_summary(&mut app);
    assert_eq!(summary.fragments.len(), 1);
    assert_eq!(summary.locations.len(), 1);
    assert_eq!(summary.agents.len(), 1);
    assert_eq!(summary.agent_cores.len(), 1);
    assert_eq!(summary.fragments[0].id, "fragment:loc-0:0");
    assert_eq!(summary.locations[0].id, "loc-0");
    assert_eq!(summary.agents[0].id, "agent-0");
    assert_eq!(summary.agent_cores[0].id, "agent-0");
    assert!(summary.fragments[0].size_px < summary.locations[0].size_px);
    assert!(summary.locations[0].size_px < summary.agents[0].size_px);
    assert!(summary.fragments[0].alpha < summary.locations[0].alpha);
    assert!(summary.locations[0].alpha < 0.5);
    assert!(summary.fragments[0].z < summary.locations[0].z);
    assert!(summary.locations[0].z < summary.agents[0].z);
    assert_eq!(
        summary.agent_cores[0].size_px,
        agent_core_size_px(&sample_render_state(12_000.0).agents[0], true)
    );
    assert!(summary.agent_cores[0].z > summary.agents[0].z);
    assert_eq!(summary.hit_regions, 2);
}
#[test]
fn bevy_ecs_reuses_hit_regions_on_unchanged_animation_frames() {
    let mut app = render_test_app(sample_render_state(12_000.0));
    let first_regions = hit_regions(&mut app);
    assert_eq!(first_regions.len(), 2);
    {
        let runtime = app.world_mut().resource::<BevyRuntimeState>();
        assert!(!runtime.hit_regions_dirty);
        assert!(runtime.hit_region_cache_key.is_some());
    }
    app.update();
    let second_regions = hit_regions(&mut app);
    assert_eq!(second_regions, first_regions);
    let runtime = app.world_mut().resource::<BevyRuntimeState>();
    assert!(!runtime.hit_regions_dirty);
    assert!(runtime.hit_region_cache_key.is_some());
}
#[path = "render_test_modules.rs"]
mod test_modules;
#[test]
fn bevy_ecs_refreshes_hit_regions_after_render_state_update() {
    let mut app = render_test_app(sample_render_state(12_000.0));
    let first_regions = hit_regions(&mut app);
    let mut updated_state = sample_render_state(12_000.0);
    updated_state.agents[0].id = "agent-updated".to_string();
    updated_state.agents[0].pos = Some(sample_position(2_500_000.0, 1_500_000.0));
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = Some(updated_state);
        runtime.render_version += 1;
        runtime.hit_regions_dirty = true;
    }
    app.update();
    let second_regions = hit_regions(&mut app);
    assert_ne!(second_regions, first_regions);
    assert!(
        second_regions
            .iter()
            .any(|region| region.id == "agent-updated")
    );
    assert!(!second_regions.iter().any(|region| region.id == "agent-0"));
}
#[test]
fn bevy_ecs_refreshes_hit_regions_after_camera_change() {
    let mut app = render_test_app(sample_render_state(12_000.0));
    let first_regions = hit_regions(&mut app);
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.camera.pan_x_px += 40.0;
    }
    app.update();
    let second_regions = hit_regions(&mut app);
    assert_ne!(second_regions, first_regions);
    assert_eq!(second_regions.len(), first_regions.len());
}
#[test]
fn bevy_ecs_removes_hidden_fragment_visuals_and_stale_cache_entries() {
    let mut app = render_test_app(sample_render_state(12_000.0));
    let before = visual_probe_summary(&mut app);
    assert_eq!(before.fragments.len(), 1);
    assert_eq!(before.fragment_entity_cache_size, 1);
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = Some(sample_render_state(100.0));
        runtime.render_version += 1;
    }
    app.update();
    let after = visual_probe_summary(&mut app);
    assert_eq!(after.fragments.len(), 0);
    assert_eq!(after.fragment_entity_cache_size, 0);
    assert_eq!(after.locations.len(), 1);
    assert_eq!(after.agents.len(), 1);
}
#[test]
fn bevy_ecs_reconciles_low_alpha_fragment_shadows_at_visible_lods_and_cleans_them_up() {
    let mut app = render_test_app(sample_render_state(20_000.0));
    let detail = visual_probe_summary(&mut app);
    let base = &detail.fragments[0];
    let shadow = &detail.fragment_shadows[0];
    assert_eq!(detail.fragment_shadows.len(), 1);
    assert_eq!(detail.fragment_shadow_entity_count, 1);
    assert_eq!(shadow.id, base.id);
    assert_eq!(shadow.size_px, base.size_px);
    assert_eq!(shadow.z, FRAGMENT_SHADOW_LAYER_Z);
    assert!(shadow.z < base.z);
    assert!((shadow.x - base.x).abs() >= 1.0);
    assert!((shadow.y - base.y).abs() >= 1.0);
    assert!((shadow.x - base.x).abs() <= base.size_px * FRAGMENT_SHADOW_OFFSET_CAP);
    assert!((shadow.y - base.y).abs() <= base.size_px * FRAGMENT_SHADOW_OFFSET_CAP);
    assert!(shadow.alpha <= base.alpha * FRAGMENT_SHADOW_ALPHA_CAP);
    let world = app.world_mut();
    let mut base_query = world.query::<(&PixelWorldFragmentVisual, &Sprite)>();
    let base_color = base_query
        .iter(world)
        .find(|(visual, _)| visual.id == shadow.id)
        .expect("base fragment sprite")
        .1
        .color
        .to_srgba();
    let mut shadow_query = world.query::<(&PixelWorldFragmentShadowVisual, &Sprite)>();
    let shadow_color = shadow_query
        .iter(world)
        .find(|(visual, _)| visual.id == shadow.id)
        .expect("fragment shadow sprite")
        .1
        .color
        .to_srgba();
    assert!(shadow_color.red < base_color.red);
    assert!(shadow_color.green < base_color.green);
    assert!(shadow_color.blue < base_color.blue);
    app.update();
    assert_eq!(
        visual_probe_summary(&mut app).fragment_shadow_entity_count,
        1,
        "a consecutive visible reconcile must reuse its shadow entity"
    );
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = Some(sample_render_state(12_000.0));
        runtime.render_version += 1;
    }
    app.update();
    let background = visual_probe_summary(&mut app);
    assert_eq!(background.fragment_shadows.len(), 1);
    assert_eq!(background.fragment_shadow_entity_count, 1);
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = Some(sample_render_state(100.0));
        runtime.render_version += 1;
    }
    app.update();
    let hidden = visual_probe_summary(&mut app);
    assert!(hidden.fragment_shadows.is_empty());
    assert_eq!(hidden.fragment_shadow_entity_count, 0);
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        let mut removed = sample_render_state(12_000.0);
        removed.fragment_terrain.clear();
        runtime.render_state = Some(removed);
        runtime.render_version += 1;
    }
    app.update();
    let removed = visual_probe_summary(&mut app);
    assert!(removed.fragment_shadows.is_empty());
    assert_eq!(removed.fragment_shadow_entity_count, 0);
}
#[test]
fn bevy_ecs_reconciles_detail_only_mineral_inset_and_cleans_it_up() {
    let mut app = render_test_app(sample_render_state(20_000.0));
    let detail = visual_probe_summary(&mut app);
    assert_eq!(detail.fragments.len(), 1);
    assert_eq!(detail.fragment_insets.len(), 1);
    assert_eq!(detail.fragment_inset_entity_count, 1);
    let base = &detail.fragments[0];
    let inset = &detail.fragment_insets[0];
    assert_eq!(inset.id, base.id);
    assert!(inset.size_px >= base.size_px * 0.35);
    assert!(inset.size_px <= base.size_px * 0.37);
    assert!(inset.z > base.z);
    assert!(inset.z < detail.locations[0].z);
    assert!((inset.x - base.x).abs() + (inset.y - base.y).abs() < base.size_px);
    app.update();
    assert_eq!(
        visual_probe_summary(&mut app).fragment_inset_entity_count,
        1,
        "a consecutive Detail reconcile must reuse its inset entity"
    );
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = Some(sample_render_state(12_000.0));
        runtime.render_version += 1;
    }
    app.update();
    let background = visual_probe_summary(&mut app);
    assert_eq!(background.fragments.len(), 1);
    assert!(background.fragment_insets.is_empty());
    assert_eq!(background.fragment_inset_entity_count, 0);
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = Some(sample_render_state(100.0));
        runtime.render_version += 1;
    }
    app.update();
    let hidden = visual_probe_summary(&mut app);
    assert!(hidden.fragments.is_empty());
    assert!(hidden.fragment_insets.is_empty());
    assert_eq!(hidden.fragment_inset_entity_count, 0);
}
#[test]
fn bevy_ecs_reconciles_detail_only_light_flecks_and_cleans_them_up() {
    let mut app = render_test_app(sample_render_state(20_000.0));
    let detail = visual_probe_summary(&mut app);
    assert_eq!(detail.fragments.len(), 1);
    assert_eq!(detail.fragment_flecks.len(), 1);
    assert_eq!(detail.fragment_fleck_entity_count, 1);
    let base = &detail.fragments[0];
    let fleck = &detail.fragment_flecks[0];
    assert_eq!(fleck.id, base.id);
    assert!(fleck.size_px >= base.size_px * 0.14);
    assert!(fleck.size_px <= base.size_px * 0.18);
    assert_eq!(fleck.z, FRAGMENT_FLECK_LAYER_Z);
    assert!(fleck.z > detail.fragment_insets[0].z);
    assert!(fleck.z < detail.locations[0].z);
    assert!(
        fleck.x < base.x,
        "fleck must remain upper-left of its terrain"
    );
    assert!(
        fleck.y > base.y,
        "fleck must remain upper-left of its terrain"
    );
    assert_eq!(detail.hit_regions, 2, "flecks must not add hit regions");
    let world = app.world_mut();
    let mut base_query = world.query::<(&PixelWorldFragmentVisual, &Sprite)>();
    let base_color = base_query
        .iter(world)
        .find(|(visual, _)| visual.id == fleck.id)
        .expect("base fragment sprite")
        .1
        .color
        .to_srgba();
    let mut fleck_query = world.query::<(&PixelWorldFragmentFleckVisual, &Sprite)>();
    let fleck_color = fleck_query
        .iter(world)
        .find(|(visual, _)| visual.id == fleck.id)
        .expect("fragment fleck sprite")
        .1
        .color
        .to_srgba();
    assert!(fleck_color.red > base_color.red);
    assert!(fleck_color.green > base_color.green);
    assert!(fleck_color.blue > base_color.blue);
    app.update();
    assert_eq!(
        visual_probe_summary(&mut app).fragment_fleck_entity_count,
        1,
        "a consecutive Detail reconcile must reuse its fleck entity"
    );
    for next_state in [sample_render_state(12_000.0), sample_render_state(100.0), {
        let mut removed = sample_render_state(20_000.0);
        removed.fragment_terrain.clear();
        removed
    }] {
        {
            let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
            runtime.render_state = Some(next_state);
            runtime.render_version += 1;
        }
        app.update();
        let summary = visual_probe_summary(&mut app);
        assert!(summary.fragment_flecks.is_empty());
        assert_eq!(summary.fragment_fleck_entity_count, 0);
    }
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = Some(sample_render_state(20_000.0));
        runtime.render_version += 1;
    }
    app.update();
    assert_eq!(
        visual_probe_summary(&mut app).fragment_fleck_entity_count,
        1
    );
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = None;
        runtime.render_version += 1;
    }
    app.update();
    let no_state = visual_probe_summary(&mut app);
    assert!(no_state.fragment_flecks.is_empty());
    assert_eq!(no_state.fragment_fleck_entity_count, 0);
}
#[test]
fn bevy_render_probe_contract_captures_visual_hierarchy() {
    let mut app = render_test_app(sample_render_state(20_000.0));
    let summary = visual_probe_summary(&mut app);
    assert_eq!(summary.fragment_entity_cache_size, 1);
    assert_eq!(summary.fragment_inset_entity_count, 1);
    assert_eq!(summary.fragment_fleck_entity_count, 1);
    assert_eq!(summary.location_entity_cache_size, 1);
    assert_eq!(summary.agent_entity_cache_size, 1);
    assert_eq!(summary.agent_core_entity_count, 1);
    assert!(summary.fragments[0].size_px > 0.0);
    assert!(summary.agents[0].size_px >= 15.0);
    assert!(summary.fragments[0].z < summary.agents[0].z);
    assert_eq!(summary.agent_cores.len(), 1);
    assert!(summary.agent_cores[0].z > summary.agents[0].z);
    assert!(summary.agent_cores[0].size_px < summary.agents[0].size_px);
    assert_eq!(summary.fragment_insets.len(), 1);
    assert!(summary.fragment_insets[0].z > summary.fragments[0].z);
    assert!(summary.fragment_insets[0].z < summary.locations[0].z);
    assert_eq!(summary.fragment_flecks.len(), 1);
    assert!(summary.fragment_flecks[0].z > summary.fragment_insets[0].z);
    assert!(summary.fragment_flecks[0].z < summary.locations[0].z);
    write_probe_summary_if_requested(&summary);
}
#[test]
fn selected_location_has_opaque_amber_two_pixel_ring_above_location_and_below_agents() {
    let mut app = render_test_app(sample_render_state_with_selection(
        12_000.0, "location", "loc-0",
    ));
    let summary = visual_probe_summary(&mut app);
    let selected_location = summary
        .locations
        .iter()
        .find(|location| location.id == "loc-0")
        .expect("selected location visual");
    assert_eq!(summary.selected_location_cues.len(), 4);
    for cue in &summary.selected_location_cues {
        assert_eq!(cue.id, "loc-0");
        assert_eq!(cue.alpha, 1.0);
        assert_eq!(cue.z, SELECTED_LOCATION_CUE_LAYER_Z);
        assert!(cue.size_px >= SELECTED_LOCATION_CUE_THICKNESS_PX);
        assert!(cue.z > selected_location.z);
        assert!(cue.z < summary.agents[0].z);
    }
    let world = app.world_mut();
    let mut cue_query = world.query::<(&Sprite, &PixelWorldSelectedLocationCue)>();
    for (sprite, _) in cue_query.iter(world) {
        let color = sprite.color.to_srgba();
        assert_eq!(
            [color.red, color.green, color.blue],
            [251.0 / 255.0, 191.0 / 255.0, 36.0 / 255.0]
        );
        assert_eq!(color.alpha, 1.0);
        assert!(
            sprite.custom_size.unwrap().x == SELECTED_LOCATION_CUE_THICKNESS_PX
                || sprite.custom_size.unwrap().y == SELECTED_LOCATION_CUE_THICKNESS_PX
        );
    }
}
#[test]
fn selected_location_ring_tracks_selection_and_leaves_no_stale_entities() {
    let mut app = render_test_app(sample_render_state_with_selection(
        12_000.0, "location", "loc-0",
    ));
    assert_eq!(
        visual_probe_summary(&mut app).selected_location_cues.len(),
        4
    );
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        runtime.render_state = Some(sample_render_state_with_selection(
            12_000.0, "agent", "agent-0",
        ));
        runtime.render_version += 1;
    }
    app.update();
    assert!(
        visual_probe_summary(&mut app)
            .selected_location_cues
            .is_empty()
    );
    {
        let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
        let mut render_state = sample_render_state(12_000.0);
        render_state.selection = None;
        runtime.render_state = Some(render_state);
        runtime.render_version += 1;
    }
    app.update();
    assert!(
        visual_probe_summary(&mut app)
            .selected_location_cues
            .is_empty()
    );
}
#[test]
fn bevy_pixel_regression_rasterizes_fragment_location_agent_hierarchy() {
    let mut app = render_test_app(sample_render_state(12_000.0));
    let (image, summary) = rasterize_pixel_regression(&mut app);
    assert!(summary.non_background_pixels >= 350);
    assert!(summary.fragment_pixels > 0);
    assert!(summary.location_pixels > 0);
    assert!(summary.agent_pixels > summary.location_pixels);
    assert!(summary.agent_pixels > summary.fragment_pixels);
    assert!(summary.agent_core_pixels > 0);
    assert_ne!(summary.agent_core_sample_rgba, PIXEL_BACKGROUND);
    assert!(summary.fragment_sample_rgba[1] > summary.fragment_sample_rgba[0]);
    assert!(summary.location_sample_rgba[1] > summary.location_sample_rgba[0]);
    write_pixel_probe_if_requested(&image, &summary);
}
#[test]
fn bevy_pixel_regression_exports_unoccluded_detail_fleck() {
    let mut app = render_test_app(sample_render_state_with_unoccluded_detail_fleck());
    let (image, summary) = rasterize_pixel_regression(&mut app);
    assert!(summary.fragment_fleck_pixels > 0);
    assert_ne!(summary.fragment_fleck_sample_rgba, PIXEL_BACKGROUND);
    assert!(
        image
            .pixels()
            .any(|pixel| pixel.0 == summary.fragment_fleck_sample_rgba),
        "the unoccluded Detail fleck must contribute visible raster pixels"
    );
    write_pixel_probe_if_requested(&image, &summary);
}
#[test]
fn bevy_pixel_regression_keeps_canvas_visible_for_agent_and_location_selection() {
    for (kind, id) in [("agent", "agent-0"), ("location", "loc-0")] {
        let mut app = render_test_app(sample_render_state_with_selection(12_000.0, kind, id));
        let (_, summary) = rasterize_pixel_regression(&mut app);
        assert!(
            summary.non_background_pixels >= 350,
            "{kind} selection should keep the pixel-world canvas nonblank"
        );
        assert!(
            summary.fragment_pixels > 0,
            "{kind} selection should keep fragment pixels"
        );
        assert!(
            summary.location_pixels > 0,
            "{kind} selection should keep location pixels"
        );
        assert!(
            summary.agent_pixels > 0,
            "{kind} selection should keep agent pixels"
        );
        assert_ne!(
            summary.raw_rgba_fnv1a64, "c3c91248ad807f7b",
            "{kind} selection should not rasterize as the all-background frame"
        );
    }
}
#[test]
fn bevy_pixel_regression_gives_selected_agent_and_location_the_same_non_color_beacon() {
    for (kind, selected_id, unselected_id) in [
        ("agent", "agent-0", "agent-1"),
        ("location", "loc-0", "loc-1"),
    ] {
        let mut app = render_test_app(sample_render_state_with_beacon_candidates(
            kind,
            selected_id,
        ));
        let summary = visual_probe_summary(&mut app);
        let (selected, unselected) = match kind {
            "agent" => (
                summary
                    .agents
                    .iter()
                    .find(|row| row.id == selected_id)
                    .unwrap(),
                summary
                    .agents
                    .iter()
                    .find(|row| row.id == unselected_id)
                    .unwrap(),
            ),
            "location" => (
                summary
                    .locations
                    .iter()
                    .find(|row| row.id == selected_id)
                    .unwrap(),
                summary
                    .locations
                    .iter()
                    .find(|row| row.id == unselected_id)
                    .unwrap(),
            ),
            _ => unreachable!("test cases use known render entity kinds"),
        };
        assert!(
            selected.size_px > unselected.size_px,
            "selected {kind} must have a geometry beacon beyond its color"
        );
        assert!(
            selected.z > unselected.z,
            "selected {kind} must have a draw-priority beacon beyond its color"
        );
    }
}

#[path = "render_spatial_overlay_tests.rs"]
mod spatial_overlay_tests;
