use super::super::{
    M4_ECONOMY_MODULE_VERSION, M4_FACTORY_ASSEMBLER_MODULE_ID, M4_FACTORY_MINER_MODULE_ID,
    M4_FACTORY_SMELTER_MODULE_ID, M4_PRODUCT_ALLOY_PLATE_MODULE_ID,
    M4_PRODUCT_CONTROL_CHIP_MODULE_ID, M4_PRODUCT_FACTORY_CORE_MODULE_ID,
    M4_PRODUCT_IRON_INGOT_MODULE_ID, M4_PRODUCT_LOGISTICS_DRONE_MODULE_ID,
    M4_PRODUCT_MODULE_RACK_MODULE_ID, M4_PRODUCT_MOTOR_MODULE_ID, M4_PRODUCT_SENSOR_PACK_MODULE_ID,
    M4_RECIPE_ASSEMBLE_CONTROL_CHIP_MODULE_ID, M4_RECIPE_ASSEMBLE_DRONE_MODULE_ID,
    M4_RECIPE_ASSEMBLE_FACTORY_CORE_MODULE_ID, M4_RECIPE_ASSEMBLE_GEAR_MODULE_ID,
    M4_RECIPE_ASSEMBLE_MODULE_RACK_MODULE_ID, M4_RECIPE_ASSEMBLE_MOTOR_MODULE_ID,
    M4_RECIPE_ASSEMBLE_SENSOR_PACK_MODULE_ID, M4_RECIPE_SMELT_ALLOY_PLATE_MODULE_ID,
    M4_RECIPE_SMELT_COPPER_WIRE_MODULE_ID, M4_RECIPE_SMELT_IRON_MODULE_ID,
    M4_RECIPE_SMELT_POLYMER_RESIN_MODULE_ID, Manifest, MaterialDefaultPriority, MaterialProfileV1,
    MaterialStack, MaterialTransportLossClass, ModuleAbiContract, ModuleActivation,
    ModuleArtifactIdentity, ModuleChangeSet, ModuleKind, ModuleLimits, ModuleManifest,
    ModuleRegistry, ModuleRole, ProductProfileV1, ProposalDecision, RecipeProfileV1, WorldError,
    util,
};
use super::World;

const M4_BOOTSTRAP_WASM_MAX_MEM_BYTES: u64 = 64 * 1024 * 1024;
const M4_BOOTSTRAP_WASM_MAX_GAS: u64 = 2_000_000;
const M4_BUILTIN_MODULE_IDS_MANIFEST: &str = include_str!("artifacts/m4_builtin_module_ids.txt");

#[derive(Debug, Clone, Copy)]
struct M4BootstrapModuleDescriptor {
    module_id: &'static str,
    manifest_name: &'static str,
    max_call_rate: u32,
}

