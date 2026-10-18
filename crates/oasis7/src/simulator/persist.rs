//! Persistence utilities: WorldSnapshot, WorldJournal, and error types.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use std::fs;
use std::path::Path;

use super::archive::WorldEvent;
use crate::chain_resource_schema::{ChainResourceDelta, ChainResourceManifest};
#[cfg(not(target_arch = "wasm32"))]
use crate::runtime::Snapshot as RuntimeSnapshot;
use oasis7_simulator::{
    ActionEnvelope, ActionId, CHUNK_GENERATION_SCHEMA_VERSION, JOURNAL_VERSION, SNAPSHOT_VERSION,
    WorldEventId, WorldTime,
};
use oasis7_simulator::{ChunkRuntimeConfig, MicroDepotPlayerFacilitySnapshot};
use oasis7_simulator::{WorldConfig, WorldModel};
use oasis7_wasm_abi::MaterialStack;
#[cfg(target_arch = "wasm32")]
use serde_json::Value as RuntimeSnapshot;

// ============================================================================
// Snapshot
// ============================================================================

fn default_snapshot_version() -> u32 {
    SNAPSHOT_VERSION
}

fn default_journal_version() -> u32 {
    JOURNAL_VERSION
}

fn default_chunk_generation_schema_version() -> u32 {
    CHUNK_GENERATION_SCHEMA_VERSION
}

fn is_supported_snapshot_version(version: u32) -> bool {
    version == SNAPSHOT_VERSION || version == SNAPSHOT_VERSION.saturating_sub(1)
}

