//! Shared consensus action wire only. Generic side payloads preserve the exact
//! legacy application contracts without a Runtime -> upper-server dependency.
use crate::MainTokenActionAuthProof;
use oasis7_agent_api::world_action::{Action, GovernanceAuthorizationV1};
use oasis7_proto::gameplay_submission_origin::GameplaySubmissionOrigin;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsensusActionPayloadEnvelope<B> {
    pub version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gameplay_submission_origin: Option<GameplaySubmissionOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<ConsensusActionAuthEnvelope>,
    pub body: B,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ConsensusActionAuthEnvelope {
    MainTokenAction(MainTokenActionAuthProof),
    GovernanceAction(GovernanceAuthorizationV1),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ConsensusActionPayloadBody<S, U: Default, C, W> {
    WorldCleanupV2 {
        request: C,
    },
    WorldServiceIntent {
        request: W,
    },
    RuntimeAction {
        action: Action,
    },
    SimulatorAction {
        action: S,
        #[serde(default)]
        submitter: U,
    },
}
impl<S, U: Default, C, W> ConsensusActionPayloadEnvelope<ConsensusActionPayloadBody<S, U, C, W>> {
    pub fn from_runtime_action(action: Action) -> Self {
        Self {
            version: 1,
            gameplay_submission_origin: None,
            auth: None,
            body: ConsensusActionPayloadBody::RuntimeAction { action },
        }
    }
    pub fn from_recipe_submission(action: Action, origin: GameplaySubmissionOrigin) -> Self {
        Self {
            version: 2,
            gameplay_submission_origin: Some(origin),
            auth: None,
            body: ConsensusActionPayloadBody::RuntimeAction { action },
        }
    }
    pub fn from_runtime_action_with_auth(
        action: Action,
        auth: ConsensusActionAuthEnvelope,
    ) -> Self {
        Self {
            version: 1,
            gameplay_submission_origin: None,
            auth: Some(auth),
            body: ConsensusActionPayloadBody::RuntimeAction { action },
        }
    }
    pub fn from_simulator_action(action: S, submitter: U) -> Self {
        Self {
            version: 1,
            gameplay_submission_origin: None,
            auth: None,
            body: ConsensusActionPayloadBody::SimulatorAction { action, submitter },
        }
    }
}

/// Resource-admitted projections may skip an unrelated side payload. They
/// must still send that original packet to its complete typed consumer; this
/// marker never accepts or applies Simulator, Service or Cleanup data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct IgnoredSidePayloadV1;
impl<'de> Deserialize<'de> for IgnoredSidePayloadV1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(deserializer).map(|_| Self)
    }
}
pub type RuntimePayloadProjectionV1 = ConsensusActionPayloadEnvelope<
    ConsensusActionPayloadBody<
        IgnoredSidePayloadV1,
        IgnoredSidePayloadV1,
        IgnoredSidePayloadV1,
        IgnoredSidePayloadV1,
    >,
>;

/// Pure views of the one generic wire declaration. A view grants no authority.
pub trait ConsensusPayloadBodyViewV1 {
    fn runtime_action(&self) -> Option<&Action>;
    fn is_cleanup_v2(&self) -> bool;
}
impl<S, U: Default, C, W> ConsensusPayloadBodyViewV1 for ConsensusActionPayloadBody<S, U, C, W> {
    fn runtime_action(&self) -> Option<&Action> {
        match self {
            Self::RuntimeAction { action } => Some(action),
            _ => None,
        }
    }
    fn is_cleanup_v2(&self) -> bool {
        matches!(self, Self::WorldCleanupV2 { .. })
    }
}
#[path = "consensus_action_wire_decode.rs"]
mod decode;
pub use decode::{
    ConsensusPayloadDecodeOpsV1, DecodedConsensusPayloadV1,
    decode_consensus_action_payload_envelope_with,
};

/// Original committed metadata predicates. Codec resource admission belongs
/// to the caller; business signature authority is still evaluated by World.
pub fn validate_committed_payload_metadata_v1<B: ConsensusPayloadBodyViewV1>(
    envelope: &ConsensusActionPayloadEnvelope<B>,
) -> Result<(), String> {
    let governance = match &envelope.auth {
        Some(ConsensusActionAuthEnvelope::GovernanceAction(proof)) => Some(proof),
        _ => None,
    };
    let admission = matches!(
        envelope.body.runtime_action(),
        Some(
            Action::SubmitGovernanceValidatorAdmission { .. }
                | Action::ApproveGovernanceValidatorAdmission { .. }
                | Action::ActivateGovernanceValidatorAdmission { .. }
                | Action::RevokeGovernanceValidatorAdmission { .. }
                | Action::RotateGovernanceValidatorFinalityKeyV1 { .. }
                | Action::RotateGovernanceBootstrapFinalityKeyV1 { .. }
        )
    );
    if admission != governance.is_some() {
        return Err("committed validator admission governance authorization mismatch".into());
    }
    if let Some(proof) = governance {
        proof.validate()?;
        if proof.signatures.is_empty() || envelope.gameplay_submission_origin.is_some() {
            return Err("invalid committed governance authorization metadata".into());
        }
    }
    Ok(())
}

#[path = "consensus_action_wire_layout.rs"]
mod layout;
pub use layout::{
    ConsensusPayloadOwnedLayoutV1, canonical_intent_result_owned_layout_v1,
    capability_invocation_context_owned_layout_v1, legacy_payload_owned_layout_v1,
    runtime_payload_owned_layout_v1, task_cleanup_request_owned_layout_v1,
    task_closed_owned_layout_v1,
};