const M4_BOOTSTRAP_MODULES: &[M4BootstrapModuleDescriptor] = &[
    M4BootstrapModuleDescriptor {
        module_id: M4_FACTORY_MINER_MODULE_ID,
        manifest_name: "M4FactoryMinerMk1",
        max_call_rate: 32,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_FACTORY_SMELTER_MODULE_ID,
        manifest_name: "M4FactorySmelterMk1",
        max_call_rate: 32,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_FACTORY_ASSEMBLER_MODULE_ID,
        manifest_name: "M4FactoryAssemblerMk1",
        max_call_rate: 32,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_RECIPE_SMELT_IRON_MODULE_ID,
        manifest_name: "M4RecipeSmelterIronIngot",
        max_call_rate: 128,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_RECIPE_SMELT_COPPER_WIRE_MODULE_ID,
        manifest_name: "M4RecipeSmelterCopperWire",
        max_call_rate: 128,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_RECIPE_SMELT_POLYMER_RESIN_MODULE_ID,
        manifest_name: "M4RecipeSmelterPolymerResin",
        max_call_rate: 128,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_RECIPE_SMELT_ALLOY_PLATE_MODULE_ID,
        manifest_name: "M4RecipeSmelterAlloyPlate",
        max_call_rate: 128,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_RECIPE_ASSEMBLE_GEAR_MODULE_ID,
        manifest_name: "M4RecipeAssemblerGear",
        max_call_rate: 128,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_RECIPE_ASSEMBLE_CONTROL_CHIP_MODULE_ID,
        manifest_name: "M4RecipeAssemblerControlChip",
        max_call_rate: 128,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_RECIPE_ASSEMBLE_MOTOR_MODULE_ID,
        manifest_name: "M4RecipeAssemblerMotorMk1",
        max_call_rate: 128,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_RECIPE_ASSEMBLE_DRONE_MODULE_ID,
        manifest_name: "M4RecipeAssemblerLogisticsDrone",
        max_call_rate: 128,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_RECIPE_ASSEMBLE_SENSOR_PACK_MODULE_ID,
        manifest_name: "M4RecipeAssemblerSensorPack",
        max_call_rate: 128,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_RECIPE_ASSEMBLE_MODULE_RACK_MODULE_ID,
        manifest_name: "M4RecipeAssemblerModuleRack",
        max_call_rate: 128,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_RECIPE_ASSEMBLE_FACTORY_CORE_MODULE_ID,
        manifest_name: "M4RecipeAssemblerFactoryCore",
        max_call_rate: 128,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_PRODUCT_IRON_INGOT_MODULE_ID,
        manifest_name: "M4ProductIronIngot",
        max_call_rate: 64,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_PRODUCT_ALLOY_PLATE_MODULE_ID,
        manifest_name: "M4ProductAlloyPlate",
        max_call_rate: 64,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_PRODUCT_CONTROL_CHIP_MODULE_ID,
        manifest_name: "M4ProductControlChip",
        max_call_rate: 64,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_PRODUCT_MOTOR_MODULE_ID,
        manifest_name: "M4ProductMotorMk1",
        max_call_rate: 64,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_PRODUCT_LOGISTICS_DRONE_MODULE_ID,
        manifest_name: "M4ProductLogisticsDrone",
        max_call_rate: 64,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_PRODUCT_SENSOR_PACK_MODULE_ID,
        manifest_name: "M4ProductSensorPack",
        max_call_rate: 64,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_PRODUCT_MODULE_RACK_MODULE_ID,
        manifest_name: "M4ProductModuleRack",
        max_call_rate: 64,
    },
    M4BootstrapModuleDescriptor {
        module_id: M4_PRODUCT_FACTORY_CORE_MODULE_ID,
        manifest_name: "M4ProductFactoryCore",
        max_call_rate: 64,
    },
];

pub(crate) fn m4_bootstrap_module_ids() -> Vec<&'static str> {
    M4_BOOTSTRAP_MODULES
        .iter()
        .map(|item| item.module_id)
        .collect()
}

fn m4_builtin_module_ids_from_manifest() -> Vec<&'static str> {
    M4_BUILTIN_MODULE_IDS_MANIFEST
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn ensure_m4_bootstrap_descriptor_consistency() -> Result<(), WorldError> {
    let descriptor_ids = m4_bootstrap_module_ids();
    let manifest_ids = m4_builtin_module_ids_from_manifest();
    if descriptor_ids != manifest_ids {
        return Err(WorldError::ModuleChangeInvalid {
            reason: format!(
                "m4 bootstrap descriptor ids mismatch module ids manifest: descriptor=[{}] manifest=[{}]",
                descriptor_ids.join(","),
                manifest_ids.join(",")
            ),
        });
    }
    Ok(())
}

