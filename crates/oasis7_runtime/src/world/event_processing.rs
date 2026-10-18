use super::super::capability_authorization::CapabilityInvocationContext;
use super::super::{
    Action, ActionEnvelope, ActionId, CausedBy, CrisisStatus, DomainEvent, EconomicContractStatus,
    EpochSettlementReport, GovernanceEvent, GovernanceIdentityPenaltyRecord,
    GovernanceIdentityProfileState, GovernanceProposalStatus, MainTokenConfig, MainTokenFeeKind,
    MainTokenGenesisAllocationBucketState, MainTokenGenesisAllocationPlan,
    MainTokenNodePointsBridgeDistribution, MaterialLedgerId, MaterialStack, NodeRewardMintRecord,
    NodeSettlement, ProposalId, ProposalStatus, RejectReason, TickConsensusRecord, WorldError,
    WorldEvent, WorldEventBody, WorldEventId, WorldTime, main_token_bucket_unlocked_amount,
    util::hash_json,
};
use super::World;
use super::body::{
    PreparedBodyAttributesUpdate, evaluate_expand_body_interface, validate_body_kernel_view,
};
use super::logistics::{
    MATERIAL_TRANSFER_MAX_DISTANCE_KM, MATERIAL_TRANSFER_MAX_INFLIGHT,
    material_transit_loss_bps_for_kind, material_transit_priority_for_kind, material_transit_ticks,
};
use oasis7_agent_api::world_geometry::space_distance_cm;
use crate::main_token::{
    MAIN_TOKEN_BPS_DENOMINATOR, MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
    MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD, main_token_account_id_from_node_public_key,
    validate_main_token_config_bounds,
};
use oasis7_agent_api::world_action::ResourceKind;
use std::collections::BTreeSet;

const GOVERNANCE_MIN_VOTING_WINDOW_TICKS: u64 = 1;
const GOVERNANCE_MAX_VOTING_WINDOW_TICKS: u64 = 1_440;
const GOVERNANCE_MIN_PASS_THRESHOLD_BPS: u16 = 5_000;
const GOVERNANCE_MAX_PASS_THRESHOLD_BPS: u16 = 10_000;
const GOVERNANCE_MAX_VOTE_WEIGHT: u32 = 100;
const WAR_MAX_INTENSITY: u32 = 10;
const WAR_MIN_ALLIANCE_MEMBERS: usize = 2;
const WAR_MAX_ALLIANCE_MEMBERS: usize = 16;
const WAR_DECLARE_BASE_ELECTRICITY_COST: i64 = 12;
const WAR_DECLARE_ELECTRICITY_COST_PER_INTENSITY: i64 = 4;
const WAR_DECLARE_BASE_DATA_COST: i64 = 8;
const WAR_DECLARE_DATA_COST_PER_INTENSITY: i64 = 3;
const CRISIS_BASE_IMPACT_PER_SEVERITY: i64 = 10;
const GAMEPLAY_POLICY_MAX_TAX_BPS: u16 = 10_000;
const GAMEPLAY_POLICY_MIN_CONTRACT_QUOTA: u16 = 1;
const GAMEPLAY_POLICY_MAX_CONTRACT_QUOTA: u16 = 64;
const GAMEPLAY_POLICY_UPDATE_MIN_GOVERNANCE_TOTAL_WEIGHT: u64 = 3;
const ECONOMIC_CONTRACT_MAX_REPUTATION_STAKE: i64 = 10_000;
const ECONOMIC_CONTRACT_SUCCESS_REPUTATION_AMOUNT_DIVISOR: i64 = 10;
const ECONOMIC_CONTRACT_SUCCESS_REPUTATION_REWARD_CAP: i64 = 12;
const ECONOMIC_CONTRACT_PAIR_COOLDOWN_TICKS: u64 = 5;
const ECONOMIC_CONTRACT_REPUTATION_WINDOW_TICKS: u64 = 20;
const ECONOMIC_CONTRACT_REPUTATION_WINDOW_CAP: i64 = 24;
const MAIN_TOKEN_POLICY_UPDATE_DELAY_EPOCHS: u64 = 2;
const STARTER_OC_CLAIM_AMOUNT: u64 = 100_000_000;

