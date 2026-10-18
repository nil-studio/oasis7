use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::types::{
    DEFAULT_ELEMENT_RECOVERABILITY_PPM, ElementComposition, FragmentElementKind,
    FragmentResourceBudget, MaterialKind,
};

pub const MIN_BLOCK_EDGE_CM: i64 = 1;
pub const CM3_PER_M3: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FragmentCompoundKind {
    SilicateMatrix,
    IronNickelAlloy,
    WaterIce,
    HydratedMineral,
    CarbonaceousOrganic,
    SulfideOre,
    RareEarthOxide,
    UraniumBearingOre,
    ThoriumBearingOre,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CompoundComposition {
    pub ppm: BTreeMap<FragmentCompoundKind, u32>,
}

impl CompoundComposition {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, kind: FragmentCompoundKind) -> u32 {
        *self.ppm.get(&kind).unwrap_or(&0)
    }

    pub fn set(&mut self, kind: FragmentCompoundKind, value: u32) {
        if value == 0 {
            self.ppm.remove(&kind);
        } else {
            self.ppm.insert(kind, value);
        }
    }

    pub fn total_ppm(&self) -> u64 {
        self.ppm.values().map(|value| *value as u64).sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridPosCm {
    pub x_cm: i64,
    pub y_cm: i64,
    pub z_cm: i64,
}

impl GridPosCm {
    pub fn new(x_cm: i64, y_cm: i64, z_cm: i64) -> Self {
        Self { x_cm, y_cm, z_cm }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CuboidSizeCm {
    pub x_cm: i64,
    pub y_cm: i64,
    pub z_cm: i64,
}

impl CuboidSizeCm {
    pub fn sanitized(mut self) -> Self {
        if self.x_cm < MIN_BLOCK_EDGE_CM {
            self.x_cm = MIN_BLOCK_EDGE_CM;
        }
        if self.y_cm < MIN_BLOCK_EDGE_CM {
            self.y_cm = MIN_BLOCK_EDGE_CM;
        }
        if self.z_cm < MIN_BLOCK_EDGE_CM {
            self.z_cm = MIN_BLOCK_EDGE_CM;
        }
        self
    }

    pub fn volume_cm3(&self) -> i64 {
        self.x_cm
            .saturating_mul(self.y_cm)
            .saturating_mul(self.z_cm)
            .max(0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FragmentBlock {
    pub origin_cm: GridPosCm,
    pub size_cm: CuboidSizeCm,
    pub density_kg_per_m3: i64,
    pub compounds: CompoundComposition,
}

impl FragmentBlock {
    pub fn sanitized(mut self) -> Self {
        self.size_cm = self.size_cm.sanitized();
        if self.density_kg_per_m3 < 0 {
            self.density_kg_per_m3 = 0;
        }
        self
    }

    pub fn volume_cm3(&self) -> i64 {
        self.size_cm.volume_cm3()
    }

    pub fn mass_g(&self) -> i64 {
        mass_grams_from_volume_density(self.volume_cm3(), self.density_kg_per_m3)
    }

    pub fn element_ppm(&self) -> ElementComposition {
        infer_element_ppm(&self.compounds)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FragmentBlockField {
    pub blocks: Vec<FragmentBlock>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FragmentPhysicalProfile {
    pub blocks: FragmentBlockField,
    pub compounds: CompoundComposition,
    pub elements: ElementComposition,
    pub total_volume_cm3: i64,
    pub total_mass_g: i64,
    pub bulk_density_kg_per_m3: i64,
}

impl FragmentBlockField {
    pub fn total_volume_cm3(&self) -> i64 {
        self.blocks
            .iter()
            .map(FragmentBlock::volume_cm3)
            .fold(0i64, |acc, value| acc.saturating_add(value))
    }

    pub fn total_mass_g(&self) -> i64 {
        self.blocks
            .iter()
            .map(FragmentBlock::mass_g)
            .fold(0i64, |acc, value| acc.saturating_add(value))
    }
}

pub fn mass_grams_from_volume_density(volume_cm3: i64, density_kg_per_m3: i64) -> i64 {
    if volume_cm3 <= 0 || density_kg_per_m3 <= 0 {
        return 0;
    }
    density_kg_per_m3
        .saturating_mul(volume_cm3)
        .saturating_div(1000)
}

pub fn synthesize_fragment_profile(
    seed: u64,
    radius_cm: i64,
    material: MaterialKind,
) -> FragmentPhysicalProfile {
    let side_cm = (radius_cm.max(1)).saturating_mul(2).max(1);
    let block_count = ((radius_cm.max(1) + 249) / 250).clamp(1, 16) as usize;
    let base_density = material_base_density(material);
    let base_compounds = material_compounds(material);

    let mut blocks = Vec::with_capacity(block_count);
    let mut cursor_x = 0i64;
    for index in 0..block_count {
        let remaining_blocks = (block_count - index) as i64;
        let remaining_x = (side_cm - cursor_x).max(1);
        let width_x = if remaining_blocks == 1 {
            remaining_x
        } else {
            (remaining_x / remaining_blocks).max(1)
        };

        let roll = splitmix64(seed.wrapping_add(index as u64));
        let y_ratio = 70 + ((roll & 0x1F) as i64); // 70~101
        let z_ratio = 70 + (((roll >> 5) & 0x1F) as i64);
        let size_y = ((side_cm * y_ratio) / 100).clamp(1, side_cm);
        let size_z = ((side_cm * z_ratio) / 100).clamp(1, side_cm);

        let origin_y = (side_cm - size_y) / 2;
        let origin_z = (side_cm - size_z) / 2;

        let density_jitter = ((roll >> 10) % 401) as i64 - 200; // [-200, 200]
        let density = (base_density + density_jitter).max(0);

        blocks.push(
            FragmentBlock {
                origin_cm: GridPosCm::new(cursor_x, origin_y, origin_z),
                size_cm: CuboidSizeCm {
                    x_cm: width_x,
                    y_cm: size_y,
                    z_cm: size_z,
                },
                density_kg_per_m3: density,
                compounds: base_compounds.clone(),
            }
            .sanitized(),
        );

        cursor_x = cursor_x.saturating_add(width_x);
    }

    let block_field = FragmentBlockField { blocks };
    let total_volume_cm3 = block_field.total_volume_cm3();
    let total_mass_g = block_field.total_mass_g();
    let bulk_density_kg_per_m3 = if total_volume_cm3 > 0 {
        total_mass_g
            .saturating_mul(1000)
            .saturating_div(total_volume_cm3)
    } else {
        0
    };

    let compounds = aggregate_compound_ppm(&block_field);
    let elements = infer_element_ppm(&compounds);

    FragmentPhysicalProfile {
        blocks: block_field,
        compounds,
        elements,
        total_volume_cm3,
        total_mass_g,
        bulk_density_kg_per_m3,
    }
}

pub fn truncate_fragment_profile_blocks(profile: &mut FragmentPhysicalProfile, max_blocks: usize) {
    if profile.blocks.blocks.len() > max_blocks {
        profile.blocks.blocks.truncate(max_blocks);
    }

    profile.total_volume_cm3 = profile.blocks.total_volume_cm3();
    profile.total_mass_g = profile.blocks.total_mass_g();
    profile.bulk_density_kg_per_m3 = if profile.total_volume_cm3 > 0 {
        profile
            .total_mass_g
            .saturating_mul(1000)
            .saturating_div(profile.total_volume_cm3)
    } else {
        0
    };
    profile.compounds = aggregate_compound_ppm(&profile.blocks);
    profile.elements = infer_element_ppm(&profile.compounds);
}

pub fn synthesize_fragment_budget(profile: &FragmentPhysicalProfile) -> FragmentResourceBudget {
    FragmentResourceBudget::from_mass_and_elements(
        profile.total_mass_g,
        &profile.elements,
        DEFAULT_ELEMENT_RECOVERABILITY_PPM,
    )
}

pub fn infer_element_ppm(compounds: &CompoundComposition) -> ElementComposition {
    let mut aggregate = BTreeMap::<FragmentElementKind, u64>::new();

    for (compound, compound_ppm) in &compounds.ppm {
        if *compound_ppm == 0 {
            continue;
        }
        let signature = compound_element_signature(*compound);
        let signature_total = signature
            .iter()
            .map(|(_, weight)| *weight as u64)
            .sum::<u64>();
        if signature_total == 0 {
            continue;
        }

        for (element, weight) in signature {
            let add_ppm = (*compound_ppm as u64)
                .saturating_mul(*weight as u64)
                .saturating_div(signature_total);
            let entry = aggregate.entry(*element).or_insert(0);
            *entry = entry.saturating_add(add_ppm);
        }
    }

    let mut output = ElementComposition::new();
    for (element, ppm) in aggregate {
        output.set(element, ppm.min(u32::MAX as u64) as u32);
    }
    output
}

fn compound_element_signature(
    compound: FragmentCompoundKind,
) -> &'static [(FragmentElementKind, u32)] {
    match compound {
        FragmentCompoundKind::SilicateMatrix => &[
            (FragmentElementKind::Oxygen, 600),
            (FragmentElementKind::Silicon, 250),
            (FragmentElementKind::Magnesium, 60),
            (FragmentElementKind::Aluminum, 50),
            (FragmentElementKind::Calcium, 40),
        ],
        FragmentCompoundKind::IronNickelAlloy => &[
            (FragmentElementKind::Iron, 850),
            (FragmentElementKind::Nickel, 120),
            (FragmentElementKind::Cobalt, 30),
        ],
        FragmentCompoundKind::WaterIce => &[
            (FragmentElementKind::Hydrogen, 111),
            (FragmentElementKind::Oxygen, 889),
        ],
        FragmentCompoundKind::HydratedMineral => &[
            (FragmentElementKind::Oxygen, 500),
            (FragmentElementKind::Hydrogen, 80),
            (FragmentElementKind::Silicon, 200),
            (FragmentElementKind::Magnesium, 120),
            (FragmentElementKind::Aluminum, 100),
        ],
        FragmentCompoundKind::CarbonaceousOrganic => &[
            (FragmentElementKind::Carbon, 700),
            (FragmentElementKind::Hydrogen, 100),
            (FragmentElementKind::Oxygen, 120),
            (FragmentElementKind::Nitrogen, 50),
            (FragmentElementKind::Sulfur, 30),
        ],
        FragmentCompoundKind::SulfideOre => &[
            (FragmentElementKind::Iron, 400),
            (FragmentElementKind::Nickel, 100),
            (FragmentElementKind::Sulfur, 500),
        ],
        FragmentCompoundKind::RareEarthOxide => &[
            (FragmentElementKind::Oxygen, 350),
            (FragmentElementKind::Neodymium, 650),
        ],
        FragmentCompoundKind::UraniumBearingOre => &[
            (FragmentElementKind::Oxygen, 200),
            (FragmentElementKind::Uranium, 800),
        ],
        FragmentCompoundKind::ThoriumBearingOre => &[
            (FragmentElementKind::Oxygen, 250),
            (FragmentElementKind::Thorium, 750),
        ],
    }
}

fn material_base_density(material: MaterialKind) -> i64 {
    match material {
        MaterialKind::Silicate => 3_200,
        MaterialKind::Metal => 7_800,
        MaterialKind::Ice => 900,
        MaterialKind::Carbon => 1_800,
        MaterialKind::Composite => 2_600,
    }
}

fn material_compounds(material: MaterialKind) -> CompoundComposition {
    let mut compounds = CompoundComposition::new();
    let mix = match material {
        MaterialKind::Silicate => &[
            (FragmentCompoundKind::SilicateMatrix, 760_000),
            (FragmentCompoundKind::HydratedMineral, 120_000),
            (FragmentCompoundKind::SulfideOre, 60_000),
            (FragmentCompoundKind::RareEarthOxide, 40_000),
            (FragmentCompoundKind::WaterIce, 20_000),
        ][..],
        MaterialKind::Metal => &[
            (FragmentCompoundKind::IronNickelAlloy, 760_000),
            (FragmentCompoundKind::SulfideOre, 100_000),
            (FragmentCompoundKind::SilicateMatrix, 60_000),
            (FragmentCompoundKind::UraniumBearingOre, 40_000),
            (FragmentCompoundKind::ThoriumBearingOre, 40_000),
        ][..],
        MaterialKind::Ice => &[
            (FragmentCompoundKind::WaterIce, 760_000),
            (FragmentCompoundKind::HydratedMineral, 140_000),
            (FragmentCompoundKind::CarbonaceousOrganic, 60_000),
            (FragmentCompoundKind::SilicateMatrix, 40_000),
        ][..],
        MaterialKind::Carbon => &[
            (FragmentCompoundKind::CarbonaceousOrganic, 700_000),
            (FragmentCompoundKind::SulfideOre, 120_000),
            (FragmentCompoundKind::HydratedMineral, 80_000),
            (FragmentCompoundKind::WaterIce, 60_000),
            (FragmentCompoundKind::RareEarthOxide, 40_000),
        ][..],
        MaterialKind::Composite => &[
            (FragmentCompoundKind::SilicateMatrix, 360_000),
            (FragmentCompoundKind::IronNickelAlloy, 260_000),
            (FragmentCompoundKind::CarbonaceousOrganic, 140_000),
            (FragmentCompoundKind::HydratedMineral, 120_000),
            (FragmentCompoundKind::RareEarthOxide, 60_000),
            (FragmentCompoundKind::UraniumBearingOre, 30_000),
            (FragmentCompoundKind::ThoriumBearingOre, 30_000),
        ][..],
    };

    for (kind, ppm) in mix {
        compounds.set(*kind, *ppm);
    }
    compounds
}

fn aggregate_compound_ppm(block_field: &FragmentBlockField) -> CompoundComposition {
    let mut weighted = BTreeMap::<FragmentCompoundKind, u128>::new();
    let mut total_mass = 0u128;

    for block in &block_field.blocks {
        let mass = block.mass_g().max(0) as u128;
        if mass == 0 {
            continue;
        }
        total_mass = total_mass.saturating_add(mass);
        for (kind, ppm) in &block.compounds.ppm {
            let entry = weighted.entry(*kind).or_insert(0);
            *entry = entry.saturating_add(mass.saturating_mul(*ppm as u128));
        }
    }

    let mut out = CompoundComposition::new();
    if total_mass == 0 {
        return out;
    }

    for (kind, value) in weighted {
        let ppm = value.saturating_div(total_mass).min(u32::MAX as u128) as u32;
        out.set(kind, ppm);
    }
    out
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}
