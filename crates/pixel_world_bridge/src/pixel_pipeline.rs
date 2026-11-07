//! World texture and native-resolution text composition. Coordinates stay in CSS units.
use super::*;
use bevy::camera::{RenderTarget, ScalingMode, visibility::RenderLayers};
use bevy::image::ImageSampler;
use bevy::render::render_resource::{Extent3d, TextureFormat};

thread_local! {
    static ACTIVE_LAYOUT: std::cell::Cell<Option<PixelLayout>> = const { std::cell::Cell::new(None) };
}

pub(super) fn cache_layout(window: &Window) {
    ACTIVE_LAYOUT.set(PixelLayout::new(
        Vec2::new(window.width(), window.height()),
        UVec2::new(window.physical_width(), window.physical_height()),
    ));
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PixelLayout {
    pub css: Vec2,
    pub frame: UVec2,
    pub logical: UVec2,
    pub scale: u32,
    pub offset: UVec2,
}
impl PixelLayout {
    pub fn new(css: Vec2, frame: UVec2) -> Option<Self> {
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
        self.css / self.frame.as_vec2() * self.scale as f32
    }
    pub fn content_size(self) -> Vec2 {
        self.logical.as_vec2() * self.pixel_css()
    }
    pub fn content_center(self) -> Vec2 {
        (self.offset.as_vec2() + self.logical.as_vec2() * self.scale as f32 / 2.0) * self.css
            / self.frame.as_vec2()
    }
    pub fn snap(self, p: bevy::math::DVec2, pan: bevy::math::DVec2) -> bevy::math::DVec2 {
        let unit = self.css.as_dvec2() / self.frame.as_dvec2() * self.scale as f64;
        let t = pan / unit;
        let snapped_pan = (t + bevy::math::DVec2::splat(0.5)).floor();
        let q = (p * self.frame.as_dvec2() / self.css.as_dvec2() - self.offset.as_dvec2())
            / self.scale as f64;
        (((q - t + snapped_pan + bevy::math::DVec2::splat(0.5)).floor() * self.scale as f64)
            + self.offset.as_dvec2())
            * self.css.as_dvec2()
            / self.frame.as_dvec2()
    }
}
#[derive(Component)]
pub(super) struct WorldCamera;
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
    commands.spawn((Camera2d, Msaa::Off, RenderLayers::layer(1)));
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
    mut composite: Query<(&mut Sprite, &mut Transform), (With<Composite>, Without<WorldCamera>)>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let layout = PixelLayout::new(
        Vec2::new(window.width(), window.height()),
        UVec2::new(window.physical_width(), window.physical_height()),
    );
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
    let size = layout.content_size();
    let center = layout.content_center() - layout.css / 2.0;
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
    let css = Vec2::new(width as f32, height as f32);
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
    PixelLayout::new(css, frame)
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
