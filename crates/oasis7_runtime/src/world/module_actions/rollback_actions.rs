use super::*;

impl World {
    pub(super) fn apply_rollback_module_instance_action(
        &mut self,
        action_id: u64,
        operator_agent_id: &str,
        instance_id: &str,
        target_module_version: &str,
        finality_certificate: Option<&GovernanceFinalityCertificate>,
    ) -> Result<bool, WorldError> {
        if !self.state.agents.contains_key(operator_agent_id) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: operator_agent_id.to_string(),
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let target_module_version = target_module_version.trim();
        if target_module_version.is_empty() {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "rollback module rejected: target_module_version is empty".to_string(),
                        ],
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
                            "rollback module rejected: instance not found {}",
                            instance_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        if instance.owner_agent_id != operator_agent_id {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "rollback module rejected: operator {} does not own instance {} (owner {})",
                            operator_agent_id, instance_id, instance.owner_agent_id
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if instance.module_version == target_module_version {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "rollback module rejected: target version equals current version {}",
                            target_module_version
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
                            "rollback module rejected: current module record missing {}",
                            current_key
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        let target_key = oasis7_wasm_abi::ModuleRegistry::record_key(
            instance.module_id.as_str(),
            target_module_version,
        );
        let Some(target_record) = self.module_registry.records.get(target_key.as_str()) else {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "rollback module rejected: target version not found {}",
                            target_key
                        )],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        };
        if let Err(reason) = Self::validate_upgrade_interface_compatible(
            &current_record.manifest,
            &target_record.manifest,
        ) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("rollback module rejected: {reason}")],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        let from_wasm_hash = current_record.manifest.wasm_hash.clone();
        let target_manifest = target_record.manifest.clone();
        if self.reject_frozen_module_admission(action_id, &target_manifest)? {
            return Ok(true);
        }
        let mut source_requests = self
            .state
            .module_release_requests
            .values()
            .filter(|request| {
                request.status == ModuleReleaseRequestStatus::Applied
                    && request.manifest.module_id == instance.module_id
                    && request.manifest.version == instance.module_version
                    && request.manifest.wasm_hash == from_wasm_hash
            });
        let source_request = source_requests.next().map(|request| request.request_id);
        let from_release_request_id = if source_requests.next().is_none() {
            source_request
        } else {
            None
        };
        let fee_kind = ResourceKind::Electricity;
        let fee_amount = Self::module_install_fee_amount(&target_manifest);
        if !self.ensure_module_fee_affordable(action_id, operator_agent_id, fee_kind, fee_amount)? {
            return Ok(true);
        }

        let mut changes = ModuleChangeSet::default();
        if instance.active {
            changes.activate.push(ModuleActivation {
                module_id: target_manifest.module_id.clone(),
                version: target_manifest.version.clone(),
            });
        }

        let module_changes_value = match serde_json::to_value(&changes) {
            Ok(value) => value,
            Err(err) => {
                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("serialize module rollback changes failed: {err}")],
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
            match self.propose_manifest_update(manifest_update, operator_agent_id.to_string()) {
                Ok(proposal_id) => proposal_id,
                Err(err) => {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!("propose module rollback rejected: {err:?}")],
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
                        notes: vec![format!("shadow module rollback rejected: {err:?}")],
                    },
                }),
                Some(CausedBy::Action(action_id)),
            )?;
            return Ok(true);
        }
        if let Err(err) = self.approve_proposal(
            proposal_id,
            operator_agent_id.to_string(),
            ProposalDecision::Approve,
        ) {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("approve module rollback rejected: {err:?}")],
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
                                notes: vec![format!("apply module rollback rejected: {err:?}")],
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
            DomainEvent::ModuleRollbackApplied {
                operator_agent_id: operator_agent_id.to_string(),
                instance_id: instance.instance_id.clone(),
                module_id: instance.module_id.clone(),
                from_module_version: instance.module_version,
                to_module_version: target_module_version.to_string(),
                wasm_hash: target_manifest.wasm_hash,
                from_wasm_hash: Some(from_wasm_hash),
                from_release_request_id,
                install_target: instance.install_target,
                active: instance.active,
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
