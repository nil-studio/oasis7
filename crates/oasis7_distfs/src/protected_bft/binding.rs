use std::fmt;

use ed25519_dalek::{Signature, VerifyingKey};
use oasis7_consensus::tendermint_v1::{
    BFT_HEIGHT_SCHEDULE_RULESET_V2, BftTimeoutsV1, BftValidatorSetV1, BftValidatorV1,
};
use oasis7_proto::distributed_bft::{BFT_MAX_IDENTITY_TEXT_BYTES_V1, BftScopeV1};
use serde::{Deserialize, Serialize};

use super::wire::{BFT_BINDING_DOMAIN_V1, digest_hex, framed_cbor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftSignerError(pub String);

impl fmt::Display for BftSignerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BftSignerError {}

pub(crate) fn invalid(message: impl Into<String>) -> BftSignerError {
    BftSignerError(message.into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerBindingV1 {
    pub schema_version: u32,
    pub scope: BftScopeV1,
    pub authority_epoch: u64,
    pub validators: Vec<BftValidatorV1>,
    pub local_validator_id: String,
    pub local_signing_public_key: String,
    pub timeouts: BftTimeoutsV1,
    pub operator_instance_id: String,
    pub requester_auth_public_key: String,
}

impl BftSignerBindingV1 {
    pub fn validate(&self) -> Result<BftValidatorSetV1, BftSignerError> {
        if self.schema_version != 1 || self.authority_epoch == 0 {
            return Err(invalid("BFT signer binding schema or epoch"));
        }
        if self.operator_instance_id.trim().is_empty()
            || self.operator_instance_id.len() > BFT_MAX_IDENTITY_TEXT_BYTES_V1
        {
            return Err(invalid("BFT signer operator instance identity"));
        }
        self.timeouts
            .validate()
            .map_err(|error| invalid(format!("BFT signer timeout policy: {error}")))?;
        let set = BftValidatorSetV1::new(self.scope.clone(), self.validators.clone())
            .map_err(|error| invalid(format!("BFT signer validator set: {error}")))?;
        let canonical_validators = set.validators().values().cloned().collect::<Vec<_>>();
        if canonical_validators != self.validators {
            return Err(invalid("BFT signer validators must use canonical ID order"));
        }
        let validator = set
            .validator(&self.local_validator_id)
            .ok_or_else(|| invalid("BFT signer local validator is outside the fixed set"))?;
        if validator.public_key_hex != self.local_signing_public_key {
            return Err(invalid(
                "BFT signer local key differs from fixed validator set",
            ));
        }
        let requester = decode_key(&self.requester_auth_public_key)?;
        let local = decode_key(&self.local_signing_public_key)?;
        if requester.as_bytes() == local.as_bytes() {
            return Err(invalid(
                "BFT requester authentication key must differ from validator key",
            ));
        }
        Ok(set)
    }

    pub fn digest(&self) -> Result<String, BftSignerError> {
        let set = self.validate()?;
        digest_hex(
            BFT_BINDING_DOMAIN_V1,
            &BindingDigestInput {
                binding: self,
                validator_set_hash: set.set_hash(),
                schedule_ruleset: BFT_HEIGHT_SCHEDULE_RULESET_V2,
            },
        )
    }
}

#[derive(Serialize)]
struct BindingDigestInput<'a> {
    binding: &'a BftSignerBindingV1,
    validator_set_hash: &'a str,
    schedule_ruleset: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerTrustV1 {
    pub schema_version: u32,
    pub provider_public_key: String,
    pub binding_digest: String,
}

impl BftSignerTrustV1 {
    pub fn validate_for(&self, binding: &BftSignerBindingV1) -> Result<(), BftSignerError> {
        if self.schema_version != 1
            || self.binding_digest != binding.digest()?
            || self.provider_public_key.len() != 64
        {
            return Err(invalid("BFT signer provider trust binding"));
        }
        let provider = decode_key(&self.provider_public_key)?;
        let validator = decode_key(&binding.local_signing_public_key)?;
        let requester = decode_key(&binding.requester_auth_public_key)?;
        if provider.as_bytes() == validator.as_bytes()
            || provider.as_bytes() == requester.as_bytes()
        {
            return Err(invalid(
                "BFT signer provider key must differ from validator and requester keys",
            ));
        }
        Ok(())
    }
}

/// Key operations are injected by the protected provider. Implementations
/// must not expose private key bytes and must only be reachable through the
/// service's journal-validated signing path.
pub trait BftSigningKeyHandleV1: Send + Sync {
    fn public_key_hex(&self) -> String;
    fn sign_preimage(&self, preimage: &[u8]) -> Result<[u8; 64], BftSignerError>;
}

pub(crate) fn decode_key(value: &str) -> Result<VerifyingKey, BftSignerError> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT signer key must be canonical lowercase hex"));
    }
    let bytes: [u8; 32] = hex::decode(value)
        .map_err(|error| invalid(format!("BFT signer public key: {error}")))?
        .try_into()
        .map_err(|_| invalid("BFT signer public key length"))?;
    let key = VerifyingKey::from_bytes(&bytes)
        .map_err(|error| invalid(format!("BFT signer public key: {error}")))?;
    if key.is_weak() {
        return Err(invalid("BFT signer public key is weak"));
    }
    Ok(key)
}

pub(crate) fn verify_signature<T: Serialize>(
    domain: &str,
    body: &T,
    signature_hex: &str,
    public_key_hex: &str,
) -> Result<(), BftSignerError> {
    let key = decode_key(public_key_hex)?;
    if signature_hex.len() != 128
        || signature_hex
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(
            "BFT signer signature must be canonical lowercase hex",
        ));
    }
    let signature: [u8; 64] = hex::decode(signature_hex)
        .map_err(|error| invalid(format!("BFT signer signature: {error}")))?
        .try_into()
        .map_err(|_| invalid("BFT signer signature length"))?;
    let preimage = framed_cbor(domain, body, super::wire::BFT_SIGNER_MAX_SIGNING_BYTES_V1)?;
    key.verify_strict(&preimage, &Signature::from_bytes(&signature))
        .map_err(|_| invalid("BFT signer signature verification failed"))
}
