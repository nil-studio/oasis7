//! Effect system types - intents, receipts, and capabilities.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::error::WorldError;
use super::types::WorldTime;

/// An intent to perform an external effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectIntent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_era: Option<u64>,
    pub intent_id: String,
    pub kind: String,
    pub params: JsonValue,
    pub cap_ref: String,
    pub origin: EffectOrigin,
}

/// Receipt returned after an effect is executed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectReceipt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_era: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_payload_digest: Option<String>,
    pub intent_id: String,
    pub status: String,
    pub payload: JsonValue,
    pub cost_cents: Option<u64>,
    pub signature: Option<ReceiptSignature>,
}

/// Cryptographic signature on a receipt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReceiptSignature {
    pub algorithm: SignatureAlgorithm,
    pub signature_hex: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consensus_height: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipts_root: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participant_signatures: Vec<ReceiptParticipantSignature>,
}

/// Per-participant proof used by threshold signatures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReceiptParticipantSignature {
    pub signer_node_id: String,
    pub signature_hex: String,
}

/// Supported signature algorithms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureAlgorithm {
    HmacSha256,
    Ed25519,
    ThresholdEd25519,
}

/// The origin/source of an effect request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum EffectOrigin {
    Reducer { name: String },
    Plan { name: String },
    Module { module_id: String },
    System,
}

/// Simplified origin kind for policy matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginKind {
    Reducer,
    Plan,
    Module,
    System,
}

impl OriginKind {
    pub fn from_origin(origin: &EffectOrigin) -> Self {
        match origin {
            EffectOrigin::Reducer { .. } => OriginKind::Reducer,
            EffectOrigin::Plan { .. } => OriginKind::Plan,
            EffectOrigin::Module { .. } => OriginKind::Module,
            EffectOrigin::System => OriginKind::System,
        }
    }
}

/// A grant of capability to perform certain effect kinds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityGrant {
    pub name: String,
    pub effect_kinds: Vec<String>,
    pub expiry: Option<WorldTime>,
}

impl CapabilityGrant {
    pub fn new(name: impl Into<String>, effect_kinds: Vec<String>) -> Self {
        Self {
            name: name.into(),
            effect_kinds,
            expiry: None,
        }
    }

    pub fn allow_all(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            effect_kinds: vec!["*".to_string()],
            expiry: None,
        }
    }

    pub fn allows(&self, kind: &str) -> bool {
        self.effect_kinds.iter().any(|allowed| {
            allowed == "*"
                || allowed == kind
                || (allowed.ends_with(".*") && kind.starts_with(&allowed[..allowed.len() - 1]))
        })
    }

    pub fn is_expired(&self, now: WorldTime) -> bool {
        match self.expiry {
            Some(expiry) => now > expiry,
            None => false,
        }
    }
}

impl EffectIntent {
    /// World-local storage key. External use must also bind the verified World.
    pub fn identity_key(&self) -> String {
        match self.birth_era {
            Some(era) => format!("effect:v2:{era}:{}", self.intent_id),
            None => self.intent_id.clone(),
        }
    }
    pub fn payload_digest(&self) -> Result<String, WorldError> {
        let bytes =
            serde_cbor::to_vec(&("oasis7.effect.intent-payload.v2", self)).map_err(|error| {
                WorldError::ResourceBalanceInvalid {
                    reason: error.to_string(),
                }
            })?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

impl EffectReceipt {
    /// Same World-local reference as the original effect. Legacy raw references
    /// never match a receipt for a newly born formal effect.
    pub fn identity_key(&self) -> String {
        match self.birth_era {
            Some(era) => format!("effect:v2:{era}:{}", self.intent_id),
            None => self.intent_id.clone(),
        }
    }
}
