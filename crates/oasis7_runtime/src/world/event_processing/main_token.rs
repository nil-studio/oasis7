use super::*;

impl World {
    pub(super) fn evaluate_apply_node_points_settlement_action(
        &self,
        action_id: ActionId,
        report: &EpochSettlementReport,
        signer_node_id: &str,
        mint_records: &[NodeRewardMintRecord],
    ) -> DomainEvent {
        let settlement_hash = match hash_json(report) {
            Ok(hash) => hash,
            Err(err) => {
                return DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("settlement hash compute failed: {err:?}")],
                    },
                };
            }
        };
        let (main_token_bridge_total_amount, main_token_bridge_distributions) =
            match self.build_main_token_bridge_distributions_for_settlement(report) {
                Ok(values) => values,
                Err(reason) => {
                    return DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("apply node points settlement rejected: {reason}")],
                        },
                    };
                }
            };

        let event = DomainEvent::NodePointsSettlementApplied {
            report: report.clone(),
            signer_node_id: signer_node_id.to_string(),
            settlement_hash,
            minted_records: mint_records.to_vec(),
            main_token_bridge_total_amount,
            main_token_bridge_distributions,
        };
        if let Err(err) =
            super::super::node_points_settlement_publication::PreparedNodePointsSettlement::prepare(
                &self.state,
                &event,
            )
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("apply node points settlement rejected: {err:?}")],
                },
            };
        }
        event
    }

    fn build_main_token_bridge_distributions_for_settlement(
        &self,
        report: &EpochSettlementReport,
    ) -> Result<(u64, Vec<MainTokenNodePointsBridgeDistribution>), String> {
        if self
            .state
            .main_token_node_points_bridge_records
            .contains_key(&report.epoch_index)
        {
            return Err(format!(
                "main token bridge already processed for epoch={}",
                report.epoch_index
            ));
        }
        let Some(issuance) = self
            .state
            .main_token_epoch_issuance_records
            .get(&report.epoch_index)
        else {
            return Ok((0, Vec::new()));
        };
        let bridge_budget = issuance.node_service_reward_amount;
        if bridge_budget == 0 {
            return Ok((0, Vec::new()));
        }
        let treasury_balance = self
            .state
            .main_token_treasury_balances
            .get(MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD)
            .copied()
            .unwrap_or(0);
        if treasury_balance < bridge_budget {
            return Err(format!(
                "main token bridge treasury insufficient for epoch={} balance={} budget={}",
                report.epoch_index, treasury_balance, bridge_budget
            ));
        }

        let eligible = report
            .settlements
            .iter()
            .filter(|settlement| settlement.awarded_points > 0)
            .cloned()
            .collect::<Vec<_>>();
        if eligible.is_empty() {
            return Ok((0, Vec::new()));
        }

        let (total_amount, raw_distributions) =
            distribute_main_token_bridge_budget(bridge_budget, eligible.as_slice());
        let mut distributions = Vec::with_capacity(raw_distributions.len());
        for item in raw_distributions {
            let account_id = self.resolve_main_token_bridge_account_id_for_node(&item.node_id)?;
            distributions.push(MainTokenNodePointsBridgeDistribution {
                node_id: item.node_id,
                account_id,
                amount: item.amount,
            });
        }

        Ok((total_amount, distributions))
    }

    fn resolve_main_token_bridge_account_id_for_node(
        &self,
        node_id: &str,
    ) -> Result<String, String> {
        if let Some(account_id) = self.state.node_main_token_account_bindings.get(node_id) {
            let account_id = account_id.trim();
            if account_id.is_empty() {
                return Err(format!(
                    "main token account binding cannot be empty: node={}",
                    node_id
                ));
            }
            return Ok(account_id.to_string());
        }

        let public_key = self
            .state
            .node_identity_bindings
            .get(node_id)
            .ok_or_else(|| format!("main token account binding missing for node={node_id}"))?;
        Ok(main_token_account_id_from_node_public_key(public_key))
    }

    fn build_main_token_genesis_allocations(
        &self,
        plans: &[MainTokenGenesisAllocationPlan],
    ) -> Result<Vec<MainTokenGenesisAllocationBucketState>, String> {
        if plans.is_empty() {
            return Err("allocations cannot be empty".to_string());
        }
        let mut seen_bucket_ids = BTreeSet::new();
        let mut ratio_sum = 0_u64;
        for plan in plans {
            if plan.bucket_id.trim().is_empty() {
                return Err("allocation bucket_id cannot be empty".to_string());
            }
            if !seen_bucket_ids.insert(plan.bucket_id.as_str()) {
                return Err(format!(
                    "duplicate allocation bucket_id: {}",
                    plan.bucket_id
                ));
            }
            if plan.recipient.trim().is_empty() {
                return Err(format!(
                    "allocation recipient cannot be empty: bucket={}",
                    plan.bucket_id
                ));
            }
            if plan.ratio_bps == 0 {
                return Err(format!(
                    "allocation ratio must be > 0: bucket={}",
                    plan.bucket_id
                ));
            }
            ratio_sum = ratio_sum.saturating_add(u64::from(plan.ratio_bps));
        }
        if ratio_sum != 10_000 {
            return Err(format!(
                "allocation ratio sum must be 10000 bps, got {}",
                ratio_sum
            ));
        }

        let initial_supply = self.state.main_token_config.initial_supply;
        if initial_supply == 0 {
            return Err("main token initial_supply must be > 0".to_string());
        }

        let mut allocations = Vec::with_capacity(plans.len());
        let mut distributed = 0_u64;
        for plan in plans {
            let allocated_u128 =
                (u128::from(initial_supply) * u128::from(plan.ratio_bps)) / u128::from(10_000_u32);
            let allocated_amount = u64::try_from(allocated_u128).map_err(|_| {
                format!(
                    "allocated amount overflow: bucket={} amount={allocated_u128}",
                    plan.bucket_id
                )
            })?;
            distributed = distributed
                .checked_add(allocated_amount)
                .ok_or_else(|| "distributed allocation overflow".to_string())?;
            allocations.push(MainTokenGenesisAllocationBucketState {
                bucket_id: plan.bucket_id.clone(),
                ratio_bps: plan.ratio_bps,
                recipient: plan.recipient.clone(),
                cliff_epochs: plan.cliff_epochs,
                linear_unlock_epochs: plan.linear_unlock_epochs,
                start_epoch: plan.start_epoch,
                allocated_amount,
                claimed_amount: 0,
            });
        }

        let mut remainder = initial_supply.saturating_sub(distributed);
        allocations.sort_by(|a, b| {
            b.ratio_bps
                .cmp(&a.ratio_bps)
                .then_with(|| a.bucket_id.cmp(&b.bucket_id))
        });
        let mut index = 0_usize;
        while remainder > 0 && !allocations.is_empty() {
            let target = index % allocations.len();
            allocations[target].allocated_amount =
                allocations[target].allocated_amount.saturating_add(1);
            remainder -= 1;
            index = index.saturating_add(1);
        }
        allocations.sort_by(|a, b| a.bucket_id.cmp(&b.bucket_id));
        Ok(allocations)
    }

    pub(super) fn evaluate_initialize_main_token_genesis_action(
        &self,
        action_id: ActionId,
        allocations: &[MainTokenGenesisAllocationPlan],
    ) -> DomainEvent {
        if !self.state.main_token_genesis_buckets.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["main token genesis is already initialized".to_string()],
                },
            };
        }
        if self.state.main_token_supply.total_supply > 0
            || self.state.main_token_supply.total_issued > 0
            || self.state.main_token_supply.total_burned > 0
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["main token supply is already initialized".to_string()],
                },
            };
        }

        let resolved_allocations = match self.build_main_token_genesis_allocations(allocations) {
            Ok(values) => values,
            Err(reason) => {
                return DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("initialize main token genesis rejected: {reason}")],
                    },
                };
            }
        };

        let event = DomainEvent::MainTokenGenesisInitialized {
            total_supply: self.state.main_token_config.initial_supply,
            allocations: resolved_allocations,
        };
        if let Err(err) =
            super::super::main_token_monetary_publication::PreparedMainTokenMonetaryEvent::prepare(
                &self.state,
                &event,
                self.state.time,
            )
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("initialize main token genesis rejected: {err:?}")],
                },
            };
        }
        event
    }

    pub(super) fn evaluate_claim_main_token_vesting_action(
        &self,
        action_id: ActionId,
        bucket_id: &str,
        beneficiary: &str,
        nonce: u64,
    ) -> DomainEvent {
        if bucket_id.trim().is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["bucket_id cannot be empty".to_string()],
                },
            };
        }
        if beneficiary.trim().is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["beneficiary cannot be empty".to_string()],
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
        let Some(bucket) = self.state.main_token_genesis_buckets.get(bucket_id) else {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("genesis bucket not found: {bucket_id}")],
                },
            };
        };
        if bucket.recipient != beneficiary {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "beneficiary mismatch: bucket recipient={} claim beneficiary={}",
                        bucket.recipient, beneficiary
                    )],
                },
            };
        }
        let unlocked = main_token_bucket_unlocked_amount(bucket, self.state.time);
        let releasable = unlocked.saturating_sub(bucket.claimed_amount);
        if releasable == 0 {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "no releasable vesting balance for bucket={} at epoch={}",
                        bucket_id, self.state.time
                    )],
                },
            };
        }

        let event = DomainEvent::MainTokenVestingClaimed {
            bucket_id: bucket_id.to_string(),
            beneficiary: beneficiary.to_string(),
            amount: releasable,
            nonce,
        };
        if let Err(err) =
            super::super::main_token_monetary_publication::PreparedMainTokenMonetaryEvent::prepare(
                &self.state,
                &event,
                self.state.time,
            )
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("claim main token vesting rejected: {err:?}")],
                },
            };
        }
        event
    }

    fn resolve_main_token_effective_config_for_epoch(&self, epoch_index: u64) -> &MainTokenConfig {
        self.state
            .main_token_scheduled_policy_updates
            .range(..=epoch_index)
            .next_back()
            .map(|(_, item)| &item.next_config)
            .unwrap_or(&self.state.main_token_config)
    }

    fn resolve_main_token_effective_rate_bps(
        &self,
        config: &MainTokenConfig,
        actual_stake_ratio_bps: u32,
    ) -> Result<u32, String> {
        if actual_stake_ratio_bps > MAIN_TOKEN_BPS_DENOMINATOR {
            return Err(format!(
                "actual_stake_ratio_bps must be <= 10000, got {}",
                actual_stake_ratio_bps
            ));
        }
        let policy = &config.inflation_policy;
        if policy.epochs_per_year == 0 {
            return Err("inflation_policy.epochs_per_year must be > 0".to_string());
        }
        if policy.min_rate_bps > policy.max_rate_bps {
            return Err(format!(
                "inflation_policy min_rate_bps > max_rate_bps: {} > {}",
                policy.min_rate_bps, policy.max_rate_bps
            ));
        }
        let target = i128::from(policy.target_stake_ratio_bps);
        let actual = i128::from(actual_stake_ratio_bps);
        let gain = i128::from(policy.stake_feedback_gain_bps);
        let base = i128::from(policy.base_rate_bps);
        let feedback = target
            .saturating_sub(actual)
            .saturating_mul(gain)
            .saturating_div(i128::from(MAIN_TOKEN_BPS_DENOMINATOR));
        let rate = base.saturating_add(feedback);
        let clamped = rate.clamp(
            i128::from(policy.min_rate_bps),
            i128::from(policy.max_rate_bps),
        );
        u32::try_from(clamped)
            .map_err(|_| format!("effective inflation rate out of range: {clamped}"))
    }

    fn resolve_main_token_epoch_issued_amount(
        &self,
        config: &MainTokenConfig,
        inflation_rate_bps: u32,
    ) -> Result<u64, String> {
        let supply = &self.state.main_token_supply;
        let policy = &config.inflation_policy;
        if policy.epochs_per_year == 0 {
            return Err("inflation_policy.epochs_per_year must be > 0".to_string());
        }
        let numerator = u128::from(supply.circulating_supply)
            .checked_mul(u128::from(inflation_rate_bps))
            .ok_or_else(|| {
                format!(
                    "main token issuance overflow: circulating={} rate_bps={}",
                    supply.circulating_supply, inflation_rate_bps
                )
            })?;
        let denominator = u128::from(policy.epochs_per_year)
            .saturating_mul(u128::from(MAIN_TOKEN_BPS_DENOMINATOR));
        if denominator == 0 {
            return Err("main token issuance denominator cannot be zero".to_string());
        }
        let mut issued = u64::try_from(numerator / denominator).map_err(|_| {
            "main token issuance amount conversion overflow while converting to u64".to_string()
        })?;

        if let Some(max_supply) = config.max_supply {
            if supply.total_supply > max_supply {
                return Err(format!(
                    "main token total_supply already exceeds max_supply: total={} max={}",
                    supply.total_supply, max_supply
                ));
            }
            let remaining = max_supply.saturating_sub(supply.total_supply);
            issued = issued.min(remaining);
        }
        Ok(issued)
    }

    fn resolve_main_token_epoch_split_amounts(
        &self,
        config: &MainTokenConfig,
        issued_amount: u64,
    ) -> Result<(u64, u64, u64, u64), String> {
        let split = &config.issuance_split;
        let split_sum = u64::from(split.staking_reward_bps)
            .saturating_add(u64::from(split.node_service_reward_bps))
            .saturating_add(u64::from(split.ecosystem_pool_bps))
            .saturating_add(u64::from(split.security_reserve_bps));
        if split_sum != u64::from(MAIN_TOKEN_BPS_DENOMINATOR) {
            return Err(format!(
                "main token issuance split sum must be 10000 bps, got {}",
                split_sum
            ));
        }

        let staking_reward_amount = issued_amount
            .saturating_mul(u64::from(split.staking_reward_bps))
            / u64::from(MAIN_TOKEN_BPS_DENOMINATOR);
        let node_service_reward_amount = issued_amount
            .saturating_mul(u64::from(split.node_service_reward_bps))
            / u64::from(MAIN_TOKEN_BPS_DENOMINATOR);
        let ecosystem_pool_amount = issued_amount
            .saturating_mul(u64::from(split.ecosystem_pool_bps))
            / u64::from(MAIN_TOKEN_BPS_DENOMINATOR);
        let distributed = staking_reward_amount
            .checked_add(node_service_reward_amount)
            .and_then(|value| value.checked_add(ecosystem_pool_amount))
            .ok_or_else(|| {
                format!(
                    "main token issuance split overflow: issued={} staking={} node_service={} ecosystem={}",
                    issued_amount,
                    staking_reward_amount,
                    node_service_reward_amount,
                    ecosystem_pool_amount
                )
            })?;
        let security_reserve_amount = issued_amount.saturating_sub(distributed);
        Ok((
            staking_reward_amount,
            node_service_reward_amount,
            ecosystem_pool_amount,
            security_reserve_amount,
        ))
    }

    pub(super) fn evaluate_apply_main_token_epoch_issuance_action(
        &self,
        action_id: ActionId,
        epoch_index: u64,
        actual_stake_ratio_bps: u32,
    ) -> DomainEvent {
        if self.state.main_token_genesis_buckets.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["main token genesis is not initialized".to_string()],
                },
            };
        }
        if self
            .state
            .main_token_epoch_issuance_records
            .contains_key(&epoch_index)
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "main token epoch issuance already exists: epoch={epoch_index}"
                    )],
                },
            };
        }

        let effective_config = self.resolve_main_token_effective_config_for_epoch(epoch_index);
        let inflation_rate_bps = match self
            .resolve_main_token_effective_rate_bps(effective_config, actual_stake_ratio_bps)
        {
            Ok(value) => value,
            Err(reason) => {
                return DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("apply epoch issuance rejected: {reason}")],
                    },
                };
            }
        };
        let issued_amount = match self
            .resolve_main_token_epoch_issued_amount(effective_config, inflation_rate_bps)
        {
            Ok(value) => value,
            Err(reason) => {
                return DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("apply epoch issuance rejected: {reason}")],
                    },
                };
            }
        };
        let (
            staking_reward_amount,
            node_service_reward_amount,
            ecosystem_pool_amount,
            security_reserve_amount,
        ) = match self.resolve_main_token_epoch_split_amounts(effective_config, issued_amount) {
            Ok(values) => values,
            Err(reason) => {
                return DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("apply epoch issuance rejected: {reason}")],
                    },
                };
            }
        };

        let event = DomainEvent::MainTokenEpochIssued {
            epoch_index,
            inflation_rate_bps,
            issued_amount,
            staking_reward_amount,
            node_service_reward_amount,
            ecosystem_pool_amount,
            security_reserve_amount,
        };
        if let Err(err) =
            super::super::main_token_monetary_publication::PreparedMainTokenMonetaryEvent::prepare(
                &self.state,
                &event,
                self.state.time,
            )
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("apply epoch issuance rejected: {err:?}")],
                },
            };
        }
        event
    }

    fn resolve_main_token_fee_burn_bps(
        &self,
        config: &MainTokenConfig,
        fee_kind: MainTokenFeeKind,
    ) -> u32 {
        let policy = &config.burn_policy;
        match fee_kind {
            MainTokenFeeKind::GasBaseFee => policy.gas_base_fee_burn_bps,
            MainTokenFeeKind::SlashPenalty => policy.slash_burn_bps,
            MainTokenFeeKind::ModuleFee => policy.module_fee_burn_bps,
        }
    }

    pub(super) fn evaluate_settle_main_token_fee_action(
        &self,
        action_id: ActionId,
        fee_kind: MainTokenFeeKind,
        amount: u64,
    ) -> DomainEvent {
        if self.state.main_token_genesis_buckets.is_empty() {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["main token genesis is not initialized".to_string()],
                },
            };
        }
        if amount == 0 {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["main token fee amount must be > 0".to_string()],
                },
            };
        }
        let effective_config = self.resolve_main_token_effective_config_for_epoch(self.state.time);
        let burn_bps = self.resolve_main_token_fee_burn_bps(effective_config, fee_kind);
        if burn_bps > MAIN_TOKEN_BPS_DENOMINATOR {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "main token burn bps must be <= 10000, got {}",
                        burn_bps
                    )],
                },
            };
        }
        let burn_amount =
            amount.saturating_mul(u64::from(burn_bps)) / u64::from(MAIN_TOKEN_BPS_DENOMINATOR);
        let treasury_amount = amount.saturating_sub(burn_amount);

        let event = DomainEvent::MainTokenFeeSettled {
            fee_kind,
            amount,
            burn_amount,
            treasury_amount,
        };
        if let Err(err) =
            super::super::main_token_monetary_publication::PreparedMainTokenMonetaryEvent::prepare(
                &self.state,
                &event,
                self.state.time,
            )
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("settle main token fee rejected: {err:?}")],
                },
            };
        }
        event
    }

    pub(super) fn evaluate_update_main_token_policy_action(
        &self,
        action_id: ActionId,
        proposal_id: ProposalId,
        next: &MainTokenConfig,
    ) -> DomainEvent {
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
                        "update main token policy rejected: governance proposal not found ({proposal_id})"
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
                            "update main token policy rejected: governance proposal must be approved or applied ({proposal_id})"
                        )],
                    },
                };
            }
        }
        if let Err(reason) = validate_main_token_config_bounds(next) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("update main token policy rejected: {reason}")],
                },
            };
        }
        if next.initial_supply != self.state.main_token_config.initial_supply {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "update main token policy rejected: initial_supply cannot change (current={} next={})",
                        self.state.main_token_config.initial_supply, next.initial_supply
                    )],
                },
            };
        }
        if let Some(max_supply) = next.max_supply
            && max_supply < self.state.main_token_supply.total_supply
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "update main token policy rejected: max_supply cannot be below total_supply (max={} total={})",
                        max_supply, self.state.main_token_supply.total_supply
                    )],
                },
            };
        }

        let effective_epoch = self
            .state
            .time
            .saturating_add(MAIN_TOKEN_POLICY_UPDATE_DELAY_EPOCHS);
        if self
            .state
            .main_token_scheduled_policy_updates
            .contains_key(&effective_epoch)
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "update main token policy rejected: effective_epoch already scheduled ({effective_epoch})"
                    )],
                },
            };
        }
        if self
            .state
            .main_token_scheduled_policy_updates
            .values()
            .any(|item| item.proposal_id == proposal_id)
        {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "update main token policy rejected: proposal already scheduled ({proposal_id})"
                    )],
                },
            };
        }

        let event = DomainEvent::MainTokenPolicyUpdateScheduled {
            proposal_id,
            effective_epoch,
            next: next.clone(),
        };
        if let Err(err) = super::super::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent::prepare(
            &self.state,
            &event,
            self.state.time,
        ) {
            return DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("update main token policy rejected: {err:?}")],
                },
            };
        }
        event
    }
}

