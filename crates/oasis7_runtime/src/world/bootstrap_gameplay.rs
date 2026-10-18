use super::super::{
    GameplayContract, GameplayModuleKind, M1ScenarioBootstrapConfig, M5_GAMEPLAY_CRISIS_MODULE_ID,
    M5_GAMEPLAY_ECONOMIC_MODULE_ID, M5_GAMEPLAY_GOVERNANCE_MODULE_ID, M5_GAMEPLAY_META_MODULE_ID,
    M5_GAMEPLAY_MODULE_VERSION, M5_GAMEPLAY_WAR_MODULE_ID, Manifest, ModuleAbiContract,
    ModuleActivation, ModuleArtifactIdentity, ModuleChangeSet, ModuleKind, ModuleLimits,
    ModuleManifest, ModuleRegistry, ModuleRole, ModuleSubscription, ModuleSubscriptionStage,
    ProposalDecision, WorldError, util,
};
use super::World;

const M5_BOOTSTRAP_WASM_MAX_MEM_BYTES: u64 = 64 * 1024 * 1024;
const M5_BOOTSTRAP_WASM_MAX_GAS: u64 = 2_000_000;

impl World {
    pub fn install_m5_gameplay_bootstrap_modules(
        &mut self,
        actor: impl Into<String>,
    ) -> Result<(), WorldError> {
        let actor = actor.into();
        let mut changes = ModuleChangeSet::default();

        ensure_bootstrap_module(
            self,
            &mut changes,
            M5_GAMEPLAY_WAR_MODULE_ID,
            &self.load_builtin_wasm_artifact_for_module("m5", M5_GAMEPLAY_WAR_MODULE_ID)?,
            M5_GAMEPLAY_MODULE_VERSION,
            "m5",
            m5_gameplay_war_manifest,
        )?;
        ensure_bootstrap_module(
            self,
            &mut changes,
            M5_GAMEPLAY_GOVERNANCE_MODULE_ID,
            &self.load_builtin_wasm_artifact_for_module("m5", M5_GAMEPLAY_GOVERNANCE_MODULE_ID)?,
            M5_GAMEPLAY_MODULE_VERSION,
            "m5",
            m5_gameplay_governance_manifest,
        )?;
        ensure_bootstrap_module(
            self,
            &mut changes,
            M5_GAMEPLAY_CRISIS_MODULE_ID,
            &self.load_builtin_wasm_artifact_for_module("m5", M5_GAMEPLAY_CRISIS_MODULE_ID)?,
            M5_GAMEPLAY_MODULE_VERSION,
            "m5",
            m5_gameplay_crisis_manifest,
        )?;
        ensure_bootstrap_module(
            self,
            &mut changes,
            M5_GAMEPLAY_ECONOMIC_MODULE_ID,
            &self.load_builtin_wasm_artifact_for_module("m5", M5_GAMEPLAY_ECONOMIC_MODULE_ID)?,
            M5_GAMEPLAY_MODULE_VERSION,
            "m5",
            m5_gameplay_economic_manifest,
        )?;
        ensure_bootstrap_module(
            self,
            &mut changes,
            M5_GAMEPLAY_META_MODULE_ID,
            &self.load_builtin_wasm_artifact_for_module("m5", M5_GAMEPLAY_META_MODULE_ID)?,
            M5_GAMEPLAY_MODULE_VERSION,
            "m5",
            m5_gameplay_meta_manifest,
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

    pub fn install_gameplay_scenario_bootstrap_modules(
        &mut self,
        actor: impl Into<String>,
        config: M1ScenarioBootstrapConfig,
    ) -> Result<(), WorldError> {
        let actor = actor.into();
        self.install_m1_scenario_bootstrap_modules(actor.clone(), config)?;
        self.install_m4_economy_bootstrap_modules(actor.clone())?;
        self.install_m5_gameplay_bootstrap_modules(actor)?;
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

fn m5_gameplay_manifest(
    module_id: &str,
    name: &str,
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
    kind: GameplayModuleKind,
    min_players: u16,
    max_players: Option<u16>,
) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.to_string(),
        name: name.to_string(),
        version: M5_GAMEPLAY_MODULE_VERSION.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Gameplay,
        artifact_identity: Some(artifact_identity),
        wasm_hash,
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract {
            abi_version: Some(1),
            input_schema: Some("gameplay.input@1".to_string()),
            output_schema: Some("gameplay.output@1".to_string()),
            cap_slots: std::collections::BTreeMap::new(),
            policy_hooks: Vec::new(),
            gameplay: Some(GameplayContract {
                kind,
                game_modes: vec!["sandbox".to_string(), "ranked".to_string()],
                min_players,
                max_players,
            }),
            declarations: Default::default(),
        },
        exports: vec!["reduce".to_string()],
        subscriptions: vec![
            ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::Tick),
                filters: None,
            },
            ModuleSubscription {
                event_kinds: vec!["domain.*".to_string()],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            },
        ],
        required_caps: Vec::new(),
        limits: ModuleLimits {
            max_mem_bytes: M5_BOOTSTRAP_WASM_MAX_MEM_BYTES,
            max_gas: M5_BOOTSTRAP_WASM_MAX_GAS,
            max_call_rate: 64,
            max_output_bytes: 4096,
            max_effects: 0,
            max_emits: 1,
        },
    }
}

fn m5_gameplay_war_manifest(
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    m5_gameplay_manifest(
        M5_GAMEPLAY_WAR_MODULE_ID,
        "M5GameplayWarCore",
        wasm_hash,
        artifact_identity,
        GameplayModuleKind::War,
        2,
        None,
    )
}

fn m5_gameplay_governance_manifest(
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    m5_gameplay_manifest(
        M5_GAMEPLAY_GOVERNANCE_MODULE_ID,
        "M5GameplayGovernanceCouncil",
        wasm_hash,
        artifact_identity,
        GameplayModuleKind::Governance,
        2,
        None,
    )
}

fn m5_gameplay_crisis_manifest(
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    m5_gameplay_manifest(
        M5_GAMEPLAY_CRISIS_MODULE_ID,
        "M5GameplayCrisisCycle",
        wasm_hash,
        artifact_identity,
        GameplayModuleKind::Crisis,
        1,
        None,
    )
}

fn m5_gameplay_economic_manifest(
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    m5_gameplay_manifest(
        M5_GAMEPLAY_ECONOMIC_MODULE_ID,
        "M5GameplayEconomicOverlay",
        wasm_hash,
        artifact_identity,
        GameplayModuleKind::Economic,
        1,
        None,
    )
}

fn m5_gameplay_meta_manifest(
    wasm_hash: String,
    artifact_identity: ModuleArtifactIdentity,
) -> ModuleManifest {
    m5_gameplay_manifest(
        M5_GAMEPLAY_META_MODULE_ID,
        "M5GameplayMetaProgression",
        wasm_hash,
        artifact_identity,
        GameplayModuleKind::Meta,
        1,
        None,
    )
}
