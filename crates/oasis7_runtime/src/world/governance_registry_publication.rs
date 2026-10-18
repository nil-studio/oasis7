use super::governance::DEFAULT_GOVERNANCE_VALIDATOR_STAKE;
use super::*;
use crate::main_token::main_token_account_id_from_node_public_key;
use crate::{GovernanceEvent, GovernanceValidatorAdmissionStatus};

#[derive(Debug)]
pub(crate) struct PreparedGovernanceRegistryEvent {
    event: GovernanceEvent,
    pub(crate) controller_registry: Option<GovernanceMainTokenControllerRegistry>,
    pub(crate) admissions: BTreeMap<String, GovernanceValidatorAdmissionRecord>,
    pub(crate) identity_bindings: BTreeMap<String, String>,
    pub(crate) account_bindings: BTreeMap<String, String>,
}

impl PreparedGovernanceRegistryEvent {
    pub(super) fn matches_event(&self, event: &GovernanceEvent) -> bool {
        &self.event == event
    }

    pub(super) fn install(self, world: &mut World) {
        world.state.governance_main_token_controller_registry = self.controller_registry;
        world.state.governance_validator_admissions = self.admissions;
        world.state.node_identity_bindings = self.identity_bindings;
        world.state.node_main_token_account_bindings = self.account_bindings;
    }
}

