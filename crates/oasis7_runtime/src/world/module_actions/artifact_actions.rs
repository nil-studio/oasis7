use super::*;

impl World {
    pub(super) fn module_deploy_fee_amount(bytes_len: usize) -> i64 {
        let bytes_len = bytes_len as i64;
        (bytes_len.saturating_add(MODULE_DEPLOY_FEE_BYTES_PER_ELECTRICITY - 1)
            / MODULE_DEPLOY_FEE_BYTES_PER_ELECTRICITY)
            .max(1)
    }

    pub(super) fn module_compile_fee_amount(source_bytes_len: usize, wasm_bytes_len: usize) -> i64 {
        let total_bytes = source_bytes_len.saturating_add(wasm_bytes_len) as i64;
        (total_bytes.saturating_add(MODULE_COMPILE_FEE_BYTES_PER_ELECTRICITY - 1)
            / MODULE_COMPILE_FEE_BYTES_PER_ELECTRICITY)
            .max(2)
    }

    pub(super) fn module_install_fee_amount(manifest: &oasis7_wasm_abi::ModuleManifest) -> i64 {
        let export_cost = manifest.exports.len() as i64;
        let subscription_cost = manifest.subscriptions.len() as i64;
        1_i64
            .saturating_add(export_cost)
            .saturating_add(subscription_cost)
            .max(1)
    }

    pub(super) fn next_module_instance_id(&self, module_id: &str) -> String {
        let seq = self.state.next_module_instance_id.max(1);
        format!("{module_id}#{seq}")
    }

    pub(super) fn validate_upgrade_interface_compatible(
        current: &oasis7_wasm_abi::ModuleManifest,
        next: &oasis7_wasm_abi::ModuleManifest,
    ) -> Result<(), String> {
        if current.interface_version != next.interface_version {
            return Err(format!(
                "upgrade interface_version mismatch: from={} to={}",
                current.interface_version, next.interface_version
            ));
        }

        let missing_exports: Vec<String> = current
            .exports
            .iter()
            .filter(|export_name| !next.exports.contains(*export_name))
            .cloned()
            .collect();
        if !missing_exports.is_empty() {
            return Err(format!(
                "upgrade exports incompatible: missing {:?}",
                missing_exports
            ));
        }

        for subscription in &current.subscriptions {
            if !next.subscriptions.contains(subscription) {
                return Err(
                    "upgrade subscriptions incompatible: existing subscription removed or modified"
                        .to_string(),
                );
            }
        }

        if current.abi_contract.abi_version != next.abi_contract.abi_version {
            return Err("upgrade abi_version mismatch".to_string());
        }
        if current.abi_contract.input_schema != next.abi_contract.input_schema
            || current.abi_contract.output_schema != next.abi_contract.output_schema
        {
            return Err("upgrade abi input/output schema mismatch".to_string());
        }
        for (slot, cap_ref) in &current.abi_contract.cap_slots {
            match next.abi_contract.cap_slots.get(slot) {
                Some(next_cap_ref) if next_cap_ref == cap_ref => {}
                _ => {
                    return Err(format!("upgrade abi cap slot mismatch for slot {}", slot));
                }
            }
        }
        for hook in &current.abi_contract.policy_hooks {
            if !next.abi_contract.policy_hooks.contains(hook) {
                return Err(format!(
                    "upgrade abi policy_hooks incompatible: missing {}",
                    hook
                ));
            }
        }
        for required_cap in &current.required_caps {
            if !next.required_caps.contains(required_cap) {
                return Err(format!(
                    "upgrade required_caps incompatible: missing {}",
                    required_cap
                ));
            }
        }
        Ok(())
    }

