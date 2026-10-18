use super::*;
use crate::main_token::{
    MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
    MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
    MainTokenTreasuryDistribution, RESTRICTED_STARTER_CLAIM_GRANT_SPEND_SCOPE_SLOT_1_ONLY,
    RestrictedStarterClaimGrantStatus, is_main_token_treasury_distribution_bucket,
};
use std::collections::BTreeSet;

impl World {
    #[expect(
        clippy::too_many_arguments,
        reason = "Main-token transfer validation keeps the signed action fields explicit and ordered."
    )]
    pub(super) fn evaluate_transfer_main_token_action(
        &self,
        action_id: ActionId,
        from_account_id: &str,
        to_account_id: &str,
        amount: u64,
        nonce: u64,
        asset_id: Option<&str>,
        memo: Option<&str>,
    ) -> DomainEvent {
        let from_account_id = from_account_id.trim();
        let to_account_id = to_account_id.trim();
        let asset_id = asset_id.map(str::trim).filter(|value| !value.is_empty());
        let memo = memo.map(str::trim).filter(|value| !value.is_empty());
        if from_account_id.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["from_account_id cannot be empty".to_string()],
                },
            };
        }
        if to_account_id.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["to_account_id cannot be empty".to_string()],
                },
            };
        }
        if from_account_id == to_account_id {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["from_account_id and to_account_id cannot be the same".to_string()],
                },
            };
        }
        if amount == 0 {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["amount must be > 0".to_string()],
                },
            };
        }
        if nonce == 0 {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["nonce must be > 0".to_string()],
                },
            };
        }

        let event = DomainEvent::MainTokenTransferred {
            from_account_id: from_account_id.to_string(),
            to_account_id: to_account_id.to_string(),
            amount,
            nonce,
            asset_id: asset_id.map(ToOwned::to_owned),
            memo: memo.map(ToOwned::to_owned),
        };
        if let Err(err) =
            super::super::super::main_token_monetary_publication::PreparedMainTokenMonetaryEvent::prepare(
                &self.state,
                &event,
                self.state.time,
            )
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("main token transfer rejected: {err:?}")],
                },
            };
        }
        event
    }

    pub(super) fn evaluate_distribute_main_token_treasury_action(
        &self,
        action_id: ActionId,
        proposal_id: ProposalId,
        distribution_id: &str,
        bucket_id: &str,
        distributions: &[MainTokenTreasuryDistribution],
    ) -> DomainEvent {
        if self.state.main_token_genesis_buckets.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["main token genesis is not initialized".to_string()],
                },
            };
        }
        if proposal_id == 0 {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["proposal_id must be > 0".to_string()],
                },
            };
        }
        let Some(proposal) = self.proposals.get(&proposal_id) else {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "distribute main token treasury rejected: governance proposal not found ({proposal_id})"
                    )],
                },
            };
        };
        match proposal.status {
            ProposalStatus::Approved { .. } | ProposalStatus::Applied { .. } => {}
            _ => {
                return DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "distribute main token treasury rejected: governance proposal must be approved or applied ({proposal_id})"
                        )],
                    },
                };
            }
        }

        let distribution_id = distribution_id.trim();
        if distribution_id.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["distribution_id cannot be empty".to_string()],
                },
            };
        }
        if self
            .state
            .main_token_treasury_distribution_records
            .contains_key(distribution_id)
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "distribute main token treasury rejected: distribution_id already exists ({distribution_id})"
                    )],
                },
            };
        }

        let bucket_id = bucket_id.trim();
        if !is_main_token_treasury_distribution_bucket(bucket_id) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "distribute main token treasury rejected: unsupported bucket ({bucket_id})"
                    )],
                },
            };
        }
        if distributions.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["distributions cannot be empty".to_string()],
                },
            };
        }

        let mut normalized_distributions = Vec::with_capacity(distributions.len());
        let mut seen_accounts = BTreeSet::new();
        let mut total_amount = 0_u64;
        for item in distributions {
            let account_id = item.account_id.trim();
            if account_id.is_empty() {
                return DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "distribute main token treasury rejected: account_id cannot be empty (distribution_id={distribution_id})"
                        )],
                    },
                };
            }
            if item.amount == 0 {
                return DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "distribute main token treasury rejected: amount must be > 0 (distribution_id={distribution_id} account_id={account_id})"
                        )],
                    },
                };
            }
            if !seen_accounts.insert(account_id.to_string()) {
                return DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "distribute main token treasury rejected: duplicate account_id ({account_id})"
                        )],
                    },
                };
            }
            total_amount = match total_amount.checked_add(item.amount) {
                Some(value) => value,
                None => {
                    return DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "distribute main token treasury rejected: total_amount overflow (distribution_id={distribution_id})"
                            )],
                        },
                    };
                }
            };
            normalized_distributions.push(MainTokenTreasuryDistribution {
                account_id: account_id.to_string(),
                amount: item.amount,
            });
        }

        let bucket_balance = self
            .state
            .main_token_treasury_balances
            .get(bucket_id)
            .copied()
            .unwrap_or(0);
        if bucket_balance < total_amount {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "distribute main token treasury rejected: treasury bucket insufficient (bucket={bucket_id} balance={bucket_balance} total={total_amount})"
                    )],
                },
            };
        }

        let event = DomainEvent::MainTokenTreasuryDistributed {
            proposal_id,
            distribution_id: distribution_id.to_string(),
            bucket_id: bucket_id.to_string(),
            total_amount,
            distributions: normalized_distributions,
        };
        if let Err(err) = super::super::super::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent::prepare(
            &self.state,
            &event,
            self.state.time,
        ) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("distribute main token treasury rejected: {err:?}")],
                },
            };
        }
        event
    }

    pub(super) fn evaluate_top_up_restricted_starter_claim_liveops_pool_action(
        &self,
        action_id: ActionId,
        controller_account_id: &str,
        top_up_id: &str,
        amount: u64,
    ) -> DomainEvent {
        let Some(registry) = self.governance_main_token_controller_registry() else {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![
                        "restricted claim liveops pool top-up rejected: main token controller registry is not configured"
                            .to_string(),
                    ],
                },
            };
        };
        let controller_account_id = controller_account_id.trim();
        if controller_account_id.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![
                        "restricted claim liveops pool top-up controller_account_id cannot be empty"
                            .to_string(),
                    ],
                },
            };
        }
        let expected_controller_account_id = match Self::ecosystem_treasury_controller_account_id(
            registry,
            "restricted claim liveops pool top-up",
        ) {
            Ok(account_id) => account_id,
            Err(err) => {
                return DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "restricted claim liveops pool top-up rejected: {err:?}"
                        )],
                    },
                };
            }
        };
        if controller_account_id != expected_controller_account_id {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "restricted claim liveops pool top-up rejected: controller_account_id does not match ecosystem treasury controller slot expected={} actual={}",
                        expected_controller_account_id, controller_account_id
                    )],
                },
            };
        }
        let top_up_id = top_up_id.trim();
        if top_up_id.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![
                        "restricted claim liveops pool top_up_id cannot be empty".to_string(),
                    ],
                },
            };
        }
        if self
            .state
            .restricted_starter_claim_liveops_pool_top_up_records
            .contains_key(top_up_id)
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "restricted claim liveops pool top-up rejected: top_up_id already exists ({top_up_id})"
                    )],
                },
            };
        }
        if amount == 0 {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![
                        "restricted claim liveops pool top-up amount must be > 0".to_string(),
                    ],
                },
            };
        }
        let source_bucket_balance =
            self.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL);
        if source_bucket_balance < amount {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "restricted claim liveops pool top-up treasury insufficient: bucket={} balance={} amount={amount}",
                        MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL, source_bucket_balance
                    )],
                },
            };
        }

        let event = DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp {
            controller_account_id: controller_account_id.to_string(),
            top_up_id: top_up_id.to_string(),
            source_treasury_bucket_id: MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.to_string(),
            target_treasury_bucket_id:
                MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL.to_string(),
            amount,
            topped_up_at_epoch: self.current_governance_epoch(),
        };
        if let Err(err) = super::super::super::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent::prepare(&self.state, &event) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "restricted claim liveops pool top-up rejected: {err:?}"
                    )],
                },
            };
        }
        event
    }

    pub(super) fn evaluate_issue_restricted_starter_claim_grant_action(
        &self,
        action_id: ActionId,
        issuer_account_id: &str,
        beneficiary_account_id: &str,
        amount: u64,
        issuance_reason: &str,
        expires_at_epoch: u64,
    ) -> DomainEvent {
        let issuer_account_id = issuer_account_id.trim();
        if issuer_account_id.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["issuer_account_id cannot be empty".to_string()],
                },
            };
        }
        if let Err(reason) = self.ensure_restricted_starter_claim_admin(issuer_account_id) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![reason],
                },
            };
        }
        let beneficiary_account_id = beneficiary_account_id.trim();
        if beneficiary_account_id.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["beneficiary_account_id cannot be empty".to_string()],
                },
            };
        }
        if amount == 0 {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["restricted grant amount must be > 0".to_string()],
                },
            };
        }
        let issuance_reason = issuance_reason.trim();
        if issuance_reason.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["issuance_reason cannot be empty".to_string()],
                },
            };
        }
        let current_epoch = self.current_governance_epoch();
        if expires_at_epoch <= current_epoch {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "restricted grant expires_at_epoch must be > current_epoch ({expires_at_epoch} <= {current_epoch})"
                    )],
                },
            };
        }
        let source_bucket_id = MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL;
        let source_bucket_balance = self.main_token_treasury_balance(source_bucket_id);
        if source_bucket_balance < amount {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "restricted grant treasury insufficient: bucket={source_bucket_id} balance={source_bucket_balance} amount={amount}"
                    )],
                },
            };
        }
        if !self.restricted_starter_claim_grant_can_be_reissued(beneficiary_account_id) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "restricted grant already active or pending settlement: beneficiary={beneficiary_account_id}"
                    )],
                },
            };
        }
        if self.main_token_restricted_starter_claim_balance(beneficiary_account_id) > 0 {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "restricted grant beneficiary already has seeded restricted balance without reusable grant slot: beneficiary={beneficiary_account_id}"
                    )],
                },
            };
        }

        let event = DomainEvent::RestrictedStarterClaimGrantIssued {
            issuer_id: issuer_account_id.to_string(),
            beneficiary_account_id: beneficiary_account_id.to_string(),
            source_treasury_bucket_id: source_bucket_id.to_string(),
            amount,
            issuance_reason: issuance_reason.to_string(),
            spend_scope: RESTRICTED_STARTER_CLAIM_GRANT_SPEND_SCOPE_SLOT_1_ONLY.to_string(),
            issued_at_epoch: current_epoch,
            expires_at_epoch,
        };
        if let Err(err) = super::super::super::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent::prepare(&self.state, &event) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("issue restricted grant rejected: {err:?}")],
                },
            };
        }
        event
    }

    pub(super) fn evaluate_revoke_restricted_starter_claim_grant_action(
        &self,
        action_id: ActionId,
        issuer_account_id: &str,
        beneficiary_account_id: &str,
        revoke_reason: &str,
    ) -> DomainEvent {
        let issuer_account_id = issuer_account_id.trim();
        if issuer_account_id.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["issuer_account_id cannot be empty".to_string()],
                },
            };
        }
        if let Err(reason) = self.ensure_restricted_starter_claim_admin(issuer_account_id) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![reason],
                },
            };
        }
        let beneficiary_account_id = beneficiary_account_id.trim();
        if beneficiary_account_id.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["beneficiary_account_id cannot be empty".to_string()],
                },
            };
        }
        let revoke_reason = revoke_reason.trim();
        if revoke_reason.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["revoke_reason cannot be empty".to_string()],
                },
            };
        }

        let Some(grant) = self
            .state
            .restricted_starter_claim_grants
            .get(beneficiary_account_id)
        else {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "restricted grant not found: beneficiary={beneficiary_account_id}"
                    )],
                },
            };
        };
        if grant.status != RestrictedStarterClaimGrantStatus::Issued {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "restricted grant is no longer active: beneficiary={} status={:?}",
                        beneficiary_account_id, grant.status
                    )],
                },
            };
        }
        if grant.issuer_id != issuer_account_id {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "restricted grant issuer mismatch: beneficiary={} expected={} actual={}",
                        beneficiary_account_id, grant.issuer_id, issuer_account_id
                    )],
                },
            };
        }

        let event = DomainEvent::RestrictedStarterClaimGrantRevoked {
            beneficiary_account_id: beneficiary_account_id.to_string(),
            issuer_id: issuer_account_id.to_string(),
            issuance_reason: grant.issuance_reason.clone(),
            spend_scope: grant.spend_scope.clone(),
            source_treasury_bucket_id: grant.source_treasury_bucket_id.clone(),
            issued_amount: grant.issued_amount,
            revoked_amount: self
                .main_token_restricted_starter_claim_balance(beneficiary_account_id),
            issued_at_epoch: grant.issued_at_epoch,
            revoked_at_epoch: self.current_governance_epoch(),
            configured_expires_at_epoch: grant.expires_at_epoch,
            revoke_reason: revoke_reason.to_string(),
        };
        if let Err(err) = super::super::super::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent::prepare(&self.state, &event) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("revoke restricted grant rejected: {err:?}")],
                },
            };
        }
        event
    }
}
