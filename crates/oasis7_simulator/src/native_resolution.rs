use crate::geometry::SPACE_UNIT_CM;

use super::{
    CM_PER_KM,
    chunking::{CHUNK_SIZE_X_CM, CHUNK_SIZE_Y_CM, CHUNK_SIZE_Z_CM},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeResolutionKind {
    CanonicalPhysicalScale,
    ChunkGrid,
    VoxelGenerator,
    SpacingGuard,
    DistanceBucket,
    LocationSite,
    FragmentBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeResolutionValue {
    FixedCentimeter {
        centimeters: i64,
    },
    FixedCuboidCm {
        x_cm: i64,
        y_cm: i64,
        z_cm: i64,
    },
    ConfiguredVoxelKm {
        field: &'static str,
        default_km: i64,
        minimum_km: i64,
    },
    ConfiguredCentimeterField {
        field: &'static str,
        default_cm: i64,
        minimum_cm: i64,
    },
    FixedDistanceBucketCm {
        bucket_cm: i64,
    },
    DiscreteLocationAnchor {
        location_id_field: &'static str,
        position_field: &'static str,
        radius_field: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmMappingRule {
    IdentityIntegerCentimeter,
    FloorDivideIntoChunkIndex,
    LocalVoxelBoundsToCentimeterSpace,
    SurfaceSpacingCentimeterGuard,
    CeilDistanceIntoKilometerBuckets,
    ResolveLocationIdToPhysicalAnchor,
    ClampBlockEdgesToAtLeastOneCentimeter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundingRule {
    ExactIntegerCentimeter,
    FloorDivideWithUpperEdgeClamp,
    RoundGeneratedCoordinatesToNearestCentimeter,
    ClampNegativeSpacingToZero,
    CeilPositiveDistance,
    BindToDiscreteLocationId,
    ClampMinimumOneCentimeter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeResolutionDeclaration {
    pub subsystem_id: &'static str,
    pub native_resolution_kind: NativeResolutionKind,
    pub native_resolution_value: NativeResolutionValue,
    pub cm_mapping_rule: CmMappingRule,
    pub rounding_rule: RoundingRule,
    pub implemented_by: &'static [&'static str],
}

const CANONICAL_PHYSICAL_SPACE_IMPL: &[&str] = &[
    "geometry::SPACE_UNIT_CM",
    "geometry::GeoPos",
    "geometry::space_distance_cm",
    "types::LocationProfile::radius_cm",
    "fragment_physics::CuboidSizeCm",
];

const CHUNK_GRID_IMPL: &[&str] = &[
    "chunking::CHUNK_SIZE_X_CM",
    "chunking::CHUNK_SIZE_Y_CM",
    "chunking::CHUNK_SIZE_Z_CM",
    "chunking::chunk_coord_of",
    "chunking::chunk_bounds",
];

const ASTEROID_FRAGMENT_VOXEL_IMPL: &[&str] = &[
    "world_model::AsteroidFragmentConfig::voxel_size_km",
    "asteroid_fragment::generate_fragments",
    "init::generate_chunk_at",
];

const ASTEROID_FRAGMENT_SPACING_IMPL: &[&str] = &[
    "world_model::AsteroidFragmentConfig::min_fragment_spacing_cm",
    "asteroid_fragment::generate_fragments",
    "init::generate_chunk_at",
];

const MOVEMENT_DISTANCE_BUCKET_IMPL: &[&str] = &[
    "world_model_config::movement_cost",
    "kernel::actions_core::WorldKernel::move_agent",
];

const POWER_DISTANCE_BUCKET_IMPL: &[&str] = &[
    "kernel::actions_resolution::WorldKernel::power_transfer_distance_km",
    "kernel::actions_resolution::WorldKernel::power_transfer_loss",
];

const LOCATION_SITE_IMPL: &[&str] = &[
    "types::Action::MoveAgent",
    "types::Action::BuildFactory",
    "types::Action::MineCompound",
    "world_model::Factory::location_id",
    "kernel::actions_resolution::WorldKernel::ensure_colocated",
];

const FRAGMENT_BLOCK_IMPL: &[&str] = &[
    "fragment_physics::MIN_BLOCK_EDGE_CM",
    "fragment_physics::CuboidSizeCm::sanitized",
    "fragment_physics::FragmentBlock::sanitized",
];

pub const RUNTIME_NATIVE_RESOLUTIONS: [NativeResolutionDeclaration; 8] = [
    NativeResolutionDeclaration {
        subsystem_id: "canonical-physical-space",
        native_resolution_kind: NativeResolutionKind::CanonicalPhysicalScale,
        native_resolution_value: NativeResolutionValue::FixedCentimeter {
            centimeters: SPACE_UNIT_CM,
        },
        cm_mapping_rule: CmMappingRule::IdentityIntegerCentimeter,
        rounding_rule: RoundingRule::ExactIntegerCentimeter,
        implemented_by: CANONICAL_PHYSICAL_SPACE_IMPL,
    },
    NativeResolutionDeclaration {
        subsystem_id: "chunk-grid",
        native_resolution_kind: NativeResolutionKind::ChunkGrid,
        native_resolution_value: NativeResolutionValue::FixedCuboidCm {
            x_cm: CHUNK_SIZE_X_CM,
            y_cm: CHUNK_SIZE_Y_CM,
            z_cm: CHUNK_SIZE_Z_CM,
        },
        cm_mapping_rule: CmMappingRule::FloorDivideIntoChunkIndex,
        rounding_rule: RoundingRule::FloorDivideWithUpperEdgeClamp,
        implemented_by: CHUNK_GRID_IMPL,
    },
    NativeResolutionDeclaration {
        subsystem_id: "asteroid-fragment-voxel",
        native_resolution_kind: NativeResolutionKind::VoxelGenerator,
        native_resolution_value: NativeResolutionValue::ConfiguredVoxelKm {
            field: "AsteroidFragmentConfig.voxel_size_km",
            default_km: 10,
            minimum_km: 1,
        },
        cm_mapping_rule: CmMappingRule::LocalVoxelBoundsToCentimeterSpace,
        rounding_rule: RoundingRule::RoundGeneratedCoordinatesToNearestCentimeter,
        implemented_by: ASTEROID_FRAGMENT_VOXEL_IMPL,
    },
    NativeResolutionDeclaration {
        subsystem_id: "asteroid-fragment-spacing",
        native_resolution_kind: NativeResolutionKind::SpacingGuard,
        native_resolution_value: NativeResolutionValue::ConfiguredCentimeterField {
            field: "AsteroidFragmentConfig.min_fragment_spacing_cm",
            default_cm: 50_000,
            minimum_cm: 0,
        },
        cm_mapping_rule: CmMappingRule::SurfaceSpacingCentimeterGuard,
        rounding_rule: RoundingRule::ClampNegativeSpacingToZero,
        implemented_by: ASTEROID_FRAGMENT_SPACING_IMPL,
    },
    NativeResolutionDeclaration {
        subsystem_id: "movement-energy-cost",
        native_resolution_kind: NativeResolutionKind::DistanceBucket,
        native_resolution_value: NativeResolutionValue::FixedDistanceBucketCm {
            bucket_cm: CM_PER_KM,
        },
        cm_mapping_rule: CmMappingRule::CeilDistanceIntoKilometerBuckets,
        rounding_rule: RoundingRule::CeilPositiveDistance,
        implemented_by: MOVEMENT_DISTANCE_BUCKET_IMPL,
    },
    NativeResolutionDeclaration {
        subsystem_id: "power-transfer-distance",
        native_resolution_kind: NativeResolutionKind::DistanceBucket,
        native_resolution_value: NativeResolutionValue::FixedDistanceBucketCm {
            bucket_cm: CM_PER_KM,
        },
        cm_mapping_rule: CmMappingRule::CeilDistanceIntoKilometerBuckets,
        rounding_rule: RoundingRule::CeilPositiveDistance,
        implemented_by: POWER_DISTANCE_BUCKET_IMPL,
    },
    NativeResolutionDeclaration {
        subsystem_id: "location-site-actions",
        native_resolution_kind: NativeResolutionKind::LocationSite,
        native_resolution_value: NativeResolutionValue::DiscreteLocationAnchor {
            location_id_field: "LocationId",
            position_field: "Location.pos",
            radius_field: "Location.profile.radius_cm",
        },
        cm_mapping_rule: CmMappingRule::ResolveLocationIdToPhysicalAnchor,
        rounding_rule: RoundingRule::BindToDiscreteLocationId,
        implemented_by: LOCATION_SITE_IMPL,
    },
    NativeResolutionDeclaration {
        subsystem_id: "fragment-block-geometry",
        native_resolution_kind: NativeResolutionKind::FragmentBlock,
        native_resolution_value: NativeResolutionValue::FixedCentimeter { centimeters: 1 },
        cm_mapping_rule: CmMappingRule::ClampBlockEdgesToAtLeastOneCentimeter,
        rounding_rule: RoundingRule::ClampMinimumOneCentimeter,
        implemented_by: FRAGMENT_BLOCK_IMPL,
    },
];

pub fn runtime_native_resolutions() -> &'static [NativeResolutionDeclaration] {
    &RUNTIME_NATIVE_RESOLUTIONS
}

pub fn native_resolution_by_subsystem(
    subsystem_id: &str,
) -> Option<&'static NativeResolutionDeclaration> {
    runtime_native_resolutions()
        .iter()
        .find(|declaration| declaration.subsystem_id == subsystem_id)
}

pub fn fragment_block_native_resolution() -> NativeResolutionDeclaration {
    *native_resolution_by_subsystem("fragment-block-geometry")
        .expect("fragment-block-geometry declared")
}