fn distribute_main_token_bridge_budget(
    total_budget: u64,
    settlements: &[NodeSettlement],
) -> (u64, Vec<MainTokenNodePointsBridgeDistribution>) {
    if total_budget == 0 || settlements.is_empty() {
        return (0, Vec::new());
    }
    let total_points = settlements
        .iter()
        .map(|settlement| settlement.awarded_points)
        .sum::<u64>();
    if total_points == 0 {
        return (0, Vec::new());
    }

    let mut distributions = Vec::with_capacity(settlements.len());
    let mut distributed = 0_u64;
    for settlement in settlements {
        let amount_u128 = u128::from(total_budget)
            .saturating_mul(u128::from(settlement.awarded_points))
            / u128::from(total_points);
        let amount = u64::try_from(amount_u128).unwrap_or(u64::MAX);
        distributed = distributed.saturating_add(amount);
        distributions.push((
            settlement.awarded_points,
            MainTokenNodePointsBridgeDistribution {
                node_id: settlement.node_id.clone(),
                account_id: settlement.node_id.clone(),
                amount,
            },
        ));
    }

    let mut remainder = total_budget.saturating_sub(distributed);
    distributions.sort_by(|(left_points, left), (right_points, right)| {
        right_points
            .cmp(left_points)
            .then_with(|| left.node_id.cmp(&right.node_id))
    });
    let mut index = 0_usize;
    while remainder > 0 && !distributions.is_empty() {
        let target = index % distributions.len();
        distributions[target].1.amount = distributions[target].1.amount.saturating_add(1);
        remainder -= 1;
        index = index.saturating_add(1);
    }

    let mut distributions = distributions
        .into_iter()
        .map(|(_, distribution)| distribution)
        .collect::<Vec<_>>();
    distributions.retain(|item| item.amount > 0);
    distributions.sort_by(|left, right| left.node_id.cmp(&right.node_id));
    (total_budget, distributions)
}
