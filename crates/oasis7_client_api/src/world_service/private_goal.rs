//! Private proof codec only: no HTTP, storage, public intent or activation.
use super::authority::{sign_read_request, signing_bytes};
use ed25519_dalek::{Signature, VerifyingKey};
use oasis7_agent_api::game::{
    GAME_GOAL_PREPARE_DOMAIN, GameGoalPrepareV1, GameGoalPublishV1, GameTaskControlV1,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;
#[cfg(feature = "signature-metrics")]
pub mod signature_metrics;

/// Kept strictly outside WorldServicePayloadV1. Debug is absent so body and
/// enumerable signature material cannot accidentally enter ordinary logs.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedGoalPrepareV1 {
    pub request: GameGoalPrepareV1,
    pub subject_public_key: String,
    pub signature_hex: String,
}

pub fn sign_goal_prepare(
    request: GameGoalPrepareV1,
    private_key_hex: &str,
) -> Result<SignedGoalPrepareV1, String> {
    request.validate().map_err(str::to_owned)?;
    let signed = sign_read_request(GAME_GOAL_PREPARE_DOMAIN, request, private_key_hex)?;
    if signed.subject_public_key != signed.request.owner_public_key {
        return Err("private goal signer differs from declared owner".into());
    }
    Ok(SignedGoalPrepareV1 {
        request: signed.request,
        subject_public_key: signed.subject_public_key,
        signature_hex: signed.signature_hex,
    })
}

impl SignedGoalPrepareV1 {
    /// Proves only the declared signer and exact private proposal. A valid
    /// signature alone never establishes current World owner authority.
    pub fn verify(&self) -> Result<(), String> {
        self.request.validate().map_err(str::to_owned)?;
        if self.subject_public_key != self.request.owner_public_key
            || self.signature_hex.len() != 128
            || !self
                .signature_hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("invalid private goal signature envelope".into());
        }
        verify_owner_signature_strict(
            GAME_GOAL_PREPARE_DOMAIN,
            &self.request,
            &self.subject_public_key,
            &self.signature_hex,
        )
    }

    /// Caller must obtain this control from freshly authenticated canonical
    /// World authority and recheck it at admission/delivery. This pure proof
    /// comparison does not perform those checks or prepare/activate a goal.
    pub fn validate_against(&self, current: &GameTaskControlV1) -> Result<(), String> {
        self.verify()?;
        let request = &self.request;
        if current.schema_revision != 1
            || request.world_id != current.world_id
            || request.genesis_digest != current.genesis_digest
            || request.task_id != current.task_id
            || request.agent_id != current.agent_id
            || request.owner_binding != current.owner_binding
            || request.owner_public_key != current.owner_public_key
            || request.owner_generation != current.owner_generation
            || request.branch_id != current.branch_id
            || request.reorg_epoch != current.reorg_epoch
            || request.expected_task_revision != current.task_revision
            || request.expected_latest_goal_revision != current.latest_goal_revision
        {
            return Err("private goal differs from trusted current control".into());
        }
        Ok(())
    }
}

/// Owned SDK signing image. It proves nothing until `verify_strict` succeeds.
/// No Deserialize, Debug, public constructor fields or caller verification flag.
pub struct PreparedOwnerSignature {
    domain: String,
    original_public_key: String,
    original_signature: String,
    signature: [u8; 64],
    key: VerifyingKey,
    canonical_bytes: Vec<u8>,
}
impl PreparedOwnerSignature {
    pub fn domain(&self) -> &str {
        &self.domain
    }
    pub fn profile(&self) -> &'static str {
        "ed25519-verify-strict/oasis7.world-service.v1"
    }
    pub fn public_key_hex(&self) -> &str {
        &self.original_public_key
    }
    pub fn signature_hex(&self) -> &str {
        &self.original_signature
    }
    pub fn signature_bytes(&self) -> &[u8; 64] {
        &self.signature
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
    /// Verifies precisely the owned bytes returned by canonical_bytes, with no serialization.
    pub fn verify_strict(&self) -> Result<(), String> {
        #[cfg(feature = "signature-metrics")]
        let mut metrics = signature_metrics::Call::new();
        #[cfg(feature = "signature-metrics")]
        metrics.strict_attempt(&self.key.to_bytes(), &self.signature, &self.canonical_bytes);
        let result = self
            .key
            .verify_strict(
                &self.canonical_bytes,
                &Signature::from_bytes(&self.signature),
            )
            .map_err(|_| "invalid private goal owner signature".into());
        #[cfg(feature = "signature-metrics")]
        if result.is_ok() {
            metrics.success();
        }
        result
    }
}
/// Preserves the strict envelope/key validation order and encodes the SDK signing
/// image exactly once. Successful preparation alone is not a verification call.
pub fn prepare_owner_signature_strict<T: Serialize>(
    domain: &str,
    request: &T,
    subject_public_key: &str,
    signature_hex: &str,
) -> Result<PreparedOwnerSignature, String> {
    let result = (|| {
        let canonical = |value: &str, length: usize| {
            value.len() == length
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if !canonical(subject_public_key, 64) || !canonical(signature_hex, 128) {
            return Err("invalid owner signature encoding".into());
        }
        let public_key: [u8; 32] = hex::decode(subject_public_key)
            .map_err(|_| "invalid private goal owner key")?
            .try_into()
            .map_err(|_| "invalid private goal owner key")?;
        let key =
            VerifyingKey::from_bytes(&public_key).map_err(|_| "invalid private goal owner key")?;
        if key.is_weak() {
            return Err("weak private goal owner key".into());
        }
        let signature: [u8; 64] = hex::decode(signature_hex)
            .map_err(|_| "invalid private goal signature")?
            .try_into()
            .map_err(|_| "invalid private goal signature")?;
        let canonical_bytes = signing_bytes(domain, request)?;
        Ok(PreparedOwnerSignature {
            domain: domain.into(),
            original_public_key: subject_public_key.into(),
            original_signature: signature_hex.into(),
            signature,
            key,
            canonical_bytes,
        })
    })();
    // Old public verify counts malformed envelopes/weak keys/encoding failures as
    // one failed call, with zero strict attempts. Successful prepare is counted
    // only by verify_strict, so the wrapper never double counts.
    #[cfg(feature = "signature-metrics")]
    if result.is_err() {
        drop(signature_metrics::Call::new());
    }
    result
}
/// Narrow shared cryptographic boundary for private goal proofs and public owner
/// goal publications. This does not change generic World Service verification.
pub fn verify_owner_signature_strict<T: Serialize>(
    domain: &str,
    request: &T,
    subject_public_key: &str,
    signature_hex: &str,
) -> Result<(), String> {
    prepare_owner_signature_strict(domain, request, subject_public_key, signature_hex)?
        .verify_strict()
}
#[cfg(test)]
#[path = "private_goal/strict_prepared_tests.rs"]
mod strict_prepared_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedGoalError {
    Invalid,
    Conflict,
    Unavailable,
}
impl std::fmt::Display for PreparedGoalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid private goal proof",
            Self::Conflict => "private goal operation conflict",
            Self::Unavailable => "private goal recovery required",
        })
    }
}
impl std::error::Error for PreparedGoalError {}