fn is_supported_journal_version(version: u32) -> bool {
    version == JOURNAL_VERSION || version == JOURNAL_VERSION.saturating_sub(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerGameplayStageId {
    FirstSessionLoop,
    PostOnboarding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerGameplayStageStatus {
    Active,
    Blocked,
    BranchReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerGameplayGoalKind {
    CreateFirstWorldFeedback,
    EstablishFirstCapability,
    TurnMaterialFlowIntoOutput,
    StartFactoryRun,
    StabilizeFirstLine,
    RecoverCapability,
    ChooseFirstExpansionTradeoff,
    ChooseMidLoopPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerGameplayExecutionState {
    Accepted,
    Executing,
    Blocked,
    Completed,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerGameplayCausalityKind {
    QueuedForExecution,
    WorldConstraint,
    AgentOverride,
    GoalProgressed,
    RequestRejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayAction {
    pub action_id: String,
    pub label: String,
    pub protocol_action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayBranchCommitment {
    pub action_id: String,
    pub route_label: String,
    pub immediate_gain: String,
    pub future_beat_changed: String,
    #[serde(default)]
    pub future_beats: Vec<String>,
    pub risk_or_lockin: String,
    pub next_session_hook: String,
    /// A read-only, player-facing quote for the first delivery produced by
    /// this branch. `None` preserves deserialization of snapshots written
    /// before specialization delivery previews existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_delivery_preview: Option<PlayerGameplayFirstDeliveryPreview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayFirstDeliveryPreview {
    pub local_need: String,
    pub expected_output: String,
    #[serde(default)]
    pub required_inputs: Vec<String>,
    pub value_timing: String,
    pub leverage_class_unlocked: String,
    pub return_visit_hook: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayRecoveryOption {
    pub kind: String,
    pub estimated_time_class: String,
    pub estimated_resource_class: String,
    pub risk_class: String,
    pub retained_benefit: String,
    pub recommendation_reason: String,
}

/// An advisory translation for a requested physical action that the current
/// runtime does not expose as a canonical world action.
///
/// This is presentation/persistence data only. It does not enqueue, validate,
/// or execute `canonical_replacement_action`; callers must continue through
/// the existing canonical action path to do so.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayFineGrainActionTranslation {
    pub requested_granularity: String,
    pub why_fine_action_deferred: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_replacement_action: Option<String>,
    pub closest_playable_goal: String,
    pub player_next_step_hint: String,
    pub replacement_value_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayRecentFeedback {
    pub action: String,
    pub stage: String,
    pub effect: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default)]
    pub delta_logical_time: WorldTime,
    #[serde(default)]
    pub delta_event_seq: u64,
}

/// Player-safe projection of an authoritative Agent decision receipt. The
/// domain event references remain pointers to the owning settlement records;
/// this projection never treats an accepted request as a world effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayCausalReceiptSnapshot {
    pub receipt_id: String,
    pub commit_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_intent_id: Option<String>,
    pub action_id: u64,
    pub action_kind: String,
    #[serde(default)]
    pub domain_event_refs: Vec<u64>,
    pub disposition: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_reason: Option<String>,
    pub next_step: String,
    pub expected_consequence: serde_json::Value,
    pub actual_consequence: serde_json::Value,
    pub alternative: serde_json::Value,
    pub stakes: serde_json::Value,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub correction_refs: Vec<String>,
    #[serde(default)]
    pub interruption_refs: Vec<String>,
    #[serde(default)]
    pub owner_control_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization: Option<PlayerGameplayDelegationUsageSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dissent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hard_boundary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayDelegationGrantSnapshot {
    pub grant_id: String,
    pub source_id: String,
    pub issuer_id: String,
    pub owner_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    pub agent_id: String,
    pub object_id: String,
    #[serde(default)]
    pub action_kinds: Vec<String>,
    pub revision: u64,
    pub period_id: String,
    pub valid_from_tick: u64,
    pub valid_until_tick: u64,
    pub limit_units: u64,
    pub resource_kind: String,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayDelegationUsageSnapshot {
    pub grant: PlayerGameplayDelegationGrantSnapshot,
    pub cost_units: u64,
    pub spent_units: u64,
    pub remaining_units: u64,
}

/// Causal and agency details are nested beneath a canonical runtime intent so
/// legacy snapshots with no such detail remain decodable and visibly unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PlayerGameplayAgencyReadModel {
    pub status: String,
    pub source: String,
    pub causal_receipt_status: String,
    pub memory_context_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causal_receipt: Option<PlayerGameplayCausalReceiptSnapshot>,
    #[serde(default)]
    pub delegation_authorizations: Vec<PlayerGameplayDelegationUsageSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referenced_memory_context: Option<serde_json::Value>,
    #[serde(default)]
    pub memory_corrections: Vec<crate::simulator::MemoryCorrectionV1>,
}

/// The durable, player-authored intent currently handed to an in-world Agent.
///
/// This deliberately carries only the player-visible instruction and its handoff
/// state. Prompt overrides and decision traces remain private implementation
/// detail and must not be projected through gameplay snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayPrimaryIntent {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub resume_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    /// Agent identity that owns this runtime-authoritative intent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Canonical runtime intent kind (for example `player_instruction`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Canonical origin of the intent (for example `player_chat`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Optional runtime target carried by the intent contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    /// World identity at the position where the intent was accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    /// Reorg epoch bound to the accepted intent position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reorg_epoch: Option<u64>,
    /// Runtime logical time of the intent transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logical_time: Option<u64>,
    /// Runtime update time of the intent transition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freshness: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_seq: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_summary: Option<String>,
    /// A committed receipt tuple. Legacy string values deserialize as JSON but
    /// are deliberately rejected by the player-facing projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_ref: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_step: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agency_read_model: Option<PlayerGameplayAgencyReadModel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayWaitResolutionQuote {
    #[serde(default)]
    pub safe_to_wait: bool,
    pub resolution_trigger: String,
    pub recheck_tick_or_event: String,
    pub expected_change: String,
    pub unresolved_risk: String,
    pub alternative_unlock_condition: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerAgentClaimQuoteSnapshot {
    pub slot_index: u8,
    pub reputation_tier: u8,
    pub claim_cap: u8,
    pub owned_claim_count: u8,
    pub activation_fee_amount: u64,
    pub claim_bond_amount: u64,
    pub upkeep_per_epoch: u64,
    pub total_upfront_amount: u64,
    #[serde(default)]
    pub transferable_liquid_balance: u64,
    #[serde(default)]
    pub restricted_starter_claim_balance: u64,
    #[serde(default)]
    pub auto_restricted_starter_claim_amount: u64,
    #[serde(default)]
    pub eligible_claim_balance: u64,
    #[serde(default)]
    pub eligible_balance_after: u64,
    #[serde(default)]
    pub upkeep_runway_epochs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_upkeep_due_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected_grace_entry_epoch: Option<u64>,
    #[serde(default)]
    pub low_runway_warning: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_claim_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_1_claim_choice_quote: Option<PlayerAgentClaimChoiceQuoteSnapshot>,
    pub release_cooldown_epochs: u64,
    pub grace_epochs: u64,
    pub idle_warning_epochs: u64,
    pub forced_idle_reclaim_epochs: u64,
    pub forced_reclaim_penalty_bps: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
}

/// A deterministic, read-only candidate fact projection for the first Agent
/// claim. It deliberately contains no ranking, suitability, or claim result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerAgentClaimChoiceCandidateSnapshot {
    pub agent_id: String,
    pub location_x_cm: i64,
    pub location_y_cm: i64,
    pub location_z_cm: i64,
    pub body_kind: String,
    pub frame_kind: String,
    #[serde(default)]
    pub installed_module_ids: Vec<String>,
}

/// The optional slot-1 candidate package, kept separate from the executable
/// claim quote so that unsupported rationale remains explicit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerAgentClaimChoiceQuoteSnapshot {
    pub status: String,
    #[serde(default)]
    pub candidates: Vec<PlayerAgentClaimChoiceCandidateSnapshot>,
    /// Canonical rationale for a uniquely complete slot-1 candidate.
    ///
    /// These fields are optional so snapshots written before the rationale
    /// contract was available remain readable and continue to fail closed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_starting_location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_specialty_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_industrial_goal_help: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_risk_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_recommendation_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    pub claim_choice_class: String,
    pub recommended_claim_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerAgentClaimOwnedSnapshot {
    pub target_agent_id: String,
    pub status: String,
    pub upkeep_paid_through_epoch: u64,
    #[serde(default)]
    pub upfront_restricted_spent_amount: u64,
    #[serde(default)]
    pub upfront_liquid_spent_amount: u64,
    #[serde(default)]
    pub claim_bond_locked_restricted_amount: u64,
    #[serde(default)]
    pub claim_bond_locked_liquid_amount: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_ready_at_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_ready_in_epochs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grace_deadline_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grace_remaining_epochs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_warning_in_epochs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forced_reclaim_in_epochs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerFirstChatUnlockPreviewSnapshot {
    pub chat_purpose: String,
    pub immediate_playable_help: String,
    pub first_question_or_action_hint: String,
    pub resource_boundary: String,
    pub defer_effect: String,
    pub recommended_unlock_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerAgentClaimSnapshot {
    pub claimer_agent_id: String,
    pub current_epoch: u64,
    pub reputation_tier: u8,
    pub claim_cap: u8,
    pub owned_claim_count: u8,
    pub liquid_main_token_balance: u64,
    #[serde(default)]
    pub restricted_starter_claim_balance: u64,
    #[serde(default)]
    pub slot_1_auto_restricted_starter_claim_amount: u64,
    #[serde(default)]
    pub slot_1_eligible_claim_balance: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_claim_quote: Option<PlayerAgentClaimQuoteSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_chat_unlock_preview: Option<PlayerFirstChatUnlockPreviewSnapshot>,
    #[serde(default)]
    pub owned_claims: Vec<PlayerAgentClaimOwnedSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerGameplaySnapshot {
    pub stage_id: PlayerGameplayStageId,
    pub stage_status: PlayerGameplayStageStatus,
    pub execution_state: PlayerGameplayExecutionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_intent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_target: Option<String>,
    #[serde(default)]
    pub can_reprioritize: bool,
    pub goal_id: String,
    pub goal_kind: PlayerGameplayGoalKind,
    pub goal_title: String,
    pub objective: String,
    pub progress_detail: String,
    #[serde(default)]
    pub progress_percent: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker_detail: Option<String>,
    pub next_step_hint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_world_change: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causality_kind: Option<PlayerGameplayCausalityKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causality_detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_hint: Option<String>,
    #[serde(default)]
    pub branch_recommendations: Vec<PlayerGameplayBranchCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_blocker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_window_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stalled_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalation_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_action_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_tradeoff_preview: Option<Vec<PlayerGameplayFallbackTradeoffOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_safe_fallback_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_next_decision_action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_next_decision_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_next_step: Option<String>,
    #[serde(default)]
    pub available_actions: Vec<PlayerGameplayAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recent_feedback: Option<PlayerGameplayRecentFeedback>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_intent: Option<PlayerGameplayPrimaryIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_agent_chat: Option<oasis7_proto::viewer::CanonicalAgentChatViewV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_resolution_quote: Option<PlayerGameplayWaitResolutionQuote>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_claim: Option<PlayerAgentClaimSnapshot>,
    #[serde(default)]
    pub micro_depot_facilities: Vec<MicroDepotPlayerFacilitySnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_player_lane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leverage_class: Option<String>,
    #[serde(default)]
    pub same_loop_repeat_count: u32,
    #[serde(default)]
    pub grind_only_flag: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub major_power_dependency_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_path_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_path_detail: Option<String>,
    #[serde(default)]
    pub recovery_options: Vec<PlayerGameplayRecoveryOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fine_grain_action_translation: Option<PlayerGameplayFineGrainActionTranslation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_major_power_sponsorship: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_available: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rebuild_available: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot_available: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_unlock_preview: Option<ProductValidationUnlockPreview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starter_industrial_feasibility: Option<PlayerStarterIndustrialFeasibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory_production_failure_disposition:
        Option<PlayerGameplayFactoryProductionFailureDisposition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerStarterIndustrialFeasibilityStatus {
    CandidateAvailable,
    NoSafeStarterChain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerStarterIndustrialFeasibility {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settled_outcome: Option<PlayerStarterIndustrialSettledOutcome>,
    pub profile_id: String,
    pub profile_revision: u64,
    pub authority_snapshot: String,
    pub status: PlayerStarterIndustrialFeasibilityStatus,
    pub evidence_class: String,
    pub completion_boundary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
    pub next_action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_recheck: Option<WorldTime>,
    pub progression_effect: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerStarterIndustrialSettledOutcome {
    pub settlement_job_id: u64,
    pub settled_at: WorldTime,
    pub output_ledger: crate::industry_types::MaterialLedgerId,
    pub settlement: oasis7_agent_api::world_industry::StarterIndustrialSettlementSummaryV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductValidationUnlockPreview {
    pub product_id: String,
    pub role_tag: String,
    pub tradable: bool,
    pub required_stage: String,
    pub current_stage: String,
    pub stage_status: String,
    pub value_summary: String,
    pub next_step_hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayFactoryProductionFailureDisposition {
    pub action_id: String,
    pub requester_agent_id: String,
    pub factory_id: String,
    pub recipe_id: String,
    pub blocker_kind: String,
    pub blocker_detail: String,
    pub disposition_kind: String,
    pub consumed_inputs: Vec<MaterialStack>,
    pub lost_inputs: Vec<MaterialStack>,
    pub consumed_power: i64,
    pub lost_power: i64,
    pub next_action: String,
    #[serde(default)]
    pub next_recheck: Option<WorldTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerGameplayFallbackTradeoffOption {
    pub value_class: String,
    pub available: bool,
    pub cost: String,
    pub progress_kept: String,
    pub opportunity_cost: String,
    pub reason: String,
    pub recommended: bool,
}

#[derive(Deserialize)]
struct PlayerGameplaySnapshotSerde {
    stage_id: PlayerGameplayStageId,
    stage_status: PlayerGameplayStageStatus,
    #[serde(default)]
    execution_state: Option<PlayerGameplayExecutionState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accepted_intent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    intent_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    intent_scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    intent_target: Option<String>,
    #[serde(default)]
    can_reprioritize: bool,
    goal_id: String,
    goal_kind: PlayerGameplayGoalKind,
    goal_title: String,
    objective: String,
    progress_detail: String,
    #[serde(default)]
    progress_percent: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    blocker_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    blocker_detail: Option<String>,
    next_step_hint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_world_change: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    causality_kind: Option<PlayerGameplayCausalityKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    causality_detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    branch_hint: Option<String>,
    #[serde(default)]
    branch_recommendations: Vec<PlayerGameplayBranchCommitment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resume_anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    primary_blocker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    response_window_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stalled_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    escalation_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fallback_action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fallback_action_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fallback_tradeoff_preview: Option<Vec<PlayerGameplayFallbackTradeoffOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    no_safe_fallback_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    required_next_decision_action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    required_next_decision_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resume_next_step: Option<String>,
    #[serde(default)]
    available_actions: Vec<PlayerGameplayAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recent_feedback: Option<PlayerGameplayRecentFeedback>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    primary_intent: Option<PlayerGameplayPrimaryIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canonical_agent_chat: Option<oasis7_proto::viewer::CanonicalAgentChatViewV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wait_resolution_quote: Option<PlayerGameplayWaitResolutionQuote>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agent_claim: Option<PlayerAgentClaimSnapshot>,
    #[serde(default)]
    micro_depot_facilities: Vec<MicroDepotPlayerFacilitySnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    small_player_lane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    leverage_class: Option<String>,
    #[serde(default)]
    same_loop_repeat_count: u32,
    #[serde(default)]
    grind_only_flag: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    major_power_dependency_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery_path_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery_path_detail: Option<String>,
    #[serde(default)]
    recovery_options: Vec<PlayerGameplayRecoveryOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fine_grain_action_translation: Option<PlayerGameplayFineGrainActionTranslation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    requires_major_power_sponsorship: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repair_available: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rebuild_available: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pivot_available: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    validation_unlock_preview: Option<ProductValidationUnlockPreview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    starter_industrial_feasibility: Option<PlayerStarterIndustrialFeasibility>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    factory_production_failure_disposition:
        Option<PlayerGameplayFactoryProductionFailureDisposition>,
}

fn derive_legacy_execution_state(
    stage_status: PlayerGameplayStageStatus,
    recent_feedback: Option<&PlayerGameplayRecentFeedback>,
) -> PlayerGameplayExecutionState {
    if let Some(feedback) = recent_feedback {
        match feedback.stage.as_str() {
            "accepted" | "submitted" | "queued" | "ack" => {
                return PlayerGameplayExecutionState::Accepted;
            }
            "rejected" => return PlayerGameplayExecutionState::Rejected,
            "blocked" | "completed_no_progress" => return PlayerGameplayExecutionState::Blocked,
            "completed_advanced" => return PlayerGameplayExecutionState::Completed,
            _ => {}
        }
    }

    match stage_status {
        PlayerGameplayStageStatus::Blocked => PlayerGameplayExecutionState::Blocked,
        PlayerGameplayStageStatus::BranchReady => PlayerGameplayExecutionState::Completed,
        PlayerGameplayStageStatus::Active => PlayerGameplayExecutionState::Executing,
    }
}

impl<'de> Deserialize<'de> for PlayerGameplaySnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let legacy = PlayerGameplaySnapshotSerde::deserialize(deserializer)?;
        let execution_state = legacy.execution_state.unwrap_or_else(|| {
            derive_legacy_execution_state(legacy.stage_status, legacy.recent_feedback.as_ref())
        });
        Ok(Self {
            stage_id: legacy.stage_id,
            stage_status: legacy.stage_status,
            execution_state,
            accepted_intent_id: legacy.accepted_intent_id,
            intent_summary: legacy.intent_summary,
            intent_scope: legacy.intent_scope,
            intent_target: legacy.intent_target,
            can_reprioritize: legacy.can_reprioritize,
            goal_id: legacy.goal_id,
            goal_kind: legacy.goal_kind,
            goal_title: legacy.goal_title,
            objective: legacy.objective,
            progress_detail: legacy.progress_detail,
            progress_percent: legacy.progress_percent,
            blocker_kind: legacy.blocker_kind,
            blocker_detail: legacy.blocker_detail,
            next_step_hint: legacy.next_step_hint,
            status_reason: legacy.status_reason,
            last_world_change: legacy.last_world_change,
            causality_kind: legacy.causality_kind,
            causality_detail: legacy.causality_detail,
            branch_hint: legacy.branch_hint,
            branch_recommendations: legacy.branch_recommendations,
            resume_anchor: legacy.resume_anchor,
            primary_blocker: legacy.primary_blocker,
            response_window_class: legacy.response_window_class,
            stalled_reason: legacy.stalled_reason,
            escalation_hint: legacy.escalation_hint,
            fallback_action_id: legacy.fallback_action_id,
            fallback_action_label: legacy.fallback_action_label,
            fallback_tradeoff_preview: legacy.fallback_tradeoff_preview,
            no_safe_fallback_reason: legacy.no_safe_fallback_reason,
            required_next_decision_action_id: legacy.required_next_decision_action_id,
            required_next_decision_class: legacy.required_next_decision_class,
            resume_next_step: legacy.resume_next_step,
            available_actions: legacy.available_actions,
            recent_feedback: legacy.recent_feedback,
            primary_intent: legacy.primary_intent,
            canonical_agent_chat: legacy.canonical_agent_chat,
            wait_resolution_quote: legacy.wait_resolution_quote,
            agent_claim: legacy.agent_claim,
            micro_depot_facilities: legacy.micro_depot_facilities,
            small_player_lane_id: legacy
                .small_player_lane_id
                .or_else(|| Some("unclassified".to_string())),
            leverage_class: legacy
                .leverage_class
                .or_else(|| Some("unclassified".to_string())),
            same_loop_repeat_count: legacy.same_loop_repeat_count,
            grind_only_flag: legacy.grind_only_flag,
            major_power_dependency_status: legacy
                .major_power_dependency_status
                .or_else(|| Some("unverified".to_string())),
            recovery_path_kind: legacy
                .recovery_path_kind
                .or_else(|| Some("unverified".to_string())),
            recovery_path_detail: legacy.recovery_path_detail,
            recovery_options: legacy.recovery_options,
            fine_grain_action_translation: legacy.fine_grain_action_translation,
            requires_major_power_sponsorship: legacy
                .requires_major_power_sponsorship
                .or_else(|| Some("unverified".to_string())),
            repair_available: legacy.repair_available,
            rebuild_available: legacy.rebuild_available,
            pivot_available: legacy.pivot_available,
            validation_unlock_preview: legacy.validation_unlock_preview,
            starter_industrial_feasibility: legacy.starter_industrial_feasibility,
            factory_production_failure_disposition: legacy.factory_production_failure_disposition,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldSnapshot {
    #[serde(default = "default_snapshot_version")]
    pub version: u32,
    #[serde(default = "default_chunk_generation_schema_version")]
    pub chunk_generation_schema_version: u32,
    pub time: WorldTime,
    pub config: WorldConfig,
    pub model: WorldModel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_snapshot: Option<RuntimeSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_gameplay: Option<PlayerGameplaySnapshot>,
    #[serde(default)]
    pub chain_resource_manifest: ChainResourceManifest,
    #[serde(default)]
    pub latest_chain_resource_delta: ChainResourceDelta,
    #[serde(default)]
    pub chunk_runtime: ChunkRuntimeConfig,
    #[serde(default)]
    pub intel_ttl_ticks: WorldTime,
    pub next_event_id: WorldEventId,
    pub next_action_id: ActionId,
    pub pending_actions: Vec<ActionEnvelope>,
    pub journal_len: usize,
}

impl WorldSnapshot {
    pub fn to_json(&self) -> Result<String, PersistError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(input: &str) -> Result<Self, PersistError> {
        let snapshot: Self = serde_json::from_str(input)?;
        snapshot.validate_version()?;
        Ok(snapshot)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), PersistError> {
        write_json_to_path(self, path.as_ref())
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, PersistError> {
        let snapshot: Self = read_json_from_path(path.as_ref())?;
        snapshot.validate_version()?;
        Ok(snapshot)
    }

    pub(crate) fn validate_version(&self) -> Result<(), PersistError> {
        if is_supported_snapshot_version(self.version) {
            Ok(())
        } else {
            Err(PersistError::UnsupportedVersion {
                kind: "snapshot".to_string(),
                version: self.version,
                expected: SNAPSHOT_VERSION,
            })
        }
    }
}

// ============================================================================
// Journal
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldJournal {
    #[serde(default = "default_journal_version")]
    pub version: u32,
    pub events: Vec<WorldEvent>,
}

impl WorldJournal {
    pub fn new() -> Self {
        Self {
            version: JOURNAL_VERSION,
            events: Vec::new(),
        }
    }

    pub fn to_json(&self) -> Result<String, PersistError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(input: &str) -> Result<Self, PersistError> {
        let journal: Self = serde_json::from_str(input)?;
        journal.validate_version()?;
        Ok(journal)
    }

    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), PersistError> {
        write_json_to_path(self, path.as_ref())
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, PersistError> {
        let journal: Self = read_json_from_path(path.as_ref())?;
        journal.validate_version()?;
        Ok(journal)
    }

    pub(crate) fn validate_version(&self) -> Result<(), PersistError> {
        if is_supported_journal_version(self.version) {
            Ok(())
        } else {
            Err(PersistError::UnsupportedVersion {
                kind: "journal".to_string(),
                version: self.version,
                expected: JOURNAL_VERSION,
            })
        }
    }
}

impl Default for WorldJournal {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Errors
// ============================================================================

pub use oasis7_simulator::PersistError;

// ============================================================================
// Helper functions
// ============================================================================

pub(crate) fn write_json_to_path<T: Serialize>(value: &T, path: &Path) -> Result<(), PersistError> {
    let data = serde_json::to_vec_pretty(value)?;
    fs::write(path, data)?;
    Ok(())
}

pub(crate) fn read_json_from_path<T: DeserializeOwned>(path: &Path) -> Result<T, PersistError> {
    let data = fs::read(path)?;
    Ok(serde_json::from_slice(&data)?)
}
