#[path = "../incremental_normal_action_contract_evaluate.rs"]
pub(in crate::world) mod fixed_contract;
#[path = "action_to_event_gameplay_policy.rs"]
mod gameplay_policy;
use super::super::governance_authorization::governance_record;
use super::action_to_event_policy_contract_rejection::economic_contract_rule_denied;
use super::*;
use crate::EconomicContractFulfillmentKind;
use std::collections::BTreeMap;

use crate::{GovernanceValidatorAdmissionRecord, GovernanceValidatorAdmissionStatus};

impl World {
    pub(in crate::world) fn action_to_event_policy_contract(
        &self,
        action_id: ActionId,
        action: &Action,
        authorization: Option<&oasis7_agent_api::world_action::GovernanceAuthorizationV1>,
    ) -> Result<WorldEventBody, WorldError> {
        match action {
            Action::UpdateGameplayPolicy { .. }
            | Action::UpdateRestrictedStarterClaimAdminRegistry { .. } => {
                self.action_to_event_policy_admin(action_id, action)
            }
            Action::SubmitGovernanceValidatorAdmission { .. }
            | Action::ApproveGovernanceValidatorAdmission { .. }
            | Action::ActivateGovernanceValidatorAdmission { .. }
            | Action::RevokeGovernanceValidatorAdmission { .. }
            | Action::RotateGovernanceBootstrapFinalityKeyV1 { .. }
            | Action::RotateGovernanceValidatorFinalityKeyV1 { .. } => {
                self.action_to_event_policy_governance_admission(action_id, action, authorization)
            }
            Action::OpenEconomicContract { .. }
            | Action::AcceptEconomicContract { .. }
            | Action::SettleEconomicContract { .. } => {
                self.action_to_event_policy_economic_contract(action_id, action)
            }
            _ => {
                unreachable!("action_to_event_policy_contract received unsupported action variant")
            }
        }
    }

    fn action_to_event_policy_admin(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        match action {
            Action::UpdateGameplayPolicy { .. } => self.gameplay_policy_event(action_id, action),
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
            _ => unreachable!("unsupported variant reached its specialized dispatcher"),
        }
    }

    fn action_to_event_policy_governance_admission(
        &self,
        action_id: ActionId,
        action: &Action,
        authorization: Option<&oasis7_agent_api::world_action::GovernanceAuthorizationV1>,
    ) -> Result<WorldEventBody, WorldError> {
        match action {
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
                        authorization: governance_record(action, authorization),
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
                        authorization: governance_record(action, authorization),
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
                        authorization: governance_record(action, authorization),
                        controller_account_id: controller_account_id.to_string(),
                        candidate_id: candidate_id.to_string(),
                        activation_epoch: *activation_epoch,
                    },
                ))
            }
            Action::RotateGovernanceBootstrapFinalityKeyV1 { .. } => {
                let result = authorization
                    .ok_or_else(|| WorldError::GovernancePolicyInvalid {
                        reason: "rotation requires controller authorization".into(),
                    })
                    .and_then(|proof| self.prepare_bootstrap_finality_key_rotation(action, proof));
                match result {
                    Ok(record) => Ok(WorldEventBody::Governance(
                        GovernanceEvent::BootstrapFinalityKeyRotated {
                            authorization: record.authorized_action,
                        },
                    )),
                    Err(error) => Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("finality rotation rejected: {error:?}")],
                        },
                    })),
                }
            }
            Action::RotateGovernanceValidatorFinalityKeyV1 { .. } => {
                let result = authorization
                    .ok_or_else(|| WorldError::GovernancePolicyInvalid {
                        reason: "rotation requires controller authorization".into(),
                    })
                    .and_then(|proof| self.prepare_finality_key_rotation(action, proof));
                match result {
                    Ok(record) => Ok(WorldEventBody::Governance(
                        GovernanceEvent::ValidatorFinalityKeyRotated {
                            authorization: record.authorized_action,
                        },
                    )),
                    Err(error) => Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("finality rotation rejected: {error:?}")],
                        },
                    })),
                }
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
                        authorization: governance_record(action, authorization),
                        controller_account_id: controller_account_id.to_string(),
                        candidate_id: resolved_candidate_id,
                        node_id: target_node_id,
                        revoked_at_epoch: current_epoch,
                        reason: reason.to_string(),
                    },
                ))
            }
            _ => unreachable!("unsupported variant reached its specialized dispatcher"),
        }
    }

    fn action_to_event_policy_economic_contract(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        fixed_contract::evaluate_contract_action_v1(
            &mut fixed_contract::LegacyContractFactsV1::new(self),
            action_id,
            action,
        )
    }
}
