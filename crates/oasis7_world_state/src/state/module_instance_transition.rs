//! Sparse, owned instance lifecycle state prepared before any canonical write.
use super::*;
use serde::ser::SerializeMap;

#[derive(Debug)]
pub struct PreparedModuleInstance {
    event: DomainEvent,
    pub agents: BTreeMap<String, AgentCell>,
    pub resources: BTreeMap<ResourceKind, i64>,
    instance: ModuleInstanceState,
    instance_key: String,
    target: ModuleInstallTarget,
    next_instance_id: u64,
    world_materials: BTreeMap<String, i64>,
    admission_freeze: Option<(String, ModuleAdmissionFreeze)>,
}

impl PreparedModuleInstance {
    pub fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub fn routed_agents(&self) -> BTreeMap<String, AgentCell> {
        let mut agents = self.agents.clone();
        if let Some(agent_id) = self.event.agent_id()
            && let Some(cell) = agents.get_mut(agent_id)
        {
            cell.mailbox.push_back(self.event.clone());
        }
        agents
    }

    pub fn install_infallible(self, state: &mut WorldState) {
        state.agents.extend(self.agents);
        state.resources.extend(self.resources);
        state
            .installed_module_targets
            .insert(self.instance.module_id.clone(), self.target);
        state
            .module_instances
            .insert(self.instance_key, self.instance);
        state.next_module_instance_id = self.next_instance_id;
        if let Some((key, marker)) = self.admission_freeze {
            state.module_admission_freezes.entry(key).or_insert(marker);
        }
        state.materials = self.world_materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.world_materials);
    }

    pub fn serialize_fields<S: serde::ser::SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        output.serialize_field(
            "module_instances",
            &SingleEntryProjection {
                base: &state.module_instances,
                key: &self.instance_key,
                value: &self.instance,
            },
        )?;
        if let Some((key, marker)) = &self.admission_freeze {
            output.serialize_field(
                "module_admission_freezes",
                &SingleEntryProjection {
                    base: &state.module_admission_freezes,
                    key,
                    value: marker,
                },
            )?;
        } else if !state.module_admission_freezes.is_empty() {
            output.serialize_field("module_admission_freezes", &state.module_admission_freezes)?;
        }
        Ok(())
    }

    pub fn has_admission_freeze(&self) -> bool {
        self.admission_freeze.is_some()
    }

    pub fn serialize_target_fields<S: serde::ser::SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        output.serialize_field(
            "installed_module_targets",
            &SingleEntryProjection {
                base: &state.installed_module_targets,
                key: &self.instance.module_id,
                value: &self.target,
            },
        )?;
        output.serialize_field("next_module_instance_id", &self.next_instance_id)
    }

    pub fn serialize_material_fields<S: serde::ser::SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        output.serialize_field("materials", &self.world_materials)?;
        output.serialize_field(
            "material_ledgers",
            &SingleEntryProjection {
                base: &state.material_ledgers,
                key: &MaterialLedgerId::world(),
                value: &self.world_materials,
            },
        )
    }
}

/// Ordered map replacement/insertion borrowing all unaffected entries.
struct SingleEntryProjection<'a, K, V> {
    base: &'a BTreeMap<K, V>,
    key: &'a K,
    value: &'a V,
}

impl<K: Ord + Serialize, V: Serialize> Serialize for SingleEntryProjection<'_, K, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(
            self.base.len() + usize::from(!self.base.contains_key(self.key)),
        ))?;
        let mut inserted = false;
        for (key, value) in self.base {
            if !inserted && key >= self.key {
                map.serialize_entry(self.key, self.value)?;
                inserted = true;
            }
            if key != self.key {
                map.serialize_entry(key, value)?;
            }
        }
        if !inserted {
            map.serialize_entry(self.key, self.value)?;
        }
        map.end()
    }
}

