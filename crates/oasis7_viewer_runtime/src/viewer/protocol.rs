use oasis7_proto::viewer as proto;

use crate::simulator::{AgentDecisionTrace, RunnerMetrics, WorldEvent, WorldSnapshot};
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

pub use oasis7_client_api::viewer_protocol::*;
pub type ViewerResponse =
    proto::ViewerResponse<WorldSnapshot, WorldEvent, AgentDecisionTrace, RunnerMetrics, WorldTime>;
pub type PromptControlAck = proto::PromptControlAck<WorldTime>;
pub type AgentChatAck = proto::AgentChatAck<WorldTime>;
pub type GameplayActionAck = proto::GameplayActionAck<WorldTime>;
pub type ControlCompletionAck = proto::ControlCompletionAck<WorldTime>;

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
