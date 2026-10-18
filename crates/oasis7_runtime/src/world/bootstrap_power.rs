use super::super::{
    M1_AGENT_DEFAULT_MODULE_VERSION, M1_MEMORY_MAX_ENTRIES, M1_MEMORY_MODULE_ID,
    M1_MOBILITY_MODULE_ID, M1_POWER_MODULE_VERSION, M1_RADIATION_POWER_MODULE_ID,
    M1_SENSOR_MODULE_ID, M1_STORAGE_CARGO_MODULE_ID, M1_STORAGE_POWER_MODULE_ID, Manifest,
    ModuleAbiContract, ModuleActivation, ModuleArtifactIdentity, ModuleChangeSet, ModuleKind,
    ModuleLimits, ModuleManifest, ModuleRegistry, ModuleRole, ModuleSubscription,
    ModuleSubscriptionStage, ProposalDecision, WorldError, util,
};
use super::World;

const M1_BOOTSTRAP_WASM_MAX_MEM_BYTES: u64 = 64 * 1024 * 1024;
const M1_BOOTSTRAP_WASM_MAX_GAS: u64 = 2_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct M1ScenarioBootstrapConfig {
    pub install_default_module_package: bool,
}

impl Default for M1ScenarioBootstrapConfig {
    fn default() -> Self {
        Self {
            install_default_module_package: true,
        }
    }
}