impl WorldState {
    fn prepare_module_action_fee(
        &self,
        agent_id: &str,
        fee_kind: ResourceKind,
        fee_amount: i64,
        now: WorldTime,
    ) -> Result<(AgentCell, BTreeMap<ResourceKind, i64>), WorldError> {
        if fee_amount < 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!("module action fee must be >= 0, got {}", fee_amount),
            });
        }
        let mut cell = self
            .agents
            .get(agent_id)
            .ok_or_else(|| WorldError::AgentNotFound {
                agent_id: agent_id.to_string(),
            })?
            .clone();
        let mut resources = BTreeMap::new();
        let after = super::module_marketplace_transition::settle_module_action_fee_from_records_v1(
            &mut cell.state.resources,
            &mut cell.last_active,
            self.resources.get(&fee_kind).copied().unwrap_or(0),
            agent_id,
            fee_kind,
            fee_amount,
            now,
        )?;
        if fee_amount > 0 {
            resources.insert(fee_kind, after);
        }
        Ok((cell, resources))
    }

    pub fn prepare_module_instance_event(
        &self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<PreparedModuleInstance, WorldError> {
        self.prepare_module_instance_event_with_registry(event, now, None)
    }

    pub fn prepare_module_instance_event_with_registry(
        &self,
        event: &DomainEvent,
        now: WorldTime,
        registry: Option<&oasis7_wasm_abi::ModuleRegistry>,
    ) -> Result<PreparedModuleInstance, WorldError> {
        let (agent_id, fee_kind, fee_amount) = match event {
            DomainEvent::ModuleInstalled {
                installer_agent_id,
                fee_kind,
                fee_amount,
                ..
            } => (installer_agent_id, *fee_kind, *fee_amount),
            DomainEvent::ModuleUpgraded {
                upgrader_agent_id,
                fee_kind,
                fee_amount,
                ..
            } => (upgrader_agent_id, *fee_kind, *fee_amount),
            DomainEvent::ModuleRollbackApplied {
                operator_agent_id,
                fee_kind,
                fee_amount,
                ..
            } => (operator_agent_id, *fee_kind, *fee_amount),
            _ => unreachable!("module instance preparation requires a lifecycle event"),
        };
        let (cell, resources) =
            self.prepare_module_action_fee(agent_id, fee_kind, fee_amount, now)?;
        let (instance, target, next_instance_id) = match event {
            DomainEvent::ModuleInstalled {
                instance_id,
                module_id,
                install_target,
                module_version,
                wasm_hash,
                active,
                ..
            } => prepare_installed_module_record_v1(
                instance_id,
                module_id,
                module_version,
                wasm_hash,
                agent_id,
                install_target,
                *active,
                now,
                self.next_module_instance_id,
            ),
            DomainEvent::ModuleUpgraded {
                instance_id,
                module_id,
                from_module_version,
                to_module_version,
                wasm_hash,
                install_target,
                active,
                ..
            }
            | DomainEvent::ModuleRollbackApplied {
                instance_id,
                module_id,
                from_module_version,
                to_module_version,
                wasm_hash,
                install_target,
                active,
                ..
            } => {
                let operation = if matches!(event, DomainEvent::ModuleRollbackApplied { .. }) {
                    ModuleInstanceReplacementKindV1::Rollback
                } else {
                    ModuleInstanceReplacementKindV1::Upgrade
                };
                let instance = prepare_replaced_module_record_v1(
                    self.module_instances.get(instance_id),
                    instance_id,
                    module_id,
                    from_module_version,
                    to_module_version,
                    wasm_hash,
                    install_target,
                    *active,
                    agent_id,
                    operation,
                )?;
                (
                    instance,
                    install_target.clone(),
                    self.next_module_instance_id,
                )
            }
            _ => unreachable!(),
        };
        let world_materials = self
            .material_ledgers
            .get(&MaterialLedgerId::world())
            .filter(|ledger| !ledger.is_empty())
            .unwrap_or(&self.materials)
            .clone();
        let instance_key = match event {
            DomainEvent::ModuleUpgraded { instance_id, .. }
            | DomainEvent::ModuleRollbackApplied { instance_id, .. } => instance_id.clone(),
            _ => instance.instance_id.clone(),
        };
        let admission_freeze = match event {
            DomainEvent::ModuleRollbackApplied {
                module_id,
                from_module_version,
                from_wasm_hash: Some(hash),
                from_release_request_id,
                proposal_id,
                ..
            } => {
                let previous = &self.module_instances[&instance_key];
                let source_hash = if previous.wasm_hash.is_empty() {
                    registry
                        .and_then(|registry| {
                            registry
                                .records
                                .get(&oasis7_wasm_abi::ModuleRegistry::record_key(
                                    &previous.module_id,
                                    &previous.module_version,
                                ))
                        })
                        .map(|record| record.manifest.wasm_hash.as_str())
                } else {
                    Some(previous.wasm_hash.as_str())
                };
                if hash.is_empty() || source_hash != Some(hash.as_str()) {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: "rollback FROM artifact identity mismatch".into(),
                    });
                }
                let key = ModuleAdmissionFreeze::key(module_id, from_module_version, hash);
                let marker = self
                    .module_admission_freezes
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| ModuleAdmissionFreeze {
                        module_id: module_id.clone(),
                        module_version: from_module_version.clone(),
                        wasm_hash: hash.clone(),
                        rollback_proposal_id: *proposal_id,
                        source_release_request_id: *from_release_request_id,
                        reason: "rollback_stop_new_admission".into(),
                    });
                Some((key, marker))
            }
            _ => None,
        };
        Ok(PreparedModuleInstance {
            event: event.clone(),
            agents: BTreeMap::from([(agent_id.clone(), cell)]),
            resources,
            instance,
            instance_key,
            target,
            next_instance_id,
            world_materials,
            admission_freeze,
        })
    }
}

