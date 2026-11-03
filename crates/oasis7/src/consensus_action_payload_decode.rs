//! Heap-backed decode temporaries preserve the public payload and wire layout.
use super::*;

#[derive(Deserialize)]
#[serde(rename = "ConsensusActionPayloadEnvelope")]
struct Envelope {
    version: u8,
    #[serde(default)]
    gameplay_submission_origin: Option<Box<runtime::GameplaySubmissionOrigin>>,
    #[serde(default)]
    auth: Option<Box<ConsensusActionAuthEnvelope>>,
    body: Box<ConsensusActionPayloadBody>,
}

impl<'de> Deserialize<'de> for ConsensusActionPayloadEnvelope {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Envelope::deserialize(deserializer).map(Envelope::into_public)
    }
}

impl Envelope {
    #[inline(never)]
    fn into_public(self) -> ConsensusActionPayloadEnvelope {
        ConsensusActionPayloadEnvelope {
            version: self.version,
            gameplay_submission_origin: self.gameplay_submission_origin.map(|value| *value),
            auth: self.auth.map(|value| *value),
            body: *self.body,
        }
    }
}

#[derive(Deserialize)]
#[serde(
    rename = "ConsensusActionPayloadBody",
    tag = "type",
    content = "data",
    rename_all = "snake_case"
)]
enum Body {
    WorldServiceIntent {
        request: Box<
            crate::world_service::SubmitIntentRequest<crate::world_service::WorldServicePayloadV1>,
        >,
    },
    RuntimeAction {
        action: Box<runtime::Action>,
    },
    SimulatorAction {
        action: Box<SimulatorAction>,
        #[serde(default)]
        submitter: Box<ActionSubmitter>,
    },
}

impl<'de> Deserialize<'de> for ConsensusActionPayloadBody {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Body::deserialize(deserializer).map(Body::into_public)
    }
}

impl Body {
    #[inline(never)]
    fn into_public(self) -> ConsensusActionPayloadBody {
        match self {
            Self::WorldServiceIntent { request } => {
                ConsensusActionPayloadBody::WorldServiceIntent { request: *request }
            }
            Self::RuntimeAction { action } => {
                ConsensusActionPayloadBody::RuntimeAction { action: *action }
            }
            Self::SimulatorAction { action, submitter } => {
                ConsensusActionPayloadBody::SimulatorAction {
                    action: *action,
                    submitter: *submitter,
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "consensus_action_payload_decode_tests.rs"]
mod tests;