    pub(super) fn ensure_module_fee_affordable(
        &mut self,
        action_id: u64,
        agent_id: &str,
        fee_kind: ResourceKind,
        fee_amount: i64,
    ) -> Result<bool, WorldError> {
        if fee_amount <= 0 {
            return Ok(true);
        }
        let available = self
            .state
            .agents
            .get(agent_id)
            .map(|cell| cell.state.resources.get(fee_kind))
            .unwrap_or(0);
        if available < fee_amount {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::InsufficientResource {
                        agent_id: agent_id.to_string(),
                        kind: fee_kind,
                        requested: fee_amount,
                        available,
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(false);
        }
        Ok(true)
    }

    pub(super) fn has_active_module_using_artifact(&self, wasm_hash: &str) -> bool {
        if self
            .state
            .module_instances
            .values()
            .any(|instance| instance.active && instance.wasm_hash == wasm_hash)
        {
            return true;
        }
        self.module_registry
            .active
            .iter()
            .any(|(module_id, version)| {
                if self
                    .state
                    .module_instances
                    .values()
                    .any(|instance| instance.module_id == *module_id)
                {
                    return false;
                }
                let key = oasis7_wasm_abi::ModuleRegistry::record_key(module_id, version);
                self.module_registry
                    .records
                    .get(&key)
                    .map(|record| record.manifest.wasm_hash == wasm_hash)
                    .unwrap_or(false)
            })
    }

    pub(super) fn peek_next_module_market_order_id(&self) -> u64 {
        self.state.next_module_market_order_id.max(1)
    }

    pub(super) fn peek_next_module_market_sale_id(&self) -> u64 {
        self.state.next_module_market_sale_id.max(1)
    }

    pub(super) fn peek_next_module_release_request_id(&self) -> u64 {
        self.state.next_module_release_request_id.max(1)
    }

    pub(super) fn prepare_module_governance_proposal(
        &mut self,
        proposal_id: ProposalId,
        finality_certificate: Option<&GovernanceFinalityCertificate>,
    ) -> Result<super::super::governance_publication::PreparedGovernanceProposalApply, WorldError>
    {
        let prepared = match finality_certificate {
            Some(certificate) => self.prepare_proposal_with_finality(proposal_id, certificate)?,
            None => {
                if !self.release_security_policy.allow_local_finality_signing {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "apply_proposal local finality path is disabled by release policy proposal_id={proposal_id}"
                        ),
                    });
                }
                let certificate = self.build_local_finality_certificate(proposal_id)?;
                self.prepare_proposal_with_finality(proposal_id, &certificate)?
            }
        };
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }
        Ok(prepared)
    }

    pub(super) fn apply_install_module_action(
        &mut self,
        action_id: u64,
        installer_agent_id: &str,
        manifest: &oasis7_wasm_abi::ModuleManifest,
        activate: bool,
        install_target: ModuleInstallTarget,
        finality_certificate: Option<&GovernanceFinalityCertificate>,
    ) -> Result<bool, WorldError> {
        self.apply_install_module_action_with_release(
            action_id,
            installer_agent_id,
            manifest,
            activate,
            install_target,
            finality_certificate,
            None,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Install action fields include the stable module release/finality transaction context."
    )]
    pub(super) fn apply_install_module_action_with_release(
        &mut self,
        action_id: u64,
        installer_agent_id: &str,
        manifest: &oasis7_wasm_abi::ModuleManifest,
        activate: bool,
        install_target: ModuleInstallTarget,
        finality_certificate: Option<&GovernanceFinalityCertificate>,
        completion: Option<super::super::module_release_publication::ModuleReleaseCompletion>,
    ) -> Result<bool, WorldError> {
        if self.reject_frozen_module_admission(action_id, manifest)? {
            return Ok(true);
        }
        if !self.state.agents.contains_key(installer_agent_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: installer_agent_id.to_string(),
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if let Some(owner_agent_id) = self.state.module_artifact_owners.get(&manifest.wasm_hash)
            && owner_agent_id != installer_agent_id
        {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "install module artifact rejected: installer {} does not own {} (owner {})",
                            installer_agent_id, manifest.wasm_hash, owner_agent_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let fee_kind = ResourceKind::Electricity;
        let fee_amount = Self::module_install_fee_amount(manifest);
        if !self.ensure_module_fee_affordable(
            action_id,
            installer_agent_id,
            fee_kind,
            fee_amount,
        )? {
            return Ok(true);
        }

        let mut changes = ModuleChangeSet::default();
        let record_key = oasis7_wasm_abi::ModuleRegistry::record_key(
            manifest.module_id.as_str(),
            manifest.version.as_str(),
        );
        if let Some(record) = self.module_registry.records.get(record_key.as_str()) {
            if record.manifest != *manifest {
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "install module rejected: existing manifest mismatch for {}",
                                record_key
                            )],
                        },
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                return Ok(true);
            }
        } else {
            changes.register.push(manifest.clone());
        }
        if activate {
            let already_active_same = self
                .module_registry
                .active
                .get(&manifest.module_id)
                .map(|version| version == &manifest.version)
                .unwrap_or(false);
            if !already_active_same {
                changes.activate.push(ModuleActivation {
                    module_id: manifest.module_id.clone(),
                    version: manifest.version.clone(),
                });
            }
        }

        let has_profile_changes = completion
            .as_ref()
            .is_some_and(|completion| !completion.profile_changes.is_empty());
        let (proposal_id, manifest_hash, governance) = if changes.is_empty() && !has_profile_changes
        {
            (0, self.current_manifest_hash()?, None)
        } else {
            let module_changes_value = match serde_json::to_value(&changes) {
                Ok(value) => value,
                Err(err) => {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!("serialize module changes failed: {err}")],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
            };

            let mut manifest_update = self.manifest.clone();
            manifest_update.version = manifest_update.version.saturating_add(1);
            let serde_json::Value::Object(content) = &mut manifest_update.content else {
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["current manifest content must be object".to_string()],
                        },
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                return Ok(true);
            };
            content.insert("module_changes".to_string(), module_changes_value);
            if let Some(completion) = completion.as_ref()
                && let Err(reason) = Self::add_module_release_profile_commitment(
                    &mut manifest_update,
                    manifest,
                    activate,
                    &completion.profile_changes,
                )
            {
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "module release profile commitment rejected: {reason}"
                            )],
                        },
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                return Ok(true);
            }

            let proposal_id = match self
                .propose_manifest_update(manifest_update, installer_agent_id.to_string())
            {
                Ok(proposal_id) => proposal_id,
                Err(err) => {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!("propose module install rejected: {err:?}")],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
            };

            if let Err(err) = self.shadow_proposal(proposal_id) {
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("shadow module install rejected: {err:?}")],
                        },
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                return Ok(true);
            }

            if let Err(err) = self.approve_proposal(
                proposal_id,
                installer_agent_id.to_string(),
                ProposalDecision::Approve,
            ) {
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("approve module install rejected: {err:?}")],
                        },
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                return Ok(true);
            }

            let prepared =
                match self.prepare_module_governance_proposal(proposal_id, finality_certificate) {
                    Ok(prepared) => prepared,
                    Err(err) => {
                        self.append_event(
                            WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!("apply module install rejected: {err:?}")],
                                },
                            }),
                            Some(CausedBy::Action(action_id)),
                        )?;
                        return Ok(true);
                    }
                };
            (
                proposal_id,
                prepared.applied_hash().to_string(),
                Some(prepared),
            )
        };

        let instance_id = self.next_module_instance_id(manifest.module_id.as_str());

        let event = DomainEvent::ModuleInstalled {
            installer_agent_id: installer_agent_id.to_string(),
            instance_id,
            module_id: manifest.module_id.clone(),
            module_version: manifest.version.clone(),
            wasm_hash: manifest.wasm_hash.clone(),
            install_target,
            active: activate,
            proposal_id,
            manifest_hash,
            fee_kind,
            fee_amount,
        };
        let caused_by = Some(CausedBy::Action(action_id));
        if let Some(prepared) = governance {
            prepared.publish_lifecycle_tail_with_release(self, event, caused_by, completion)?;
        } else if let Some(completion) = completion {
            self.append_module_install_with_release(event, caused_by, completion)?;
        } else {
            self.append_event(WorldEventBody::Domain(event), caused_by)?;
        }
        Ok(true)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Upgrade action fields include the stable module release/finality transaction context."
    )]
    pub(super) fn apply_upgrade_module_action(
        &mut self,
        action_id: u64,
        upgrader_agent_id: &str,
        instance_id: &str,
        from_module_version: &str,
        manifest: &oasis7_wasm_abi::ModuleManifest,
        activate: bool,
        finality_certificate: Option<&GovernanceFinalityCertificate>,
    ) -> Result<bool, WorldError> {
        if self.reject_frozen_module_admission(action_id, manifest)? {
            return Ok(true);
        }
        if !self.state.agents.contains_key(upgrader_agent_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: upgrader_agent_id.to_string(),
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }

        let Some(instance) = self.state.module_instances.get(instance_id).cloned() else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "upgrade module rejected: instance not found {}",
                            instance_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        if instance.owner_agent_id != upgrader_agent_id {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "upgrade module rejected: upgrader {} does not own instance {} (owner {})",
                            upgrader_agent_id, instance_id, instance.owner_agent_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if instance.module_version != from_module_version {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "upgrade module rejected: from_version mismatch for instance {} expected {} got {}",
                            instance_id, instance.module_version, from_module_version
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if manifest.module_id != instance.module_id {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "upgrade module rejected: manifest module_id mismatch for instance {} expected {} got {}",
                            instance_id, instance.module_id, manifest.module_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if manifest.version == instance.module_version {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "upgrade module rejected: target version equals current version {}",
                            manifest.version
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if let Some(owner_agent_id) = self.state.module_artifact_owners.get(&manifest.wasm_hash)
            && owner_agent_id != upgrader_agent_id
        {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "upgrade module artifact rejected: upgrader {} does not own {} (owner {})",
                            upgrader_agent_id, manifest.wasm_hash, owner_agent_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }

        let current_key = oasis7_wasm_abi::ModuleRegistry::record_key(
            instance.module_id.as_str(),
            instance.module_version.as_str(),
        );
        let Some(current_record) = self.module_registry.records.get(current_key.as_str()) else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "upgrade module rejected: current module record missing {}",
                            current_key
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        if let Err(reason) =
            Self::validate_upgrade_interface_compatible(&current_record.manifest, manifest)
        {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("upgrade module rejected: {reason}")],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }

        let fee_kind = ResourceKind::Electricity;
        let fee_amount = Self::module_install_fee_amount(manifest);
        if !self.ensure_module_fee_affordable(action_id, upgrader_agent_id, fee_kind, fee_amount)? {
            return Ok(true);
        }

        let mut changes = ModuleChangeSet {
            upgrade: vec![ModuleUpgrade {
                module_id: instance.module_id.clone(),
                from_version: instance.module_version.clone(),
                to_version: manifest.version.clone(),
                wasm_hash: manifest.wasm_hash.clone(),
                manifest: manifest.clone(),
            }],
            ..ModuleChangeSet::default()
        };
        if activate {
            changes.activate.push(ModuleActivation {
                module_id: manifest.module_id.clone(),
                version: manifest.version.clone(),
            });
        }

        let module_changes_value = match serde_json::to_value(&changes) {
            Ok(value) => value,
            Err(err) => {
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("serialize module changes failed: {err}")],
                        },
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                return Ok(true);
            }
        };

        let mut manifest_update = self.manifest.clone();
        manifest_update.version = manifest_update.version.saturating_add(1);
        let serde_json::Value::Object(content) = &mut manifest_update.content else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["current manifest content must be object".to_string()],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        content.insert("module_changes".to_string(), module_changes_value);

        let proposal_id =
            match self.propose_manifest_update(manifest_update, upgrader_agent_id.to_string()) {
                Ok(proposal_id) => proposal_id,
                Err(err) => {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!("propose module upgrade rejected: {err:?}")],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
            };

        if let Err(err) = self.shadow_proposal(proposal_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("shadow module upgrade rejected: {err:?}")],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }

        if let Err(err) = self.approve_proposal(
            proposal_id,
            upgrader_agent_id.to_string(),
            ProposalDecision::Approve,
        ) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("approve module upgrade rejected: {err:?}")],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }

        let prepared =
            match self.prepare_module_governance_proposal(proposal_id, finality_certificate) {
                Ok(prepared) => prepared,
                Err(err) => {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!("apply module upgrade rejected: {err:?}")],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
            };

        let manifest_hash = prepared.applied_hash().to_string();
        prepared.publish_lifecycle_tail(
            self,
            DomainEvent::ModuleUpgraded {
                upgrader_agent_id: upgrader_agent_id.to_string(),
                instance_id: instance.instance_id,
                module_id: instance.module_id,
                from_module_version: from_module_version.to_string(),
                to_module_version: manifest.version.clone(),
                wasm_hash: manifest.wasm_hash.clone(),
                install_target: instance.install_target,
                active: activate,
                proposal_id,
                manifest_hash,
                fee_kind,
                fee_amount,
            },
            Some(CausedBy::Action(action_id)),
        )?;
        Ok(true)
    }
}
