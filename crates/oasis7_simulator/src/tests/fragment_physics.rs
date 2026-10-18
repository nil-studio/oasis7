use super::*;

#[test]
fn cuboid_size_is_sanitized_to_minimum_1cm() {
    let size = CuboidSizeCm {
        x_cm: 0,
        y_cm: -3,
        z_cm: 5,
    }
    .sanitized();

    assert_eq!(size.x_cm, 1);
    assert_eq!(size.y_cm, 1);
    assert_eq!(size.z_cm, 5);
}

#[test]
fn block_volume_and_mass_follow_formula() {
    let block = FragmentBlock {
        origin_cm: GridPosCm::new(0, 0, 0),
        size_cm: CuboidSizeCm {
            x_cm: 100,
            y_cm: 200,
            z_cm: 300,
        },
        density_kg_per_m3: 2_500,
        compounds: CompoundComposition::new(),
    };

    assert_eq!(block.volume_cm3(), 6_000_000);
    assert_eq!(block.mass_g(), 15_000_000);
}

#[test]
fn block_field_aggregates_volume_and_mass() {
    let mut compounds = CompoundComposition::new();
    compounds.set(FragmentCompoundKind::SilicateMatrix, 1_000_000);

    let field = FragmentBlockField {
        blocks: vec![
            FragmentBlock {
                origin_cm: GridPosCm::new(0, 0, 0),
                size_cm: CuboidSizeCm {
                    x_cm: 10,
                    y_cm: 10,
                    z_cm: 10,
                },
                density_kg_per_m3: 3_000,
                compounds: compounds.clone(),
            },
            FragmentBlock {
                origin_cm: GridPosCm::new(10, 0, 0),
                size_cm: CuboidSizeCm {
                    x_cm: 20,
                    y_cm: 10,
                    z_cm: 10,
                },
                density_kg_per_m3: 2_000,
                compounds,
            },
        ],
    };

    assert_eq!(field.total_volume_cm3(), 3_000);
    assert_eq!(field.total_mass_g(), 7_000);
}

#[test]
fn infer_element_ppm_prefers_compound_signatures() {
    let mut compounds = CompoundComposition::new();
    compounds.set(FragmentCompoundKind::WaterIce, 400_000);
    compounds.set(FragmentCompoundKind::IronNickelAlloy, 600_000);

    let elements = infer_element_ppm(&compounds);

    assert!(elements.get(FragmentElementKind::Oxygen) > 0);
    assert!(elements.get(FragmentElementKind::Hydrogen) > 0);
    assert!(elements.get(FragmentElementKind::Iron) > 0);
    assert!(elements.get(FragmentElementKind::Nickel) > 0);
    assert!(elements.get(FragmentElementKind::Silicon) == 0);
}

#[test]
fn compound_composition_total_ppm_is_accumulated() {
    let mut compounds = CompoundComposition::new();
    compounds.set(FragmentCompoundKind::SilicateMatrix, 300_000);
    compounds.set(FragmentCompoundKind::CarbonaceousOrganic, 200_000);
    compounds.set(FragmentCompoundKind::SulfideOre, 0);

    assert_eq!(compounds.total_ppm(), 500_000);
}

#[test]
fn synthesized_fragment_profile_contains_blocks_and_physics() {
    let profile = synthesize_fragment_profile(123, 120, MaterialKind::Composite);

    assert!(!profile.blocks.blocks.is_empty());
    assert!(profile.total_volume_cm3 > 0);
    assert!(profile.total_mass_g > 0);
    assert!(profile.bulk_density_kg_per_m3 > 0);
    assert!(profile.compounds.total_ppm() > 0);
    assert!(profile.elements.total_ppm() > 0);

    for block in &profile.blocks.blocks {
        assert!(block.size_cm.x_cm >= MIN_BLOCK_EDGE_CM);
        assert!(block.size_cm.y_cm >= MIN_BLOCK_EDGE_CM);
        assert!(block.size_cm.z_cm >= MIN_BLOCK_EDGE_CM);
        assert!(block.volume_cm3() > 0);
        assert!(block.mass_g() >= 0);
    }
}

#[test]
fn synthesized_fragment_profile_is_deterministic() {
    let a = synthesize_fragment_profile(999, 250, MaterialKind::Silicate);
    let b = synthesize_fragment_profile(999, 250, MaterialKind::Silicate);

    assert_eq!(a, b);
}

#[test]
fn synthesized_fragment_budget_initializes_total_and_remaining() {
    let profile = synthesize_fragment_profile(321, 200, MaterialKind::Metal);
    let budget = synthesize_fragment_budget(&profile);

    assert!(!budget.total_by_element_g.is_empty());
    assert_eq!(budget.total_by_element_g, budget.remaining_by_element_g);
    let total_g: i64 = budget.total_by_element_g.values().copied().sum();
    assert!(total_g > 0);
}