impl World {
    pub(super) fn prepare_governance_registry_event(
        &self,
        event: &GovernanceEvent,
    ) -> Result<PreparedGovernanceRegistryEvent, WorldError> {
        let mut controller_registry = self.state.governance_main_token_controller_registry.clone();
        let mut admissions = self.state.governance_validator_admissions.clone();
        let mut identity_bindings = self.state.node_identity_bindings.clone();
        let mut account_bindings = self.state.node_main_token_account_bindings.clone();
        match event {
            GovernanceEvent::RestrictedStarterClaimAdminRegistryUpdated {
                controller_account_id,
                previous_admin_account_ids,
                next_admin_account_ids,
            } => {
                let Some(current) = controller_registry.clone() else {
                    return Err(WorldError::GovernancePolicyInvalid { reason: "restricted claim admin registry update missing main token controller registry".into() });
                };
                let expected =
                    Self::restricted_starter_claim_admin_registry_controller_account_id(&current)?;
                if expected != controller_account_id {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "restricted claim admin registry controller mismatch expected={expected} actual={controller_account_id}"
                        ),
                    });
                }
                let mut current_admins = current
                    .restricted_starter_claim_admin_account_ids
                    .iter()
                    .map(|v| v.trim())
                    .filter(|v| !v.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                current_admins.sort();
                if current_admins != *previous_admin_account_ids {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "restricted claim admin registry drift before apply: expected_previous={previous_admin_account_ids:?} actual_current={current_admins:?}"
                        ),
                    });
                }
                let mut next = current;
                next.restricted_starter_claim_admin_account_ids = next_admin_account_ids
                    .iter()
                    .map(|v| v.trim())
                    .filter(|v| !v.is_empty())
                    .map(ToString::to_string)
                    .collect();
                controller_registry = Some(
                    Self::validate_governance_main_token_controller_registry(next)?,
                );
            }
            GovernanceEvent::ValidatorAdmissionSubmitted {
                controller_account_id,
                candidate_id,
                node_id,
                finality_signer_public_key,
                stake,
                operator_owner,
                public_manifest_hash,
                requested_at_epoch,
            } => {
                let Some(current) = controller_registry.as_ref() else {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: "validator admission submit missing main token controller registry"
                            .into(),
                    });
                };
                let expected = Self::validator_admission_controller_account_id(current)?;
                if expected != controller_account_id {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "validator admission submit controller mismatch expected={expected} actual={controller_account_id}"
                        ),
                    });
                }
                let record = self.validate_governance_validator_admission_record(
                    GovernanceValidatorAdmissionRecord {
                        candidate_id: candidate_id.clone(),
                        node_id: node_id.clone(),
                        finality_signer_public_key: finality_signer_public_key.clone(),
                        stake: *stake,
                        operator_owner: operator_owner.clone(),
                        public_manifest_hash: public_manifest_hash.clone(),
                        requested_at_epoch: *requested_at_epoch,
                        last_transition_tick: self.state.time,
                        ..Default::default()
                    },
                )?;
                if admissions.contains_key(record.candidate_id.as_str()) {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "validator admission candidate already exists candidate_id={}",
                            record.candidate_id
                        ),
                    });
                }
                if self
                    .resolve_governance_effective_finality_signer_registry()?
                    .is_some_and(|registry| {
                        registry
                            .signer_bindings
                            .contains_key(record.node_id.as_str())
                    })
                {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "validator admission node_id is already active node_id={}",
                            record.node_id
                        ),
                    });
                }
                if let Some(existing) = identity_bindings.get(record.node_id.as_str())
                    && existing != &record.finality_signer_public_key
                {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "validator admission node identity binding mismatch node_id={} expected={} actual={}",
                            record.node_id, existing, record.finality_signer_public_key
                        ),
                    });
                }
                identity_bindings.insert(
                    record.node_id.clone(),
                    record.finality_signer_public_key.clone(),
                );
                account_bindings
                    .entry(record.node_id.clone())
                    .or_insert_with(|| {
                        main_token_account_id_from_node_public_key(
                            &record.finality_signer_public_key,
                        )
                    });
                admissions.insert(record.candidate_id.clone(), record);
            }
            GovernanceEvent::ValidatorAdmissionApproved {
                controller_account_id,
                candidate_id,
                approved_at_epoch,
            } => {
                self.validate_admission_controller(
                    controller_registry.as_ref(),
                    controller_account_id,
                    "approve",
                )?;
                let record = admissions.get_mut(candidate_id).ok_or_else(|| {
                    WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "validator admission candidate not found candidate_id={candidate_id}"
                        ),
                    }
                })?;
                if record.status != GovernanceValidatorAdmissionStatus::Applied {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "validator admission candidate is not in applied status candidate_id={candidate_id} status={:?}",
                            record.status
                        ),
                    });
                }
                record.status = GovernanceValidatorAdmissionStatus::ApprovedCandidate;
                record.approved_at_epoch = Some(*approved_at_epoch);
                record.last_transition_tick = self.state.time;
            }
            GovernanceEvent::ValidatorAdmissionActivated {
                controller_account_id,
                candidate_id,
                activation_epoch,
            } => {
                self.validate_admission_controller(
                    controller_registry.as_ref(),
                    controller_account_id,
                    "activate",
                )?;
                let current_epoch = self.current_governance_epoch();
                let record = admissions.get_mut(candidate_id).ok_or_else(|| {
                    WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "validator admission candidate not found candidate_id={candidate_id}"
                        ),
                    }
                })?;
                if !matches!(
                    record.status,
                    GovernanceValidatorAdmissionStatus::ApprovedCandidate
                        | GovernanceValidatorAdmissionStatus::ProbationReady
                ) {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "validator admission candidate cannot activate from status candidate_id={candidate_id} status={:?}",
                            record.status
                        ),
                    });
                }
                record.activation_epoch = Some(*activation_epoch);
                record.status = Self::governance_validator_admission_status_for_activation_epoch(
                    current_epoch,
                    *activation_epoch,
                );
                record.last_transition_tick = self.state.time;
                self.resolve_governance_effective_finality_signer_registry_from_admissions(
                    &admissions,
                )?;
            }
            GovernanceEvent::ValidatorAdmissionRevoked {
                controller_account_id,
                candidate_id,
                node_id,
                revoked_at_epoch,
                reason,
            } => {
                self.validate_admission_controller(
                    controller_registry.as_ref(),
                    controller_account_id,
                    "revoke",
                )?;
                let resolved = if admissions.contains_key(candidate_id) {
                    candidate_id.clone()
                } else if let Some(key) =
                    Self::governance_validator_admission_record_key_for_node(&admissions, node_id)
                {
                    key
                } else {
                    format!("legacy-revoked:{node_id}")
                };
                let public_key = identity_bindings.get(node_id).cloned().or_else(|| self.resolve_governance_effective_finality_signer_registry().ok().flatten().and_then(|r| r.signer_bindings.get(node_id).cloned())).ok_or_else(|| WorldError::GovernancePolicyInvalid { reason: format!("validator admission revoke missing node identity binding node_id={node_id}") })?;
                let duplicates =
                    Self::governance_validator_admission_keys_for_node(&admissions, node_id);
                if duplicates.len() > 1 && !duplicates.contains(&resolved) {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "validator admission revoke found multiple candidate records for node_id={node_id} candidate_keys={duplicates:?}"
                        ),
                    });
                }
                let record = admissions.entry(resolved.clone()).or_insert_with(|| {
                    GovernanceValidatorAdmissionRecord {
                        candidate_id: resolved.clone(),
                        node_id: node_id.clone(),
                        finality_signer_public_key: public_key,
                        stake: DEFAULT_GOVERNANCE_VALIDATOR_STAKE,
                        operator_owner: "governance.revocation".into(),
                        public_manifest_hash: "synthetic-revocation".into(),
                        requested_at_epoch: *revoked_at_epoch,
                        last_transition_tick: self.state.time,
                        approved_at_epoch: Some(*revoked_at_epoch),
                        activation_epoch: Some(*revoked_at_epoch),
                        status: GovernanceValidatorAdmissionStatus::Applied,
                        revoked_at_epoch: None,
                        revocation_reason: None,
                    }
                });
                if record.node_id != *node_id {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "validator admission revoke node_id mismatch candidate_id={resolved} expected={} actual={node_id}",
                            record.node_id
                        ),
                    });
                }
                record.candidate_id = resolved;
                record.status = GovernanceValidatorAdmissionStatus::Revoked;
                record.last_transition_tick = self.state.time;
                record.revoked_at_epoch = Some(*revoked_at_epoch);
                record.revocation_reason = Some(reason.clone());
                self.resolve_governance_effective_finality_signer_registry_from_admissions(
                    &admissions,
                )?;
            }
            _ => {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: "unsupported prepared governance registry event".into(),
                });
            }
        }
        Ok(PreparedGovernanceRegistryEvent {
            event: event.clone(),
            controller_registry,
            admissions,
            identity_bindings,
            account_bindings,
        })
    }

    fn validate_admission_controller(
        &self,
        registry: Option<&GovernanceMainTokenControllerRegistry>,
        actual: &str,
        verb: &str,
    ) -> Result<(), WorldError> {
        let Some(registry) = registry else {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "validator admission {verb} missing main token controller registry"
                ),
            });
        };
        let expected = Self::validator_admission_controller_account_id(registry)?;
        if expected != actual {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "validator admission {verb} controller mismatch expected={expected} actual={actual}"
                ),
            });
        }
        Ok(())
    }
}
