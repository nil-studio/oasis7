use super::*;

#[test]
fn native_resolution_registry_declares_expected_runtime_subsystems() {
    let subsystem_ids: Vec<_> = runtime_native_resolutions()
        .iter()
        .map(|declaration| declaration.subsystem_id)
        .collect();

    assert_eq!(
        subsystem_ids,
        vec![
            "canonical-physical-space",
            "chunk-grid",
            "asteroid-fragment-voxel",
            "asteroid-fragment-spacing",
            "movement-energy-cost",
            "power-transfer-distance",
            "location-site-actions",
            "fragment-block-geometry",
        ]
    );
}

#[test]
fn chunk_grid_native_resolution_matches_runtime_constants() {
    let declaration = native_resolution_by_subsystem("chunk-grid").expect("chunk-grid declared");

    assert_eq!(
        declaration.native_resolution_kind,
        NativeResolutionKind::ChunkGrid
    );
    assert_eq!(
        declaration.cm_mapping_rule,
        CmMappingRule::FloorDivideIntoChunkIndex
    );
    assert_eq!(
        declaration.rounding_rule,
        RoundingRule::FloorDivideWithUpperEdgeClamp
    );
    assert_eq!(
        declaration.native_resolution_value,
        NativeResolutionValue::FixedCuboidCm {
            x_cm: CHUNK_SIZE_X_CM,
            y_cm: CHUNK_SIZE_Y_CM,
            z_cm: CHUNK_SIZE_Z_CM,
        }
    );
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn asteroid_fragment_native_resolution_matches_config_defaults_and_sanitization() {
    let voxel = native_resolution_by_subsystem("asteroid-fragment-voxel")
        .expect("asteroid-fragment-voxel declared");
    assert_eq!(
        voxel.native_resolution_value,
        NativeResolutionValue::ConfiguredVoxelKm {
            field: "AsteroidFragmentConfig.voxel_size_km",
            default_km: AsteroidFragmentConfig::default().voxel_size_km,
            minimum_km: 1,
        }
    );

    let spacing = native_resolution_by_subsystem("asteroid-fragment-spacing")
        .expect("asteroid-fragment-spacing declared");
    assert_eq!(
        spacing.native_resolution_value,
        NativeResolutionValue::ConfiguredCentimeterField {
            field: "AsteroidFragmentConfig.min_fragment_spacing_cm",
            default_cm: AsteroidFragmentConfig::default().min_fragment_spacing_cm,
            minimum_cm: 0,
        }
    );

    let mut config = AsteroidFragmentConfig::default();
    config.voxel_size_km = 0;
    config.min_fragment_spacing_cm = -50;
    let sanitized = config.sanitized();
    assert_eq!(sanitized.voxel_size_km, 1);
    assert_eq!(sanitized.min_fragment_spacing_cm, 0);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
)]
fn distance_bucket_declarations_match_runtime_round_up_behavior() {
    for subsystem_id in ["movement-energy-cost", "power-transfer-distance"] {
        let declaration =
            native_resolution_by_subsystem(subsystem_id).expect("distance bucket declared");
        assert_eq!(
            declaration.native_resolution_value,
            NativeResolutionValue::FixedDistanceBucketCm {
                bucket_cm: CM_PER_KM,
            }
        );
        assert_eq!(
            declaration.cm_mapping_rule,
            CmMappingRule::CeilDistanceIntoKilometerBuckets
        );
        assert_eq!(
            declaration.rounding_rule,
            RoundingRule::CeilPositiveDistance
        );
    }

    let mut config = WorldConfig::default();
    config.move_cost_per_km_electricity = 2;
    assert_eq!(config.movement_cost(1), 2);
    assert_eq!(config.movement_cost(CM_PER_KM), 2);
    assert_eq!(config.movement_cost(CM_PER_KM + 1), 4);
}

#[test]
fn location_site_actions_and_fragment_blocks_have_explicit_snapping_rules() {
    let site = native_resolution_by_subsystem("location-site-actions")
        .expect("location-site-actions declared");
    assert_eq!(
        site.native_resolution_kind,
        NativeResolutionKind::LocationSite
    );
    assert_eq!(
        site.native_resolution_value,
        NativeResolutionValue::DiscreteLocationAnchor {
            location_id_field: "LocationId",
            position_field: "Location.pos",
            radius_field: "Location.profile.radius_cm",
        }
    );
    assert_eq!(
        site.cm_mapping_rule,
        CmMappingRule::ResolveLocationIdToPhysicalAnchor
    );
    assert_eq!(site.rounding_rule, RoundingRule::BindToDiscreteLocationId);

    let block = native_resolution_by_subsystem("fragment-block-geometry")
        .expect("fragment-block-geometry declared");
    assert_eq!(
        block.native_resolution_kind,
        NativeResolutionKind::FragmentBlock
    );
    assert_eq!(
        block.native_resolution_value,
        NativeResolutionValue::FixedCentimeter { centimeters: 1 }
    );
    assert_eq!(
        block.cm_mapping_rule,
        CmMappingRule::ClampBlockEdgesToAtLeastOneCentimeter
    );
    assert_eq!(block.rounding_rule, RoundingRule::ClampMinimumOneCentimeter);
    assert_eq!(*block, fragment_block_native_resolution());
}
