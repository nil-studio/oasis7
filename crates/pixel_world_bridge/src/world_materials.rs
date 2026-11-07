//! Shared authored pixels decorate existing authorized render entities only.
use super::*;
use std::collections::{HashMap, HashSet};

#[derive(Component)]
pub(super) struct MaterialShell;
#[derive(Default)]
pub(super) struct Materials {
    textures: Option<[Handle<Image>; 3]>,
    entities: HashMap<String, Entity>,
}
fn textures(images: &mut Assets<Image>) -> [Handle<Image>; 3] {
    [
        include_bytes!("../../../doc/world-simulator/viewer/assets/ui-ux/pixel-world/ore.png")
            .as_slice(),
        include_bytes!("../../../doc/world-simulator/viewer/assets/ui-ux/pixel-world/smelter.png")
            .as_slice(),
        include_bytes!("../../../doc/world-simulator/viewer/assets/ui-ux/pixel-world/depot.png")
            .as_slice(),
    ]
    .map(|bytes| {
        images.add(
            Image::from_buffer(
                bytes,
                bevy::image::ImageType::Extension("png"),
                bevy::image::CompressedImageFormats::NONE,
                true,
                bevy::image::ImageSampler::nearest(),
                bevy::asset::RenderAssetUsages::default(),
            )
            .expect("checked-in shared material must decode"),
        )
    })
}

fn shell(
    commands: &mut Commands,
    materials: &mut Materials,
    active: &mut HashSet<String>,
    key: String,
    transform: &Transform,
    sprite: Sprite,
    behind: bool,
) {
    active.insert(key.clone());
    let mut transform = *transform;
    transform.rotation = Quat::IDENTITY;
    transform.translation.z += if behind { -0.005 } else { 0.005 };
    if let Some(entity) = materials.entities.get(&key) {
        commands.entity(*entity).insert((sprite, transform));
    } else {
        let entity = commands.spawn((sprite, transform, MaterialShell)).id();
        materials.entities.insert(key, entity);
    }
}