impl World {
    pub fn install_m1_power_bootstrap_modules(
        &mut self,
        actor: impl Into<String>,
    ) -> Result<(), WorldError> {
        let radiation_power_artifact =
            self.load_builtin_wasm_artifact_for_module("m1", M1_RADIATION_POWER_MODULE_ID)?;
        let storage_power_artifact =
            self.load_builtin_wasm_artifact_for_module("m1", M1_STORAGE_POWER_MODULE_ID)?;
        let actor = actor.into();
        let mut changes = ModuleChangeSet::default();

        ensure_bootstrap_module(
            self,
            &mut changes,
            M1_RADIATION_POWER_MODULE_ID,
            &radiation_power_artifact,
            M1_POWER_MODULE_VERSION,
            "m1",
            m1_radiation_power_manifest,
        )?;
        ensure_bootstrap_module(
            self,
            &mut changes,
            M1_STORAGE_POWER_MODULE_ID,
            &storage_power_artifact,
            M1_POWER_MODULE_VERSION,
            "m1",
            m1_storage_power_manifest,
        )?;

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

    pub fn install_m1_scenario_bootstrap_modules(
        &mut self,
        actor: impl Into<String>,
        config: M1ScenarioBootstrapConfig,
    ) -> Result<(), WorldError> {
        let actor = actor.into();
        self.install_m1_power_bootstrap_modules(actor.clone())?;
        if config.install_default_module_package {
            self.install_m1_agent_default_modules(actor)?;
        }
        Ok(())
    }

    pub fn install_m1_agent_default_modules(
        &mut self,
        actor: impl Into<String>,
    ) -> Result<(), WorldError> {
        let sensor_artifact =
            self.load_builtin_wasm_artifact_for_module("m1", M1_SENSOR_MODULE_ID)?;
        let mobility_artifact =
            self.load_builtin_wasm_artifact_for_module("m1", M1_MOBILITY_MODULE_ID)?;
        let memory_artifact =
            self.load_builtin_wasm_artifact_for_module("m1", M1_MEMORY_MODULE_ID)?;
        let storage_cargo_artifact =
            self.load_builtin_wasm_artifact_for_module("m1", M1_STORAGE_CARGO_MODULE_ID)?;
        let actor = actor.into();
        let mut changes = ModuleChangeSet::default();

        ensure_bootstrap_module(
            self,
            &mut changes,
            M1_SENSOR_MODULE_ID,
            &sensor_artifact,
            M1_AGENT_DEFAULT_MODULE_VERSION,
            "m1",
            m1_sensor_manifest,
        )?;
        ensure_bootstrap_module(
            self,
            &mut changes,
            M1_MOBILITY_MODULE_ID,
            &mobility_artifact,
            M1_AGENT_DEFAULT_MODULE_VERSION,
            "m1",
            m1_mobility_manifest,
        )?;
        ensure_bootstrap_module(
            self,
            &mut changes,
            M1_MEMORY_MODULE_ID,
            &memory_artifact,
            M1_AGENT_DEFAULT_MODULE_VERSION,
            "m1",
            m1_memory_manifest,
        )?;
        ensure_bootstrap_module(
            self,
            &mut changes,
            M1_STORAGE_CARGO_MODULE_ID,
            &storage_cargo_artifact,
            M1_AGENT_DEFAULT_MODULE_VERSION,
            "m1",
            m1_storage_cargo_manifest,
        )?;

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
}

fn ensure_bootstrap_module(
    world: &mut World,
    changes: &mut ModuleChangeSet,
    module_id: &str,
    artifact: &[u8],
    version: &str,
    module_set: &str,
    make_manifest: fn(String, ModuleArtifactIdentity) -> ModuleManifest,
) -> Result<(), WorldError> {
    if world
        .module_registry
        .active
        .get(module_id)
        .is_some_and(|active_version| active_version == version)
    {
        return Ok(());
    }

    let record_key = ModuleRegistry::record_key(module_id, version);
    if !world.module_registry.records.contains_key(&record_key) {
        let wasm_hash = util::sha256_hex(artifact);
        world.register_module_artifact(wasm_hash.clone(), artifact)?;
        let identity =
            world.resolve_builtin_module_artifact_identity(module_set, module_id, &wasm_hash)?;
        changes.register.push(make_manifest(wasm_hash, identity));
    }

    changes.activate.push(ModuleActivation {
        module_id: module_id.to_string(),
        version: version.to_string(),
    });

    Ok(())
}

fn m1_radiation_power_manifest(
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    ModuleManifest {
        module_id: M1_RADIATION_POWER_MODULE_ID.to_string(),
        name: "M1RadiationPower".to_string(),
        version: M1_POWER_MODULE_VERSION.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![
            ModuleSubscription {
                event_kinds: vec![
                    "domain.agent_registered".to_string(),
                    "domain.agent_moved".to_string(),
                ],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            },
            ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.*".to_string()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            },
        ],
        required_caps: Vec::new(),
        artifact_identity: Some(artifact_identity),
        limits: ModuleLimits {
            max_mem_bytes: M1_BOOTSTRAP_WASM_MAX_MEM_BYTES,
            max_gas: M1_BOOTSTRAP_WASM_MAX_GAS,
            max_call_rate: 32,
            max_output_bytes: 4096,
            max_effects: 0,
            max_emits: 1,
        },
    }
}

fn m1_storage_power_manifest(
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    ModuleManifest {
        module_id: M1_STORAGE_POWER_MODULE_ID.to_string(),
        name: "M1StoragePower".to_string(),
        version: M1_POWER_MODULE_VERSION.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Body,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![
            ModuleSubscription {
                event_kinds: vec![
                    "domain.agent_registered".to_string(),
                    "domain.agent_moved".to_string(),
                ],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            },
            ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.move_agent".to_string()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            },
        ],
        required_caps: Vec::new(),
        artifact_identity: Some(artifact_identity),
        limits: ModuleLimits {
            max_mem_bytes: M1_BOOTSTRAP_WASM_MAX_MEM_BYTES,
            max_gas: M1_BOOTSTRAP_WASM_MAX_GAS,
            max_call_rate: 64,
            max_output_bytes: 4096,
            max_effects: 0,
            max_emits: 1,
        },
    }
}

fn m1_sensor_manifest(
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    ModuleManifest {
        module_id: M1_SENSOR_MODULE_ID.to_string(),
        name: "M1SensorBasic".to_string(),
        version: M1_AGENT_DEFAULT_MODULE_VERSION.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Body,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![
            ModuleSubscription {
                event_kinds: vec![
                    "domain.agent_registered".to_string(),
                    "domain.agent_moved".to_string(),
                ],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            },
            ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.query_observation".to_string()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            },
        ],
        required_caps: Vec::new(),
        artifact_identity: Some(artifact_identity),
        limits: ModuleLimits {
            max_mem_bytes: M1_BOOTSTRAP_WASM_MAX_MEM_BYTES,
            max_gas: M1_BOOTSTRAP_WASM_MAX_GAS,
            max_call_rate: 32,
            max_output_bytes: 4096,
            max_effects: 0,
            max_emits: 1,
        },
    }
}

fn m1_mobility_manifest(
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    ModuleManifest {
        module_id: M1_MOBILITY_MODULE_ID.to_string(),
        name: "M1MobilityBasic".to_string(),
        version: M1_AGENT_DEFAULT_MODULE_VERSION.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Body,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![
            ModuleSubscription {
                event_kinds: vec![
                    "domain.agent_registered".to_string(),
                    "domain.agent_moved".to_string(),
                ],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            },
            ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.move_agent".to_string()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            },
        ],
        required_caps: Vec::new(),
        artifact_identity: Some(artifact_identity),
        limits: ModuleLimits {
            max_mem_bytes: M1_BOOTSTRAP_WASM_MAX_MEM_BYTES,
            max_gas: M1_BOOTSTRAP_WASM_MAX_GAS,
            max_call_rate: 32,
            max_output_bytes: 4096,
            max_effects: 0,
            max_emits: 1,
        },
    }
}

fn m1_memory_manifest(
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    ModuleManifest {
        module_id: M1_MEMORY_MODULE_ID.to_string(),
        name: "M1MemoryCore".to_string(),
        version: M1_AGENT_DEFAULT_MODULE_VERSION.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::AgentInternal,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.*".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(artifact_identity),
        limits: ModuleLimits {
            max_mem_bytes: M1_BOOTSTRAP_WASM_MAX_MEM_BYTES,
            max_gas: M1_BOOTSTRAP_WASM_MAX_GAS,
            max_call_rate: 64,
            max_output_bytes: 8 * 1024,
            max_effects: 0,
            max_emits: 0,
        },
    }
}

fn m1_storage_cargo_manifest(
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    let max_output = (M1_MEMORY_MAX_ENTRIES as u64).saturating_mul(64).max(4096);
    ModuleManifest {
        module_id: M1_STORAGE_CARGO_MODULE_ID.to_string(),
        name: "M1StorageCargo".to_string(),
        version: M1_AGENT_DEFAULT_MODULE_VERSION.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Body,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec![
                "domain.body_interface_expanded".to_string(),
                "domain.body_interface_expand_rejected".to_string(),
            ],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(artifact_identity),
        limits: ModuleLimits {
            max_mem_bytes: M1_BOOTSTRAP_WASM_MAX_MEM_BYTES,
            max_gas: M1_BOOTSTRAP_WASM_MAX_GAS,
            max_call_rate: 64,
            max_output_bytes: max_output,
            max_effects: 0,
            max_emits: 0,
        },
    }
}
