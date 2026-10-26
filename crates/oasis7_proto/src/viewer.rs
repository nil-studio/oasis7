use serde::{Deserialize, Serialize};
mod rollback_v2;
pub use rollback_v2::*;
mod authoritative;
pub use authoritative::*;
mod director;
pub use director::*;
mod responses;
pub use responses::*;
mod world_feed;
pub use world_feed::*;
mod negotiation;
pub use negotiation::*;
mod collect_data;
pub use collect_data::*;
mod refine_quote;
pub use refine_quote::*;
mod schedule_recipe_quote;
pub use schedule_recipe_quote::*;
mod live_control_conversion;
mod product_validation_quote;
#[cfg(test)]
mod schedule_recipe_quote_tests;
pub use product_validation_quote::*;
mod power_survival_quote;
pub use power_survival_quote::*;
mod power_sale_quote;
pub use power_sale_quote::*;
mod fragment_refill_preview;
pub use fragment_refill_preview::*;
mod market_quote_decision;
pub use market_quote_decision::*;
mod social_quote;
pub use social_quote::*;
mod transfer_material_quote;
pub use transfer_material_quote::*;
/// Signed, advisory preflight for an existing governance proposal vote.
mod governance_vote_quote;
pub use governance_vote_quote::*;
/// Signed, advisory preflight for the existing DeclareWar action.
mod war_declaration_quote;
pub use war_declaration_quote::*;
pub const VIEWER_PROTOCOL_VERSION: u32 = 2;
/// Feature gate for the signed, read-only social-fact revocation quote.
///
/// This remains a Viewer protocol v2 capability so clients can avoid sending
/// the request to older v2 readers that silently drop unknown enum variants.
pub const REVOKE_SOCIAL_FACT_QUOTE_CAPABILITY: &str = "revoke_social_fact_quote_v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerAuthProof {
    #[serde(default)]
    pub scheme: PlayerAuthScheme,
    pub player_id: String,
    pub public_key: String,
    pub nonce: u64,
    pub signature: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedStrongAuthGrant {
    pub version: u8,
    pub action_id: String,
    pub player_id: String,
    pub player_public_key: String,
    pub agent_id: String,
    pub issued_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub signer_public_key: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewerRequest {
    Hello {
        client: String,
        version: u32,
    },
    HelloV2 {
        client: String,
        version: u32,
        #[serde(default)]
        capabilities: Vec<String>,
    },
    Subscribe {
        streams: Vec<ViewerStream>,
        #[serde(default)]
        event_kinds: Vec<ViewerEventKind>,
    },
    RequestSnapshot,
    RequestScopeSnapshot {
        request_generation: String,
    },
    RequestWorldView {
        query: serde_json::Value,
    },
    RequestCanonicalAgentOwnerReadContext {
        agent_id: String,
    },
    CanonicalAgentOwnerRead {
        request: serde_json::Value,
    },
    SafeAlternative {
        request: serde_json::Value,
        #[serde(default)]
        recover: bool,
    },
    RequestWorldFeed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        limit: usize,
    },
    PlaybackControl {
        mode: PlaybackControl,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    LiveControl {
        mode: LiveControl,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    // Legacy mixed control channel. Prefer PlaybackControl/LiveControl.
    Control {
        mode: ViewerControl,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
    },
    PromptControl {
        command: Box<PromptControlCommand>,
    },
    AgentChat {
        request: AgentChatRequest,
    },
    GameplayAction {
        request: GameplayActionRequest,
    },
    /// Read the disposition of the exact signed original; never submit it.
    LookupGameplayAction {
        request: GameplayActionRequest,
    },
    CollectData {
        command: CollectDataCommand,
    },
    QuoteRefineCompound {
        request: RefineQuoteRequest,
    },
    QuoteScheduleRecipe {
        request: ScheduleRecipeQuoteRequest,
    },
    QuoteProductValidation {
        request: ProductValidationQuoteRequest,
    },
    QuotePowerSurvival {
        request: PowerSurvivalQuoteRequest,
    },
    QuotePowerSale {
        request: PowerSaleQuoteRequest,
    },
    QuoteDeclareSocialEdge {
        request: DeclareSocialEdgeQuoteRequest,
    },
    QuotePublishSocialFact {
        request: PublishSocialFactQuoteRequest,
    },
    QuoteAdjudicateSocialFact {
        request: AdjudicateSocialFactQuoteRequest,
    },
    QuoteRevokeSocialFact {
        request: RevokeSocialFactQuoteRequest,
    },
    QuoteSocialContact {
        request: SocialContactQuoteRequest,
    },
    QuoteGovernanceVote {
        request: GovernanceVoteQuoteRequest,
    },
    QuoteDeclareWar {
        request: WarDeclarationQuoteRequest,
    },
    PreviewFragmentReplenishment {
        request: FragmentRefillRequest,
    },
    QuoteMarketDecision {
        request: MarketQuoteDecisionRequest,
    },
    QuoteTransferMaterial {
        request: TransferMaterialQuoteRequest,
    },
    AuthoritativeChallenge {
        command: AuthoritativeChallengeCommand,
    },
    AuthoritativeRecovery {
        command: AuthoritativeRecoveryCommand,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PromptControlCommand {
    Preview {
        request: PromptControlApplyRequest,
    },
    Apply {
        request: PromptControlApplyRequest,
    },
    Rollback {
        request: PromptControlRollbackRequest,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PromptControlApplyRequest {
    pub agent_id: String,
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_authority_epoch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<PlayerAuthProof>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strong_auth_grant: Option<HostedStrongAuthGrant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_override_field"
    )]
    pub system_prompt_override: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_override_field"
    )]
    pub short_term_goal_override: Option<Option<String>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_override_field"
    )]
    pub long_term_goal_override: Option<Option<String>>,
}
fn deserialize_override_field<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    Ok(Some(value))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PromptControlRollbackRequest {
    pub agent_id: String,
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_authority_epoch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<PlayerAuthProof>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strong_auth_grant: Option<HostedStrongAuthGrant>,
    pub to_version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentChatRequest {
    pub agent_id: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<PlayerAuthProof>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_tick: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_seq: Option<u64>,
    /// Canonical world identity for the signed Agent Intent authority envelope.
    ///
    /// These fields are optional on the wire so legacy Viewer clients remain
    /// deserializable.  The authoritative V2 endpoint must require the complete
    /// tuple before accepting an intent; a partial tuple is invalid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reorg_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority_scope: Option<String>,
    /// Explicit causal replacement target.  Ordinary retries leave this empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces_intent_id: Option<String>,
    /// Signed canonical branch and Agent identity fence; absent on legacy requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_authority: Option<CanonicalAgentChatAuthorityV1>,
}
#[path = "canonical_agent_chat.rs"]
mod canonical_agent_chat;
pub use canonical_agent_chat::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameplayActionRequest {
    pub action_id: String,
    pub target_agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_agent_id: Option<String>,
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<PlayerAuthProof>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AuthoritativeChallengeCommand {
    Submit {
        request: AuthoritativeChallengeSubmitRequest,
    },
    Resolve {
        request: AuthoritativeChallengeResolveRequest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeChallengeSubmitRequest {
    pub batch_id: String,
    pub watcher_id: String,
    pub recomputed_state_root: String,
    pub recomputed_data_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeChallengeResolveRequest {
    pub challenge_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AuthoritativeRecoveryCommand {
    ReleaseHostedSession {
        request: HostedSessionReleaseRequest,
    },
    RegisterSession {
        request: AuthoritativeSessionRegisterRequest,
    },
    Rollback {
        request: AuthoritativeRollbackRequest,
    },
    RollbackV2 {
        request: AuthoritativeRollbackV2Request,
    },
    GetRollbackReceipt {
        request: RollbackReceiptAccessRequest,
    },
    ReevaluateRollbackReadiness {
        authorization_nonce: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        audit_evidence: Option<RollbackStrictAuditEvidence>,
    },
    TransitionRollbackCompensation {
        request: RollbackCompensationTransitionRequest,
    },
    ResolveRollbackAttribution {
        request: RollbackAttributionResolutionRequest,
    },
    ReconnectSync {
        request: AuthoritativeReconnectSyncRequest,
    },
    RevokeSession {
        request: AuthoritativeSessionRevokeRequest,
    },
    RotateSession {
        request: AuthoritativeSessionRotateRequest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedSessionReleaseRequest {
    pub authorization_grant: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeRollbackRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_batch_id: Option<String>,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<RollbackAuthorizationEnvelope>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackAuthorizationEnvelope {
    pub intent: RollbackIntent,
    pub signatures: Vec<RollbackApprovalSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackIntent {
    pub schema_version: u32,
    pub rollback_ticket: String,
    #[serde(default)]
    pub snapshot_hash: String,
    #[serde(default)]
    pub snapshot_journal_len: usize,
    #[serde(default)]
    pub target_journal_len: usize,
    #[serde(default)]
    pub expected_target_state_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_batch_id: Option<String>,
    pub reason: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub nonce: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_checkpoint: Option<RollbackCheckpointRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_target: Option<RollbackReplayTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_reorg_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_replay_events: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_replay_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackApprovalSignature {
    pub authority_id: String,
    pub role: RollbackAuthorityRole,
    pub signature_scheme: String,
    pub signature_hex: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackAuthorityRole {
    OnCall,
    Governance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeReconnectSyncRequest {
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_pubkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_known_log_cursor: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_reorg_epoch: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeSessionRegisterRequest {
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registration_grant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<PlayerAuthProof>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_agent_id: Option<String>,
    #[serde(default)]
    pub force_rebind: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeSessionRevokeRequest {
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_pubkey: Option<String>,
    pub revoke_reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritativeSessionRotateRequest {
    pub player_id: String,
    pub old_session_pubkey: String,
    pub new_session_pubkey: String,
    pub rotate_reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotated_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerStream {
    Snapshot,
    Events,
    Metrics,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerEventKind {
    LocationRegistered,
    AgentRegistered,
    AgentMoved,
    AgentSpoke,
    TargetInspected,
    SimpleInteractionPerformed,
    ResourceTransferred,
    RadiationHarvested,
    ActionRejected,
    Power,
    PromptUpdated,
    RuntimeEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ViewerControlProfile {
    #[default]
    Playback,
    Live,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PlaybackControl {
    Pause,
    Play,
    Step { count: usize },
    Seek { tick: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum LiveControl {
    Pause,
    Play,
    Step { count: usize },
}

// Legacy mixed control channel. Prefer PlaybackControl/LiveControl.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ViewerControl {
    Pause,
    Play,
    Step { count: usize },
    Seek { tick: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "ViewerResponse is a public wire/API enum; boxing a response branch would break existing Rust pattern matches even though serde could preserve its bytes."
)]
pub enum ViewerResponse<Snapshot, Event, DecisionTrace, Metrics, Time> {
    ScopeSnapshot {
        request_generation: String,
        snapshot: Snapshot,
    },
    WorldView {
        view: serde_json::Value,
    },
    SafeAlternativeResult {
        request: serde_json::Value,
        result: serde_json::Value,
    },
    CanonicalAgentOwnerReadContext {
        context: serde_json::Value,
    },
    CanonicalAgentOwnerView {
        version: serde_json::Value,
        request_digest: String,
        request: serde_json::Value,
        view: CanonicalAgentChatViewV1,
    },
    HelloAck {
        server: String,
        version: u32,
        #[serde(default)]
        min_version: u32,
        #[serde(default)]
        max_version: u32,
        #[serde(default)]
        capabilities: Vec<String>,
        world_id: String,
        #[serde(default)]
        control_profile: ViewerControlProfile,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authority_epoch: Option<String>,
    },
    Snapshot {
        snapshot: Snapshot,
    },
    Event {
        event: Event,
    },
    WorldFeed {
        feed: WorldFeedEnvelope,
    },
    AuthoritativeBatch {
        batch: AuthoritativeBatchFinality,
    },
    AuthoritativeChallengeAck {
        ack: AuthoritativeChallengeAck<Time>,
    },
    AuthoritativeChallengeError {
        error: AuthoritativeChallengeError,
    },
    AuthoritativeRecoveryAck {
        ack: AuthoritativeRecoveryAck<Time>,
    },
    AuthoritativeRecoveryError {
        error: AuthoritativeRecoveryError,
    },
    DecisionTrace {
        trace: DecisionTrace,
    },
    Metrics {
        time: Option<Time>,
        metrics: Metrics,
    },
    ControlCompletionAck {
        ack: ControlCompletionAck<Time>,
    },
    PromptControlAck {
        ack: PromptControlAck<Time>,
    },
    PromptControlError {
        error: PromptControlError,
    },
    AgentChatAck {
        ack: AgentChatAck<Time>,
    },
    AgentChatError {
        error: AgentChatError,
    },
    GameplayActionAck {
        ack: GameplayActionAck<Time>,
    },
    GameplayActionError {
        error: GameplayActionError,
    },
    GameplayActionResult {
        action_id: String,
        target_agent_id: String,
        player_id: String,
        request_auth_nonce: Option<u64>,
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        receipt: Option<serde_json::Value>,
    },
    CollectDataPreflight {
        quote: CollectDataPreflight,
    },
    RefineQuotePreflight {
        quote: RefineQuotePreflight,
    },
    ScheduleRecipeQuotePreflight {
        quote: ScheduleRecipeQuotePreflight,
    },
    ProductValidationQuotePreflight {
        quote: ProductValidationQuotePreflight,
    },
    PowerSurvivalQuotePreflight {
        quote: PowerSurvivalQuotePreflight,
    },
    PowerSaleQuotePreflight {
        quote: PowerSaleQuotePreflight,
    },
    DeclareSocialEdgeQuotePreflight {
        quote: DeclareSocialEdgeQuotePreflight,
    },
    PublishSocialFactQuotePreflight {
        quote: PublishSocialFactQuotePreflight,
    },
    AdjudicateSocialFactQuotePreflight {
        quote: AdjudicateSocialFactQuotePreflight,
    },
    RevokeSocialFactQuotePreflight {
        quote: RevokeSocialFactQuotePreflight,
    },
    SocialContactQuotePreflight {
        quote: SocialContactQuotePreflight,
    },
    GovernanceVoteQuotePreflight {
        quote: GovernanceVoteQuotePreflight,
    },
    WarDeclarationQuotePreflight {
        quote: WarDeclarationQuotePreflight,
    },
    FragmentRefillPreviewPreflight {
        quote: FragmentRefillResponse,
    },
    MarketQuoteDecisionPreflight {
        quote: MarketQuoteDecisionPreflight,
    },
    TransferMaterialQuotePreflight {
        quote: TransferMaterialQuotePreflight,
    },
    Error {
        message: String,
    },
}

impl From<PlaybackControl> for ViewerControl {
    fn from(value: PlaybackControl) -> Self {
        match value {
            PlaybackControl::Pause => Self::Pause,
            PlaybackControl::Play => Self::Play,
            PlaybackControl::Step { count } => Self::Step { count },
            PlaybackControl::Seek { tick } => Self::Seek { tick },
        }
    }
}

impl From<ViewerControl> for PlaybackControl {
    fn from(value: ViewerControl) -> Self {
        match value {
            ViewerControl::Pause => Self::Pause,
            ViewerControl::Play => Self::Play,
            ViewerControl::Step { count } => Self::Step { count },
            ViewerControl::Seek { tick } => Self::Seek { tick },
        }
    }
}

#[cfg(test)]
mod authoritative_batch_roundtrip_tests;
#[cfg(test)]
mod authoritative_challenge_roundtrip_tests;
#[cfg(test)]
#[path = "viewer/collect_data_tests.rs"]
mod collect_data_tests;
#[cfg(test)]
#[path = "viewer/control_roundtrip_tests.rs"]
mod control_roundtrip_tests;
#[cfg(test)]
mod market_quote_decision_tests;
#[cfg(test)]
#[path = "viewer/protocol_tests.rs"]
mod protocol_tests;
#[cfg(test)]
#[path = "viewer/protocol_v2_tests.rs"]
mod protocol_v2_tests;
