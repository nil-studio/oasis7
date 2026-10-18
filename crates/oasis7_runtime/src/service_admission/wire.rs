//! Main execution aliases of the unique client contract schema.
pub use oasis7_client_api::world_service_wire::{
    AgentSignerDelegationChangeV1, CHANGES_PATH, CognitionCausalProposalV1, CognitionIntentV1,
    DESCRIBE_PATH, FeedbackAckIntentV1, LOOKUP_PATH, SUBMIT_PATH, SignedReadRequest,
    SignedServiceResponse, VIEW_PATH,
};
pub type SchedulerOperationV1 = oasis7_client_api::world_service_wire::SchedulerOperationV1<
    oasis7_agent_api::ContinuousAgentRequestContextV1,
    crate::CognitionLeaseRequestV1,
    crate::CognitionContinuationProposalV1,
    crate::ContinuationStatusV1,
    crate::CognitionContinuationResumeRequestV1,
    crate::CognitionWakeDispositionV1,
    crate::CognitionContextDigestsV1,
>;
pub type SchedulerIntentV1 =
    oasis7_client_api::world_service_wire::SchedulerIntentV1<SchedulerOperationV1>;
pub type WorldServicePayloadV1 = oasis7_client_api::world_service_wire::WorldServicePayloadV1<
    oasis7_proto::viewer::AgentChatRequest,
    SchedulerIntentV1,
>;
pub type AuthenticatedLookup = oasis7_client_api::world_service_wire::AuthenticatedLookup<
    oasis7_proto::viewer::AgentChatRequest,
    SchedulerIntentV1,
>;
pub type CanonicalIntentResultV1 = oasis7_client_api::world_service_wire::CanonicalIntentResultV1<
    oasis7_proto::viewer::AgentChatRequest,
    SchedulerIntentV1,
>;

use serde::{Deserialize, Serialize};
pub const MEMBERSHIP_PATH: &str = "/v1/world/governance-membership";

/// A lookup carries the caller's signed, externally anchored read request.
/// The original payload retains its independently signed request identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalAuthenticatedLookupV1 {
    pub schema_version: u32,
    pub lookup: AuthenticatedLookup,
    pub read: SignedReadRequest<super::ReadWorldViewRequest>,
}

/// The nested signature authenticates the current audience and anchored view.
/// Cursor and bounds are echoed exactly and independently checked by the client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalAuthenticatedChangesV1 {
    pub schema_version: u32,
    pub cursor: oasis7_client_api::world_service::FormalBusinessChangesCursorV1,
    pub max_items: u32,
    pub max_bytes: u64,
    pub read: SignedReadRequest<super::ReadWorldViewRequest>,
}

#[cfg(test)]
mod feedback_ack_codec_tests {
    use super::*;
    #[test]
    fn feedback_ack_preserves_canonical_digest_encoding_and_rejects_legacy_forms() {
        let digest = oasis7_agent_api::h_v1("oasis7.feedback-ack.codec-test.v1", &"actual input")
            .to_string();
        let original = FeedbackAckIntentV1 {
            agent_id: "agent".into(),
            agent_session_id: "session".into(),
            agent_turn_id: "turn".into(),
            decision_request_id: "decision".into(),
            request_digest: digest.clone(),
            feedback_id: "feedback".into(),
            feedback_seq: 1,
            original_envelope_digest: digest.clone(),
            private_acceptance_digest: digest.clone(),
            runtime_receipt_id: Some("receipt".into()),
            delegation_generation: 1,
        };
        original.validate().unwrap();
        assert_eq!(
            serde_json::from_value::<FeedbackAckIntentV1>(serde_json::to_value(&original).unwrap())
                .unwrap(),
            original
        );
        for invalid in [
            digest.strip_prefix("blake3:").unwrap().to_string(),
            digest.to_uppercase(),
            format!("sha256:{}", "0".repeat(64)),
            "blake3:00".into(),
            format!("blake3:{}", "g".repeat(64)),
        ] {
            let mut value = original.clone();
            value.request_digest = invalid.clone();
            assert!(value.validate().is_err());
            let mut value = original.clone();
            value.original_envelope_digest = invalid.clone();
            assert!(value.validate().is_err());
            let mut value = original.clone();
            value.private_acceptance_digest = invalid;
            assert!(value.validate().is_err());
        }
    }
}

#[cfg(test)]
mod task_profile_wire_tests {
    use super::*;
    use oasis7_client_api::world_service::{sign_read_request, signing_bytes};
    use oasis7_agent_api::game::{DecimalU64, GAME_TASK_CREATE_DOMAIN, GameTaskCreateV1};

    #[test]
    fn execution_and_native_task_profiles_preserve_signed_json_and_canonical_cbor() {
        let signed = sign_read_request(
            GAME_TASK_CREATE_DOMAIN,
            GameTaskCreateV1 {
                api_rev: 1,
                client_operation_id: "create".into(),
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(0),
                agent_id: "agent".into(),
                owner_binding: "owner".into(),
                owner_generation: DecimalU64(1),
            },
            &hex::encode([7; 32]),
        )
        .unwrap();
        let execution = WorldServicePayloadV1::GameTaskCreate(signed.clone());
        let native: oasis7_client_api::world_service_wire::WorldServicePayloadV1 =
            oasis7_client_api::world_service_wire::WorldServicePayloadV1::GameTaskCreate(signed);
        assert_eq!(
            serde_json::to_vec(&execution).unwrap(),
            serde_json::to_vec(&native).unwrap()
        );
        assert_eq!(
            signing_bytes("intent", &execution).unwrap(),
            signing_bytes("intent", &native).unwrap()
        );
        let world = oasis7_client_api::world_service::WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        };
        assert_eq!(
            crate::service_admission::derive_correlation(world.clone(), &execution)
                .unwrap(),
            oasis7_client_api::world_service_correlation::derive_correlation(world, &native)
                .unwrap()
        );
    }
}
