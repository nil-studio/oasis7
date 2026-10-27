//! Unique World Service intent wire schema. Generic parameters preserve legacy
//! browser/provider DTOs without importing runtime or model implementations.
//! The native task profile rejects unsupported legacy variants during decoding.
use serde::{Deserialize, Serialize};

pub use crate::world_service::{
    CHANGES_PATH, DESCRIBE_PATH, LOOKUP_PATH, SUBMIT_PATH, SignedReadRequest,
    SignedServiceResponse, VIEW_PATH,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthenticatedLookup<C = UnsupportedWireVariant, S = TaskSchedulerIntentV1> {
    pub request: crate::world_service::LookupIntentRequest,
    pub original: WorldServicePayloadV1<C, S>,
}

/// Canonical execution result stored inside the journaled authorization state.
/// The enclosing execution record supplies the final CommitRef after hashing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalIntentResultV1<C = UnsupportedWireVariant, S = TaskSchedulerIntentV1> {
    pub request: crate::world_service::SubmitIntentRequest<WorldServicePayloadV1<C, S>>,
    pub action_id: u64,
    pub committed_height: u64,
    pub receipt: serde_json::Value,
    pub rejected: Option<String>,
}

/// Preserve legacy JSON payload bytes. Wrapping them adds no legacy authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "Published signed payload variants preserve the existing typed API and canonical serialization boundary"
)]
pub enum WorldServicePayloadV1<C = UnsupportedWireVariant, S = TaskSchedulerIntentV1> {
    GameStarterClaim(SignedReadRequest<oasis7_agent_api::game::GameStarterClaimV1>),
    GameParticipantRegister {
        owner: SignedReadRequest<oasis7_agent_api::game::GameParticipantRegisterV1>,
        admission: SignedReadRequest<oasis7_agent_api::game::GameParticipantAdmissionV1>,
    },
    GameplayJson(Vec<u8>),
    AgentChat(C),
    SafeAlternative(SignedReadRequest<crate::safe_alternative::SafeAlternativeCommandV1>),
    Cognition(SignedReadRequest<CognitionIntentV1>),
    FeedbackAck(SignedReadRequest<FeedbackAckIntentV1>),
    Delegation(SignedReadRequest<AgentSignerDelegationChangeV1>),
    Scheduler(SignedReadRequest<S>),
    GameTaskCreate(SignedReadRequest<oasis7_agent_api::game::GameTaskCreateV1>),
    GameGoalPublish(SignedReadRequest<oasis7_agent_api::game::GameGoalPublishV1>),
    GameExecutorControl(SignedReadRequest<oasis7_agent_api::game::GameExecutorControlV1>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchedulerIntentV1<O = TaskSchedulerOperationV1> {
    pub agent_id: String,
    pub request_id: String,
    pub delegation_generation: u64,
    /// Zero preserves historical exact-base semantics and signed wire bytes.
    /// One selects the bounded task-prefix lease; it is valid only for TaskProviderPrefix.
    #[serde(default, skip_serializing_if = "is_zero_revision")]
    pub task_prefix_binding_revision: u16,
    pub captured_base_binding: oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1,
    pub operation: O,
}

fn is_zero_revision(value: &u16) -> bool {
    *value == 0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", content = "data", rename_all = "snake_case")]
pub enum SchedulerOperationV1<
    P = UnsupportedWireVariant,
    L = UnsupportedWireVariant,
    C = UnsupportedWireVariant,
    S = UnsupportedWireVariant,
    R = UnsupportedWireVariant,
    D = UnsupportedWireVariant,
    X = UnsupportedWireVariant,
> {
    TaskProviderPrefix {
        context: oasis7_agent_api::game::TaskContextIdentityV1,
    },
    TaskProviderFailure {
        context: oasis7_agent_api::game::TaskContextIdentityV1,
        reason: String,
    },
    ProviderPrefix {
        request: P,
        context_digest: String,
    },
    ProviderFailure {
        request: P,
        reason: String,
    },
    ReserveLease(L),
    SettleLease {
        lease_id: String,
        consumed_amount: u64,
    },
    ReleaseLease {
        lease_id: String,
    },
    AdmitContinuation(C),
    TransitionContinuation {
        continuation_id: String,
        to: S,
        logical_tick: u64,
    },
    ConsumeContinuationBudget {
        continuation_id: String,
        budget_spent: u64,
        current_context: X,
    },
    ResumeWake {
        wake_id: String,
        proposal: C,
        budget_spent: u64,
        resume: R,
        current_context: X,
    },
    HandoffWake {
        wake_id: String,
        disposition: D,
        current_context: X,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CognitionIntentV1 {
    pub request: oasis7_agent_api::world_cognition::RuntimeCognitionCommitRequestV1,
    pub action: oasis7_agent_api::world_action::Action,
    pub response_artifact: oasis7_agent_api::world_cognition::RuntimeCognitionResponseArtifactV1,
    pub delegation_generation: u64,
    // Omitting legacy absence preserves the original signed codec bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causal_proposal: Option<CognitionCausalProposalV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_fence: Option<oasis7_agent_api::game::GameTaskFenceV1>,
}

/// Public Agent explanation, signed as part of the cognition intent. Canonical
/// intent and owner-control authority are deliberately absent from this codec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CognitionCausalProposalV1 {
    pub expected_consequence: serde_json::Value,
    pub alternative: serde_json::Value,
    pub stakes: serde_json::Value,
    pub reason: Option<String>,
    pub evidence_refs: Vec<String>,
    pub correction_refs: Vec<String>,
    pub dissent: Option<String>,
}

/// Owner-signed, canonical registration/revocation. Generation is a fence,
/// never inferred from a provider-local key cache.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSignerDelegationChangeV1 {
    pub world_id: String,
    pub branch_id: String,
    pub agent_id: String,
    pub owner_binding: String,
    pub agent_identity_generation: u64,
    pub generation: u64,
    pub delegate_public_key: String,
    pub revoked: bool,
    pub nonce: u64,
}

/// Agent acknowledgement of an already published canonical feedback envelope.
/// The digest is of the original canonical payload, never a sanitized View.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackAckIntentV1 {
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: String,
    pub feedback_id: String,
    pub feedback_seq: u64,
    pub original_envelope_digest: String,
    /// Opaque signed Agent statement. Runtime validates its codec, not private
    /// memory contents or acceptance; the App persists acceptance before ACK.
    pub private_acceptance_digest: String,
    pub runtime_receipt_id: Option<String>,
    pub delegation_generation: u64,
}
impl FeedbackAckIntentV1 {
    pub fn validate(&self) -> Result<(), String> {
        let bounded = |value: &str| !value.trim().is_empty() && value.len() <= 256;
        let digest = |value: &str| {
            value.strip_prefix("blake3:").is_some_and(|hex| {
                hex.len() == 64
                    && hex
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        };
        if ![
            &self.agent_id,
            &self.agent_session_id,
            &self.agent_turn_id,
            &self.decision_request_id,
            &self.feedback_id,
        ]
        .iter()
        .all(|v| bounded(v))
            || self.feedback_seq == 0
            || !digest(&self.request_digest)
            || !digest(&self.original_envelope_digest)
            || !digest(&self.private_acceptance_digest)
            || self
                .runtime_receipt_id
                .as_deref()
                .is_some_and(|v| !bounded(v))
        {
            return Err("feedback acknowledgement identity invalid".into());
        }
        Ok(())
    }
}

/// Uninhabited native-task profile marker: unsupported legacy wire variants
/// fail decoding rather than accepting an unvalidated or opaque JSON payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnsupportedWireVariant {}

pub type TaskSchedulerOperationV1 = SchedulerOperationV1;
pub type TaskSchedulerIntentV1 = SchedulerIntentV1<TaskSchedulerOperationV1>;

/// A fully typed execution profile using the same canonical enum schema.
pub type WorldServicePayloadProfileV1<C, P, L, CP, ST, R, D, X> =
    WorldServicePayloadV1<C, SchedulerIntentV1<SchedulerOperationV1<P, L, CP, ST, R, D, X>>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_service::{WorldIdentity, sign_read_request, signing_bytes};
    use oasis7_agent_api::game::{DecimalU64, GAME_TASK_CREATE_DOMAIN, GameTaskCreateV1};
    #[test]
    fn native_profile_preserves_task_signed_codec_and_rejects_unsupported_variants() {
        let task = GameTaskCreateV1 {
            api_rev: 1,
            client_operation_id: "create".into(),
            world_id: "w".into(),
            genesis_digest: "g".into(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(0),
            agent_id: "agent".into(),
            owner_binding: "owner".into(),
            owner_generation: DecimalU64(1),
        };
        let native: WorldServicePayloadV1 = WorldServicePayloadV1::GameTaskCreate(
            sign_read_request(GAME_TASK_CREATE_DOMAIN, task, &hex::encode([7; 32])).unwrap(),
        );
        type OtherProfile = WorldServicePayloadV1<
            String,
            SchedulerIntentV1<
                SchedulerOperationV1<String, String, String, String, String, String, String>,
            >,
        >;
        let delegation = WorldServicePayloadV1::Delegation(
            sign_read_request(
                "delegation",
                AgentSignerDelegationChangeV1 {
                    world_id: "w".into(),
                    branch_id: "main".into(),
                    agent_id: "agent".into(),
                    owner_binding: "owner".into(),
                    agent_identity_generation: 1,
                    generation: 2,
                    delegate_public_key: hex::encode([8; 32]),
                    revoked: false,
                    nonce: 3,
                },
                &hex::encode([7; 32]),
            )
            .unwrap(),
        );
        let digest = format!("blake3:{}", "a".repeat(64));
        let feedback = WorldServicePayloadV1::FeedbackAck(
            sign_read_request(
                "feedback_ack",
                FeedbackAckIntentV1 {
                    agent_id: "agent".into(),
                    agent_session_id: "session".into(),
                    agent_turn_id: "turn".into(),
                    decision_request_id: "request".into(),
                    request_digest: digest.clone(),
                    feedback_id: "feedback".into(),
                    feedback_seq: 1,
                    original_envelope_digest: digest.clone(),
                    private_acceptance_digest: digest,
                    runtime_receipt_id: None,
                    delegation_generation: 1,
                },
                &hex::encode([7; 32]),
            )
            .unwrap(),
        );
        for payload in [&native, &delegation, &feedback] {
            let other: OtherProfile =
                serde_json::from_slice(&serde_json::to_vec(payload).unwrap()).unwrap();
            assert_eq!(
                serde_json::to_vec(payload).unwrap(),
                serde_json::to_vec(&other).unwrap()
            );
            assert_eq!(
                signing_bytes("intent", payload).unwrap(),
                signing_bytes("intent", &other).unwrap()
            );
        }
        let other: OtherProfile =
            serde_json::from_slice(&serde_json::to_vec(&native).unwrap()).unwrap();
        let world = WorldIdentity {
            world_id: "w".into(),
            genesis_digest: "g".into(),
        };
        let expected =
            crate::world_service_correlation::derive_correlation(world.clone(), &native).unwrap();
        assert_eq!(
            expected,
            crate::world_service_correlation::derive_correlation_with_legacy(
                world.clone(),
                &other,
                |_| panic!("task cannot invoke browser verifier")
            )
            .unwrap()
        );
        let mut altered = native.clone();
        if let WorldServicePayloadV1::GameTaskCreate(signed) = &mut altered {
            signed.request.agent_id = "tampered".into();
        }
        assert!(crate::world_service_correlation::derive_correlation(world, &altered).is_err());
        assert!(
            serde_json::from_value::<WorldServicePayloadV1>(
                serde_json::json!({"kind":"agent_chat","payload":"legacy"})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<TaskSchedulerOperationV1>(serde_json::json!({"operation":"provider_prefix","data":{"request":{},"context_digest":"d"}})).is_err());
    }
}