mod action_to_event_core;
pub(super) mod action_to_event_economy;
mod action_to_event_gameplay;
mod action_to_event_gameplay_meta;
mod action_to_event_policy_contract;
mod action_to_event_policy_contract_rejection;
mod core_policy_root;
mod main_token;
pub(super) mod prepared_governance_events;
mod publication;

use publication::PreparedEventStateDelta;

impl World {
    // ---------------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------------

    fn agent_alliance_id(&self, agent_id: &str) -> Option<&str> {
        self.state
            .alliances
            .iter()
            .find(|(_, alliance)| alliance.members.iter().any(|member| member == agent_id))
            .map(|(alliance_id, _)| alliance_id.as_str())
    }

    fn alliance_has_active_war(&self, alliance_id: &str) -> bool {
        self.state.wars.values().any(|war| {
            war.active
                && (war.aggressor_alliance_id == alliance_id
                    || war.defender_alliance_id == alliance_id)
        })
    }

    fn has_policy_update_governance_authorization(&self, operator_agent_id: &str) -> bool {
        self.state.governance_proposals.values().any(|proposal| {
            proposal.proposer_agent_id == operator_agent_id
                && proposal.status == GovernanceProposalStatus::Passed
                && proposal.total_weight_at_finalize
                    >= GAMEPLAY_POLICY_UPDATE_MIN_GOVERNANCE_TOTAL_WEIGHT
        })
    }

    pub(super) fn war_mobilization_costs(intensity: u32) -> (i64, i64) {
        let intensity = i64::from(intensity.max(1));
        let electricity = WAR_DECLARE_BASE_ELECTRICITY_COST
            .saturating_add(intensity.saturating_mul(WAR_DECLARE_ELECTRICITY_COST_PER_INTENSITY));
        let data = WAR_DECLARE_BASE_DATA_COST
            .saturating_add(intensity.saturating_mul(WAR_DECLARE_DATA_COST_PER_INTENSITY));
        (electricity, data)
    }

    fn economic_contract_success_reputation_reward(
        settlement_amount: i64,
        reputation_stake: i64,
    ) -> i64 {
        let amount_based_reward = settlement_amount
            .saturating_div(ECONOMIC_CONTRACT_SUCCESS_REPUTATION_AMOUNT_DIVISOR)
            .max(1);
        amount_based_reward
            .min(reputation_stake)
            .min(ECONOMIC_CONTRACT_SUCCESS_REPUTATION_REWARD_CAP)
    }

    /// A context remains pending until its nonce and accepted receipt form a
    /// complete durable lifecycle record.  Contexts are intentionally kept in
    /// the snapshot after completion for audit/replay binding, so their mere
    /// presence cannot be used as evidence of an interrupted command.
    fn capability_context_has_durable_completion(
        &self,
        context: &CapabilityInvocationContext,
    ) -> bool {
        let Some(encoded_grant) = self.capability_grants_v2.get(&context.grant_id) else {
            return false;
        };
        let Ok(grant) =
            serde_json::from_value::<oasis7_wasm_abi::CapabilityGrantV2>(encoded_grant.clone())
        else {
            return false;
        };
        let Ok(nonce_key) = super::capability_authorization_events::authorization_nonce_key(
            &grant,
            context.response_nonce.as_str(),
        ) else {
            return false;
        };
        let Some(nonce_record) = self.capability_nonce_records.get(&nonce_key) else {
            return false;
        };
        if nonce_record.state != "committed" {
            return false;
        }
        let Some(receipt_id) = nonce_record.committed_receipt_id.as_deref() else {
            return false;
        };
        let Some(receipt) = self.capability_authorization_receipts.get(receipt_id) else {
            return false;
        };
        let Ok(subject) = serde_json::to_value(&context.subject) else {
            return false;
        };
        let Ok(presenter) = serde_json::to_value(&context.presenter) else {
            return false;
        };
        let Ok(audience) = serde_json::to_value(&context.audience) else {
            return false;
        };
        receipt.decision == "accepted"
            && receipt.grant_id.as_deref() == Some(context.grant_id.as_str())
            && receipt.response_nonce.as_deref() == Some(context.response_nonce.as_str())
            && receipt.authorization_nonce_key_hash.as_deref() == Some(nonce_key.as_str())
            && receipt.subject == subject
            && receipt.presenter.as_ref() == Some(&presenter)
            && receipt.audience == audience
            && receipt.catalog_snapshot_id.as_deref() == Some(context.catalog_snapshot_id.as_str())
            && receipt.module_id.as_deref() == Some(context.module_id.as_str())
            && receipt.module_version.as_deref() == Some(context.module_version.as_str())
            && nonce_record.request_hash == receipt.canonical_request_hash
            && nonce_record.outcome_hash == receipt.canonical_result_hash
    }

