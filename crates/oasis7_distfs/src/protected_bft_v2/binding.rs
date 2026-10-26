use std::fmt;

use ed25519_dalek::VerifyingKey;
use oasis7_consensus::tendermint_v1::BftTimeoutsV1;
use oasis7_consensus::tendermint_v2::{
    BFT_EPOCH_MAPPING_RULESET_V2, BFT_HEIGHT_SCHEDULE_RULESET_V2, BFT_VALIDATOR_CHANGE_RULESET_V2,
    BftSchedulePolicyV2, BftValidatorSetV2,
};
use oasis7_proto::distributed_bft::v2::{BFT_MAX_VALIDATORS_V2, BftScopeV2, BftValidatorV2};
use serde::{Deserialize, Serialize};

use super::crypto::digest_hex;

pub const BFT_SIGNER_BINDING_DOMAIN_V2: &str = "oasis7.bft.protected-signer.v2.binding";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftSignerErrorV2(pub String);

impl fmt::Display for BftSignerErrorV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BftSignerErrorV2 {}

pub(crate) fn invalid(message: impl Into<String>) -> BftSignerErrorV2 {
    BftSignerErrorV2(message.into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerKeyIdentityV2 {
    pub validator_id: String,
    pub public_key_hex: String,
}

/// Immutable operator provisioning for the dynamic-set signer.
///
/// `local_validator_id` and its provisioned keys are intentionally not
/// required to appear in the genesis set. Membership is derived by replaying
/// the committed V2 certificate history; provisioned keys can only sign when
/// the exact identity is active at the requested height.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerBindingV2 {
    pub schema_version: u32,
    pub scope: BftScopeV2,
    pub genesis_authority_epoch: u64,
    pub genesis_validators: Vec<BftValidatorV2>,
    pub genesis_state_root: String,
    pub genesis_manifest_hash: String,
    pub schedule_policy: BftSchedulePolicyV2,
    pub local_validator_id: String,
    pub provisioned_signing_keys: Vec<BftSignerKeyIdentityV2>,
    pub timeouts: BftTimeoutsV1,
    pub operator_instance_id: String,
    pub requester_auth_public_key: String,
}

impl BftSignerBindingV2 {
    pub fn validate(&self) -> Result<BftValidatorSetV2, BftSignerErrorV2> {
        if self.schema_version != 2 || self.genesis_authority_epoch == 0 {
            return Err(invalid("BFT V2 signer binding schema or epoch"));
        }
        self.scope
            .validate()
            .map_err(|error| invalid(format!("BFT V2 signer scope: {error}")))?;
        validate_hash("genesis state root", &self.genesis_state_root)?;
        validate_hash("genesis manifest hash", &self.genesis_manifest_hash)?;
        self.schedule_policy
            .validate()
            .map_err(|error| invalid(format!("BFT V2 schedule policy: {error}")))?;
        self.timeouts
            .validate()
            .map_err(|error| invalid(format!("BFT V2 timeout policy: {error}")))?;
        validate_identity("local validator id", &self.local_validator_id)?;
        validate_identity("operator instance id", &self.operator_instance_id)?;

        let set = BftValidatorSetV2::new(self.scope.clone(), self.genesis_validators.clone())
            .map_err(|error| invalid(format!("BFT V2 genesis validator set: {error}")))?;
        let canonical_validators = set.validators().values().cloned().collect::<Vec<_>>();
        if canonical_validators != self.genesis_validators {
            return Err(invalid(
                "BFT V2 genesis validators must use canonical ID order",
            ));
        }

        if self.provisioned_signing_keys.is_empty()
            || self.provisioned_signing_keys.len() > BFT_MAX_VALIDATORS_V2
        {
            return Err(invalid(
                "BFT V2 provisioned signer key count is outside its bound",
            ));
        }
        let mut previous: Option<(&str, &str)> = None;
        let mut keys = std::collections::BTreeSet::new();
        for identity in &self.provisioned_signing_keys {
            validate_identity("provisioned validator id", &identity.validator_id)?;
            if identity.validator_id != self.local_validator_id {
                return Err(invalid(
                    "BFT V2 provisioned keys must match the local validator identity",
                ));
            }
            decode_key(&identity.public_key_hex)?;
            if !keys.insert(identity.public_key_hex.as_str()) {
                return Err(invalid("BFT V2 provisioned signer keys must be unique"));
            }
            let ordering = (
                identity.validator_id.as_str(),
                identity.public_key_hex.as_str(),
            );
            if previous.is_some_and(|prior| prior >= ordering) {
                return Err(invalid(
                    "BFT V2 provisioned keys must use canonical identity/key order",
                ));
            }
            previous = Some(ordering);
        }

        decode_key(&self.requester_auth_public_key)?;
        if keys.contains(self.requester_auth_public_key.as_str()) {
            return Err(invalid(
                "BFT V2 requester key must differ from provisioned validator keys",
            ));
        }
        Ok(set)
    }

    pub fn digest(&self) -> Result<String, BftSignerErrorV2> {
        let set = self.validate()?;
        let schedule_policy_hash = self
            .schedule_policy
            .hash()
            .map_err(|error| invalid(format!("BFT V2 schedule policy hash: {error}")))?;
        digest_hex(
            BFT_SIGNER_BINDING_DOMAIN_V2,
            &BindingDigestInputV2 {
                binding: self,
                genesis_validator_set_hash: set.set_hash(),
                schedule_policy_hash: &schedule_policy_hash,
                height_schedule_ruleset: BFT_HEIGHT_SCHEDULE_RULESET_V2,
                validator_change_ruleset: BFT_VALIDATOR_CHANGE_RULESET_V2,
                epoch_mapping_ruleset: BFT_EPOCH_MAPPING_RULESET_V2,
            },
        )
    }
}

#[derive(Serialize)]
struct BindingDigestInputV2<'a> {
    binding: &'a BftSignerBindingV2,
    genesis_validator_set_hash: &'a str,
    schedule_policy_hash: &'a str,
    height_schedule_ruleset: &'static str,
    validator_change_ruleset: &'static str,
    epoch_mapping_ruleset: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerTrustV2 {
    pub schema_version: u32,
    pub provider_public_key: String,
    pub binding_digest: String,
}

impl BftSignerTrustV2 {
    pub fn validate_for(&self, binding: &BftSignerBindingV2) -> Result<(), BftSignerErrorV2> {
        if self.schema_version != 2
            || self.binding_digest != binding.digest()?
            || self.provider_public_key.len() != 64
        {
            return Err(invalid("BFT V2 signer provider trust binding"));
        }
        let provider = decode_key(&self.provider_public_key)?;
        let requester = decode_key(&binding.requester_auth_public_key)?;
        if provider.as_bytes() == requester.as_bytes()
            || binding
                .provisioned_signing_keys
                .iter()
                .any(|key| key.public_key_hex == self.provider_public_key)
        {
            return Err(invalid(
                "BFT V2 provider key must differ from requester and validator keys",
            ));
        }
        Ok(())
    }
}

/// Key operations are supplied by an independently provisioned provider.
/// Private key bytes are never returned to the caller.
pub trait BftSigningKeyHandleV2: Send + Sync {
    fn public_key_hex(&self) -> String;
    fn sign_preimage(&self, preimage: &[u8]) -> Result<[u8; 64], BftSignerErrorV2>;
}

pub(crate) fn decode_key(value: &str) -> Result<VerifyingKey, BftSignerErrorV2> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT V2 public key must be canonical lowercase hex"));
    }
    let bytes: [u8; 32] = hex::decode(value)
        .map_err(|error| invalid(format!("BFT V2 public key: {error}")))?
        .try_into()
        .map_err(|_| invalid("BFT V2 public key length"))?;
    let key = VerifyingKey::from_bytes(&bytes)
        .map_err(|error| invalid(format!("BFT V2 public key: {error}")))?;
    if key.is_weak() {
        return Err(invalid("BFT V2 public key is weak"));
    }
    Ok(key)
}

fn validate_hash(label: &str, value: &str) -> Result<(), BftSignerErrorV2> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(format!(
            "BFT V2 {label} must be canonical 32-byte lowercase hex"
        )));
    }
    Ok(())
}

fn validate_identity(label: &str, value: &str) -> Result<(), BftSignerErrorV2> {
    if value.trim().is_empty() || value.len() > 256 {
        return Err(invalid(format!("BFT V2 {label} is empty or oversized")));
    }
    Ok(())
}
