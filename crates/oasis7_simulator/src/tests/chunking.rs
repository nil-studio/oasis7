use super::*;

#[test]
fn chunk_grid_dims_match_20x20x10km_partition() {
    let space = SpaceConfig::default();
    let dims = chunk_grid_dims(&space);
    assert_eq!(dims, (5, 5, 1));
}

#[test]
fn chunk_coord_of_maps_positions_consistently() {
    let space = SpaceConfig::default();

    let a = chunk_coord_of(GeoPos::new(10_000, 10_000, 1_000), &space).unwrap();
    assert_eq!(a, ChunkCoord { x: 0, y: 0, z: 0 });

    let b = chunk_coord_of(GeoPos::new(2_500_000, 4_200_000, 10_000), &space).unwrap();
    assert_eq!(b, ChunkCoord { x: 1, y: 2, z: 0 });

    let c = chunk_coord_of(GeoPos::new(9_999_999, 9_999_999, 999_999), &space).unwrap();
    assert_eq!(c, ChunkCoord { x: 4, y: 4, z: 0 });
}

#[test]
fn chunk_bounds_clip_at_space_edge() {
    let space = SpaceConfig {
        width_cm: 9_000_000,
        depth_cm: 7_500_000,
        height_cm: 1_500_000,
    };

    let bounds = chunk_bounds(ChunkCoord { x: 4, y: 3, z: 1 }, &space).unwrap();
    assert_eq!(bounds.min, GeoPos::new(8_000_000, 6_000_000, 1_000_000));
    assert_eq!(bounds.max, GeoPos::new(9_000_000, 7_500_000, 1_500_000));
}

#[test]
fn chunk_seed_is_stable_and_coord_sensitive() {
    let world_seed = 42;
    let a1 = chunk_seed(world_seed, ChunkCoord { x: 1, y: 2, z: 0 });
    let a2 = chunk_seed(world_seed, ChunkCoord { x: 1, y: 2, z: 0 });
    let b = chunk_seed(world_seed, ChunkCoord { x: 2, y: 1, z: 0 });

    assert_eq!(a1, a2);
    assert_ne!(a1, b);
}

#[test]
fn chunk_coords_cover_default_grid() {
    let space = SpaceConfig::default();
    let coords = chunk_coords(&space);

    assert_eq!(coords.len(), 25);
    assert!(coords.contains(&ChunkCoord { x: 0, y: 0, z: 0 }));
    assert!(coords.contains(&ChunkCoord { x: 4, y: 4, z: 0 }));
}

#[test]
fn element_composition_tracks_ppm_totals() {
    let mut composition = ElementComposition::new();
    composition.set(FragmentElementKind::Iron, 120_000);
    composition.set(FragmentElementKind::Nickel, 40_000);
    composition.set(FragmentElementKind::Silicon, 0);

    assert_eq!(composition.get(FragmentElementKind::Iron), 120_000);
    assert_eq!(composition.get(FragmentElementKind::Nickel), 40_000);
    assert_eq!(composition.get(FragmentElementKind::Silicon), 0);
    assert_eq!(composition.total_ppm(), 160_000);
}
