//! Viewer wire contracts and capability negotiation, independent of a host world.
use oasis7_agent_api::world_geometry::ChunkCoord;
use oasis7_proto::viewer as proto;
pub use proto::RollbackApprovalSignature;
pub use proto::{
    AdjudicateSocialFactQuotePreflight, AdjudicateSocialFactQuoteRequest, AgentChatError,
    AgentChatRequest, AuthoritativeBatchFinality, AuthoritativeChallengeAck,
    AuthoritativeChallengeCommand, AuthoritativeChallengeError,
    AuthoritativeChallengeResolveRequest, AuthoritativeChallengeStatus,
    AuthoritativeChallengeSubmitRequest, AuthoritativeFinalityState,
    AuthoritativeReconnectSyncRequest, AuthoritativeRecoveryAck, AuthoritativeRecoveryCommand,
    AuthoritativeRecoveryError, AuthoritativeRecoveryStatus, AuthoritativeRollbackReceipt,
    AuthoritativeRollbackRequest, AuthoritativeRollbackV2Request,
    AuthoritativeSessionRegisterRequest, AuthoritativeSessionRevokeRequest,
    AuthoritativeSessionRotateRequest, CollectDataCommand, CollectDataPreflight,
    CollectDataRequest, ControlCompletionStatus, DIRECTOR_CAPABILITY_ACTION,
    DIRECTOR_CAPABILITY_AUDIENCE, DIRECTOR_CAPABILITY_DOMAIN, DIRECTOR_CAPABILITY_GRANT_VERSION,
    DIRECTOR_CAPABILITY_MAX_TTL_MS, DIRECTOR_CAPABILITY_SCOPE,
    DIRECTOR_CAPABILITY_SIGNATURE_V1_PREFIX, DeclareSocialEdgeQuotePreflight,
    DeclareSocialEdgeQuoteRequest, DirectorCapabilityGrant, FirstContactClass,
    FragmentRefillElementRemaining, FragmentRefillPreviewChunk,
    FragmentRefillPreviewProtocolRequest, FragmentRefillPreviewResponse,
    GOVERNED_ROLLBACK_REPLAY_CAPABILITY, GameplayActionError, GameplayActionRequest,
    GovernanceVoteQuotePreflight, GovernanceVoteQuoteRequest, HostedSessionReleaseRequest,
    HostedStrongAuthGrant, LiveControl, MarketQuoteDecisionPreflight, MarketQuoteDecisionRequest,
    MarketQuoteMaterialContribution, MarketQuoteMaterialRequest, NegotiatedViewerProtocol,
    PROMPT_CONTROL_RESULT_CAPABILITY, PlaybackControl, PlayerActionDisposition, PlayerAuthProof,
    PlayerAuthScheme, PlayerCompensationState, PlayerCompensationStatus, PlayerRollbackDisposition,
    PowerSaleQuotePreflight, PowerSaleQuoteRequest, PowerSurvivalQuotePreflight,
    PowerSurvivalQuoteRequest, PowerSurvivalRecoveryAction, ProductValidationQuotePreflight,
    ProductValidationQuoteRequest, PromptControlApplicationScope, PromptControlApplyRequest,
    PromptControlCommand, PromptControlError, PromptControlOperation, PromptControlResultStatus,
    PromptControlRollbackRequest, PromptControlValueVisibility, PublishSocialFactQuotePreflight,
    PublishSocialFactQuoteRequest, PublishSocialFactQuoteStake,
    REVOKE_SOCIAL_FACT_QUOTE_CAPABILITY, RefineQuotePreflight, RefineQuoteRequest,
    RevokeSocialFactQuotePreflight, RevokeSocialFactQuoteRequest, RollbackAttributionResolution,
    RollbackAttributionResolutionRequest, RollbackAuthorityRole, RollbackAuthorizationEnvelope,
    RollbackCheckpointRef, RollbackCompensationTransitionRequest, RollbackIntent,
    RollbackOperatorAuthorization, RollbackReceiptAccessRequest, RollbackReplayTarget,
    RollbackSourceEventRef, RollbackStrictAuditEvidence, ScheduleRecipeQuotePreflight,
    ScheduleRecipeQuoteRequest, SocialAdjudicationDecision, SocialContactQuotePreflight,
    SocialContactQuoteRequest, TransferMaterialPriority, TransferMaterialQuotePreflight,
    TransferMaterialQuoteRequest, VIEWER_PROTOCOL_VERSION, ViewerControl, ViewerControlProfile,
    ViewerEventKind, ViewerRequest, ViewerStream, WORLD_FEED_SCHEMA_VERSION,
    WarDeclarationQuotePreflight, WarDeclarationQuoteRequest, WorldFeedEnvelope, WorldFeedEvent,
    WorldFeedGapReason, WorldFeedMajorEvent, WorldFeedMajorEventAnchor,
    WorldFeedMajorEventCausalReference, WorldFeedMajorEventIdentity, WorldFeedMajorEventPosition,
    WorldFeedMajorEventSource, WorldFeedStatus, WorldFeedUnavailableReason,
};

/// Whether a negotiated Viewer session may issue the social-fact revocation quote.
///
/// The capability is intentionally checked independently of authentication: this
/// only protects mixed-version protocol readers from an unsupported request; the
/// quote handler still validates the signed request itself.
pub fn viewer_protocol_supports_revoke_social_fact_quote(
    protocol: &NegotiatedViewerProtocol,
) -> bool {
    protocol.version >= 2
        && protocol
            .capabilities
            .iter()
            .any(|value| value == REVOKE_SOCIAL_FACT_QUOTE_CAPABILITY)
}

pub fn viewer_protocol_supports_prompt_control_result(protocol: &NegotiatedViewerProtocol) -> bool {
    protocol.version >= VIEWER_PROTOCOL_VERSION
        && protocol
            .capabilities
            .iter()
            .any(|value| value == PROMPT_CONTROL_RESULT_CAPABILITY)
}

/// A signed, read-only request for the current chunk's fragment-replenishment forecast.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FragmentRefillPreviewRequest {
    pub chunk: ChunkCoord,
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<PlayerAuthProof>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainLinkPolicy {
    Enforcing,
    Shadow,
}

impl ChainLinkPolicy {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "enforcing" => Some(Self::Enforcing),
            "shadow" => Some(Self::Shadow),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enforcing => "enforcing",
            Self::Shadow => "shadow",
        }
    }

    pub fn records_player_facing_chain_failures(self) -> bool {
        matches!(self, Self::Enforcing)
    }
}

/// A client wire reader may retain rich host payloads as JSON without linking a world.
pub type ViewerResponse = proto::ViewerResponse<
    serde_json::Value,
    serde_json::Value,
    serde_json::Value,
    serde_json::Value,
    u64,
>;

const RUNTIME_AGENT_CHAT_ECHO_ENV: &str = "OASIS7_RUNTIME_AGENT_CHAT_ECHO";
pub fn runtime_agent_chat_echo_enabled_from_env() -> bool {
    std::env::var(RUNTIME_AGENT_CHAT_ECHO_ENV)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}