/// Original installed-row construction shared by Legacy and the selected
/// Current writer. This returns DATA, not installation or state permission.
#[allow(clippy::too_many_arguments)]
pub fn prepare_installed_module_record_v1(
    instance_id: &str,
    module_id: &str,
    module_version: &str,
    wasm_hash: &str,
    owner: &str,
    target: &ModuleInstallTarget,
    active: bool,
    now: WorldTime,
    next_instance_id: u64,
) -> (ModuleInstanceState, ModuleInstallTarget, u64) {
    let resolved = if instance_id.trim().is_empty() {
        module_id.to_owned()
    } else {
        instance_id.trim().to_owned()
    };
    (
        ModuleInstanceState {
            instance_id: resolved,
            module_id: module_id.to_owned(),
            module_version: module_version.to_owned(),
            wasm_hash: wasm_hash.to_owned(),
            owner_agent_id: owner.to_owned(),
            install_target: target.clone(),
            active,
            installed_at: now,
        },
        target.clone(),
        next_instance_id.saturating_add(1),
    )
}

#[derive(Clone, Copy)]
pub enum ModuleInstanceReplacementKindV1 {
    Upgrade,
    Rollback,
}
/// The original ordered row checks and data replacement. The containing
/// selected Action, source provenance, fee, freeze and publication stay outer.
#[allow(clippy::too_many_arguments)]
pub fn prepare_replaced_module_record_v1(
    before: Option<&ModuleInstanceState>,
    instance_id: &str,
    module_id: &str,
    from_version: &str,
    to_version: &str,
    wasm_hash: &str,
    target: &ModuleInstallTarget,
    active: bool,
    actor: &str,
    kind: ModuleInstanceReplacementKindV1,
) -> Result<ModuleInstanceState, WorldError> {
    let (operation, actor_label) = match kind {
        ModuleInstanceReplacementKindV1::Upgrade => ("upgrade", "upgrader"),
        ModuleInstanceReplacementKindV1::Rollback => ("rollback", "operator"),
    };
    let mut instance = before
        .ok_or_else(|| WorldError::ResourceBalanceInvalid {
            reason: format!("module instance missing for {operation} {instance_id}"),
        })?
        .clone();
    if instance.owner_agent_id != actor {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!(
                "module instance owner mismatch for {operation}: instance={} owner={} {actor_label}={}",
                instance_id, instance.owner_agent_id, actor
            ),
        });
    }
    if instance.module_id != module_id {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!(
                "module instance module_id mismatch for {operation}: instance={} state_module_id={} event_module_id={}",
                instance_id, instance.module_id, module_id
            ),
        });
    }
    if instance.module_version != from_version {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!(
                "module instance from_version mismatch for {operation}: instance={} state_version={} event_from={}",
                instance_id, instance.module_version, from_version
            ),
        });
    }
    instance.module_version = to_version.to_owned();
    instance.wasm_hash = wasm_hash.to_owned();
    instance.install_target = target.clone();
    instance.active = active;
    Ok(instance)
}