/// Private only: clients verify this exact proof and recompute its salted
/// reference before signing the separate public PublishGoal. No Debug.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedGoalReceiptV1 {
    pub schema_revision: u32,
    pub goal_ref: String,
    pub reference_salt: String,
    pub owner_proof: SignedGoalPrepareV1,
}
impl PreparedGoalReceiptV1 {
    pub fn validate(&self) -> Result<(), PreparedGoalError> {
        if self.schema_revision != 1 {
            return Err(PreparedGoalError::Invalid);
        }
        self.owner_proof
            .verify()
            .map_err(|_| PreparedGoalError::Invalid)?;
        let salt = decode_salt(&self.reference_salt)?;
        if self.goal_ref != prepared_goal_commitment(&salt, &self.owner_proof)? {
            return Err(PreparedGoalError::Invalid);
        }
        Ok(())
    }

    /// Expected proposal is retained on the owner device, not reconstructed
    /// from the server receipt. Parsing or self-consistency is insufficient.
    pub fn validate_against(
        &self,
        original: &SignedGoalPrepareV1,
    ) -> Result<(), PreparedGoalError> {
        self.validate()?;
        if &self.owner_proof != original {
            return Err(PreparedGoalError::Conflict);
        }
        Ok(())
    }

    pub fn validate_publish(&self, publish: &GameGoalPublishV1) -> Result<(), PreparedGoalError> {
        self.validate()?;
        publish.validate().map_err(|_| PreparedGoalError::Invalid)?;
        let r = &self.owner_proof.request;
        if self.goal_ref != publish.goal_ref
            || r.client_operation_id != publish.prepare_operation_id
            || r.world_id != publish.world_id
            || r.genesis_digest != publish.genesis_digest
            || r.task_id != publish.task_id
            || r.agent_id != publish.agent_id
            || r.owner_public_key != publish.owner_public_key
            || r.owner_binding != publish.owner_binding
            || r.owner_generation != publish.owner_generation
            || r.branch_id != publish.branch_id
            || r.reorg_epoch != publish.reorg_epoch
            || r.expected_task_revision != publish.expected_task_revision
            || r.expected_latest_goal_revision != publish.expected_latest_goal_revision
            || r.goal_revision != publish.goal_revision
        {
            return Err(PreparedGoalError::Conflict);
        }
        Ok(())
    }
}

fn decode_salt(value: &str) -> Result<[u8; 32], PreparedGoalError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(PreparedGoalError::Invalid);
    }
    hex::decode(value)
        .map_err(|_| PreparedGoalError::Invalid)?
        .try_into()
        .map_err(|_| PreparedGoalError::Invalid)
}
pub fn prepared_goal_commitment(
    salt: &[u8; 32],
    proof: &SignedGoalPrepareV1,
) -> Result<String, PreparedGoalError> {
    let bytes = Zeroizing::new(
        oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7/private-goal-reference/v1",
            hex::encode(salt),
            proof,
        ))
        .map_err(|_| PreparedGoalError::Invalid)?,
    );
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

#[cfg(test)]
mod tests;
