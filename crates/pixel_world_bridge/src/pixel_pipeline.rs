//! World texture and native-resolution text composition. Coordinates stay in CSS units.
use super::*;
use bevy::camera::{RenderTarget, ScalingMode, visibility::RenderLayers};
use bevy::image::ImageSampler;
use bevy::math::DVec2;
use bevy::render::render_resource::{Extent3d, TextureFormat};

thread_local! {
    static ACTIVE_LAYOUT: std::cell::Cell<Option<PixelLayout>> = const { std::cell::Cell::new(None) };
}

fn desired_frame(css: DVec2, dpr: f64) -> Option<UVec2> {
    if !css.is_finite() || css.min_element() <= 0. || !dpr.is_finite() || dpr <= 0. {
        return None;
    }
    Some(UVec2::new(
        (css.x as f64 * dpr).floor() as u32,
        (css.y as f64 * dpr).floor() as u32,
    ))
}

pub(super) fn sync_canvas_backing(mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    #[cfg(target_arch = "wasm32")]
    {
        let request = BRIDGE_SHARED.with(|shared| {
            let browser = web_sys::window()?;
            let selector = shared.borrow().canvas_selector.clone()?;
            let canvas = browser
                .document()?
                .query_selector(&selector)
                .ok()??
                .dyn_into::<HtmlCanvasElement>()
                .ok()?;
            // Restore parent-driven CSS before measuring: Winit may have written
            // the previous pixel size inline. Measure the canvas content itself,
            // not its parent's border box (normal mode has a 1px stage border).
            let _ = canvas.style().set_property("width", "100%");
            let _ = canvas.style().set_property("height", "100%");
            let rect = canvas.get_bounding_client_rect();
            desired_frame(
                DVec2::new(rect.width(), rect.height()),
                browser.device_pixel_ratio(),
            )
        });
        if let (Some(frame), Ok(mut window)) = (request, windows.single_mut()) {
            if window.physical_width() != frame.x || window.physical_height() != frame.y {
                window.resolution.set_physical_resolution(frame.x, frame.y);
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &mut windows;
}

pub(super) fn window_layout(window: &Window) -> Option<PixelLayout> {
    #[cfg(target_arch = "wasm32")]
    if let Some((css, frame)) = BRIDGE_SHARED.with(|shared| {
        let selector = shared.borrow().canvas_selector.clone()?;
        let canvas = web_sys::window()?
            .document()?
            .query_selector(&selector)
            .ok()??
            .dyn_into::<HtmlCanvasElement>()
            .ok()?;
        let rect = canvas.get_bounding_client_rect();
        Some((
            DVec2::new(rect.width(), rect.height()),
            UVec2::new(canvas.width(), canvas.height()),
        ))
    }) {
        return canvas_css_layout(css, frame);
    }
    canvas_css_layout(
        DVec2::new(window.width() as f64, window.height() as f64),
        UVec2::new(window.physical_width(), window.physical_height()),
    )
}
fn canvas_css_layout(css: DVec2, frame: UVec2) -> Option<PixelLayout> {
    PixelLayout::from_css(css, frame)
}

pub(super) fn cache_layout(window: &Window) {
    ACTIVE_LAYOUT.set(window_layout(window));
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PixelLayout {
    pub css: DVec2,
    pub frame: UVec2,
    pub logical: UVec2,
    pub scale: u32,
    pub offset: UVec2,
}
impl PixelLayout {
    #[cfg(test)]
    pub fn new(css: Vec2, frame: UVec2) -> Option<Self> {
        Self::from_css(css.as_dvec2(), frame)
    }
    pub fn from_css(css: DVec2, frame: UVec2) -> Option<Self> {
        if !css.is_finite() || css.min_element() <= 0.0 || frame.min_element() == 0 {
            return None;
        }
        let scale = (frame.x / 640).min(frame.y / 360).max(1);
        let logical = frame / scale;
        Some(Self {
            css,
            frame,
            logical,
            scale,
            offset: (frame - logical * scale) / 2,
        })
    }
    pub fn pixel_css(self) -> Vec2 {
        (self.css / self.frame.as_dvec2() * self.scale as f64).as_vec2()
    }
    pub fn content_size(self) -> Vec2 {
        self.logical.as_vec2() * self.pixel_css()
    }
    pub fn content_center(self) -> Vec2 {
        ((self.offset.as_dvec2() + self.logical.as_dvec2() * self.scale as f64 / 2.) * self.css
            / self.frame.as_dvec2())
        .as_vec2()
    }
    pub fn snap(self, p: bevy::math::DVec2, pan: bevy::math::DVec2) -> bevy::math::DVec2 {
        let unit = self.css / self.frame.as_dvec2() * self.scale as f64;
        let t = pan / unit;
        let snapped_pan = (t + bevy::math::DVec2::splat(0.5)).floor();
        let q = (p * self.frame.as_dvec2() / self.css - self.offset.as_dvec2()) / self.scale as f64;
        (((q - t + snapped_pan + bevy::math::DVec2::splat(0.5)).floor() * self.scale as f64)
            + self.offset.as_dvec2())
            * self.css
            / self.frame.as_dvec2()
    }
}
#[derive(Component)]
pub(super) struct WorldCamera;
#[derive(Component)]
pub(super) struct OuterCamera;
#[derive(Component)]
pub(super) struct Composite;
#[derive(Resource)]
pub(super) struct Pipeline {
    image: Handle<Image>,
    layout: Option<PixelLayout>,
}

pub(super) fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mut image = Image::new_target_texture(1, 1, TextureFormat::Bgra8UnormSrgb, None);
    image.sampler = ImageSampler::nearest();
    let handle = images.add(image);
    commands.spawn((
        Camera2d,
        Camera {
            order: -1,
            ..default()
        },
        RenderTarget::Image(handle.clone().into()),
        Msaa::Off,
        WorldCamera,
        RenderLayers::layer(0),
    ));
    commands.spawn((
        Sprite::from_image(handle.clone()),
        Composite,
        RenderLayers::layer(1),
    ));
    commands.spawn((Camera2d, Msaa::Off, OuterCamera, RenderLayers::layer(1)));
    commands.insert_resource(Pipeline {
        image: handle,
        layout: None,
    });
}

pub(super) fn resize(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut pipeline: ResMut<Pipeline>,
    mut runtime: ResMut<BevyRuntimeState>,
    mut images: ResMut<Assets<Image>>,
    mut cameras: Query<(&mut Camera, &mut Projection, &mut Transform), With<WorldCamera>>,
    mut outer: Query<&mut Projection, (With<OuterCamera>, Without<WorldCamera>)>,
    mut composite: Query<(&mut Sprite, &mut Transform), (With<Composite>, Without<WorldCamera>)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let layout = window_layout(window);
    let Ok((mut camera, mut projection, mut camera_transform)) = cameras.single_mut() else {
        return;
    };
    camera.is_active = layout.is_some();
    let Some(layout) = layout else {
        return;
    };
    if pipeline.layout == Some(layout) {
        return;
    }
    let Some(mut image) = images.get_mut(&pipeline.image) else {
        return;
    };
    image.resize(Extent3d {
        width: layout.logical.x,
        height: layout.logical.y,
        ..default()
    });
    if let Ok(mut outer) = outer.single_mut() {
        if let Projection::Orthographic(projection) = &mut *outer {
            projection.scaling_mode = ScalingMode::Fixed {
                width: layout.css.x as f32,
                height: layout.css.y as f32,
            };
        }
    }
    let size = layout.content_size();
    let center = layout.content_center() - layout.css.as_vec2() / 2.0;
    if let Projection::Orthographic(projection) = &mut *projection {
        projection.scaling_mode = ScalingMode::Fixed {
            width: size.x,
            height: size.y,
        };
    }
    camera_transform.translation.x = center.x;
    camera_transform.translation.y = -center.y;
    if let Ok((mut sprite, mut transform)) = composite.single_mut() {
        sprite.custom_size = Some(size);
        transform.translation = Vec3::new(center.x, -center.y, -100.0);
    }
    pipeline.layout = Some(layout);
    runtime.needs_reconcile = true;
    runtime.hit_regions_dirty = true;
}

pub(super) fn canvas_layout(width: f64, height: f64) -> Option<PixelLayout> {
    let css = DVec2::new(width, height);
    if let Some(layout) = ACTIVE_LAYOUT.get().filter(|layout| layout.css == css) {
        return Some(layout);
    }
    #[cfg(target_arch = "wasm32")]
    let frame = BRIDGE_SHARED
        .with(|shared| {
            let selector = shared.borrow().canvas_selector.clone()?;
            let document = web_sys::window()?.document()?;
            let canvas = document
                .query_selector(&selector)
                .ok()??
                .dyn_into::<HtmlCanvasElement>()
                .ok()?;
            Some(UVec2::new(canvas.width(), canvas.height()))
        })
        .unwrap_or_else(|| css.as_uvec2());
    #[cfg(not(target_arch = "wasm32"))]
    let frame = css.as_uvec2();
    PixelLayout::from_css(css, frame)
}

pub(super) fn assign_layers(
    mut commands: Commands,
    entities: Query<EntityRef, (Or<(With<Sprite>, With<Text2d>)>, Without<RenderLayers>)>,
) {
    for entity in &entities {
        let world = render::is_world_visual(&entity);
        commands
            .entity(entity.id())
            .insert(RenderLayers::layer(if world { 0 } else { 1 }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn production_sizes_and_odd_center_match_contract() {
        for (css, frame, s, logical) in [
            ((1280., 720.), (1280, 720), 2, (640, 360)),
            ((1280., 720.), (2560, 1440), 4, (640, 360)),
            ((390., 844.), (390, 844), 1, (390, 844)),
            ((390., 844.), (780, 1688), 1, (780, 1688)),
            ((1365., 768.), (1365, 768), 2, (682, 384)),
        ] {
            let layout =
                PixelLayout::new(Vec2::new(css.0, css.1), UVec2::new(frame.0, frame.1)).unwrap();
            assert_eq!(layout.scale, s);
            assert_eq!(layout.logical, UVec2::new(logical.0, logical.1));
        }
        let layout = PixelLayout::new(Vec2::new(1365., 768.), UVec2::new(1365, 768)).unwrap();
        assert_eq!(
            layout.snap(bevy::math::DVec2::new(682.5, 384.), bevy::math::DVec2::ZERO),
            bevy::math::DVec2::new(682., 384.)
        );
        assert!(PixelLayout::new(Vec2::ZERO, UVec2::ZERO).is_none());
    }
    #[test]
    fn fractional_pan_and_sub_f32_threshold_keep_double_precision() {
        use bevy::math::DVec2;
        let layout = PixelLayout::new(Vec2::new(1280., 720.), UVec2::new(1280, 720)).unwrap();
        assert_eq!(
            layout.snap(DVec2::new(640.6, 360.), DVec2::new(0.6, 0.)),
            DVec2::new(640., 360.)
        );
        assert_eq!(
            layout.snap(DVec2::new(641. - 1e-8, 360.), DVec2::ZERO).x,
            640.
        );
        assert_eq!(
            layout.snap(DVec2::new(641. + 1e-8, 360.), DVec2::ZERO).x,
            642.
        );
    }

    #[test]
    fn dpr2_canvas_css_is_not_winit_scaled_logical_size() {
        use bevy::math::DVec2;
        assert_eq!(
            desired_frame(DVec2::new(1440., 1000.), 2.),
            Some(UVec2::new(2880, 2000))
        );
        assert_eq!(
            desired_frame(DVec2::new(390., 844.), 2.),
            Some(UVec2::new(780, 1688))
        );
        assert!(desired_frame(DVec2::ZERO, 2.).is_none());
        // Real browser failure: CSS=1440x1000, backing=1440x1000,
        // while Winit logical dimensions were 720x500 at DPR2.
        let layout = canvas_css_layout(DVec2::new(1440., 1000.), UVec2::new(1440, 1000)).unwrap();
        assert_eq!(layout.logical, UVec2::new(720, 500));
        assert_eq!(layout.pixel_css(), Vec2::splat(2.));
        assert_eq!(
            layout.snap(DVec2::new(720., 500.), DVec2::ZERO),
            DVec2::new(720., 500.)
        );
        assert!(canvas_css_layout(DVec2::ZERO, UVec2::new(1440, 1000)).is_none());
    }

    #[test]
    fn fractional_css_keeps_shared_projection_on_the_same_snap_cell() {
        let css = 1365.3333333333;
        let layout = PixelLayout::from_css(DVec2::new(css, 768.), UVec2::new(1365, 768)).unwrap();
        let point = DVec2::new(20. + 1_728_326. / 10_000_000. * (css - 40.), 384.);
        assert!((layout.snap(point, DVec2::ZERO).x - 250.061050061044).abs() < 1e-9);
    }

    #[test]
    fn background_grid_stays_behind_world_while_text_is_native_resolution() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let grid = app
            .world_mut()
            .spawn((Sprite::default(), render::PixelWorldGridVisual))
            .id();
        let text = app.world_mut().spawn(Text2d::new("中文")).id();
        app.add_systems(Update, assign_layers);
        app.update();
        assert_eq!(
            app.world().get::<RenderLayers>(grid),
            Some(&RenderLayers::layer(0))
        );
        assert_eq!(
            app.world().get::<RenderLayers>(text),
            Some(&RenderLayers::layer(1))
        );
    }

    #[test]
    fn resize_reuses_texture_and_preserves_native_overlay_camera() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<Assets<Image>>();
        app.insert_resource(BevyRuntimeState::default());
        app.world_mut().spawn((
            Window {
                resolution: (1280u32, 720u32).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        app.add_systems(Startup, setup);
        app.add_systems(Update, resize);
        app.update();
        let handle = app.world().resource::<Pipeline>().image.clone();
        assert_eq!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&handle)
                .unwrap()
                .size(),
            UVec2::new(640, 360)
        );
        let count = app.world().resource::<Assets<Image>>().len();
        {
            let world = app.world_mut();
            let mut window = world.query::<&mut Window>().single_mut(world).unwrap();
            window.resolution.set(390., 844.);
        }
        app.update();
        assert_eq!(app.world().resource::<Assets<Image>>().len(), count);
        assert_eq!(app.world().resource::<Pipeline>().image, handle);
        assert_eq!(
            app.world()
                .resource::<Assets<Image>>()
                .get(&handle)
                .unwrap()
                .size(),
            UVec2::new(390, 844)
        );
        let world = app.world_mut();
        assert_eq!(world.query::<&Camera>().iter(world).count(), 2);
        assert!(world.resource::<BevyRuntimeState>().hit_regions_dirty);
    }
}