impl World {
    pub fn install_m4_economy_bootstrap_modules(
        &mut self,
        actor: impl Into<String>,
    ) -> Result<(), WorldError> {
        ensure_m4_bootstrap_descriptor_consistency()?;
        self.install_m4_profile_catalog_defaults()?;

        let actor = actor.into();
        let mut changes = ModuleChangeSet::default();

        for descriptor in M4_BOOTSTRAP_MODULES {
            let artifact =
                self.load_builtin_wasm_artifact_for_module("m4", descriptor.module_id)?;
            ensure_bootstrap_module(
                self,
                &mut changes,
                descriptor,
                &artifact,
                M4_ECONOMY_MODULE_VERSION,
                "m4",
            )?;
        }

        if changes.is_empty() {
            return Ok(());
        }

        let mut content = serde_json::Map::new();
        content.insert(
            "module_changes".to_string(),
            serde_json::to_value(&changes)?,
        );
        let manifest = Manifest {
            version: self.manifest.version.saturating_add(1),
            content: serde_json::Value::Object(content),
        };

        let proposal_id = self.propose_manifest_update(manifest, actor.clone())?;
        self.shadow_proposal(proposal_id)?;
        self.approve_proposal(proposal_id, actor.clone(), ProposalDecision::Approve)?;
        self.apply_proposal(proposal_id)?;

        Ok(())
    }

    fn install_m4_profile_catalog_defaults(&mut self) -> Result<(), WorldError> {
        for profile in m4_default_material_profiles() {
            let key = profile.kind.clone();
            if self.material_profile(key.as_str()).is_none() {
                self.upsert_material_profile(profile)?;
            }
        }
        for profile in m4_default_product_profiles() {
            let key = profile.product_id.clone();
            if self.product_profile(key.as_str()).is_none() {
                self.upsert_product_profile(profile)?;
            }
        }
        for profile in m4_default_recipe_profiles() {
            let key = profile.recipe_id.clone();
            if self.recipe_profile(key.as_str()).is_none() {
                self.upsert_recipe_profile(profile)?;
            }
        }
        Ok(())
    }
}

fn m4_default_material_profiles() -> Vec<MaterialProfileV1> {
    vec![
        material_profile(
            "iron_ore",
            1,
            "ore",
            1_200,
            MaterialTransportLossClass::High,
        ),
        material_profile(
            "copper_ore",
            1,
            "ore",
            1_200,
            MaterialTransportLossClass::High,
        ),
        material_profile(
            "carbon_fuel",
            1,
            "ore",
            1_000,
            MaterialTransportLossClass::High,
        ),
        material_profile(
            "silicate_ore",
            1,
            "ore",
            1_000,
            MaterialTransportLossClass::High,
        ),
        material_profile(
            "rare_earth_raw",
            1,
            "ore",
            800,
            MaterialTransportLossClass::High,
        ),
        material_profile(
            "iron_ingot",
            2,
            "intermediate",
            900,
            MaterialTransportLossClass::Medium,
        ),
        material_profile(
            "copper_wire",
            2,
            "intermediate",
            900,
            MaterialTransportLossClass::Medium,
        ),
        material_profile(
            "alloy_plate",
            2,
            "intermediate",
            700,
            MaterialTransportLossClass::Medium,
        ),
        material_profile(
            "polymer_resin",
            2,
            "intermediate",
            700,
            MaterialTransportLossClass::Medium,
        ),
        material_profile(
            "circuit_substrate",
            2,
            "intermediate",
            700,
            MaterialTransportLossClass::Medium,
        ),
        material_profile("gear", 3, "component", 600, MaterialTransportLossClass::Low),
        material_profile(
            "control_chip",
            3,
            "component",
            600,
            MaterialTransportLossClass::Low,
        ),
        material_profile(
            "motor_mk1",
            3,
            "component",
            600,
            MaterialTransportLossClass::Low,
        ),
        material_profile(
            "sensor_pack",
            3,
            "component",
            500,
            MaterialTransportLossClass::Low,
        ),
        material_profile(
            "power_core",
            3,
            "component",
            500,
            MaterialTransportLossClass::Low,
        ),
        material_profile(
            "logistics_drone",
            4,
            "product",
            200,
            MaterialTransportLossClass::Low,
        ),
        material_profile(
            "field_repair_kit",
            4,
            "product",
            200,
            MaterialTransportLossClass::Low,
        ),
        material_profile(
            "survey_probe",
            4,
            "product",
            200,
            MaterialTransportLossClass::Low,
        ),
        material_profile(
            "module_rack",
            4,
            "product",
            150,
            MaterialTransportLossClass::Low,
        ),
        material_profile(
            "factory_core",
            5,
            "infrastructure",
            100,
            MaterialTransportLossClass::Medium,
        ),
        material_profile(
            "relay_tower_kit",
            5,
            "infrastructure",
            100,
            MaterialTransportLossClass::Medium,
        ),
        material_profile(
            "grid_buffer_pack",
            5,
            "infrastructure",
            100,
            MaterialTransportLossClass::Medium,
        ),
        MaterialProfileV1 {
            kind: "hardware_part".to_string(),
            tier: 3,
            category: "component".to_string(),
            stack_limit: 500,
            transport_loss_class: MaterialTransportLossClass::Low,
            decay_bps_per_tick: 0,
            default_priority: MaterialDefaultPriority::Urgent,
        },
    ]
}

