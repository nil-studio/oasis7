//! Closed private participant actions. No caller-supplied authority or arbitrary signing bytes.
use oasis7_agent_api::game::GameParticipantAdmissionV1;
use oasis7_client_api::world_service::{
    SignedReadRequest, participant_discovery::ParticipantPolicyOriginalV1,
};
use oasis7_custody_core::provisioning::{
    ParticipantFinalResultV1, ParticipantProvisionRequestV1, ParticipantSessionEvidenceV1,
    ParticipantSignBundleV1, ParticipantStepV1,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ParticipantRuntimeAction {
    GetPermit {
        step: ParticipantStepV1,
    },
    PublishBundle {
        candidate: ParticipantSignBundleV1,
    },
    RecordPolicy {
        original: ParticipantPolicyOriginalV1,
        session: ParticipantSessionEvidenceV1,
    },
    RecordAdmission {
        admission: SignedReadRequest<GameParticipantAdmissionV1>,
        session: ParticipantSessionEvidenceV1,
    },
    RecordResult {
        result: Box<ParticipantFinalResultV1>,
        session: ParticipantSessionEvidenceV1,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ParticipantRuntimeRpc {
    pub request: ParticipantProvisionRequestV1,
    pub action: ParticipantRuntimeAction,
}
