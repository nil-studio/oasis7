//! Opt-in wire data only. No producer, dispatch token, Current or paid grant.
use super::{task_cleanup_v2::*, task_mutation_admission_v2::*, task_stop_v2::*, *};
use oasis7_agent_api::game::{DecimalU64, GameTaskClosedV2, GameTaskStopRecordV2};
use serde::{Deserialize, Serialize};
pub const STOP_QUALIFIED_EFFECT_DOMAIN_V2: &str = "oasis7.world.task-stop-qualified-effect.v2";
pub const CLOSE_QUALIFIED_EFFECT_DOMAIN_V2: &str = "oasis7.world.task-close-qualified-effect.v2";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskMutationReplyProfileV2 {
    QualifiedEffectV2,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TaskMutationSubmittedV2 {
    GuardedStop(Box<TaskStopSubmitV2>),
    RevokeAndClose(Box<TaskCleanupSubmitV2>),
}
impl TaskMutationSubmittedV2 {
    pub fn kind(&self) -> TaskMutationKindV2 {
        match self {
            Self::GuardedStop(_) => TaskMutationKindV2::GuardedStop,
            Self::RevokeAndClose(_) => TaskMutationKindV2::RevokeAndClose,
        }
    }
    pub fn world(&self) -> &WorldIdentity {
        match self {
            Self::GuardedStop(q) => &q.expected_world,
            Self::RevokeAndClose(q) => &q.expected_world,
        }
    }
    pub fn original_digest(&self) -> Result<String, String> {
        match self {
            Self::GuardedStop(q) => q.original_digest(),
            Self::RevokeAndClose(q) => q.original_digest(),
        }
    }
    pub fn validate_scope(&self) -> Result<u64, String> {
        match self {
            Self::GuardedStop(q) => {
                q.verify_owner_signature()?;
                let grant = q
                    .original
                    .request
                    .observation_grant
                    .as_ref()
                    .ok_or("qualified Stop reply requires Owner observation grant")?;
                grant.validate_for(&q.original.request)?;
                Ok(grant.historical_read_until_ms())
            }
            Self::RevokeAndClose(q) => {
                q.verify_owner_signature()?;
                Ok(q.original.request.cleanup_until_ms.0)
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskMutationWaitSubmitV2 {
    pub api_revision: u8,
    pub reply_profile: TaskMutationReplyProfileV2,
    pub submitted: TaskMutationSubmittedV2,
    pub delivery_nonce: String,
    pub delivery_until_ms: DecimalU64,
}
impl TaskMutationWaitSubmitV2 {
    pub fn validate(&self) -> Result<(), String> {
        let until = self.submitted.validate_scope()?;
        if self.api_revision != 2
            || !hex64(&self.delivery_nonce)
            || self.delivery_until_ms.0 == 0
            || self.delivery_until_ms.0 > until
        {
            return Err("invalid qualified mutation reply profile".into());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        request_digest(self.submitted.kind().path(), self)
    }
}
fn hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskMutationWaitOutcomeV2 {
    Ended,
    Unavailable,
    Expired,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskMutationExecutionV2 {
    pub height: DecimalU64,
    pub slot: DecimalU64,
    pub epoch: DecimalU64,
    pub node_block_hash: String,
    pub action_root: String,
    pub committed_at_unix_ms: DecimalU64,
}
fn required_predecessor<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Box<GameTaskStopRecordV2>>, D::Error> {
    Option::<Box<GameTaskStopRecordV2>>::deserialize(d)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskMutationQualifiedResultV2 {
    GuardedStop {
        record: Box<GameTaskStopRecordV2>,
    },
    RevokeAndClose {
        closed: Box<GameTaskClosedV2>,
        #[serde(deserialize_with = "required_predecessor")]
        stop_predecessor: Option<Box<GameTaskStopRecordV2>>,
    },
}
/// Signed only by the future fixed Node producer after actual qualification.
/// Deserialization is not evidence that any of these assertions is true.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskMutationQualifiedEffectV2 {
    pub api_revision: u8,
    pub kind: TaskMutationKindV2,
    pub path: String,
    pub world: WorldIdentity,
    pub chain_id: String,
    pub delivery_nonce: String,
    pub original_digest: String,
    pub action_id: DecimalU64,
    pub canonical_catalog_digest: String,
    pub execution: TaskMutationExecutionV2,
    pub witnessed_in_commit: CommitRef,
    pub result: TaskMutationQualifiedResultV2,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskMutationSubmitReplyV2 {
    Queued {
        admission: SignedServiceResponse<TaskMutationAdmissionV2>,
        wait_outcome: TaskMutationWaitOutcomeV2,
    },
    Qualified {
        admission: SignedServiceResponse<TaskMutationAdmissionV2>,
        effect: Box<SignedServiceResponse<TaskMutationQualifiedEffectV2>>,
    },
}

/// Decode this opt-in closed profile only. Typed parsing happens first so known
/// duplicate struct fields cannot disappear into serde_json::Value's map.
fn strict_profile_json<T: serde::de::DeserializeOwned + Serialize>(
    raw: &[u8],
) -> Result<T, String> {
    if raw.len() > 64 * 1024 {
        return Err("qualified mutation wire exceeds 64 KiB".into());
    }
    let typed: T = serde_json::from_slice(raw).map_err(|error| error.to_string())?;
    let supplied: serde_json::Value =
        serde_json::from_slice(raw).map_err(|error| error.to_string())?;
    let closed = serde_json::to_value(&typed).map_err(|error| error.to_string())?;
    // This closed tree consists only of structs/enums/scalars/vectors, with no
    // arbitrary Value or map-valued extension fields. Unknown keys at any depth
    // survive the supplied tree but cannot survive the typed roundtrip.
    if supplied != closed {
        return Err("qualified mutation wire contains unknown or noncanonical fields".into());
    }
    Ok(typed)
}
impl TaskMutationWaitSubmitV2 {
    pub fn decode_strict(raw: &[u8]) -> Result<Self, String> {
        let request: Self = strict_profile_json(raw)?;
        request.validate()?;
        Ok(request)
    }
}
impl TaskMutationSubmitReplyV2 {
    /// Strict wire shape only; callers must independently verify the configured
    /// Node signature, exact request binding and qualified effect semantics.
    pub fn decode_strict(raw: &[u8]) -> Result<Self, String> {
        strict_profile_json(raw)
    }
}