fn material_profile(
    kind: &str,
    tier: u8,
    category: &str,
    stack_limit: i64,
    transport_loss_class: MaterialTransportLossClass,
) -> MaterialProfileV1 {
    MaterialProfileV1 {
        kind: kind.to_string(),
        tier,
        category: category.to_string(),
        stack_limit,
        transport_loss_class,
        decay_bps_per_tick: 0,
        default_priority: MaterialDefaultPriority::Standard,
    }
}

pub(super) fn m4_default_product_profiles() -> Vec<ProductProfileV1> {
    vec![
        product_profile("iron_ingot", "scale", true, "bootstrap"),
        product_profile_with_sink(
            "control_chip",
            "scale",
            true,
            "scale_out",
            &[("hardware_part", 1)],
        ),
        product_profile_with_sink(
            "motor_mk1",
            "scale",
            true,
            "scale_out",
            &[("hardware_part", 1)],
        ),
        product_profile_with_sink(
            "sensor_pack",
            "explore",
            true,
            "scale_out",
            &[("hardware_part", 1)],
        ),
        product_profile_with_sink(
            "logistics_drone",
            "explore",
            true,
            "scale_out",
            &[("hardware_part", 2)],
        ),
        product_profile_with_sink(
            "module_rack",
            "governance",
            true,
            "governance",
            &[("hardware_part", 2)],
        ),
        product_profile_with_sink(
            "factory_core",
            "governance",
            false,
            "governance",
            &[("hardware_part", 4), ("alloy_plate", 1)],
        ),
        product_profile("field_repair_kit", "survival", true, "bootstrap"),
        product_profile("survey_probe", "explore", true, "scale_out"),
        product_profile("alloy_plate", "scale", true, "scale_out"),
    ]
}

fn product_profile(
    product_id: &str,
    role_tag: &str,
    tradable: bool,
    unlock_stage: &str,
) -> ProductProfileV1 {
    product_profile_with_sink(product_id, role_tag, tradable, unlock_stage, &[])
}

fn product_profile_with_sink(
    product_id: &str,
    role_tag: &str,
    tradable: bool,
    unlock_stage: &str,
    maintenance_sink: &[(&str, i64)],
) -> ProductProfileV1 {
    ProductProfileV1 {
        product_id: product_id.to_string(),
        role_tag: role_tag.to_string(),
        maintenance_sink: maintenance_sink
            .iter()
            .filter(|(_, amount)| *amount > 0)
            .map(|(kind, amount)| MaterialStack::new(*kind, *amount))
            .collect(),
        tradable,
        unlock_stage: unlock_stage.to_string(),
    }
}

