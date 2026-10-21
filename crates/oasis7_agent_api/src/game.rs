//! External Game API revision 1 wire contracts. Parsing never admits a world
//! operation: authentication, context validation and durable admission belong
//! to the host and Runtime.
//!
//! Implements the initial codec portion of the external-agent Game API design
//! (#4521, following product #4493). Hosts must call [`parse_request`] on raw
//! bytes, then [`ActionSubmissionV1::validate`], before verifying the actor proof
//! and checking the Runtime context. Deserializing a prebuilt JSON value cannot
//! detect duplicate keys that were already discarded. Action arguments remain
//! dynamic and must subsequently be checked against the Runtime action schema.
//!
//! HTTP routing, pairing, proof preparation, signed World Service reads and
//! durable operation admission are not implemented by this module. In
//! particular, an operation status is a wire value, never a finality proof.

mod participant;
pub use participant::*;
mod executor;
mod goal;
mod goal_prepare;
mod goal_publish;
pub use executor::*;
mod json;
mod task;
pub use goal::*;
pub use goal_prepare::*;
pub use goal_publish::*;
pub use json::{parse_bounded_json, parse_request};
pub use task::*;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

pub const API_REVISION: u32 = 1;
pub const MAX_REQUEST_BYTES: usize = 256 * 1024;
pub const MAX_JSON_DEPTH: usize = 16;

pub const READ_SCHEMA_REVISION: u32 = 4;
pub const READ_OPENAPI_PATH: &str = "/v1/game/openapi.json";
pub const MAX_READ_PROOF_HEADER_BYTES: usize = 8192;
pub const MAX_READ_HTTP_HEADER_BYTES: usize = 16384;
pub const MAX_ACTIVE_READS: usize = 8;
pub const MAX_READ_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Machine-readable contract for the implemented signed-read bootstrap only.
/// Publishing this document grants no authority or action capability.
pub fn read_bootstrap_openapi() -> serde_json::Value {
    serde_json::from_str(include_str!("game/read_bootstrap.openapi.json"))
        .expect("bundled Game API OpenAPI document must be valid JSON")
}

pub fn read_bootstrap_limits() -> serde_json::Value {
    serde_json::json!({
        "max_proof_header_bytes": MAX_READ_PROOF_HEADER_BYTES,
        "max_http_header_bytes": MAX_READ_HTTP_HEADER_BYTES,
        "max_json_depth": MAX_JSON_DEPTH,
        "max_active_reads": MAX_ACTIVE_READS,
        "max_response_bytes": MAX_READ_RESPONSE_BYTES,
    })
}

/// Wire state registry. Unknown states fail deserialization and cannot be
/// mistaken for success by an executor. This value alone proves no settlement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatusV1 {
    Pending,
    RecoveryRequired,
    Committed,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryAdviceV1 {
    LookupOriginal,
    Wait,
    None,
}

impl OperationStatusV1 {
    /// A network timeout must never authorize a new client operation ID.
    pub fn retry_advice(self) -> RetryAdviceV1 {
        match self {
            Self::Pending => RetryAdviceV1::Wait,
            Self::RecoveryRequired => RetryAdviceV1::LookupOriginal,
            Self::Committed | Self::Rejected | Self::Failed => RetryAdviceV1::None,
        }
    }
}

/// Lossless JSON representation for revisions, ticks and executor epochs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecimalU64(pub u64);

impl Serialize for DecimalU64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for DecimalU64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let number = value.parse::<u64>().map_err(de::Error::custom)?;
        if number.to_string() != value {
            return Err(de::Error::custom("expected canonical decimal u64 string"));
        }
        Ok(Self(number))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorProofV1 {
    pub challenge_id: String,
    pub subject_public_key: String,
    pub signature_hex: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionSubmissionV1 {
    pub api_revision: u32,
    pub client_operation_id: String,
    pub task_id: String,
    pub executor_epoch: DecimalU64,
    pub context_id: String,
    pub action_ref: String,
    pub arguments: serde_json::Value,
    pub actor_proof: ActorProofV1,
}

impl ActionSubmissionV1 {
    /// Structural checks only; this is neither proof verification nor authority.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.api_revision != API_REVISION {
            return Err("unsupported API revision");
        }
        for id in [
            &self.client_operation_id,
            &self.task_id,
            &self.context_id,
            &self.action_ref,
            &self.actor_proof.challenge_id,
        ] {
            if id.trim().is_empty() || id.len() > 256 {
                return Err("invalid request identity");
            }
        }
        if !self.arguments.is_object() {
            return Err("action arguments must be an object");
        }
        Ok(())
    }
}

mod starter;
pub use starter::*;
