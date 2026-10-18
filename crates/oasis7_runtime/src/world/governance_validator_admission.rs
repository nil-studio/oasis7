use super::*;

use crate::{GovernanceValidatorAdmissionRecord, GovernanceValidatorAdmissionStatus};

impl World {
    pub(crate) fn governance_validator_admission_status_for_activation_epoch(
        current_epoch: u64,
        activation_epoch: u64,
    ) -> GovernanceValidatorAdmissionStatus {
        if activation_epoch <= current_epoch {
            GovernanceValidatorAdmissionStatus::Active
        } else {
            GovernanceValidatorAdmissionStatus::ProbationReady
        }
    }

    pub(crate) fn validate_governance_validator_admission_record(
        &self,
        mut record: GovernanceValidatorAdmissionRecord,
    ) -> Result<GovernanceValidatorAdmissionRecord, WorldError> {
        record.candidate_id = record.candidate_id.trim().to_string();
        record.node_id = record.node_id.trim().to_string();
        record.finality_signer_public_key = record.finality_signer_public_key.trim().to_string();
        record.operator_owner = record.operator_owner.trim().to_string();
        record.public_manifest_hash = record.public_manifest_hash.trim().to_string();
        if record.candidate_id.is_empty()
            || record.node_id.is_empty()
            || record.finality_signer_public_key.is_empty()
            || record.operator_owner.is_empty()
            || record.public_manifest_hash.is_empty()
        {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: "validator admission record fields cannot be empty".to_string(),
            });
        }
        if record.stake == 0 {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "validator admission stake must be > 0 candidate_id={} node_id={}",
                    record.candidate_id, record.node_id
                ),
            });
        }
        decode_hex_array::<32>(
            record.finality_signer_public_key.as_str(),
            format!(
                "validator admission public key candidate_id={} node_id={}",
                record.candidate_id, record.node_id
            )
            .as_str(),
        )?;
        Ok(record)
    }

    pub(crate) fn governance_validator_admission_effective_epoch(
        record: &GovernanceValidatorAdmissionRecord,
    ) -> u64 {
        match record.status {
            GovernanceValidatorAdmissionStatus::Revoked => record
                .revoked_at_epoch
                .or(record.activation_epoch)
                .or(record.approved_at_epoch)
                .unwrap_or(record.requested_at_epoch),
            GovernanceValidatorAdmissionStatus::Active
            | GovernanceValidatorAdmissionStatus::ProbationReady => record
                .activation_epoch
                .or(record.approved_at_epoch)
                .unwrap_or(record.requested_at_epoch),
            GovernanceValidatorAdmissionStatus::ApprovedCandidate => record
                .approved_at_epoch
                .unwrap_or(record.requested_at_epoch),
            GovernanceValidatorAdmissionStatus::Applied => record.requested_at_epoch,
        }
    }

    pub(crate) fn governance_validator_admission_status_precedence(
        status: GovernanceValidatorAdmissionStatus,
    ) -> u8 {
        match status {
            GovernanceValidatorAdmissionStatus::Revoked => 5,
            GovernanceValidatorAdmissionStatus::Active => 4,
            GovernanceValidatorAdmissionStatus::ProbationReady => 3,
            GovernanceValidatorAdmissionStatus::ApprovedCandidate => 2,
            GovernanceValidatorAdmissionStatus::Applied => 1,
        }
    }

    pub(crate) fn governance_effective_validator_admission_records(
        admissions: &BTreeMap<String, GovernanceValidatorAdmissionRecord>,
    ) -> BTreeMap<String, &GovernanceValidatorAdmissionRecord> {
        let mut effective = BTreeMap::new();
        for record in admissions.values() {
            let candidate = effective.entry(record.node_id.clone()).or_insert(record);
            let candidate_order = (
                candidate.last_transition_tick,
                Self::governance_validator_admission_effective_epoch(candidate),
                Self::governance_validator_admission_status_precedence(candidate.status),
            );
            let record_order = (
                record.last_transition_tick,
                Self::governance_validator_admission_effective_epoch(record),
                Self::governance_validator_admission_status_precedence(record.status),
            );
            if record_order > candidate_order {
                *candidate = record;
            }
        }
        effective
    }

    pub(crate) fn governance_validator_admission_keys_for_node(
        admissions: &BTreeMap<String, GovernanceValidatorAdmissionRecord>,
        node_id: &str,
    ) -> Vec<String> {
        admissions
            .iter()
            .filter(|(_, record)| record.node_id == node_id)
            .map(|(candidate_id, _)| candidate_id.clone())
            .collect()
    }

    pub(crate) fn governance_validator_admission_record_key_for_node(
        admissions: &BTreeMap<String, GovernanceValidatorAdmissionRecord>,
        node_id: &str,
    ) -> Option<String> {
        let per_node = Self::governance_effective_validator_admission_records(admissions);
        per_node
            .get(node_id)
            .map(|record| record.candidate_id.clone())
            .or_else(|| {
                admissions.iter().find_map(|(candidate_id, record)| {
                    (record.node_id == node_id).then(|| candidate_id.clone())
                })
            })
    }

    pub(crate) fn resolve_governance_effective_finality_signer_registry_from_admissions(
        &self,
        admissions: &BTreeMap<String, GovernanceValidatorAdmissionRecord>,
    ) -> Result<Option<GovernanceFinalitySignerRegistry>, WorldError> {
        let Some(mut registry) = self.state.governance_finality_signer_registry.clone() else {
            return Ok(None);
        };
        let current_epoch = self.current_governance_epoch();
        for record in Self::governance_effective_validator_admission_records(admissions).values() {
            match record.status {
                GovernanceValidatorAdmissionStatus::Revoked => {
                    registry.signer_bindings.remove(record.node_id.as_str());
                    registry.validator_stakes.remove(record.node_id.as_str());
                }
                GovernanceValidatorAdmissionStatus::ProbationReady
                | GovernanceValidatorAdmissionStatus::Active => {
                    let activation_epoch = record.activation_epoch.unwrap_or(current_epoch);
                    if activation_epoch <= current_epoch {
                        registry.signer_bindings.insert(
                            record.node_id.clone(),
                            record.finality_signer_public_key.clone(),
                        );
                        registry
                            .validator_stakes
                            .insert(record.node_id.clone(), record.stake);
                    }
                }
                GovernanceValidatorAdmissionStatus::Applied
                | GovernanceValidatorAdmissionStatus::ApprovedCandidate => {}
            }
        }
        self.validate_governance_finality_signer_registry(registry)
            .map(Some)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Validator admission submission fields are a stable governance state-machine input set."
    )]
    pub(crate) fn apply_governance_validator_admission_submitted(
        &mut self,
        controller_account_id: &str,
        candidate_id: &str,
        node_id: &str,
        finality_signer_public_key: &str,
        stake: u64,
        operator_owner: &str,
        public_manifest_hash: &str,
        requested_at_epoch: u64,
    ) -> Result<(), WorldError> {
        let Some(current_registry) = self.state.governance_main_token_controller_registry.clone()
        else {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: "validator admission submit missing main token controller registry"
                    .to_string(),
            });
        };
        let expected_controller_account_id =
            Self::validator_admission_controller_account_id(&current_registry)?;
        if expected_controller_account_id != controller_account_id {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "validator admission submit controller mismatch expected={} actual={}",
                    expected_controller_account_id, controller_account_id
                ),
            });
        }
        let record = self.validate_governance_validator_admission_record(
            GovernanceValidatorAdmissionRecord {
                candidate_id: candidate_id.to_string(),
                node_id: node_id.to_string(),
                finality_signer_public_key: finality_signer_public_key.to_string(),
                stake,
                operator_owner: operator_owner.to_string(),
                public_manifest_hash: public_manifest_hash.to_string(),
                requested_at_epoch,
                last_transition_tick: self.state.time,
                ..GovernanceValidatorAdmissionRecord::default()
            },
        )?;
        if self
            .state
            .governance_validator_admissions
            .contains_key(record.candidate_id.as_str())
        {
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
        if let Some(existing_public_key) = self.node_identity_public_key(record.node_id.as_str())
            && existing_public_key != record.finality_signer_public_key
        {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "validator admission node identity binding mismatch node_id={} expected={} actual={}",
                    record.node_id, existing_public_key, record.finality_signer_public_key
                ),
            });
        }
        self.bind_node_identity(
            record.node_id.as_str(),
            record.finality_signer_public_key.as_str(),
        )?;
        self.state
            .governance_validator_admissions
            .insert(record.candidate_id.clone(), record);
        Ok(())
    }

    pub(crate) fn apply_governance_validator_admission_approved(
        &mut self,
        controller_account_id: &str,
        candidate_id: &str,
        approved_at_epoch: u64,
    ) -> Result<(), WorldError> {
        let Some(current_registry) = self.state.governance_main_token_controller_registry.clone()
        else {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: "validator admission approve missing main token controller registry"
                    .to_string(),
            });
        };
        let expected_controller_account_id =
            Self::validator_admission_controller_account_id(&current_registry)?;
        if expected_controller_account_id != controller_account_id {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "validator admission approve controller mismatch expected={} actual={}",
                    expected_controller_account_id, controller_account_id
                ),
            });
        }
        let record = self
            .state
            .governance_validator_admissions
            .get_mut(candidate_id)
            .ok_or_else(|| WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "validator admission candidate not found candidate_id={}",
                    candidate_id
                ),
            })?;
        if record.status != GovernanceValidatorAdmissionStatus::Applied {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "validator admission candidate is not in applied status candidate_id={} status={:?}",
                    candidate_id, record.status
                ),
            });
        }
        record.status = GovernanceValidatorAdmissionStatus::ApprovedCandidate;
        record.approved_at_epoch = Some(approved_at_epoch);
        record.last_transition_tick = self.state.time;
        Ok(())
    }

    pub(crate) fn apply_governance_validator_admission_activated(
        &mut self,
        controller_account_id: &str,
        candidate_id: &str,
        activation_epoch: u64,
    ) -> Result<(), WorldError> {
        let Some(current_registry) = self.state.governance_main_token_controller_registry.clone()
        else {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: "validator admission activate missing main token controller registry"
                    .to_string(),
            });
        };
        let expected_controller_account_id =
            Self::validator_admission_controller_account_id(&current_registry)?;
        if expected_controller_account_id != controller_account_id {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "validator admission activate controller mismatch expected={} actual={}",
                    expected_controller_account_id, controller_account_id
                ),
            });
        }
        let current_epoch = self.current_governance_epoch();
        let preview_admissions = {
            let mut admissions = self.state.governance_validator_admissions.clone();
            let record = admissions.get_mut(candidate_id).ok_or_else(|| {
                WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "validator admission candidate not found candidate_id={}",
                        candidate_id
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
                        "validator admission candidate cannot activate from status candidate_id={} status={:?}",
                        candidate_id, record.status
                    ),
                });
            }
            record.activation_epoch = Some(activation_epoch);
            record.status = Self::governance_validator_admission_status_for_activation_epoch(
                current_epoch,
                activation_epoch,
            );
            record.last_transition_tick = self.state.time;
            admissions
        };
        self.resolve_governance_effective_finality_signer_registry_from_admissions(
            &preview_admissions,
        )?;
        self.state.governance_validator_admissions = preview_admissions;
        Ok(())
    }

    pub(crate) fn apply_governance_validator_admission_revoked(
        &mut self,
        controller_account_id: &str,
        candidate_id: &str,
        node_id: &str,
        revoked_at_epoch: u64,
        reason: &str,
    ) -> Result<(), WorldError> {
        let Some(current_registry) = self.state.governance_main_token_controller_registry.clone()
        else {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: "validator admission revoke missing main token controller registry"
                    .to_string(),
            });
        };
        let expected_controller_account_id =
            Self::validator_admission_controller_account_id(&current_registry)?;
        if expected_controller_account_id != controller_account_id {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "validator admission revoke controller mismatch expected={} actual={}",
                    expected_controller_account_id, controller_account_id
                ),
            });
        }
        let resolved_candidate_id = if self
            .state
            .governance_validator_admissions
            .contains_key(candidate_id)
        {
            candidate_id.to_string()
        } else if let Some(existing_key) = Self::governance_validator_admission_record_key_for_node(
            &self.state.governance_validator_admissions,
            node_id,
        ) {
            existing_key
        } else {
            format!("legacy-revoked:{node_id}")
        };
        let preview_admissions = {
            let mut admissions = self.state.governance_validator_admissions.clone();
            let current_public_key = self
                .node_identity_public_key(node_id)
                .map(str::to_string)
                .or_else(|| {
                    self.resolve_governance_effective_finality_signer_registry()
                        .ok()
                        .flatten()
                        .and_then(|registry| registry.signer_bindings.get(node_id).cloned())
                })
                .ok_or_else(|| WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "validator admission revoke missing node identity binding node_id={}",
                        node_id
                    ),
                })?;
            let duplicate_keys =
                Self::governance_validator_admission_keys_for_node(&admissions, node_id);
            if duplicate_keys.len() > 1 && !duplicate_keys.contains(&resolved_candidate_id) {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "validator admission revoke found multiple candidate records for node_id={} candidate_keys={:?}",
                        node_id, duplicate_keys
                    ),
                });
            }
            let record = admissions
                .entry(resolved_candidate_id.clone())
                .or_insert_with(|| GovernanceValidatorAdmissionRecord {
                    candidate_id: resolved_candidate_id.clone(),
                    node_id: node_id.to_string(),
                    finality_signer_public_key: current_public_key.clone(),
                    stake: DEFAULT_GOVERNANCE_VALIDATOR_STAKE,
                    operator_owner: "governance.revocation".to_string(),
                    public_manifest_hash: "synthetic-revocation".to_string(),
                    requested_at_epoch: revoked_at_epoch,
                    last_transition_tick: self.state.time,
                    approved_at_epoch: Some(revoked_at_epoch),
                    activation_epoch: Some(revoked_at_epoch),
                    status: GovernanceValidatorAdmissionStatus::Applied,
                    revoked_at_epoch: None,
                    revocation_reason: None,
                });
            if record.node_id != node_id {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "validator admission revoke node_id mismatch candidate_id={} expected={} actual={}",
                        resolved_candidate_id, record.node_id, node_id
                    ),
                });
            }
            record.candidate_id = resolved_candidate_id.clone();
            record.status = GovernanceValidatorAdmissionStatus::Revoked;
            record.last_transition_tick = self.state.time;
            record.revoked_at_epoch = Some(revoked_at_epoch);
            record.revocation_reason = Some(reason.to_string());
            admissions
        };
        self.resolve_governance_effective_finality_signer_registry_from_admissions(
            &preview_admissions,
        )?;
        self.state.governance_validator_admissions = preview_admissions;
        Ok(())
    }
}
