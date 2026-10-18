use oasis7_proto::viewer as proto;

use crate::simulator::{AgentDecisionTrace, RunnerMetrics, WorldEvent, WorldSnapshot};
use oasis7_agent_api::world_geometry::ChunkCoord;
#[cfg(test)]
use oasis7_simulator::WorldEventKind;
use oasis7_simulator::{FragmentRefillPreview, WorldTime};

mod event_kind_match;
pub use event_kind_match::viewer_event_kind_matches;

#[cfg(test)]
mod capability_tests {
    use super::*;

    #[test]
    fn legacy_v2_ack_without_revoke_capability_suppresses_revoke_request() {
        let legacy_v2 = NegotiatedViewerProtocol {
            version: 2,
            capabilities: Vec::new(),
        };
        let current_v2 = NegotiatedViewerProtocol {
            version: 2,
            capabilities: vec![REVOKE_SOCIAL_FACT_QUOTE_CAPABILITY.to_string()],
        };

        assert!(!viewer_protocol_supports_revoke_social_fact_quote(
            &legacy_v2
        ));
        assert!(viewer_protocol_supports_revoke_social_fact_quote(
            &current_v2
        ));
    }
}

#[cfg(test)]
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

pub type ViewerResponse =
    proto::ViewerResponse<WorldSnapshot, WorldEvent, AgentDecisionTrace, RunnerMetrics, WorldTime>;
pub type PromptControlAck = proto::PromptControlAck<WorldTime>;
pub type AgentChatAck = proto::AgentChatAck<WorldTime>;
pub type GameplayActionAck = proto::GameplayActionAck<WorldTime>;
pub type ControlCompletionAck = proto::ControlCompletionAck<WorldTime>;

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

/// The authoritative, non-mutating kernel forecast returned for a fragment-refill preflight.
pub type FragmentRefillPreviewPreflight = FragmentRefillPreview;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_request_round_trip() {
        let request = ViewerRequest::Control {
            mode: ViewerControl::Step { count: 2 },
            request_id: Some(1),
        };
        let json = serde_json::to_string(&request).expect("serialize request");
        let parsed: ViewerRequest = serde_json::from_str(&json).expect("deserialize request");
        assert_eq!(parsed, request);
    }

    #[test]
    fn transfer_material_quote_request_and_response_round_trip() {
        // RED contract for issue #3148: the signed, read-only logistics quote must be a
        // first-class Viewer request/response pair.  Keep this fixture JSON-based so the
        // protocol test fails on the missing production variants rather than depending on
        // test-only DTOs or setup.
        let request_json = serde_json::json!({
            "type": "quote_transfer_material",
            "request": {
                "requester_agent_id": "agent-quote",
                "from_ledger": "site:source",
                "to_ledger": "site:destination",
                "kind": "iron_ingot",
                "amount": 20,
                "distance_km": 200,
                "player_id": "player-quote",
                "public_key": "11".repeat(32),
            }
        });
        let request: ViewerRequest =
            serde_json::from_value(request_json).expect("decode transfer-material quote request");
        let request_encoded = serde_json::to_value(&request).expect("encode quote request");
        let reparsed: ViewerRequest =
            serde_json::from_value(request_encoded).expect("redecode quote request");
        assert_eq!(reparsed, request);

        let response_json = serde_json::json!({
            "type": "transfer_material_quote_preflight",
            "quote": {
                "requester_agent_id": "agent-quote",
                "from_ledger": "site:source",
                "to_ledger": "site:destination",
                "kind": "iron_ingot",
                "requested_amount": 20,
                "submission_feasible": true,
                "max_transferable_amount": 40,
                "sent_amount": 20,
                "distance_km": 200,
                "loss_bps": 5,
                "expected_loss_amount": 2,
                "expected_received_amount": 18,
                "source_amount_before": 40,
                "source_amount_after": 20,
                "destination_amount_before": 0,
                "destination_expected_amount_after": 18,
                "ticks_until_arrival": 2,
                "ready_at": 3,
                "effective_priority": "standard",
                "priority_reason": "material_default_priority",
                "inflight_before": 0,
                "inflight_capacity": 2,
                "recommendation": "submit_transfer",
                "conditional": true,
            }
        });
        let response: ViewerResponse =
            serde_json::from_value(response_json).expect("decode transfer-material quote response");
        let response_encoded = serde_json::to_value(&response).expect("encode quote response");
        let reparsed: ViewerResponse =
            serde_json::from_value(response_encoded).expect("redecode quote response");
        assert_eq!(reparsed, response);
    }

    #[test]
    fn viewer_event_kind_matches_world_event_kind() {
        assert!(viewer_event_kind_matches(
            &ViewerEventKind::AgentMoved,
            &WorldEventKind::AgentMoved {
                agent_id: "a1".to_string(),
                from: "loc-a".to_string(),
                to: "loc-b".to_string(),
                distance_cm: 100,
                electricity_cost: 1,
            },
        ));
        assert!(!viewer_event_kind_matches(
            &ViewerEventKind::PromptUpdated,
            &WorldEventKind::AgentMoved {
                agent_id: "a1".to_string(),
                from: "loc-a".to_string(),
                to: "loc-b".to_string(),
                distance_cm: 100,
                electricity_cost: 1,
            },
        ));
        assert!(viewer_event_kind_matches(
            &ViewerEventKind::AgentSpoke,
            &WorldEventKind::AgentSpoke {
                agent_id: "a1".to_string(),
                location_id: "loc-a".to_string(),
                message: "hi".to_string(),
                target_agent_id: None,
            },
        ));
        assert!(viewer_event_kind_matches(
            &ViewerEventKind::TargetInspected,
            &WorldEventKind::TargetInspected {
                agent_id: "a1".to_string(),
                target_kind: "location".to_string(),
                target_id: "loc-a".to_string(),
            },
        ));
        assert!(viewer_event_kind_matches(
            &ViewerEventKind::SimpleInteractionPerformed,
            &WorldEventKind::SimpleInteractionPerformed {
                agent_id: "a1".to_string(),
                target_kind: "location".to_string(),
                target_id: "loc-a".to_string(),
                interaction: "press_console".to_string(),
            },
        ));
        assert!(viewer_event_kind_matches(
            &ViewerEventKind::RuntimeEvent,
            &WorldEventKind::RuntimeEvent {
                kind: "snapshot_created".to_string(),
                domain_kind: None,
            },
        ));
    }
}
