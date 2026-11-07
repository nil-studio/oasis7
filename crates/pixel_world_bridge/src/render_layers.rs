//! Keep authored sprite geometry in one depth space; only explicit UI is native.
use super::*;

pub(crate) fn is_world_visual(entity: &EntityRef<'_>) -> bool {
    if entity.contains::<Text2d>() {
        return false;
    }
    let native = entity.contains::<PixelWorldAgentPowerCue>()
        || entity.contains::<PixelWorldDerivedPositionCue>()
        || entity.contains::<PixelWorldMissingPositionCue>()
        || entity.contains::<PixelWorldSelectedAgentCue>()
        || entity.contains::<PixelWorldSelectedLocationCue>()
        || entity.contains::<PixelWorldLocationCornerFrame>()
        || entity.contains::<PixelWorldReceiptTargetCue>()
        || entity.contains::<PixelWorldRecommendedTargetCue>()
        || entity.contains::<PixelWorldActiveIntentCue>()
        || entity.contains::<PixelWorldLocationResourceCue>()
        || entity.contains::<micro_depot_facilities::PixelWorldMicroDepotStockRunwayVisual>()
        || entity
            .contains::<micro_depot_facilities::PixelWorldMicroDepotStockRunwayBackingVisual>()
        || entity.contains::<micro_depot_facilities::PixelWorldMicroDepotZeroStockCrossVisual>()
        || entity.contains::<micro_depot_facilities::PixelWorldMicroDepotThroughputBackingVisual>();
    entity.contains::<Sprite>() && !native
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::visibility::RenderLayers;
    #[test]
    fn module_glyphs_and_other_authored_sprites_keep_world_depth_but_ui_stays_native() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let module = app
            .world_mut()
            .spawn((
                Sprite::default(),
                module_visual_entities::PixelWorldModuleIdentityChipVisual {
                    id: "M".into(),
                    part: module_visual_entities::ModuleIdentityChipPart::BeaconStem,
                },
                Transform::from_xyz(0., 0., 2.71),
            ))
            .id();
        let actor = app
            .world_mut()
            .spawn((
                Sprite::default(),
                PixelWorldAgentVisual { id: "A".into() },
                Transform::from_xyz(0., 0., 3.),
            ))
            .id();
        let world = app.world_mut().spawn(Sprite::default()).id();
        let backing = app
            .world_mut()
            .spawn((
                Sprite::default(),
                micro_depot_facilities::PixelWorldMicroDepotThroughputBackingVisual {
                    id: "D".into(),
                },
            ))
            .id();
        let text = app.world_mut().spawn(Text2d::new("label")).id();
        app.add_systems(Update, pixel_pipeline::assign_layers);
        app.update();
        for entity in [module, actor, world] {
            assert_eq!(
                app.world().get::<RenderLayers>(entity),
                Some(&RenderLayers::layer(0))
            );
        }
        for entity in [backing, text] {
            assert_eq!(
                app.world().get::<RenderLayers>(entity),
                Some(&RenderLayers::layer(1))
            );
        }
        assert!(
            app.world().get::<Transform>(module).unwrap().translation.z
                < app.world().get::<Transform>(actor).unwrap().translation.z
        );
    }
    #[test]
    fn actual_selection_ui_is_native_while_its_agent_remains_world() {
        let mut app =
            super::super::tests::render_test_app(super::super::tests::sample_render_state(12000.));
        app.add_systems(Update, pixel_pipeline::assign_layers.after(render_scene));
        app.update();
        let world = app.world_mut();
        let mut actor = world.query_filtered::<&RenderLayers, With<PixelWorldAgentVisual>>();
        assert_eq!(actor.single(world).unwrap(), &RenderLayers::layer(0));
        let mut cues = world.query_filtered::<&RenderLayers, With<PixelWorldSelectedAgentCue>>();
        let layers = cues.iter(world).collect::<Vec<_>>();
        assert_eq!(layers.len(), 8);
        assert!(layers.iter().all(|layer| **layer == RenderLayers::layer(1)));
    }
}