fn m4_default_recipe_profiles() -> Vec<RecipeProfileV1> {
    vec![
        recipe_profile(
            "recipe.smelter.iron_ingot",
            vec!["iron_ore"],
            "bootstrap",
            vec!["smelter"],
        ),
        recipe_profile(
            "recipe.smelter.copper_wire",
            vec!["copper_ore"],
            "bootstrap",
            vec!["smelter"],
        ),
        recipe_profile(
            "recipe.smelter.polymer_resin",
            vec!["carbon_fuel", "silicate_ore"],
            "bootstrap",
            vec!["smelter", "thermal"],
        ),
        recipe_profile(
            "recipe.assembler.gear",
            vec!["iron_ingot"],
            "bootstrap",
            vec!["assembler"],
        ),
        recipe_profile(
            "recipe.assembler.control_chip",
            vec!["copper_wire"],
            "bootstrap",
            vec!["assembler", "precision"],
        ),
        recipe_profile(
            "recipe.assembler.motor_mk1",
            vec!["control_chip"],
            "bootstrap",
            vec!["assembler", "precision"],
        ),
        recipe_profile(
            "recipe.assembler.logistics_drone",
            vec!["motor_mk1", "control_chip"],
            "bootstrap",
            vec!["assembler"],
        ),
        recipe_profile(
            "recipe.smelter.alloy_plate",
            vec!["iron_ingot", "copper_wire"],
            "scale_out",
            vec!["smelter"],
        ),
        recipe_profile(
            "recipe.assembler.sensor_pack",
            vec!["control_chip", "copper_wire"],
            "scale_out",
            vec!["assembler", "precision"],
        ),
        recipe_profile(
            "recipe.assembler.module_rack",
            vec!["sensor_pack", "control_chip"],
            "governance",
            vec!["assembler", "precision"],
        ),
        recipe_profile(
            "recipe.assembler.factory_core",
            vec!["module_rack", "alloy_plate"],
            "governance",
            vec!["assembler", "heavy"],
        ),
    ]
}

fn recipe_profile(
    recipe_id: &str,
    bottleneck_tags: Vec<&str>,
    stage_gate: &str,
    preferred_factory_tags: Vec<&str>,
) -> RecipeProfileV1 {
    RecipeProfileV1 {
        recipe_id: recipe_id.to_string(),
        bottleneck_tags: bottleneck_tags.into_iter().map(str::to_string).collect(),
        stage_gate: stage_gate.to_string(),
        preferred_factory_tags: preferred_factory_tags
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

fn ensure_bootstrap_module(
    world: &mut World,
    changes: &mut ModuleChangeSet,
    descriptor: &M4BootstrapModuleDescriptor,
    artifact: &[u8],
    version: &str,
    module_set: &str,
) -> Result<(), WorldError> {
    if world
        .module_registry
        .active
        .get(descriptor.module_id)
        .is_some_and(|active_version| active_version == version)
    {
        return Ok(());
    }

    let record_key = ModuleRegistry::record_key(descriptor.module_id, version);
    if !world.module_registry.records.contains_key(&record_key) {
        let wasm_hash = util::sha256_hex(artifact);
        world.register_module_artifact(wasm_hash.clone(), artifact)?;
        let identity = world.resolve_builtin_module_artifact_identity(
            module_set,
            descriptor.module_id,
            &wasm_hash,
        )?;
        changes
            .register
            .push(m4_manifest(descriptor, wasm_hash, identity));
    }

    changes.activate.push(ModuleActivation {
        module_id: descriptor.module_id.to_string(),
        version: version.to_string(),
    });

    Ok(())
}

fn m4_manifest(
    descriptor: &M4BootstrapModuleDescriptor,
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    ModuleManifest {
        module_id: descriptor.module_id.to_string(),
        name: descriptor.manifest_name.to_string(),
        version: M4_ECONOMY_MODULE_VERSION.to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        artifact_identity: Some(artifact_identity),
        wasm_hash,
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        limits: ModuleLimits {
            max_mem_bytes: M4_BOOTSTRAP_WASM_MAX_MEM_BYTES,
            max_gas: M4_BOOTSTRAP_WASM_MAX_GAS,
            max_call_rate: descriptor.max_call_rate,
            max_output_bytes: 4096,
            max_effects: 0,
            max_emits: 2,
        },
    }
}
