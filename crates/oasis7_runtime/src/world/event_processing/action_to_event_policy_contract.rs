use super::action_to_event_policy_contract_rejection::economic_contract_rule_denied;
use super::*;
use crate::EconomicContractFulfillmentKind;
use std::collections::BTreeMap;

use crate::{GovernanceValidatorAdmissionRecord, GovernanceValidatorAdmissionStatus};

impl World {
    pub(super) fn action_to_event_policy_contract(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        match action {
            Action::UpdateGameplayPolicy {
                operator_agent_id,
                electricity_tax_bps,
                data_tax_bps,
                power_trade_fee_bps,
                max_open_contracts_per_agent,
                blocked_agents,
                forbidden_location_ids,
            } => {
                if !self.state.agents.contains_key(operator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: operator_agent_id.clone(),
                        },
                    }));
                }
                if !self.has_policy_update_governance_authorization(operator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "update gameplay policy requires passed governance proposal total_weight >= {}",
                                GAMEPLAY_POLICY_UPDATE_MIN_GOVERNANCE_TOTAL_WEIGHT
                            )],
                        },
                    }));
                }
                if *electricity_tax_bps > GAMEPLAY_POLICY_MAX_TAX_BPS {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "electricity_tax_bps must be <= {}",
                                GAMEPLAY_POLICY_MAX_TAX_BPS
                            )],
                        },
                    }));
                }
                if *data_tax_bps > GAMEPLAY_POLICY_MAX_TAX_BPS {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "data_tax_bps must be <= {}",
                                GAMEPLAY_POLICY_MAX_TAX_BPS
                            )],
                        },
                    }));
                }
                if *power_trade_fee_bps > GAMEPLAY_POLICY_MAX_TAX_BPS {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "power_trade_fee_bps must be <= {}",
                                GAMEPLAY_POLICY_MAX_TAX_BPS
                            )],
                        },
                    }));
                }
                if *max_open_contracts_per_agent < GAMEPLAY_POLICY_MIN_CONTRACT_QUOTA
                    || *max_open_contracts_per_agent > GAMEPLAY_POLICY_MAX_CONTRACT_QUOTA
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "max_open_contracts_per_agent must be within {}..={}",
                                GAMEPLAY_POLICY_MIN_CONTRACT_QUOTA,
                                GAMEPLAY_POLICY_MAX_CONTRACT_QUOTA
                            )],
                        },
                    }));
                }
                let mut normalized_blocked_agents = BTreeSet::new();
                for value in blocked_agents {
                    let candidate = value.trim();
                    if candidate.is_empty() {
                        continue;
                    }
                    if !self.state.agents.contains_key(candidate) {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: candidate.to_string(),
                            },
                        }));
                    }
                    normalized_blocked_agents.insert(candidate.to_string());
                }
                let mut normalized_forbidden_location_ids = BTreeSet::new();
                for value in forbidden_location_ids {
                    let candidate = value.trim();
                    if candidate.is_empty() {
                        continue;
                    }
                    normalized_forbidden_location_ids.insert(candidate.to_string());
                }
                Ok(WorldEventBody::Domain(DomainEvent::GameplayPolicyUpdated {
                    operator_agent_id: operator_agent_id.clone(),
                    electricity_tax_bps: *electricity_tax_bps,
                    data_tax_bps: *data_tax_bps,
                    power_trade_fee_bps: *power_trade_fee_bps,
                    max_open_contracts_per_agent: *max_open_contracts_per_agent,
                    blocked_agents: normalized_blocked_agents.into_iter().collect(),
                    forbidden_location_ids: normalized_forbidden_location_ids.into_iter().collect(),
                }))
            }
            Action::UpdateRestrictedStarterClaimAdminRegistry {
                controller_account_id,
                next_admin_account_ids,
            } => {
                let Some(current_registry) = self.governance_main_token_controller_registry()
                else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "update restricted claim admin registry rejected: main token controller registry is not configured"
                                    .to_string(),
                            ],
                        },
                    }));
                };
                let controller_account_id = controller_account_id.trim();
                let expected_controller_account_id =
                    match Self::restricted_starter_claim_admin_registry_controller_account_id(
                        current_registry,
                    ) {
                        Ok(account_id) => account_id,
                        Err(err) => {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "update restricted claim admin registry rejected: {err:?}"
                                    )],
                                },
                            }));
                        }
                    };
                if controller_account_id != expected_controller_account_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "update restricted claim admin registry rejected: controller_account_id does not match ecosystem treasury controller slot expected={} actual={}",
                                expected_controller_account_id, controller_account_id
                            )],
                        },
                    }));
                }
                let next_admin_account_ids = next_admin_account_ids
                    .iter()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .collect::<BTreeSet<String>>();
                let mut next_registry = current_registry.clone();
                next_registry.restricted_starter_claim_admin_account_ids =
                    next_admin_account_ids.clone();
                if let Err(err) =
                    Self::validate_governance_main_token_controller_registry(next_registry)
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "update restricted claim admin registry rejected: {err:?}"
                            )],
                        },
                    }));
                }
                Ok(WorldEventBody::Governance(
                    GovernanceEvent::RestrictedStarterClaimAdminRegistryUpdated {
                        controller_account_id: controller_account_id.to_string(),
                        previous_admin_account_ids: current_registry
                            .restricted_starter_claim_admin_account_ids
                            .iter()
                            .cloned()
                            .collect(),
                        next_admin_account_ids: next_admin_account_ids.into_iter().collect(),
                    },
                ))
            }
            Action::SubmitGovernanceValidatorAdmission {
                controller_account_id,
                candidate_id,
                node_id,
                finality_signer_public_key,
                stake,
                operator_owner,
                public_manifest_hash,
            } => {
                let Some(current_registry) = self.governance_main_token_controller_registry()
                else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "submit validator admission rejected: main token controller registry is not configured"
                                    .to_string(),
                            ],
                        },
                    }));
                };
                let controller_account_id = controller_account_id.trim();
                let expected_controller_account_id =
                    match Self::validator_admission_controller_account_id(current_registry) {
                        Ok(account_id) => account_id,
                        Err(err) => {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "submit validator admission rejected: {err:?}"
                                    )],
                                },
                            }));
                        }
                    };
                if controller_account_id != expected_controller_account_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "submit validator admission rejected: controller_account_id does not match validator admission controller expected={} actual={}",
                                expected_controller_account_id, controller_account_id
                            )],
                        },
                    }));
                }
                let requested_at_epoch = self.current_governance_epoch();
                let record = match self.validate_governance_validator_admission_record(
                    GovernanceValidatorAdmissionRecord {
                        candidate_id: candidate_id.clone(),
                        node_id: node_id.clone(),
                        finality_signer_public_key: finality_signer_public_key.clone(),
                        stake: *stake,
                        operator_owner: operator_owner.clone(),
                        public_manifest_hash: public_manifest_hash.clone(),
                        requested_at_epoch,
                        last_transition_tick: self.state.time,
                        ..GovernanceValidatorAdmissionRecord::default()
                    },
                ) {
                    Ok(record) => record,
                    Err(err) => {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "submit validator admission rejected: {err:?}"
                                )],
                            },
                        }));
                    }
                };
                if self
                    .state
                    .governance_validator_admissions
                    .contains_key(record.candidate_id.as_str())
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "submit validator admission rejected: candidate already exists: {}",
                                record.candidate_id
                            )],
                        },
                    }));
                }
                if self
                    .state
                    .governance_validator_admissions
                    .values()
                    .any(|existing| {
                        existing.node_id == record.node_id
                            && existing.status != GovernanceValidatorAdmissionStatus::Revoked
                    })
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "submit validator admission rejected: node_id already has an active admission record: {}",
                                record.node_id
                            )],
                        },
                    }));
                }
                match self.resolve_governance_effective_finality_signer_registry() {
                    Ok(Some(registry))
                        if registry
                            .signer_bindings
                            .contains_key(record.node_id.as_str()) =>
                    {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "submit validator admission rejected: node_id is already active in finality registry: {}",
                                    record.node_id
                                )],
                            },
                        }));
                    }
                    Ok(_) => {}
                    Err(err) => {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "submit validator admission rejected: {err:?}"
                                )],
                            },
                        }));
                    }
                }
                if let Some(existing_public_key) =
                    self.node_identity_public_key(record.node_id.as_str())
                    && existing_public_key != record.finality_signer_public_key
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "submit validator admission rejected: node identity binding mismatch node_id={} expected={} actual={}",
                                record.node_id,
                                existing_public_key,
                                record.finality_signer_public_key
                            )],
                        },
                    }));
                }
                if self
                    .state
                    .governance_validator_admissions
                    .values()
                    .any(|existing| {
                        existing.finality_signer_public_key == record.finality_signer_public_key
                            && existing.node_id != record.node_id
                            && existing.status != GovernanceValidatorAdmissionStatus::Revoked
                    })
                    || match self.resolve_governance_effective_finality_signer_registry() {
                        Ok(Some(registry)) => {
                            registry
                                .signer_bindings
                                .iter()
                                .any(|(existing_node_id, public_key)| {
                                    public_key == &record.finality_signer_public_key
                                        && existing_node_id != &record.node_id
                                })
                        }
                        Ok(None) => false,
                        Err(err) => {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "submit validator admission rejected: {err:?}"
                                    )],
                                },
                            }));
                        }
                    }
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "submit validator admission rejected: finality signer public key is already assigned to another node"
                            )],
                        },
                    }));
                }
                Ok(WorldEventBody::Governance(
                    GovernanceEvent::ValidatorAdmissionSubmitted {
                        controller_account_id: controller_account_id.to_string(),
                        candidate_id: record.candidate_id,
                        node_id: record.node_id,
                        finality_signer_public_key: record.finality_signer_public_key,
                        stake: record.stake,
                        operator_owner: record.operator_owner,
                        public_manifest_hash: record.public_manifest_hash,
                        requested_at_epoch,
                    },
                ))
            }
            Action::ApproveGovernanceValidatorAdmission {
                controller_account_id,
                candidate_id,
            } => {
                let Some(current_registry) = self.governance_main_token_controller_registry()
                else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "approve validator admission rejected: main token controller registry is not configured"
                                    .to_string(),
                            ],
                        },
                    }));
                };
                let controller_account_id = controller_account_id.trim();
                let expected_controller_account_id =
                    match Self::validator_admission_controller_account_id(current_registry) {
                        Ok(account_id) => account_id,
                        Err(err) => {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "approve validator admission rejected: {err:?}"
                                    )],
                                },
                            }));
                        }
                    };
                if controller_account_id != expected_controller_account_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "approve validator admission rejected: controller_account_id does not match validator admission controller expected={} actual={}",
                                expected_controller_account_id, controller_account_id
                            )],
                        },
                    }));
                }
                let candidate_id = candidate_id.trim();
                let Some(record) = self.state.governance_validator_admissions.get(candidate_id)
                else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "approve validator admission rejected: candidate not found: {candidate_id}"
                            )],
                        },
                    }));
                };
                if record.status != GovernanceValidatorAdmissionStatus::Applied {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "approve validator admission rejected: candidate is not in applied status: {}",
                                candidate_id
                            )],
                        },
                    }));
                }
                Ok(WorldEventBody::Governance(
                    GovernanceEvent::ValidatorAdmissionApproved {
                        controller_account_id: controller_account_id.to_string(),
                        candidate_id: candidate_id.to_string(),
                        approved_at_epoch: self.current_governance_epoch(),
                    },
                ))
            }
            Action::ActivateGovernanceValidatorAdmission {
                controller_account_id,
                candidate_id,
                activation_epoch,
            } => {
                let Some(current_registry) = self.governance_main_token_controller_registry()
                else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "activate validator admission rejected: main token controller registry is not configured"
                                    .to_string(),
                            ],
                        },
                    }));
                };
                let controller_account_id = controller_account_id.trim();
                let expected_controller_account_id =
                    match Self::validator_admission_controller_account_id(current_registry) {
                        Ok(account_id) => account_id,
                        Err(err) => {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "activate validator admission rejected: {err:?}"
                                    )],
                                },
                            }));
                        }
                    };
                if controller_account_id != expected_controller_account_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "activate validator admission rejected: controller_account_id does not match validator admission controller expected={} actual={}",
                                expected_controller_account_id, controller_account_id
                            )],
                        },
                    }));
                }
                let current_epoch = self.current_governance_epoch();
                let candidate_id = candidate_id.trim();
                let Some(record) = self.state.governance_validator_admissions.get(candidate_id)
                else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "activate validator admission rejected: candidate not found: {candidate_id}"
                            )],
                        },
                    }));
                };
                if !matches!(
                    record.status,
                    GovernanceValidatorAdmissionStatus::ApprovedCandidate
                        | GovernanceValidatorAdmissionStatus::ProbationReady
                ) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "activate validator admission rejected: candidate is not in approvable status: {}",
                                candidate_id
                            )],
                        },
                    }));
                }
                let mut preview_admissions = self.state.governance_validator_admissions.clone();
                let mut preview_record = record.clone();
                preview_record.activation_epoch = Some(*activation_epoch);
                preview_record.status =
                    Self::governance_validator_admission_status_for_activation_epoch(
                        current_epoch,
                        *activation_epoch,
                    );
                preview_record.last_transition_tick = self.state.time;
                preview_admissions.insert(candidate_id.to_string(), preview_record);
                if let Err(err) = self
                    .resolve_governance_effective_finality_signer_registry_from_admissions(
                        &preview_admissions,
                    )
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("activate validator admission rejected: {err:?}")],
                        },
                    }));
                }
                Ok(WorldEventBody::Governance(
                    GovernanceEvent::ValidatorAdmissionActivated {
                        controller_account_id: controller_account_id.to_string(),
                        candidate_id: candidate_id.to_string(),
                        activation_epoch: *activation_epoch,
                    },
                ))
            }
            Action::RevokeGovernanceValidatorAdmission {
                controller_account_id,
                candidate_id,
                node_id,
                reason,
            } => {
                let Some(current_registry) = self.governance_main_token_controller_registry()
                else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "revoke validator admission rejected: main token controller registry is not configured"
                                    .to_string(),
                            ],
                        },
                    }));
                };
                let controller_account_id = controller_account_id.trim();
                let expected_controller_account_id =
                    match Self::validator_admission_controller_account_id(current_registry) {
                        Ok(account_id) => account_id,
                        Err(err) => {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "revoke validator admission rejected: {err:?}"
                                    )],
                                },
                            }));
                        }
                    };
                if controller_account_id != expected_controller_account_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "revoke validator admission rejected: controller_account_id does not match validator admission controller expected={} actual={}",
                                expected_controller_account_id, controller_account_id
                            )],
                        },
                    }));
                }
                let candidate_id = candidate_id.trim();
                let node_id = node_id.trim();
                let reason = reason.trim();
                if candidate_id.is_empty() || reason.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "revoke validator admission rejected: candidate_id and reason cannot be empty"
                                    .to_string(),
                            ],
                        },
                    }));
                }
                let current_epoch = self.current_governance_epoch();
                let resolved_candidate_id = if self
                    .state
                    .governance_validator_admissions
                    .contains_key(candidate_id)
                {
                    candidate_id.to_string()
                } else if let Some(existing_key) =
                    Self::governance_validator_admission_record_key_for_node(
                        &self.state.governance_validator_admissions,
                        node_id,
                    )
                {
                    existing_key
                } else {
                    format!("legacy-revoked:{node_id}")
                };
                let existing = self
                    .state
                    .governance_validator_admissions
                    .get(resolved_candidate_id.as_str())
                    .cloned();
                let target_node_id = if let Some(record) = existing.as_ref() {
                    if !node_id.is_empty() && record.node_id != node_id {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "revoke validator admission rejected: node_id mismatch candidate_id={} expected={} actual={}",
                                    candidate_id, record.node_id, node_id
                                )],
                            },
                        }));
                    }
                    record.node_id.clone()
                } else {
                    node_id.to_string()
                };
                if target_node_id.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "revoke validator admission rejected: node_id cannot be empty when no candidate record exists"
                                    .to_string(),
                            ],
                        },
                    }));
                }
                let current_public_key = self
                    .node_identity_public_key(target_node_id.as_str())
                    .map(str::to_string)
                    .or_else(|| {
                        self.resolve_governance_effective_finality_signer_registry()
                            .ok()
                            .flatten()
                            .and_then(|registry| {
                                registry
                                    .signer_bindings
                                    .get(target_node_id.as_str())
                                    .cloned()
                            })
                    });
                if existing.is_none()
                    && !self
                        .resolve_governance_effective_finality_signer_registry()
                        .ok()
                        .flatten()
                        .is_some_and(|registry| {
                            registry
                                .signer_bindings
                                .contains_key(target_node_id.as_str())
                        })
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "revoke validator admission rejected: candidate or active validator not found for node_id={}",
                                target_node_id
                            )],
                        },
                    }));
                }
                let Some(current_public_key) = current_public_key else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "revoke validator admission rejected: missing node identity binding for node_id={}",
                                target_node_id
                            )],
                        },
                    }));
                };
                let mut preview_admissions: BTreeMap<String, GovernanceValidatorAdmissionRecord> =
                    self.state.governance_validator_admissions.clone();
                let duplicate_keys = Self::governance_validator_admission_keys_for_node(
                    &preview_admissions,
                    target_node_id.as_str(),
                );
                if duplicate_keys.len() > 1 && !duplicate_keys.contains(&resolved_candidate_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "revoke validator admission rejected: multiple candidate records exist for node_id={} candidate_keys={:?}",
                                target_node_id, duplicate_keys
                            )],
                        },
                    }));
                }
                let mut revoked_record = existing.unwrap_or(GovernanceValidatorAdmissionRecord {
                    candidate_id: resolved_candidate_id.clone(),
                    node_id: target_node_id.clone(),
                    finality_signer_public_key: current_public_key.clone(),
                    stake: 100,
                    operator_owner: "governance.revocation".to_string(),
                    public_manifest_hash: "synthetic-revocation".to_string(),
                    requested_at_epoch: current_epoch,
                    last_transition_tick: self.state.time,
                    approved_at_epoch: Some(current_epoch),
                    activation_epoch: Some(current_epoch),
                    status: GovernanceValidatorAdmissionStatus::Applied,
                    revoked_at_epoch: None,
                    revocation_reason: None,
                });
                revoked_record.candidate_id = resolved_candidate_id.clone();
                revoked_record.status = GovernanceValidatorAdmissionStatus::Revoked;
                revoked_record.last_transition_tick = self.state.time;
                revoked_record.revoked_at_epoch = Some(current_epoch);
                revoked_record.revocation_reason = Some(reason.to_string());
                preview_admissions.insert(resolved_candidate_id.clone(), revoked_record);
                if let Err(err) = self
                    .resolve_governance_effective_finality_signer_registry_from_admissions(
                        &preview_admissions,
                    )
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("revoke validator admission rejected: {err:?}")],
                        },
                    }));
                }
                Ok(WorldEventBody::Governance(
                    GovernanceEvent::ValidatorAdmissionRevoked {
                        controller_account_id: controller_account_id.to_string(),
                        candidate_id: resolved_candidate_id,
                        node_id: target_node_id,
                        revoked_at_epoch: current_epoch,
                        reason: reason.to_string(),
                    },
                ))
            }
            Action::OpenEconomicContract {
                creator_agent_id,
                contract_id,
                counterparty_agent_id,
                fulfillment_kind,
                settlement_kind,
                settlement_amount,
                reputation_stake,
                expires_at,
                description,
            } => {
                if *fulfillment_kind == EconomicContractFulfillmentKind::Service {
                    return Ok(economic_contract_rule_denied(
                        action_id,
                        "service contracts unavailable: collateral/evidence/remedy not implemented",
                    ));
                }
                if !self.state.agents.contains_key(creator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: creator_agent_id.clone(),
                        },
                    }));
                }
                if !self.state.agents.contains_key(counterparty_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: counterparty_agent_id.clone(),
                        },
                    }));
                }
                if creator_agent_id == counterparty_agent_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["economic contract requires distinct parties".to_string()],
                        },
                    }));
                }
                let contract_id = contract_id.trim();
                if contract_id.is_empty() {
                    return Ok(economic_contract_rule_denied(
                        action_id,
                        "contract_id cannot be empty",
                    ));
                }
                if self.state.economic_contracts.contains_key(contract_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("economic contract already exists: {contract_id}")],
                        },
                    }));
                }
                if *settlement_amount <= 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InvalidAmount {
                            amount: *settlement_amount,
                        },
                    }));
                }
                if *reputation_stake <= 0
                    || *reputation_stake > ECONOMIC_CONTRACT_MAX_REPUTATION_STAKE
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "reputation_stake must be within 1..={}",
                                ECONOMIC_CONTRACT_MAX_REPUTATION_STAKE
                            )],
                        },
                    }));
                }
                if *expires_at <= self.state.time {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "expires_at must be greater than current world time".to_string(),
                            ],
                        },
                    }));
                }
                let description = description.trim();
                if description.is_empty() {
                    return Ok(economic_contract_rule_denied(
                        action_id,
                        "economic contract description cannot be empty",
                    ));
                }
                if self
                    .state
                    .gameplay_policy
                    .blocked_agents
                    .iter()
                    .any(|value| value == creator_agent_id || value == counterparty_agent_id)
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["economic contract blocked by gameplay policy".to_string()],
                        },
                    }));
                }
                let active_contract_count = self
                    .state
                    .economic_contracts
                    .values()
                    .filter(|contract| {
                        contract.creator_agent_id == *creator_agent_id
                            && matches!(
                                contract.status,
                                EconomicContractStatus::Open | EconomicContractStatus::Accepted
                            )
                    })
                    .count();
                if active_contract_count
                    >= usize::from(self.state.gameplay_policy.max_open_contracts_per_agent)
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "economic contract quota exceeded for creator {}",
                                creator_agent_id
                            )],
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(
                    DomainEvent::EconomicContractOpened {
                        creator_agent_id: creator_agent_id.clone(),
                        contract_id: contract_id.to_string(),
                        counterparty_agent_id: counterparty_agent_id.clone(),
                        fulfillment_kind: *fulfillment_kind,
                        settlement_kind: *settlement_kind,
                        settlement_amount: *settlement_amount,
                        reputation_stake: *reputation_stake,
                        expires_at: *expires_at,
                        description: description.to_string(),
                    },
                ))
            }
            Action::AcceptEconomicContract {
                accepter_agent_id,
                contract_id,
            } => {
                if !self.state.agents.contains_key(accepter_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: accepter_agent_id.clone(),
                        },
                    }));
                }
                let contract_id = contract_id.trim();
                if contract_id.is_empty() {
                    return Ok(economic_contract_rule_denied(
                        action_id,
                        "contract_id cannot be empty",
                    ));
                }
                let Some(contract) = self.state.economic_contracts.get(contract_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("economic contract not found: {contract_id}")],
                        },
                    }));
                };
                if contract.status != EconomicContractStatus::Open {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("economic contract is not open: {}", contract_id)],
                        },
                    }));
                }
                if contract.counterparty_agent_id != *accepter_agent_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "contract accepter mismatch expected {}",
                                contract.counterparty_agent_id
                            )],
                        },
                    }));
                }
                if self.state.time > contract.expires_at {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "economic contract has expired at {}",
                                contract.expires_at
                            )],
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(
                    DomainEvent::EconomicContractAccepted {
                        accepter_agent_id: accepter_agent_id.clone(),
                        contract_id: contract_id.to_string(),
                    },
                ))
            }
            Action::SettleEconomicContract {
                operator_agent_id,
                contract_id,
                success,
                notes,
            } => {
                if !self.state.agents.contains_key(operator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: operator_agent_id.clone(),
                        },
                    }));
                }
                let contract_id = contract_id.trim();
                if contract_id.is_empty() {
                    return Ok(economic_contract_rule_denied(
                        action_id,
                        "contract_id cannot be empty",
                    ));
                }
                let Some(contract) = self.state.economic_contracts.get(contract_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("economic contract not found: {contract_id}")],
                        },
                    }));
                };
                if contract.status != EconomicContractStatus::Accepted {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "economic contract status is not accepted: {}",
                                contract_id
                            )],
                        },
                    }));
                }
                if contract.fulfillment_kind == EconomicContractFulfillmentKind::Service {
                    return Ok(economic_contract_rule_denied(
                        action_id,
                        "service contracts unavailable: collateral/evidence/remedy not implemented",
                    ));
                }
                if !success {
                    return Ok(economic_contract_rule_denied(
                        action_id,
                        "atomic exchange settlement requires success=true",
                    ));
                }
                if contract.creator_agent_id != *operator_agent_id
                    && contract.counterparty_agent_id != *operator_agent_id
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "settlement operator must belong to contract parties".to_string(),
                            ],
                        },
                    }));
                }
                let notes = notes.trim();
                if notes.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "economic contract settlement notes cannot be empty".to_string(),
                            ],
                        },
                    }));
                }

                let (
                    transfer_amount,
                    tax_amount,
                    creator_reputation_delta,
                    counterparty_reputation_delta,
                ) = if *success {
                    if let Some(ready_at) = self.state.economic_contract_pair_cooldown_ready_at(
                        contract.creator_agent_id.as_str(),
                        contract.counterparty_agent_id.as_str(),
                        ECONOMIC_CONTRACT_PAIR_COOLDOWN_TICKS,
                    ) && self.state.time < ready_at
                    {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "economic contract settlement denied: pair cooldown active until tick {}",
                                    ready_at
                                )],
                            },
                        }));
                    }
                    if contract.settlement_kind == ResourceKind::Data
                        && !self.state.has_data_access_permission(
                            contract.creator_agent_id.as_str(),
                            contract.counterparty_agent_id.as_str(),
                        )
                    {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "economic contract data settlement denied: missing access grant owner={} grantee={}",
                                    contract.creator_agent_id, contract.counterparty_agent_id
                                )],
                            },
                        }));
                    }
                    let tax_bps = match contract.settlement_kind {
                        ResourceKind::Electricity => self
                            .state
                            .gameplay_policy
                            .electricity_tax_bps
                            .saturating_add(self.state.gameplay_policy.power_trade_fee_bps)
                            .min(GAMEPLAY_POLICY_MAX_TAX_BPS),
                        ResourceKind::Data => self.state.gameplay_policy.data_tax_bps,
                    };
                    let tax_amount = contract
                        .settlement_amount
                        .saturating_mul(i64::from(tax_bps))
                        .saturating_div(10_000);
                    let total_required = contract.settlement_amount.saturating_add(tax_amount);
                    let available = self
                        .state
                        .agents
                        .get(&contract.creator_agent_id)
                        .map(|cell| cell.state.resources.get(contract.settlement_kind))
                        .unwrap_or(0);
                    if available < total_required {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::InsufficientResource {
                                agent_id: contract.creator_agent_id.clone(),
                                kind: contract.settlement_kind,
                                requested: total_required,
                                available,
                            },
                        }));
                    }
                    let success_reputation_reward =
                        Self::economic_contract_success_reputation_reward(
                            contract.settlement_amount,
                            contract.reputation_stake,
                        );
                    let creator_reward_budget = self.state.available_reputation_reward_budget(
                        contract.creator_agent_id.as_str(),
                        self.state.time,
                        ECONOMIC_CONTRACT_REPUTATION_WINDOW_TICKS,
                        ECONOMIC_CONTRACT_REPUTATION_WINDOW_CAP,
                    );
                    let counterparty_reward_budget = self.state.available_reputation_reward_budget(
                        contract.counterparty_agent_id.as_str(),
                        self.state.time,
                        ECONOMIC_CONTRACT_REPUTATION_WINDOW_TICKS,
                        ECONOMIC_CONTRACT_REPUTATION_WINDOW_CAP,
                    );
                    let creator_reward = success_reputation_reward.min(creator_reward_budget);
                    let counterparty_reward =
                        success_reputation_reward.min(counterparty_reward_budget);
                    (
                        contract.settlement_amount,
                        tax_amount,
                        creator_reward,
                        counterparty_reward,
                    )
                } else {
                    unreachable!("atomic exchanges require success=true before settlement")
                };

                Ok(WorldEventBody::Domain(
                    DomainEvent::EconomicContractSettled {
                        operator_agent_id: operator_agent_id.clone(),
                        contract_id: contract_id.to_string(),
                        success: *success,
                        transfer_amount,
                        tax_amount,
                        notes: notes.to_string(),
                        creator_reputation_delta,
                        counterparty_reputation_delta,
                    },
                ))
            }
            _ => {
                unreachable!("action_to_event_policy_contract received unsupported action variant")
            }
        }
    }
}
