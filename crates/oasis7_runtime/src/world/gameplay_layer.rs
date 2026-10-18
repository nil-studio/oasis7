use std::collections::{BTreeMap, BTreeSet};

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
        match (&module.role, &module.abi_contract.gameplay) {
            (ModuleRole::Gameplay, Some(gameplay)) => {
                self.validate_gameplay_contract(module, gameplay)
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

    fn validate_gameplay_contract(
        &self,
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
        let mut post_change_active = self.module_registry.active.clone();
        for deactivation in &changes.deactivate {
            post_change_active.remove(&deactivation.module_id);
        }
        for upgrade in &changes.upgrade {
            if post_change_active
                .get(&upgrade.module_id)
                .is_some_and(|version| version == &upgrade.from_version)
            {
                post_change_active.insert(upgrade.module_id.clone(), upgrade.to_version.clone());
            }
        }
        for activation in &changes.activate {
            post_change_active.insert(activation.module_id.clone(), activation.version.clone());
        }

        let mut claimed_slots: BTreeMap<(String, GameplayModuleKind), String> = BTreeMap::new();
        for (module_id, version) in &post_change_active {
            let manifest =
                self.resolve_module_manifest_for_version(changes, module_id.as_str(), version)?;
            if manifest.role != ModuleRole::Gameplay {
                continue;
            }
            let Some(gameplay) = &manifest.abi_contract.gameplay else {
                continue;
            };
            for mode in &gameplay.game_modes {
                let slot = (mode.clone(), gameplay.kind);
                match claimed_slots.get(&slot) {
                    Some(existing_module_id) if existing_module_id != module_id => {
                        return Err(WorldError::ModuleChangeInvalid {
                            reason: format!(
                                "gameplay slot conflict mode='{}' kind='{}': {} vs {}",
                                slot.0,
                                slot.1.as_str(),
                                existing_module_id,
                                module_id
                            ),
                        });
                    }
                    Some(_) => {}
                    None => {
                        claimed_slots.insert(slot, module_id.clone());
                    }
                }
            }
        }

        Ok(())
    }

    fn resolve_module_manifest_for_version<'a>(
        &'a self,
        changes: &'a super::super::ModuleChangeSet,
        module_id: &str,
        version: &str,
    ) -> Result<&'a ModuleManifest, WorldError> {
        if let Some(manifest) = changes
            .register
            .iter()
            .find(|manifest| manifest.module_id == module_id && manifest.version == version)
        {
            return Ok(manifest);
        }
        if let Some(manifest) = changes.upgrade.iter().find_map(|upgrade| {
            if upgrade.module_id == module_id && upgrade.to_version == version {
                Some(&upgrade.manifest)
            } else {
                None
            }
        }) {
            return Ok(manifest);
        }
        let record_key = ModuleRegistry::record_key(module_id, version);
        self.module_registry
            .records
            .get(&record_key)
            .map(|record| &record.manifest)
            .ok_or_else(|| WorldError::ModuleChangeInvalid {
                reason: format!("module record missing for activation {record_key}"),
            })
    }
}
