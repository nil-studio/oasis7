use std::fmt;

use ed25519_dalek::VerifyingKey;
use oasis7_consensus::tendermint_v1::BftTimeoutsV1;
use oasis7_formal_protocol::bft::{
    BftHeaderHeightScheduleV3, BftProfileTargetV2, VerifiedBftHandoffTargetV2,
};
use oasis7_proto::distributed_bft::v2::BftScopeV2;
use oasis7_proto::distributed_bft::v3::BFT_MAX_VALIDATORS_V3;
use serde::{Deserialize, Serialize};

use super::crypto::digest_hex;

pub const BFT_SIGNER_BINDING_DOMAIN_V3: &str = "oasis7.bft.protected-signer.v3.binding";
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftSignerErrorV3(pub String);

impl fmt::Display for BftSignerErrorV3 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BftSignerErrorV3 {}

pub(crate) fn invalid(message: impl Into<String>) -> BftSignerErrorV3 {
    BftSignerErrorV3(message.into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerKeyIdentityV3 {
    pub validator_id: String,
    pub public_key_hex: String,
}

/// An operator-admitted qualified-H capability, never deserialized from the wire.
/// Public fields are checked against the retained opaque H and initial schedule.
#[derive(Debug, Clone, Serialize)]
pub struct BftSignerBindingV3 {
    pub schema_version: u32,
    pub scope: BftScopeV2,
    pub target_config: BftProfileTargetV2,
    pub handoff_record_id: String,
    pub handoff_position: u64,
    pub activation_digest: String,
    pub initial_schedule_digest: String,
    pub material_identity: String,
    pub local_validator_id: String,
    pub provisioned_signing_keys: Vec<BftSignerKeyIdentityV3>,
    pub timeouts: BftTimeoutsV1,
    pub operator_instance_id: String,
    pub requester_auth_public_key: String,
    #[serde(skip)]
    handoff: VerifiedBftHandoffTargetV2,
    #[serde(skip)]
    initial_schedule: BftHeaderHeightScheduleV3,
}
impl BftSignerBindingV3 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        handoff: VerifiedBftHandoffTargetV2,
        initial_schedule: BftHeaderHeightScheduleV3,
        material_identity: String,
        local_validator_id: String,
        provisioned_signing_keys: Vec<BftSignerKeyIdentityV3>,
        timeouts: BftTimeoutsV1,
        operator_instance_id: String,
        requester_auth_public_key: String,
    ) -> Result<Self, BftSignerErrorV3> {
        let binding = Self {
            schema_version: 3,
            scope: handoff.target().scope.clone(),
            target_config: handoff.target().clone(),
            handoff_record_id: handoff.handoff_head().record_id.clone(),
            handoff_position: handoff.handoff_position(),
            activation_digest: handoff.activation_digest().into(),
            initial_schedule_digest: initial_schedule.digest().map_err(invalid)?,
            material_identity,
            local_validator_id,
            provisioned_signing_keys,
            timeouts,
            operator_instance_id,
            requester_auth_public_key,
            handoff,
            initial_schedule,
        };
        binding.validate()?;
        Ok(binding)
    }
    pub fn initial_schedule(&self) -> &BftHeaderHeightScheduleV3 {
        &self.initial_schedule
    }
    pub fn handoff(&self) -> &VerifiedBftHandoffTargetV2 {
        &self.handoff
    }
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        let expected = self
            .handoff
            .initial_schedule_after_handoff_replay(
                self.initial_schedule.parent_state_root(),
                self.initial_schedule.parent_manifest_hash(),
            )
            .map_err(invalid)?;
        if self.schema_version != 3
            || self.scope != self.handoff.target().scope
            || self.target_config != *self.handoff.target()
            || self.handoff_record_id != self.handoff.handoff_head().record_id
            || self.handoff_position != self.handoff.handoff_position()
            || self.activation_digest != self.handoff.activation_digest()
            || self.initial_schedule != expected
            || self.initial_schedule_digest != self.initial_schedule.digest().map_err(invalid)?
            || self.initial_schedule.pending_transition().is_some()
        {
            return Err(invalid("BFT V3 signer binding differs from qualified H"));
        }
        validate_hash("material identity", &self.material_identity)?;
        self.timeouts.validate().map_err(invalid)?;
        validate_identity("local validator id", &self.local_validator_id)?;
        validate_identity("operator instance id", &self.operator_instance_id)?;
        if self.provisioned_signing_keys.is_empty()
            || self.provisioned_signing_keys.len() > BFT_MAX_VALIDATORS_V3
        {
            return Err(invalid("BFT V3 provisioned signer key count"));
        }
        let mut previous = None;
        let mut keys = std::collections::BTreeSet::new();
        for identity in &self.provisioned_signing_keys {
            if identity.validator_id != self.local_validator_id {
                return Err(invalid("BFT V3 key identity differs from local validator"));
            }
            decode_key(&identity.public_key_hex)?;
            let ordering = (
                identity.validator_id.as_str(),
                identity.public_key_hex.as_str(),
            );
            if previous.is_some_and(|p| p >= ordering)
                || !keys.insert(identity.public_key_hex.as_str())
            {
                return Err(invalid(
                    "BFT V3 provisioned keys are not canonical and unique",
                ));
            }
            previous = Some(ordering);
        }
        decode_key(&self.requester_auth_public_key)?;
        if keys.contains(self.requester_auth_public_key.as_str()) {
            return Err(invalid("BFT V3 requester and validator keys must differ"));
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        self.validate()?;
        digest_hex(BFT_SIGNER_BINDING_DOMAIN_V3, self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignerTrustV3 {
    pub schema_version: u32,
    pub provider_public_key: String,
    pub binding_digest: String,
}

impl BftSignerTrustV3 {
    pub fn validate_for(&self, binding: &BftSignerBindingV3) -> Result<(), BftSignerErrorV3> {
        if self.schema_version != 3
            || self.binding_digest != binding.digest()?
            || self.provider_public_key.len() != 64
        {
            return Err(invalid("BFT V3 signer provider trust binding"));
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
                "BFT V3 provider key must differ from requester and validator keys",
            ));
        }
        Ok(())
    }
}

/// Key operations are supplied by an independently provisioned provider.
/// Private key bytes are never returned to the caller.
pub trait BftSigningKeyHandleV3: Send + Sync {
    fn public_key_hex(&self) -> String;
    fn sign_preimage(&self, preimage: &[u8]) -> Result<[u8; 64], BftSignerErrorV3>;
}

pub(crate) fn decode_key(value: &str) -> Result<VerifyingKey, BftSignerErrorV3> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT V3 public key must be canonical lowercase hex"));
    }
    let bytes: [u8; 32] = hex::decode(value)
        .map_err(|error| invalid(format!("BFT V3 public key: {error}")))?
        .try_into()
        .map_err(|_| invalid("BFT V3 public key length"))?;
    let key = VerifyingKey::from_bytes(&bytes)
        .map_err(|error| invalid(format!("BFT V3 public key: {error}")))?;
    if key.is_weak() {
        return Err(invalid("BFT V3 public key is weak"));
    }
    Ok(key)
}

fn validate_hash(label: &str, value: &str) -> Result<(), BftSignerErrorV3> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(format!(
            "BFT V3 {label} must be canonical 32-byte lowercase hex"
        )));
    }
    Ok(())
}

fn validate_identity(label: &str, value: &str) -> Result<(), BftSignerErrorV3> {
    if value.trim().is_empty() || value.len() > 256 {
        return Err(invalid(format!("BFT V3 {label} is empty or oversized")));
    }
    Ok(())
}