    pub(super) fn replay_from(&mut self, start_index: usize) -> Result<(), WorldError> {
        let start_index = start_index.min(self.journal.events.len());
        let events: Vec<WorldEvent> = self.journal.events[start_index..].to_vec();
        let mut replaying_tick: Option<WorldTime> = None;
        let mut previous_event_time = self.state.time;
        // A stale snapshot can contain historical receipts and more than one
        // pending invocation context.  Recovery must not use either of those
        // facts as a global gate: a state update for one nonce needs its own
        // command commit, even when another nonce already has a receipt or a
        // commit in the same journal tail.  Keep the identity pair rather than
        // the hashed map key so this also covers legacy grant-id map entries.
        let mut pending_capability_contexts: BTreeSet<(String, String)> = self
            .capability_invocation_contexts
            .values()
            .filter(|context| !self.capability_context_has_durable_completion(context))
            .map(|context| (context.grant_id.clone(), context.response_nonce.clone()))
            .collect();
        let mut capability_command_activity: BTreeSet<(String, String)> = BTreeSet::new();
        let mut capability_command_commits: BTreeSet<(String, String)> = BTreeSet::new();
        let mut replayed_intent_sequences: BTreeSet<u64> = BTreeSet::new();
        for event in events {
            if event.time < previous_event_time {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: format!(
                        "journal event time {} cannot precede prior event time {}",
                        event.time, previous_event_time
                    ),
                });
            }
            if let Some(tick) = replaying_tick
                && event.time != tick
            {
                self.record_tick_consensus_for_tick(tick)?;
            }
            match &event.body {
                WorldEventBody::PolicyDecisionRecorded(record) => {
                    self.reconcile_replayed_intent_allocator(
                        record.intent_id.as_str(),
                        &mut replayed_intent_sequences,
                    );
                }
                WorldEventBody::ModuleStateUpdated(update) => {
                    // ModuleStateUpdated is also emitted by ordinary module
                    // ticks and commands.  Only a state update carrying the
                    // trace/module identity of a pending trusted invocation
                    // context indicates a capability command that may have
                    // crashed before its CommandCommitted journal event.
                    for context in self.capability_invocation_contexts.values() {
                        let identity = (context.grant_id.clone(), context.response_nonce.clone());
                        if pending_capability_contexts.contains(&identity)
                            && context.module_id == update.module_id
                            && update.trace_id
                                == format!("trusted-command-{}", context.response_nonce)
                        {
                            capability_command_activity.insert(identity);
                            break;
                        }
                    }
                }
                WorldEventBody::ModuleRuntimeCharged(charge) => {
                    // Trusted commands charge runtime resources before their
                    // output events.  Treat that charge as command activity
                    // so an effect-only or emit-only tail cannot be replayed
                    // without the following CommandCommitted event.
                    for context in self.capability_invocation_contexts.values() {
                        let identity = (context.grant_id.clone(), context.response_nonce.clone());
                        if pending_capability_contexts.contains(&identity)
                            && context.module_id == charge.module_id
                            && charge.trace_id
                                == format!("trusted-command-{}", context.response_nonce)
                        {
                            capability_command_activity.insert(identity);
                            break;
                        }
                    }
                }
                WorldEventBody::EffectQueued(intent) => {
                    self.reconcile_replayed_intent_allocator(
                        intent.intent_id.as_str(),
                        &mut replayed_intent_sequences,
                    );
                    // EffectIntent does not carry the command nonce, so use
                    // the durable v2 effect grant and its audience/subject
                    // binding to distinguish a trusted command effect from a
                    // legacy or unrelated module effect.  This covers hosts
                    // without an artifact owner (and therefore without a
                    // ModuleRuntimeCharged event).
                    if let super::super::EffectOrigin::Module { module_id } = &intent.origin
                        && let Some(encoded_grant) = self.capability_grants_v2.get(&intent.cap_ref)
                        && let Ok(effect_grant) = serde_json::from_value::<
                            oasis7_wasm_abi::CapabilityGrantV2,
                        >(encoded_grant.clone())
                    {
                        for context in self.capability_invocation_contexts.values() {
                            let identity =
                                (context.grant_id.clone(), context.response_nonce.clone());
                            if pending_capability_contexts.contains(&identity)
                                && module_id == &context.module_id
                                && effect_grant.scope.object_kind == "effect"
                                && effect_grant.scope.module_id == context.module_id
                                && effect_grant.subject == context.subject
                                && effect_grant.audience == context.audience
                            {
                                capability_command_activity.insert(identity);
                                break;
                            }
                        }
                    }
                }
                WorldEventBody::ModuleEmitted(event) => {
                    // ModuleEmitted carries the same host-bound trusted trace
                    // as state updates.  It is therefore independently
                    // sufficient to identify a command tail that still needs
                    // its durable authorization commit.
                    for context in self.capability_invocation_contexts.values() {
                        let identity = (context.grant_id.clone(), context.response_nonce.clone());
                        if pending_capability_contexts.contains(&identity)
                            && context.module_id == event.module_id
                            && event.trace_id
                                == format!("trusted-command-{}", context.response_nonce)
                        {
                            capability_command_activity.insert(identity);
                            break;
                        }
                    }
                }
                WorldEventBody::CapabilityAuthorization(
                    super::super::CapabilityAuthorizationEvent::CommandCommitted {
                        grant,
                        receipt,
                        ..
                    },
                ) => {
                    if let Some(response_nonce) = receipt.response_nonce.as_deref() {
                        let identity = (grant.grant_id.clone(), response_nonce.to_string());
                        if pending_capability_contexts.contains(&identity) {
                            capability_command_commits.insert(identity);
                        }
                    }
                }
                WorldEventBody::CapabilityAuthorization(
                    super::super::CapabilityAuthorizationEvent::InvocationContextInstalled {
                        context,
                        ..
                    },
                ) => {
                    // A snapshot may precede the context-install event itself.
                    // Include that context once its durable installation is
                    // replayed so a later state update cannot bypass the
                    // per-context crash-window check.
                    let identity = (context.grant_id.clone(), context.response_nonce.clone());
                    if !self.capability_context_has_durable_completion(context) {
                        pending_capability_contexts.insert(identity);
                    }
                }
                _ => {}
            }
            self.apply_event_body_at(&event.body, event.time, Some(event.id))?;
            self.state.time = event.time;
            let (expected_event_id, next_event_id, next_event_id_era) =
                Self::preview_next_event_id(self.next_event_id, self.next_event_id_era);
            if expected_event_id == event.id {
                self.next_event_id = next_event_id;
                self.next_event_id_era = next_event_id_era;
            } else {
                // Legacy snapshots did not persist enough allocator context
                // to prove the first event in a tail.  Preserve the old
                // monotonic recovery fallback, while still advancing the era
                // when a replayed event is the rolling maximum.
                self.next_event_id = event.id.saturating_add(1).max(1);
                if event.id == u64::MAX {
                    self.next_event_id_era = self.next_event_id_era.saturating_add(1);
                }
            }
            replaying_tick = Some(event.time);
            previous_event_time = event.time;
        }
        if let Some(tick) = replaying_tick {
            self.record_tick_consensus_for_tick(tick)?;
        }
        if capability_command_activity
            .iter()
            .any(|identity| !capability_command_commits.contains(identity))
        {
            return Err(WorldError::JournalMismatch);
        }
        Ok(())
    }

    pub(super) fn action_to_event(
        &self,
        envelope: &ActionEnvelope,
    ) -> Result<WorldEventBody, WorldError> {
        let action_id = envelope.id;
        if let Err(error) = self.delegation_action_admission(&envelope.action) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("{error:?}")],
                },
            }));
        }
        match &envelope.action {
            Action::WorldServiceIntent { .. } => Err(WorldError::DistributedValidationFailed {
                reason: "world-service intent requires canonical driver admission".into(),
            }),
            Action::RegisterAgent { .. }
            | Action::MoveAgent { .. }
            | Action::QueryObservation { .. }
            | Action::EmitObservation { .. }
            | Action::BodyAction { .. }
            | Action::EmitBodyAttributes { .. }
            | Action::ExpandBodyInterface { .. }
            | Action::DeployModuleArtifact { .. }
            | Action::CompileModuleArtifactFromSource { .. }
            | Action::InstallModuleFromArtifact { .. }
            | Action::InstallModuleFromArtifactWithFinality { .. }
            | Action::InstallModuleToTargetFromArtifact { .. }
            | Action::InstallModuleToTargetFromArtifactWithFinality { .. }
            | Action::UpgradeModuleFromArtifact { .. }
            | Action::UpgradeModuleFromArtifactWithFinality { .. }
            | Action::RollbackModuleInstance { .. }
            | Action::RollbackModuleInstanceWithFinality { .. }
            | Action::ModuleReleaseSubmit { .. }
            | Action::ModuleReleaseShadow { .. }
            | Action::ModuleReleaseApproveRole { .. }
            | Action::ModuleReleaseBindRoles { .. }
            | Action::ModuleReleaseSubmitAttestation { .. }
            | Action::ModuleReleaseReject { .. }
            | Action::ModuleReleaseApply { .. }
            | Action::ModuleReleaseApplyWithFinality { .. }
            | Action::ListModuleArtifactForSale { .. }
            | Action::BuyModuleArtifact { .. }
            | Action::DelistModuleArtifact { .. }
            | Action::DestroyModuleArtifact { .. }
            | Action::PlaceModuleArtifactBid { .. }
            | Action::CancelModuleArtifactBid { .. }
            | Action::TransferResource { .. }
            | Action::RedeemPower { .. }
            | Action::RedeemPowerSigned { .. }
            | Action::ApplyNodePointsSettlementSigned { .. }
            | Action::InitializeMainTokenGenesis { .. }
            | Action::ClaimMainTokenVesting { .. }
            | Action::TransferMainToken { .. }
            | Action::ApplyMainTokenEpochIssuance { .. }
            | Action::SettleMainTokenFee { .. }
            | Action::UpdateMainTokenPolicy { .. }
            | Action::DistributeMainTokenTreasury { .. }
            | Action::TopUpRestrictedStarterClaimLiveopsPool { .. }
            | Action::IssueRestrictedStarterClaimGrant { .. }
            | Action::RevokeRestrictedStarterClaimGrant { .. }
            | Action::RegisterLogisticsRoute { .. }
            | Action::SetLogisticsRouteAvailability { .. }
            | Action::TransferMaterial { .. } => {
                self.action_to_event_core(action_id, &envelope.action)
            }
            Action::FormAlliance { .. }
            | Action::JoinAlliance { .. }
            | Action::LeaveAlliance { .. }
            | Action::DissolveAlliance { .. }
            | Action::DeclareWar { .. }
            | Action::OpenGovernanceProposal { .. }
            | Action::CastGovernanceVote { .. }
            | Action::ResolveCrisis { .. }
            | Action::GrantMetaProgress { .. } => {
                self.action_to_event_gameplay(action_id, &envelope.action)
            }
            Action::ClaimAgent { .. }
            | Action::ClaimStarterOc { .. }
            | Action::ReleaseAgentClaim { .. } => {
                self.action_to_event_gameplay(action_id, &envelope.action)
            }
            Action::UpdateGameplayPolicy { .. }
            | Action::UpdateRestrictedStarterClaimAdminRegistry { .. }
            | Action::SubmitGovernanceValidatorAdmission { .. }
            | Action::ApproveGovernanceValidatorAdmission { .. }
            | Action::ActivateGovernanceValidatorAdmission { .. }
            | Action::RevokeGovernanceValidatorAdmission { .. }
            | Action::OpenEconomicContract { .. }
            | Action::AcceptEconomicContract { .. }
            | Action::SettleEconomicContract { .. } => {
                self.action_to_event_policy_contract(action_id, &envelope.action)
            }
            Action::EmitResourceTransfer { .. }
            | Action::CollectData { .. }
            | Action::CollectDataAuthenticated { .. }
            | Action::GrantDataAccess { .. }
            | Action::RevokeDataAccess { .. }
            | Action::BuildFactory { .. }
            | Action::BuildFactoryWithModule { .. }
            | Action::MaintainFactory { .. }
            | Action::RecycleFactory { .. }
            | Action::ScheduleRecipe { .. }
            | Action::PauseFactoryProduction { .. }
            | Action::ScheduleRecipeWithModule { .. }
            | Action::ValidateProduct { .. }
            | Action::ValidateProductWithModule { .. }
            | Action::GovernMaterialProfile { .. }
            | Action::GovernProductProfile { .. }
            | Action::GovernRecipeProfile { .. }
            | Action::GovernFactoryProfile { .. } => {
                let mut body = self.action_to_event_economy(action_id, &envelope.action)?;
                if let WorldEventBody::Domain(DomainEvent::RecipeStarted {
                    committed_recipe_origin,
                    ..
                }) = &mut body
                {
                    *committed_recipe_origin = envelope.committed_recipe_origin.clone();
                }
                Ok(body)
            }
        }
    }

    fn select_material_consume_ledger_with_world_fallback(
        &self,
        preferred_ledger: MaterialLedgerId,
        consume: &[MaterialStack],
    ) -> MaterialLedgerId {
        if self.has_materials_in_ledger(&preferred_ledger, consume) {
            preferred_ledger
        } else {
            MaterialLedgerId::world()
        }
    }

    fn evaluate_redeem_power_action(
        &self,
        node_id: &str,
        target_agent_id: &str,
        redeem_credits: u64,
        nonce: u64,
        signed: Option<(&str, &str)>,
    ) -> DomainEvent {
        if self
            .state
            .reward_signature_governance_policy
            .require_redeem_signature
            && signed.is_none()
        {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                "redeem signature is required by governance policy".to_string(),
            );
        }
        if let Some((signer_node_id, signature)) = signed {
            if signer_node_id.trim().is_empty() {
                return self.power_redeem_rejected(
                    node_id,
                    target_agent_id,
                    redeem_credits,
                    nonce,
                    "signer_node_id cannot be empty".to_string(),
                );
            }
            if signature.trim().is_empty() {
                return self.power_redeem_rejected(
                    node_id,
                    target_agent_id,
                    redeem_credits,
                    nonce,
                    "redeem signature cannot be empty".to_string(),
                );
            }
            if self
                .state
                .reward_signature_governance_policy
                .require_redeem_signer_match_node_id
                && signer_node_id != node_id
            {
                return self.power_redeem_rejected(
                    node_id,
                    target_agent_id,
                    redeem_credits,
                    nonce,
                    format!(
                        "redeem signer_node_id must match node_id by governance policy: signer={} node={}",
                        signer_node_id, node_id
                    ),
                );
            }
            if let Err(reason) = self.verify_redeem_power_signature(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                signer_node_id,
                signature,
            ) {
                return self.power_redeem_rejected(
                    node_id,
                    target_agent_id,
                    redeem_credits,
                    nonce,
                    format!("redeem signature verification failed: {reason}"),
                );
            }
        }

        if node_id.trim().is_empty() {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                "node_id cannot be empty".to_string(),
            );
        }
        if !self.state.node_identity_bindings.contains_key(node_id) {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                format!("node identity not bound: {node_id}"),
            );
        }
        if !self.state.agents.contains_key(target_agent_id) {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                format!("target agent not found: {target_agent_id}"),
            );
        }
        if redeem_credits == 0 {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                "redeem_credits must be > 0".to_string(),
            );
        }
        if nonce == 0 {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                "nonce must be > 0".to_string(),
            );
        }
        if let Some(last_nonce) = self.state.node_redeem_nonces.get(node_id)
            && nonce <= *last_nonce
        {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                format!(
                    "nonce replay detected: nonce={} last_nonce={}",
                    nonce, last_nonce
                ),
            );
        }
        let credits_per_power_unit = self.state.reward_asset_config.credits_per_power_unit;
        if credits_per_power_unit == 0 {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                "credits_per_power_unit must be positive".to_string(),
            );
        }
        let granted_power_units_u64 = redeem_credits / credits_per_power_unit;
        if granted_power_units_u64 == 0 {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                format!(
                    "redeem credits below minimum conversion: credits={} per_unit={}",
                    redeem_credits, credits_per_power_unit
                ),
            );
        }
        if granted_power_units_u64 > i64::MAX as u64 {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                "granted power units overflow".to_string(),
            );
        }
        let granted_power_units = granted_power_units_u64 as i64;
        let min_redeem_power_unit = self.state.reward_asset_config.min_redeem_power_unit;
        if min_redeem_power_unit <= 0 {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                "min_redeem_power_unit must be positive".to_string(),
            );
        }
        if granted_power_units < min_redeem_power_unit {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                format!(
                    "granted power below minimum unit: granted={} min={}",
                    granted_power_units, min_redeem_power_unit
                ),
            );
        }
        let max_redeem_power_per_epoch = self.state.reward_asset_config.max_redeem_power_per_epoch;
        if max_redeem_power_per_epoch <= 0 {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                "max_redeem_power_per_epoch must be positive".to_string(),
            );
        }
        let next_redeemed = match self
            .state
            .protocol_power_reserve
            .redeemed_power_units
            .checked_add(granted_power_units)
        {
            Some(value) => value,
            None => {
                return self.power_redeem_rejected(
                    node_id,
                    target_agent_id,
                    redeem_credits,
                    nonce,
                    "redeemed_power_units overflow".to_string(),
                );
            }
        };
        if next_redeemed > max_redeem_power_per_epoch {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                format!(
                    "epoch redeem cap exceeded: next={} cap={}",
                    next_redeemed, max_redeem_power_per_epoch
                ),
            );
        }
        let available_credits = self.node_power_credit_balance(node_id);
        if available_credits < redeem_credits {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                format!(
                    "insufficient power credits: balance={} requested={}",
                    available_credits, redeem_credits
                ),
            );
        }
        if self.state.protocol_power_reserve.available_power_units < granted_power_units {
            return self.power_redeem_rejected(
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                format!(
                    "insufficient protocol power reserve: available={} requested={}",
                    self.state.protocol_power_reserve.available_power_units, granted_power_units
                ),
            );
        }

        DomainEvent::PowerRedeemed {
            node_id: node_id.to_string(),
            target_agent_id: target_agent_id.to_string(),
            burned_credits: redeem_credits,
            granted_power_units,
            reserve_remaining: self.state.protocol_power_reserve.available_power_units
                - granted_power_units,
            nonce,
        }
    }

    fn power_redeem_rejected(
        &self,
        node_id: &str,
        target_agent_id: &str,
        redeem_credits: u64,
        nonce: u64,
        reason: String,
    ) -> DomainEvent {
        DomainEvent::PowerRedeemRejected {
            node_id: node_id.to_string(),
            target_agent_id: target_agent_id.to_string(),
            redeem_credits,
            nonce,
            reason,
        }
    }

    fn apply_event_body_at(
        &mut self,
        body: &WorldEventBody,
        time: WorldTime,
        envelope_event_seq: Option<WorldEventId>,
    ) -> Result<(), WorldError> {
        self.apply_event_body_at_with_prepared_body(body, time, envelope_event_seq, None)
    }

    fn apply_event_body_at_with_prepared_body(
        &mut self,
        body: &WorldEventBody,
        time: WorldTime,
        envelope_event_seq: Option<WorldEventId>,
        prepared_body: Option<&PreparedBodyAttributesUpdate>,
    ) -> Result<(), WorldError> {
        match body {
            WorldEventBody::Domain(event) => {
                let committed_receipt_event_id =
                    self.validate_agent_intent_receipt_reference(event, envelope_event_seq)?;
                if matches!(
                    event,
                    DomainEvent::ModuleInstalled { .. }
                        | DomainEvent::ModuleUpgraded { .. }
                        | DomainEvent::ModuleRollbackApplied { .. }
                ) {
                    let prepared = self.state.prepare_module_instance_event_with_registry(
                        event,
                        time,
                        Some(&self.module_registry),
                    )?;
                    let schedule = self.prepare_module_instance_schedule(event, time)?;
                    prepared.install_infallible(&mut self.state);
                    self.install_prepared_module_instance_schedule(schedule);
                    self.state.route_domain_event(event);
                    self.state.time = time;
                    return Ok(());
                }
                if let Some(prepared) = prepared_body {
                    if !prepared.matches_event(event) {
                        return Err(WorldError::ResourceBalanceInvalid {
                            reason: "prepared body delta does not match body event".to_string(),
                        });
                    }
                    prepared.clone().install(self)?;
                } else {
                    self.state.apply_domain_event_at(
                        event,
                        time,
                        envelope_event_seq,
                        committed_receipt_event_id,
                    )?;
                }
                self.state.route_domain_event(event);
            }
            WorldEventBody::EffectQueued(intent) => {
                self.push_pending_effect_bounded(intent.clone())?;
            }
            WorldEventBody::ReceiptAppended(receipt) => {
                let identity_key = self.effect_receipt_key(receipt)?;
                let mut removed = false;
                if self.inflight_effects.remove(&identity_key).is_some() {
                    removed = true;
                }
                let before = self.pending_effects.len();
                self.pending_effects
                    .retain(|intent| intent.identity_key() != identity_key);
                if before != self.pending_effects.len() {
                    removed = true;
                }
                if !removed {
                    return Err(WorldError::ReceiptUnknownIntent {
                        intent_id: receipt.intent_id.clone(),
                    });
                }
            }
            WorldEventBody::CapabilityAuthorization(event) => {
                self.apply_capability_authorization_event(event, time)?;
            }
            WorldEventBody::PolicyDecisionRecorded(_) => {}
            WorldEventBody::RuleDecisionRecorded(_) => {}
            WorldEventBody::ActionOverridden(_) => {}
            WorldEventBody::Governance(event) => {
                self.apply_governance_event(event)?;
            }
            WorldEventBody::ModuleEvent(event) => {
                self.apply_module_event(event, time)?;
            }
            WorldEventBody::ModuleCallFailed(_) => {}
            WorldEventBody::ModuleEmitted(event) => {
                self.state.apply_module_visual_event_at(event, time)?;
            }
            WorldEventBody::ModuleStateUpdated(update) => {
                self.state
                    .module_states
                    .insert(update.module_id.clone(), update.state.clone());
            }
            WorldEventBody::ModuleRuntimeCharged(charge) => {
                self.apply_module_runtime_charge_event(charge, time)?;
            }
            WorldEventBody::SnapshotCreated(_) => {}
            WorldEventBody::ManifestUpdated(update) => {
                self.manifest = update.manifest.clone();
            }
            WorldEventBody::RollbackApplied(_) => {}
            WorldEventBody::ProductValidationDeliveryCursorUpdated(cursor) => {
                self.state
                    .product_validation_delivery_cursor
                    .advance_to(cursor.event_id_era, cursor.routed_through_event_id);
            }
        }
        self.state.time = time;
        Ok(())
    }

    pub(super) fn validate_agent_intent_receipt_reference(
        &self,
        event: &DomainEvent,
        envelope_event_seq: Option<WorldEventId>,
    ) -> Result<Option<WorldEventId>, WorldError> {
        let DomainEvent::AgentIntentTransitioned { intent } = event else {
            return Ok(None);
        };
        if intent.status != "completed" {
            return Ok(None);
        }
        let receipt_event_id = intent
            .receipt_ref
            .as_deref()
            .and_then(|value| value.strip_prefix("world-event:"))
            .and_then(|value| value.parse::<WorldEventId>().ok())
            .filter(|event_id| *event_id > 0)
            .ok_or_else(|| WorldError::ResourceBalanceInvalid {
                reason: "completed AgentIntentV2 requires a world-event receipt reference"
                    .to_string(),
            })?;
        if intent
            .world_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
            || intent.reorg_epoch.is_none()
        {
            return Err(WorldError::ResourceBalanceInvalid {
                reason:
                    "completed AgentIntentV2 requires world_id and reorg_epoch authority context"
                        .to_string(),
            });
        }
        let Some(envelope_event_seq) = envelope_event_seq else {
            // The state reducer will fail closed without the witness. Keep
            // this validator side-effect free for any non-journal caller.
            return Ok(None);
        };
        if receipt_event_id >= envelope_event_seq {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "completed AgentIntentV2 receipt {} must precede transition event {}",
                    receipt_event_id, envelope_event_seq
                ),
            });
        }
        let expected_effect_intent_id = intent.effect_intent_id.as_deref().ok_or_else(|| {
            WorldError::ResourceBalanceInvalid {
                reason: "completed AgentIntentV2 requires a bound effect_intent_id".to_string(),
            }
        })?;
        let committed = self.journal.events.iter().find(|event| {
            event.id == receipt_event_id
                && matches!(
                    &event.body,
                    WorldEventBody::ReceiptAppended(receipt)
                        if receipt.identity_key() == expected_effect_intent_id
                            && !receipt.status.trim().is_empty()
                )
        });
        let Some(committed) = committed else {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "completed AgentIntentV2 receipt {} is not committed for effect intent {}",
                    receipt_event_id, expected_effect_intent_id
                ),
            });
        };
        if committed.time < intent.logical_time {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "completed AgentIntentV2 receipt {} precedes intent logical time {}",
                    receipt_event_id, intent.logical_time
                ),
            });
        }
        Ok(Some(receipt_event_id))
    }
}
