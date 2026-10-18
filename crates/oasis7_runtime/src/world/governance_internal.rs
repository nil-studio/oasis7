use super::*;
impl World {
    pub(crate) fn governance_finality_registry_epoch_snapshot(
        &self,
        epoch_id: u64,
    ) -> Option<GovernanceFinalityEpochSnapshot> {
        let registry = match self.resolve_governance_effective_finality_signer_registry() {
            Ok(Some(registry)) => registry,
            Ok(None) => return None,
            Err(_) => self.state.governance_finality_signer_registry.clone()?,
        };
        let signer_node_ids: Vec<String> = registry.signer_bindings.keys().cloned().collect();
        let threshold = registry.threshold;
        let threshold_bps = if registry.threshold_bps > 0 {
            registry.threshold_bps
        } else {
            governance_threshold_bps(threshold, signer_node_ids.len())
        };
        Some(GovernanceFinalityEpochSnapshot {
            epoch_id,
            threshold_bps,
            min_unique_signers: threshold,
            validator_set_hash: governance_finality_validator_set_hash(signer_node_ids.as_slice()),
            stake_root: governance_finality_stake_root(
                signer_node_ids.as_slice(),
                &registry.validator_stakes,
            ),
            threshold,
            signer_node_ids,
            validator_stakes: registry.validator_stakes.clone(),
        })
    }
    pub(crate) fn normalize_governance_finality_epoch_snapshot(
        &self,
        snapshot: &mut GovernanceFinalityEpochSnapshot,
    ) -> Result<(), WorldError> {
        let mut unique = BTreeSet::new();
        for node_id in snapshot.signer_node_ids.iter().map(|id| id.trim()) {
            if node_id.is_empty() {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality epoch snapshot signer node_id is empty epoch_id={}",
                        snapshot.epoch_id
                    ),
                });
            }
            if self.node_identity_public_key(node_id).is_none() {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality epoch snapshot signer is untrusted epoch_id={} node_id={}",
                        snapshot.epoch_id, node_id
                    ),
                });
            }
            unique.insert(node_id.to_string());
        }
        let unique_signers: Vec<String> = unique.into_iter().collect();
        let min_unique_signers = if snapshot.min_unique_signers > 0 {
            snapshot.min_unique_signers
        } else {
            snapshot.threshold
        };
        if min_unique_signers == 0 {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "finality epoch snapshot min_unique_signers must be > 0 epoch_id={}",
                    snapshot.epoch_id
                ),
            });
        }
        if unique_signers.len() < min_unique_signers as usize {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "finality epoch snapshot signatures below threshold epoch_id={} signers={} min_unique_signers={}",
                    snapshot.epoch_id,
                    unique_signers.len(),
                    min_unique_signers
                ),
            });
        }
        let threshold_bps = if snapshot.threshold_bps > 0 {
            snapshot.threshold_bps
        } else {
            governance_threshold_bps(min_unique_signers, unique_signers.len())
        };
        if threshold_bps == 0 || threshold_bps > 10_000 {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "finality epoch snapshot threshold_bps must be within 1..=10000 epoch_id={} threshold_bps={}",
                    snapshot.epoch_id, threshold_bps
                ),
            });
        }
        let computed_validator_set_hash =
            governance_finality_validator_set_hash(unique_signers.as_slice());
        let validator_set_hash = if snapshot.validator_set_hash.trim().is_empty() {
            computed_validator_set_hash
        } else {
            let normalized = snapshot.validator_set_hash.trim().to_string();
            if normalized != computed_validator_set_hash {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality epoch snapshot validator_set_hash mismatch epoch_id={} expected={} found={}",
                        snapshot.epoch_id, computed_validator_set_hash, normalized
                    ),
                });
            }
            normalized
        };
        let mut normalized_stakes = BTreeMap::new();
        for (node_id, stake) in snapshot.validator_stakes.iter() {
            let node_id = node_id.trim().to_string();
            if node_id.is_empty() || !unique_signers.iter().any(|signer| signer == &node_id) {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality epoch snapshot stake references unknown signer epoch_id={} node_id={}",
                        snapshot.epoch_id, node_id
                    ),
                });
            }
            if *stake == 0 {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality epoch snapshot stake must be > 0 epoch_id={} node_id={}",
                        snapshot.epoch_id, node_id
                    ),
                });
            }
            normalized_stakes.insert(node_id, *stake);
        }
        for node_id in &unique_signers {
            normalized_stakes
                .entry(node_id.clone())
                .or_insert(DEFAULT_GOVERNANCE_VALIDATOR_STAKE);
        }
        let computed_stake_root =
            governance_finality_stake_root(unique_signers.as_slice(), &normalized_stakes);
        let stake_root = if snapshot.stake_root.trim().is_empty() {
            computed_stake_root
        } else {
            let normalized = snapshot.stake_root.trim().to_string();
            if normalized != computed_stake_root {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality epoch snapshot stake_root mismatch epoch_id={} expected={} found={}",
                        snapshot.epoch_id, computed_stake_root, normalized
                    ),
                });
            }
            normalized
        };
        snapshot.threshold = min_unique_signers;
        snapshot.min_unique_signers = min_unique_signers;
        snapshot.threshold_bps = threshold_bps;
        snapshot.validator_set_hash = validator_set_hash;
        snapshot.stake_root = stake_root;
        snapshot.signer_node_ids = unique_signers;
        snapshot.validator_stakes = normalized_stakes;
        Ok(())
    }
    pub(crate) fn validate_governance_finality_signer_registry(
        &self,
        mut registry: GovernanceFinalitySignerRegistry,
    ) -> Result<GovernanceFinalitySignerRegistry, WorldError> {
        registry.slot_id = registry.slot_id.trim().to_string();
        if registry.slot_id.is_empty() {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: "finality signer registry slot_id cannot be empty".to_string(),
            });
        }
        if registry.threshold == 0 {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "finality signer registry threshold must be > 0 slot_id={}",
                    registry.slot_id
                ),
            });
        }
        let mut normalized_bindings = BTreeMap::new();
        let mut unique_public_keys = BTreeSet::new();
        for (node_id, public_key_hex) in registry.signer_bindings {
            let node_id = node_id.trim().to_string();
            if node_id.is_empty() {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality signer registry node_id cannot be empty slot_id={}",
                        registry.slot_id
                    ),
                });
            }
            let public_key_hex = public_key_hex.trim().to_string();
            decode_hex_array::<32>(
                public_key_hex.as_str(),
                format!(
                    "finality signer public key slot_id={} node_id={node_id}",
                    registry.slot_id
                )
                .as_str(),
            )?;
            if !unique_public_keys.insert(public_key_hex.clone()) {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality signer registry public key must be unique slot_id={} public_key={}",
                        registry.slot_id, public_key_hex
                    ),
                });
            }
            normalized_bindings.insert(node_id, public_key_hex);
        }
        let mut normalized_stakes = BTreeMap::new();
        for (node_id, stake) in registry.validator_stakes {
            let node_id = node_id.trim().to_string();
            if node_id.is_empty() {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality signer registry stake node_id cannot be empty slot_id={}",
                        registry.slot_id
                    ),
                });
            }
            if !normalized_bindings.contains_key(node_id.as_str()) {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality signer registry stake references unknown signer slot_id={} node_id={}",
                        registry.slot_id, node_id
                    ),
                });
            }
            if stake == 0 {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality signer registry stake must be > 0 slot_id={} node_id={}",
                        registry.slot_id, node_id
                    ),
                });
            }
            normalized_stakes.insert(node_id, stake);
        }
        for node_id in normalized_bindings.keys() {
            normalized_stakes
                .entry(node_id.clone())
                .or_insert(DEFAULT_GOVERNANCE_VALIDATOR_STAKE);
        }
        if normalized_bindings.len() < usize::from(registry.threshold) {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "finality signer registry threshold exceeds signer count slot_id={} threshold={} signers={}",
                    registry.slot_id,
                    registry.threshold,
                    normalized_bindings.len()
                ),
            });
        }
        if registry.threshold_bps == 0 {
            registry.threshold_bps =
                governance_threshold_bps(registry.threshold, normalized_bindings.len());
        }
        if registry.threshold_bps == 0 || registry.threshold_bps > 10_000 {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "finality signer registry threshold_bps must be within 1..=10000 slot_id={} threshold_bps={}",
                    registry.slot_id, registry.threshold_bps
                ),
            });
        }
        registry.signer_bindings = normalized_bindings;
        registry.validator_stakes = normalized_stakes;
        Ok(registry)
    }
    fn validate_governance_threshold_signer_policy(
        account_id: &str,
        mut policy: GovernanceThresholdSignerPolicy,
    ) -> Result<GovernanceThresholdSignerPolicy, WorldError> {
        if policy.threshold == 0 {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "controller signer policy threshold must be > 0 controller_account_id={account_id}"
                ),
            });
        }
        let mut normalized_public_keys = BTreeSet::new();
        for public_key_hex in policy.allowed_public_keys {
            let public_key_hex = public_key_hex.trim().to_string();
            decode_hex_array::<32>(
                public_key_hex.as_str(),
                format!("controller signer policy public key controller_account_id={account_id}")
                    .as_str(),
            )?;
            normalized_public_keys.insert(public_key_hex);
        }
        if normalized_public_keys.len() < usize::from(policy.threshold) {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "controller signer policy threshold exceeds signer count controller_account_id={account_id} threshold={} signers={}",
                    policy.threshold,
                    normalized_public_keys.len()
                ),
            });
        }
        policy.allowed_public_keys = normalized_public_keys;
        Ok(policy)
    }
    pub(crate) fn validate_governance_main_token_controller_registry(
        mut registry: GovernanceMainTokenControllerRegistry,
    ) -> Result<GovernanceMainTokenControllerRegistry, WorldError> {
        registry.genesis_controller_account_id =
            registry.genesis_controller_account_id.trim().to_string();
        if registry.genesis_controller_account_id.is_empty() {
            return Err(WorldError::GovernancePolicyInvalid {
                reason:
                    "main token controller registry genesis_controller_account_id cannot be empty"
                        .to_string(),
            });
        }
        let mut normalized_policies = BTreeMap::new();
        for (account_id, policy) in registry.controller_signer_policies {
            let account_id = account_id.trim().to_string();
            if account_id.is_empty() {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: "main token controller registry account_id cannot be empty".to_string(),
                });
            }
            normalized_policies.insert(
                account_id.clone(),
                Self::validate_governance_threshold_signer_policy(account_id.as_str(), policy)?,
            );
        }
        if !normalized_policies.contains_key(registry.genesis_controller_account_id.as_str()) {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "main token controller registry missing genesis controller policy account_id={}",
                    registry.genesis_controller_account_id
                ),
            });
        }
        let mut normalized_slots = BTreeMap::new();
        for (bucket_id, controller_account_id) in registry.treasury_bucket_controller_slots {
            let bucket_id = bucket_id.trim().to_string();
            let controller_account_id = controller_account_id.trim().to_string();
            if bucket_id.is_empty() || controller_account_id.is_empty() {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason:
                        "main token controller registry treasury bucket binding cannot be empty"
                            .to_string(),
                });
            }
            if !normalized_policies.contains_key(controller_account_id.as_str()) {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "main token controller registry missing treasury controller policy bucket_id={} controller_account_id={}",
                        bucket_id, controller_account_id
                    ),
                });
            }
            normalized_slots.insert(bucket_id, controller_account_id);
        }
        let mut normalized_restricted_admins = BTreeSet::new();
        for account_id in registry.restricted_starter_claim_admin_account_ids {
            let account_id = account_id.trim().to_string();
            if account_id.is_empty() {
                continue;
            }
            if !normalized_policies.contains_key(account_id.as_str()) {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "main token controller registry missing restricted grant admin signer policy account_id={account_id}"
                    ),
                });
            }
            normalized_restricted_admins.insert(account_id);
        }
        registry.controller_signer_policies = normalized_policies;
        registry.treasury_bucket_controller_slots = normalized_slots;
        registry.restricted_starter_claim_admin_account_ids = normalized_restricted_admins;
        Ok(registry)
    }
    pub(crate) fn current_governance_epoch(&self) -> u64 {
        self.governance_epoch_for_time(self.state.time)
    }
    fn governance_epoch_for_time(&self, time: u64) -> u64 {
        let epoch_len = self.governance_execution_policy.epoch_length_ticks.max(1);
        time / epoch_len
    }

    pub(crate) fn is_governance_emergency_brake_active(&self) -> bool {
        self.governance_emergency_brake_until_tick
            .is_some_and(|until| self.state.time < until)
    }

    pub(crate) fn validate_guardian_signers(
        &self,
        signer_node_ids: &[String],
        threshold: u16,
    ) -> Result<Vec<String>, WorldError> {
        if threshold == 0 {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: "guardian threshold must be > 0".to_string(),
            });
        }
        let mut unique = BTreeSet::new();
        for node_id in signer_node_ids {
            if self.node_identity_public_key(node_id.as_str()).is_none() {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!("untrusted guardian signer node_id={node_id}"),
                });
            }
            unique.insert(node_id.clone());
        }
        if unique.len() < threshold as usize {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "guardian signatures below threshold: signers={} threshold={threshold}",
                    unique.len()
                ),
            });
        }
        Ok(unique.into_iter().collect())
    }

    pub(crate) fn validate_governance_finality_certificate(
        &self,
        proposal_id: ProposalId,
        manifest_hash: &str,
        epoch_id: u64,
        certificate: &GovernanceFinalityCertificate,
    ) -> Result<(), WorldError> {
        let snapshot = self.governance_finality_epoch_snapshot_for_epoch(epoch_id);
        let snapshot_min_unique_signers = snapshot.effective_min_unique_signers();
        let epoch_signers: BTreeSet<String> = snapshot.signer_node_ids.iter().cloned().collect();
        if certificate.proposal_id != proposal_id {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "proposal_id mismatch: expected={} found={}",
                    proposal_id, certificate.proposal_id
                ),
            });
        }
        if certificate.manifest_hash != manifest_hash {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: "manifest_hash mismatch".to_string(),
            });
        }
        if certificate.consensus_height == 0 {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: "consensus_height must be > 0".to_string(),
            });
        }
        if certificate.epoch_id != epoch_id {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "epoch_id mismatch: expected={} found={}",
                    epoch_id, certificate.epoch_id
                ),
            });
        }
        if certificate.validator_set_hash.trim().is_empty() {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: "validator_set_hash must be non-empty".to_string(),
            });
        }
        if certificate.validator_set_hash != snapshot.validator_set_hash {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "validator_set_hash mismatch: epoch_id={} certificate={} snapshot={}",
                    epoch_id, certificate.validator_set_hash, snapshot.validator_set_hash
                ),
            });
        }
        if certificate.stake_root.trim().is_empty() {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: "stake_root must be non-empty".to_string(),
            });
        }
        if certificate.stake_root != snapshot.stake_root {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "stake_root mismatch: epoch_id={} certificate={} snapshot={}",
                    epoch_id, certificate.stake_root, snapshot.stake_root
                ),
            });
        }
        if certificate.threshold_bps == 0 || certificate.threshold_bps > 10_000 {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "threshold_bps must be within 1..=10000: {}",
                    certificate.threshold_bps
                ),
            });
        }
        if certificate.threshold_bps != snapshot.threshold_bps {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "threshold_bps mismatch: epoch_id={} certificate={} snapshot={}",
                    epoch_id, certificate.threshold_bps, snapshot.threshold_bps
                ),
            });
        }
        let certificate_min_unique_signers = certificate.effective_min_unique_signers();
        if certificate_min_unique_signers == 0 {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: "min_unique_signers must be > 0".to_string(),
            });
        }
        if certificate_min_unique_signers != snapshot_min_unique_signers {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "min_unique_signers mismatch: epoch_id={} certificate={} snapshot={}",
                    epoch_id, certificate_min_unique_signers, snapshot_min_unique_signers
                ),
            });
        }
        if certificate.signatures.len() < certificate_min_unique_signers as usize {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "signatures below min_unique_signers: signatures={} min_unique_signers={}",
                    certificate.signatures.len(),
                    certificate_min_unique_signers
                ),
            });
        }
        for (node_id, signature_with_prefix) in &certificate.signatures {
            if !epoch_signers.contains(node_id) {
                return Err(WorldError::GovernanceFinalityInvalid {
                    reason: format!(
                        "signer node_id={} is not part of finality epoch snapshot epoch_id={}",
                        node_id, epoch_id
                    ),
                });
            }
            let signer_public_key = self.node_identity_public_key(node_id).ok_or_else(|| {
                WorldError::GovernanceFinalityInvalid {
                    reason: format!("untrusted signer node_id: {node_id}"),
                }
            })?;
            let signature_hex = signature_with_prefix
                .strip_prefix(GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1)
                .ok_or_else(|| WorldError::GovernanceFinalityInvalid {
                    reason: format!("signature prefix mismatch for signer {node_id}"),
                })?;
            let payload = GovernanceFinalityCertificate::signing_payload_v1(
                certificate.proposal_id,
                certificate.manifest_hash.as_str(),
                certificate.consensus_height,
                certificate.epoch_id,
                certificate.validator_set_hash.as_str(),
                certificate.stake_root.as_str(),
                certificate.threshold_bps,
                certificate_min_unique_signers,
                node_id.as_str(),
            );
            let public_key_bytes =
                decode_hex_array::<32>(signer_public_key, "governance finality signer public key")?;
            let signature_bytes =
                decode_hex_array::<64>(signature_hex, "governance finality signature")?;
            let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).map_err(|_| {
                WorldError::GovernanceFinalityInvalid {
                    reason: format!("invalid signer public key for {node_id}"),
                }
            })?;
            let signature = Signature::from_bytes(&signature_bytes);
            verifying_key
                .verify(payload.as_slice(), &signature)
                .map_err(|error| WorldError::GovernanceFinalityInvalid {
                    reason: format!("signature verification failed for {node_id}: {error}"),
                })?;
        }
        let signed_signer_node_ids = certificate.signatures.keys().cloned().collect();
        let signed_stake_bps = governance_finality_signed_stake_bps(
            snapshot.signer_node_ids.as_slice(),
            &snapshot.validator_stakes,
            &signed_signer_node_ids,
        );
        if signed_stake_bps < certificate.threshold_bps {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "signed stake below threshold_bps: epoch_id={} signed_stake_bps={} threshold_bps={}",
                    epoch_id, signed_stake_bps, certificate.threshold_bps
                ),
            });
        }
        Ok(())
    }

    pub(crate) fn apply_governance_event(
        &mut self,
        event: &GovernanceEvent,
    ) -> Result<(), WorldError> {
        match event {
            GovernanceEvent::Proposed {
                proposal_id,
                author,
                base_manifest_hash,
                manifest,
                patch,
            } => {
                let proposal = Proposal {
                    id: *proposal_id,
                    author: author.clone(),
                    base_manifest_hash: base_manifest_hash.clone(),
                    manifest: manifest.clone(),
                    patch: patch.clone(),
                    queued_at_tick: None,
                    not_before_tick: None,
                    activate_epoch: None,
                    timelock_ticks: 0,
                    status: ProposalStatus::Proposed,
                };
                self.proposals.insert(*proposal_id, proposal);
                let (allocated, next_proposal_id, next_proposal_id_era) =
                    World::preview_next_proposal_id(
                        self.next_proposal_id,
                        self.next_proposal_id_era,
                    );
                if allocated == *proposal_id {
                    self.next_proposal_id = next_proposal_id;
                    self.next_proposal_id_era = next_proposal_id_era;
                } else {
                    self.next_proposal_id =
                        self.next_proposal_id.max(proposal_id.saturating_add(1));
                }
            }
            GovernanceEvent::ShadowReport {
                proposal_id,
                manifest_hash,
            } => {
                let proposal =
                    self.proposals
                        .get_mut(proposal_id)
                        .ok_or(WorldError::ProposalNotFound {
                            proposal_id: *proposal_id,
                        })?;
                proposal.status = ProposalStatus::Shadowed {
                    manifest_hash: manifest_hash.clone(),
                };
            }
            GovernanceEvent::Approved {
                proposal_id,
                approver,
                decision,
            } => {
                let proposal =
                    self.proposals
                        .get_mut(proposal_id)
                        .ok_or(WorldError::ProposalNotFound {
                            proposal_id: *proposal_id,
                        })?;
                match decision {
                    ProposalDecision::Approve => {
                        let ProposalStatus::Shadowed { manifest_hash } = &proposal.status else {
                            return Err(WorldError::ProposalInvalidState {
                                proposal_id: *proposal_id,
                                expected: "shadowed".to_string(),
                                found: proposal.status.label(),
                            });
                        };
                        proposal.status = ProposalStatus::Approved {
                            manifest_hash: manifest_hash.clone(),
                            approver: approver.clone(),
                        };
                    }
                    ProposalDecision::Reject { reason } => {
                        proposal.queued_at_tick = None;
                        proposal.not_before_tick = None;
                        proposal.activate_epoch = None;
                        proposal.timelock_ticks = 0;
                        proposal.status = ProposalStatus::Rejected {
                            reason: reason.clone(),
                        };
                    }
                }
            }
            GovernanceEvent::Queued {
                proposal_id,
                manifest_hash,
                queued_at_tick,
                not_before_tick,
                activate_epoch,
                timelock_ticks,
            } => {
                let proposal =
                    self.proposals
                        .get_mut(proposal_id)
                        .ok_or(WorldError::ProposalNotFound {
                            proposal_id: *proposal_id,
                        })?;
                let ProposalStatus::Approved {
                    manifest_hash: approved_hash,
                    ..
                } = &proposal.status
                else {
                    return Err(WorldError::ProposalInvalidState {
                        proposal_id: *proposal_id,
                        expected: "approved".to_string(),
                        found: proposal.status.label(),
                    });
                };
                if approved_hash != manifest_hash {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "queued manifest hash drift: proposal_id={} approved={} queued={}",
                            proposal_id, approved_hash, manifest_hash
                        ),
                    });
                }
                if not_before_tick < queued_at_tick {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "invalid queued timeline: proposal_id={} queued_at={} not_before={}",
                            proposal_id, queued_at_tick, not_before_tick
                        ),
                    });
                }
                proposal.queued_at_tick = Some(*queued_at_tick);
                proposal.not_before_tick = Some(*not_before_tick);
                proposal.activate_epoch = Some(*activate_epoch);
                proposal.timelock_ticks = *timelock_ticks;
            }
            GovernanceEvent::Applied {
                proposal_id,
                manifest_hash,
                ..
            } => {
                let proposal =
                    self.proposals
                        .get(proposal_id)
                        .ok_or(WorldError::ProposalNotFound {
                            proposal_id: *proposal_id,
                        })?;
                let next = crate::world::event_processing::prepared_governance_events::prepare_applied_proposal(
                    proposal, *proposal_id, manifest_hash,
                )?;
                self.proposals.insert(*proposal_id, next);
            }
            GovernanceEvent::FinalityEpochSnapshotSet { snapshot, previous } => {
                self.validate_governance_finality_epoch_snapshot_set(snapshot, previous)?;
                self.governance_finality_epoch_snapshots
                    .insert(snapshot.epoch_id, snapshot.clone());
            }
            GovernanceEvent::FinalityEpochSnapshotRemoved { epoch_id, snapshot } => {
                self.validate_governance_finality_epoch_snapshot_removal(*epoch_id, snapshot)?;
                self.governance_finality_epoch_snapshots.remove(epoch_id);
            }
            GovernanceEvent::EmergencyBrakeActivated {
                active_until_tick,
                threshold,
                signer_node_ids,
                ..
            } => {
                self.validate_guardian_signers(signer_node_ids, *threshold)?;
                let next_until = self
                    .governance_emergency_brake_until_tick
                    .map_or(*active_until_tick, |current| {
                        current.max(*active_until_tick)
                    });
                self.governance_emergency_brake_until_tick = Some(next_until);
            }
            GovernanceEvent::EmergencyBrakeReleased {
                threshold,
                signer_node_ids,
                ..
            } => {
                self.validate_guardian_signers(signer_node_ids, *threshold)?;
                self.governance_emergency_brake_until_tick = None;
            }
            GovernanceEvent::EmergencyVetoed {
                proposal_id,
                reason,
                threshold,
                signer_node_ids,
                ..
            } => {
                let proposal = self.prepare_governance_emergency_veto(
                    *proposal_id,
                    reason,
                    *threshold,
                    signer_node_ids,
                )?;
                self.proposals.insert(*proposal_id, proposal);
            }
            GovernanceEvent::IdentityPenaltyApplied {
                penalty_id,
                target_agent_id,
                evidence_hash,
                initiator,
                reason,
                slash_stake,
                appeal_deadline_tick,
                threshold,
                signer_node_ids,
            } => {
                let (target_agent_id, next, next_profile, _, next_penalty_id) = self
                    .prepare_governance_identity_penalty_application(
                        *penalty_id,
                        target_agent_id,
                        evidence_hash,
                        initiator,
                        reason,
                        *slash_stake,
                        *appeal_deadline_tick,
                        *threshold,
                        signer_node_ids,
                    )?;
                self.governance_identity_penalties.insert(*penalty_id, next);
                self.state
                    .governance_identity_profiles
                    .insert(target_agent_id, next_profile);
                self.next_governance_identity_penalty_id = next_penalty_id;
            }
            GovernanceEvent::IdentityPenaltyAppealed {
                penalty_id,
                appellant,
                reason,
            } => {
                let next = self.prepare_governance_identity_penalty_appeal(
                    *penalty_id,
                    appellant,
                    reason,
                )?;
                self.governance_identity_penalties.insert(*penalty_id, next);
            }
            GovernanceEvent::IdentityPenaltyResolved {
                penalty_id,
                resolver,
                accepted,
                reason,
            } => {
                let (target_agent_id, next, next_profile) = self
                    .prepare_governance_identity_penalty_resolution(
                        *penalty_id,
                        resolver,
                        *accepted,
                        reason,
                    )?;
                self.governance_identity_penalties.insert(*penalty_id, next);
                self.state
                    .governance_identity_profiles
                    .insert(target_agent_id, next_profile);
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
            } => self.apply_governance_validator_admission_submitted(
                controller_account_id,
                candidate_id,
                node_id,
                finality_signer_public_key,
                *stake,
                operator_owner,
                public_manifest_hash,
                *requested_at_epoch,
            )?,
            GovernanceEvent::ValidatorAdmissionApproved {
                controller_account_id,
                candidate_id,
                approved_at_epoch,
            } => self.apply_governance_validator_admission_approved(
                controller_account_id,
                candidate_id,
                *approved_at_epoch,
            )?,
            GovernanceEvent::ValidatorAdmissionActivated {
                controller_account_id,
                candidate_id,
                activation_epoch,
            } => self.apply_governance_validator_admission_activated(
                controller_account_id,
                candidate_id,
                *activation_epoch,
            )?,
            GovernanceEvent::ValidatorAdmissionRevoked {
                controller_account_id,
                candidate_id,
                node_id,
                revoked_at_epoch,
                reason,
            } => self.apply_governance_validator_admission_revoked(
                controller_account_id,
                candidate_id,
                node_id,
                *revoked_at_epoch,
                reason,
            )?,
            GovernanceEvent::RestrictedStarterClaimAdminRegistryUpdated {
                controller_account_id,
                previous_admin_account_ids,
                next_admin_account_ids,
            } => {
                let Some(current_registry) =
                    self.state.governance_main_token_controller_registry.clone()
                else {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: "restricted claim admin registry update missing main token controller registry"
                            .to_string(),
                    });
                };
                let expected_controller_account_id =
                    Self::restricted_starter_claim_admin_registry_controller_account_id(
                        &current_registry,
                    )?;
                if expected_controller_account_id != *controller_account_id {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "restricted claim admin registry controller mismatch expected={} actual={}",
                            expected_controller_account_id, controller_account_id
                        ),
                    });
                }
                let mut current_admins = current_registry
                    .restricted_starter_claim_admin_account_ids
                    .iter()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                current_admins.sort();
                if current_admins != *previous_admin_account_ids {
                    return Err(WorldError::GovernancePolicyInvalid {
                        reason: format!(
                            "restricted claim admin registry drift before apply: expected_previous={:?} actual_current={:?}",
                            previous_admin_account_ids, current_admins
                        ),
                    });
                }
                let mut next_registry = current_registry;
                next_registry.restricted_starter_claim_admin_account_ids = next_admin_account_ids
                    .iter()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .collect();
                next_registry =
                    Self::validate_governance_main_token_controller_registry(next_registry)?;
                self.state.governance_main_token_controller_registry = Some(next_registry);
            }
        }
        Ok(())
    }
}
