use std::collections::BTreeSet;

use super::super::{
    ActiveGameplayModule, GAMEPLAY_BASELINE_KINDS, GameplayContract, GameplayKindCoverage,
    GameplayModeReadiness, GameplayModuleKind, ModuleManifest, ModuleRegistry, ModuleRole,
    WorldError,
};
use super::World;

impl World {
    pub fn gameplay_modules(&self) -> Vec<ActiveGameplayModule> {
        let mut out = Vec::new();
        for (module_id, version) in &self.module_registry.active {
            let key = ModuleRegistry::record_key(module_id, version);
            let Some(record) = self.module_registry.records.get(&key) else {
                continue;
            };
            if record.manifest.role != ModuleRole::Gameplay {
                continue;
            }
            let Some(gameplay) = &record.manifest.abi_contract.gameplay else {
                continue;
            };
            let mut game_modes = gameplay.game_modes.clone();
            game_modes.sort();
            out.push(ActiveGameplayModule {
                module_id: module_id.clone(),
                version: version.clone(),
                kind: gameplay.kind,
                game_modes,
            });
        }
        out.sort_by(|left, right| left.module_id.cmp(&right.module_id));
        out
    }

    pub fn gameplay_mode_readiness(&self, mode: impl Into<String>) -> GameplayModeReadiness {
        let mode = mode.into();
        let mode = mode.trim().to_string();
        let mut active_modules = self
            .gameplay_modules()
            .into_iter()
            .filter(|module| module.game_modes.iter().any(|entry| entry == &mode))
            .collect::<Vec<_>>();
        active_modules.sort_by(|left, right| left.module_id.cmp(&right.module_id));

        let mut coverage = Vec::new();
        let mut missing_kinds = Vec::new();
        for kind in GAMEPLAY_BASELINE_KINDS {
            let active_count = active_modules
                .iter()
                .filter(|module| module.kind == kind)
                .count() as u32;
            if active_count == 0 {
                missing_kinds.push(kind);
            }
            coverage.push(GameplayKindCoverage { kind, active_count });
        }

        GameplayModeReadiness {
            mode,
            active_modules,
            coverage,
            missing_kinds,
        }
    }

    pub(super) fn validate_gameplay_contract_for_manifest(
        &self,
        module: &ModuleManifest,
    ) -> Result<(), WorldError> {
        Self::validate_gameplay_contract_for_manifest_data_v1(module)
    }

    pub(super) fn validate_gameplay_contract_for_manifest_data_v1(
        module:&ModuleManifest,
    )->Result<(),WorldError>{
        match (&module.role, &module.abi_contract.gameplay) {
            (ModuleRole::Gameplay, Some(gameplay)) => {
                Self::validate_gameplay_contract_data_v1(module, gameplay)
            }
            (ModuleRole::Gameplay, None) => Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module gameplay contract missing for gameplay role {}",
                    module.module_id
                ),
            }),
            (_, Some(_)) => Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module gameplay contract requires gameplay role for {}",
                    module.module_id
                ),
            }),
            _ => Ok(()),
        }
    }

    fn validate_gameplay_contract_data_v1(
        module: &ModuleManifest,
        gameplay: &GameplayContract,
    ) -> Result<(), WorldError> {
        if gameplay.game_modes.is_empty() {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module gameplay contract requires at least one game mode for {}",
                    module.module_id
                ),
            });
        }

        let mut seen_modes = BTreeSet::new();
        for mode in &gameplay.game_modes {
            let normalized = mode.trim();
            if normalized.is_empty() {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "module gameplay contract includes empty game mode for {}",
                        module.module_id
                    ),
                });
            }
            if !seen_modes.insert(normalized.to_string()) {
                return Err(WorldError::ModuleChangeInvalid {
                    reason: format!(
                        "module gameplay contract has duplicate game mode '{}' for {}",
                        normalized, module.module_id
                    ),
                });
            }
        }

        if gameplay.min_players == 0 {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module gameplay contract min_players must be >= 1 for {}",
                    module.module_id
                ),
            });
        }

        if let Some(max_players) = gameplay.max_players
            && max_players < gameplay.min_players
        {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!(
                    "module gameplay contract max_players < min_players for {}",
                    module.module_id
                ),
            });
        }

        Ok(())
    }

    pub(super) fn validate_gameplay_activation_conflicts(
        &self,
        changes: &super::super::ModuleChangeSet,
    ) -> Result<(), WorldError> {
        crate::world::validate_gameplay_activation_conflicts_from_legacy_v1(self, changes)
    }

}
