//! Module types and registry for WASM runtime integration.

pub use oasis7_wasm_abi::{
    EconomyModuleKind, FactoryBuildDecision, FactoryBuildRequest, FactoryModuleApi,
    FactoryModuleSpec, FactoryProfileV1, GameplayContract, GameplayModuleKind,
    MaterialDefaultPriority, MaterialProfileV1, MaterialStack, MaterialTransportLossClass,
    ModuleAbiContract, ModuleActivation, ModuleArtifact, ModuleArtifactIdentity, ModuleCache,
    ModuleChangeSet, ModuleCommandCatalogEntry, ModuleDeactivation, ModuleEvent, ModuleEventKind,
    ModuleKind, ModuleLimits, ModuleManifest, ModuleRecord, ModuleRegistry, ModuleRole,
    ModuleSubscription, ModuleSubscriptionStage, ModuleUpgrade, ProductModuleApi,
    ProductModuleSpec, ProductProfileV1, ProductValidationDecision, ProductValidationRequest,
    RecipeExecutionPlan, RecipeExecutionRequest, RecipeModuleApi, RecipeModuleSpec,
    RecipeProfileV1,
};

/// A deterministic, read-only projection of commands exposed by active modules.
///
/// This is an Agent-facing discovery surface only. It describes which versioned
/// module commands are available; it does not grant authority to invoke them or
/// alter the simulator's closed `ActionCatalogEntry` surface.
/// Project active module declarations into a deterministic Agent-facing catalog.
///
/// The active map is authoritative: records without an active pointer are
/// excluded, and stale pointers to missing or mismatched records are ignored.
/// Entries are totally sorted before duplicate command identities are removed.
/// If malformed registry data contains the same command identity with
/// conflicting schema metadata, the lexicographically first entry wins, making
/// the projection deterministic while preserving a single identity.
pub fn module_command_catalog(registry: &ModuleRegistry) -> Vec<ModuleCommandCatalogEntry> {
    let mut entries = Vec::new();

    for (module_id, module_version) in &registry.active {
        let record_key = ModuleRegistry::record_key(module_id, module_version);
        let Some(record) = registry.records.get(&record_key) else {
            continue;
        };
        if record.manifest.module_id != *module_id || record.manifest.version != *module_version {
            continue;
        }
        if oasis7_wasm_abi::validate_module_command_declarations(
            &record.manifest.abi_contract.declarations,
        )
        .is_err()
        {
            continue;
        }

        entries.extend(
            record
                .manifest
                .abi_contract
                .declarations
                .commands
                .iter()
                .map(|declaration| ModuleCommandCatalogEntry {
                    module_id: module_id.clone(),
                    module_version: module_version.clone(),
                    namespace: declaration.namespace.clone(),
                    name: declaration.name.clone(),
                    schema_version: declaration.schema_version,
                    schema_hash: declaration.schema_hash.clone(),
                    max_payload_bytes: declaration.max_payload_bytes,
                }),
        );
    }

    entries.sort_unstable();
    entries.dedup_by(|left, right| {
        left.module_id == right.module_id
            && left.module_version == right.module_version
            && left.namespace == right.namespace
            && left.name == right.name
            && left.schema_version == right.schema_version
    });
    entries
}