pub(super) fn reconcile(
    mut commands: Commands,
    runtime: Res<BevyRuntimeState>,
    mut images: ResMut<Assets<Image>>,
    mut materials: Local<Materials>,
    bases: Query<&Transform, Without<MaterialShell>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let mut active = HashSet::new();
    if runtime.mounted
        && let Some(state) = &runtime.render_state
        && let Some(bounds) = &state.world_bounds
        && let Ok(window) = windows.single()
    {
        let (width, height) = (window.width() as f64, window.height() as f64);
        if materials.textures.is_none() {
            materials.textures = Some(textures(&mut images));
        }
        let textures = materials.textures.as_ref().unwrap().clone();
        for fragment in &state.fragment_terrain {
            if fragment.dominant_compound == "unknown" {
                continue;
            }
            let Some(style) =
                render::fragment_visual_style(fragment, bounds, width, height, &runtime.camera)
            else {
                continue;
            };
            if style.lod != render::FragmentTerrainLod::Detail {
                continue;
            }
            let Some(base) = runtime
                .fragment_entities
                .get(&fragment.id)
                .and_then(|entity| bases.get(*entity).ok())
            else {
                continue;
            };
            shell(
                &mut commands,
                &mut materials,
                &mut active,
                format!("terrain:{}", fragment.id),
                base,
                Sprite {
                    image: textures[0].clone(),
                    custom_size: Some(Vec2::splat(style.size_px as f32)),
                    color: Color::srgba_u8(
                        fragment.color[0],
                        fragment.color[1],
                        fragment.color[2],
                        (style.alpha * 255.0).round() as u8,
                    ),
                    ..default()
                },
                false,
            );
        }
        let scale = agent_pixel_scale(width, height);
        for facility in &state.module_visual_entities {
            if facility.kind != "factory"
                || facility.factory_kind.as_deref() != Some("factory.smelter.mk1")
            {
                continue;
            }
            let Some(base) = runtime
                .module_visual_entities
                .get(&facility.id)
                .and_then(|entity| bases.get(*entity).ok())
            else {
                continue;
            };
            shell(
                &mut commands,
                &mut materials,
                &mut active,
                format!("factory:{}", facility.id),
                base,
                Sprite {
                    image: textures[1].clone(),
                    custom_size: Some(scale * 48.0),
                    ..default()
                },
                true,
            );
        }
        for facility in &state.micro_depot_facilities {
            let Some(base) = runtime
                .micro_depot_entities
                .get(&facility.id)
                .and_then(|entity| bases.get(*entity).ok())
            else {
                continue;
            };
            shell(
                &mut commands,
                &mut materials,
                &mut active,
                format!("depot:{}", facility.id),
                base,
                Sprite {
                    image: textures[2].clone(),
                    custom_size: Some(scale * 32.0),
                    ..default()
                },
                true,
            );
        }
    }
    materials.entities.retain(|key, entity| {
        if active.contains(key) {
            true
        } else {
            commands.entity(*entity).despawn();
            false
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn materials_follow_existing_authorized_entities_and_clear_on_type_change() {
        let input = json!({"snapshot":{"config":{"space":{"width_cm":100,"depth_cm":100,"height_cm":100}},"model":{"module_visual_entities":{"F":{"entity_id":"F","module_id":"runtime_factory","kind":"factory","factory_kind":"factory.smelter.mk1","anchor":{"type":"absolute","data":{"pos":{"x_cm":10,"y_cm":20,"z_cm":0}}}}}}}});
        let mut state: RenderState =
            serde_json::from_value(host_state::build_render_state(&input)).unwrap();
        state.fragment_terrain = vec![FragmentTerrainPatch {
            id: "patch".into(),
            location_id: "L".into(),
            pos: Position {
                x_cm: 10.,
                y_cm: 20.,
                z_cm: 0.,
            },
            footprint_cm: 10.,
            dominant_compound: "silicate_matrix".into(),
            color: [100, 110, 120],
            emphasis: None,
        }];
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<Assets<Image>>();
        app.world_mut().spawn((
            Window {
                resolution: (1280u32, 720u32).into(),
                ..default()
            },
            PrimaryWindow,
        ));
        let base = app
            .world_mut()
            .spawn(Transform::from_xyz(12., 34., 5.))
            .id();
        let terrain_base = app
            .world_mut()
            .spawn(Transform::from_xyz(56., 78., 1.))
            .id();
        let mut runtime = BevyRuntimeState::default();
        runtime.mounted = true;
        runtime.last_canvas_size = Some((1280, 720));
        runtime.render_state = Some(state);
        runtime.module_visual_entities.insert("F".into(), base);
        runtime
            .fragment_entities
            .insert("patch".into(), terrain_base);
        app.insert_resource(runtime);
        app.add_systems(Update, reconcile);
        app.update();
        {
            let world = app.world_mut();
            let mut query = world.query_filtered::<(&Sprite, &Transform), With<MaterialShell>>();
            let shells = query.iter(world).collect::<Vec<_>>();
            assert_eq!(shells.len(), 2);
            let smelter = shells
                .iter()
                .find(|(_, transform)| transform.translation.x == 12.)
                .unwrap();
            assert_eq!(smelter.0.custom_size, Some(Vec2::splat(96.)));
            assert_eq!(smelter.1.translation.y, 34.);
            assert_eq!(
                world
                    .resource::<Assets<Image>>()
                    .get(&smelter.0.image)
                    .unwrap()
                    .size(),
                UVec2::splat(48)
            );
            assert_eq!(world.resource::<BevyRuntimeState>().hit_regions.len(), 0);
        }
        {
            let mut runtime = app.world_mut().resource_mut::<BevyRuntimeState>();
            let state = runtime.render_state.as_mut().unwrap();
            state.module_visual_entities[0].factory_kind = Some("unsupported".into());
            state.fragment_terrain[0].dominant_compound = "unknown".into();
        }
        app.update();
        {
            let world = app.world_mut();
            assert_eq!(
                world
                    .query_filtered::<Entity, With<MaterialShell>>()
                    .iter(world)
                    .count(),
                0
            );
            assert_eq!(world.resource::<Assets<Image>>().len(), 3);
        }
        app.world_mut().resource_mut::<BevyRuntimeState>().mounted = false;
        app.update();
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 3);
    }
}
