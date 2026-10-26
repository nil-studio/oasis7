use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

use super::binding::{
    BftSignerBindingV2, BftSignerErrorV2, BftSigningKeyHandleV2, decode_key, invalid,
};
use super::crypto::{BFT_SIGNER_MAX_SIGNING_BYTES_V2, digest_hex, framed_cbor, verify_signature};
use super::state::BftSignerCheckpointV2;

pub const BFT_TRANSITION_INSTALL_INTENT_DOMAIN_V2: &str =
    "oasis7.bft.protected-signer.v2.install-intent";
pub const BFT_TRANSITION_INSTALL_ACK_DOMAIN_V2: &str = "oasis7.bft.protected-signer.v2.install-ack";
pub const BFT_TRANSITION_INSTALL_RECEIPT_DOMAIN_V2: &str =
    "oasis7.bft.protected-signer.v2.install-receipt";

/// Local software's signed assertion that it installed the schedule and
/// execution state named by the provider's independently verified ticket.
/// This is an authenticated application receipt, not hardware attestation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftLocalInstallEvidenceV2 {
    pub schema_version: u32,
    pub runtime_instance_id: String,
    pub installed_head_height: u64,
    pub installed_head_commit_ref: String,
    pub installed_state_root: String,
    pub installed_manifest_hash: String,
    pub installed_active_set_hash: String,
    pub installed_authority_epoch: u64,
    pub local_validator_id: String,
    pub local_signing_public_key: String,
    pub wal_install_digest: String,
    pub driver_install_digest: String,
}

impl BftLocalInstallEvidenceV2 {
    pub fn validate_for(
        &self,
        intent: &BftTransitionInstallIntentV2,
        binding: &BftSignerBindingV2,
    ) -> Result<(), BftSignerErrorV2> {
        let body = &intent.body;
        if self.schema_version != 2
            || self.runtime_instance_id != body.runtime_instance_id
            || self.runtime_instance_id != binding.operator_instance_id
            // The install receipt proves the actual committed tip observed
            // when this request was prepared, while the separately scheduled
            // activation height remains future authority.
            || self.installed_head_height != body.install_head_height
            || self.installed_head_commit_ref != body.install_parent_commit_ref
            || self.installed_state_root != body.install_state_root
            || self.installed_manifest_hash != body.install_manifest_hash
            || self.installed_active_set_hash != body.new_validator_set_hash
            || self.installed_active_set_hash != body.install_active_set_hash
            || self.installed_authority_epoch != body.target_authority_epoch
            || self.local_validator_id != binding.local_validator_id
            || self.local_signing_public_key != body.local_signing_public_key
            || !binding.provisioned_signing_keys.iter().any(|key| {
                key.validator_id == self.local_validator_id
                    && key.public_key_hex == self.local_signing_public_key
            })
        {
            return Err(invalid(
                "BFT V2 local install evidence does not match intent",
            ));
        }
        validate_hash("installed CommitRef", &self.installed_head_commit_ref)?;
        validate_hash("installed state root", &self.installed_state_root)?;
        validate_hash("installed manifest", &self.installed_manifest_hash)?;
        validate_hash("installed set hash", &self.installed_active_set_hash)?;
        validate_hash("WAL install digest", &self.wal_install_digest)?;
        validate_hash("driver install digest", &self.driver_install_digest)?;
        validate_identity("runtime instance id", &self.runtime_instance_id)?;
        validate_identity("local validator id", &self.local_validator_id)?;
        decode_key(&self.local_signing_public_key)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftTransitionInstallIntentBodyV2 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub expected_revision: u64,
    pub expected_state_digest: String,
    pub lease_generation: u64,
    pub request_id: String,
    pub intent_nonce: String,
    pub runtime_instance_id: String,
    pub transition_source_height: u64,
    pub transition_source_parent_commit_ref: String,
    pub transition_block_id: String,
    pub transition_digest: String,
    pub confirmation_height: u64,
    pub confirmation_block_id: String,
    pub activation_height: u64,
    pub install_head_height: u64,
    pub old_validator_set_hash: String,
    pub new_validator_set_hash: String,
    pub install_active_set_hash: String,
    pub target_authority_epoch: u64,
    pub install_parent_commit_ref: String,
    pub install_state_root: String,
    pub install_manifest_hash: String,
    pub local_validator_id: String,
    pub local_signing_public_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftTransitionInstallIntentV2 {
    pub body: BftTransitionInstallIntentBodyV2,
    pub provider_public_key: String,
    pub provider_signature_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BftTransitionInstallPreparationV2 {
    Pending {
        intent: BftTransitionInstallIntentV2,
        checkpoint: BftSignerCheckpointV2,
    },
    Completed {
        receipt: BftTransitionInstallReceiptV2,
        checkpoint: BftSignerCheckpointV2,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftTransitionInstallResultV2 {
    pub receipt: BftTransitionInstallReceiptV2,
    pub checkpoint: BftSignerCheckpointV2,
}

impl BftTransitionInstallIntentV2 {
    pub(crate) fn sign(
        body: BftTransitionInstallIntentBodyV2,
        provider_key: &dyn BftSigningKeyHandleV2,
    ) -> Result<Self, BftSignerErrorV2> {
        let provider_public_key = provider_key.public_key_hex();
        decode_key(&provider_public_key)?;
        let preimage = framed_cbor(
            BFT_TRANSITION_INSTALL_INTENT_DOMAIN_V2,
            &body,
            BFT_SIGNER_MAX_SIGNING_BYTES_V2,
        )?;
        let signature = provider_key.sign_preimage(&preimage)?;
        Ok(Self {
            body,
            provider_public_key,
            provider_signature_hex: hex::encode(signature),
        })
    }

    pub fn digest(&self) -> Result<String, BftSignerErrorV2> {
        digest_hex(BFT_TRANSITION_INSTALL_INTENT_DOMAIN_V2, self)
    }

    pub fn verify_for(
        &self,
        binding: &BftSignerBindingV2,
        provider_public_key: &str,
    ) -> Result<(), BftSignerErrorV2> {
        validate_intent_body(&self.body, binding)?;
        if self.provider_public_key != provider_public_key {
            return Err(invalid("BFT V2 install intent provider identity mismatch"));
        }
        verify_signature(
            BFT_TRANSITION_INSTALL_INTENT_DOMAIN_V2,
            &self.body,
            &self.provider_signature_hex,
            provider_public_key,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftTransitionInstallAckBodyV2 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub intent_digest: String,
    pub evidence: BftLocalInstallEvidenceV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSignedTransitionInstallAckV2 {
    pub body: BftTransitionInstallAckBodyV2,
    pub requester_public_key: String,
    pub requester_signature_hex: String,
}

impl BftSignedTransitionInstallAckV2 {
    pub(crate) fn sign(
        intent: &BftTransitionInstallIntentV2,
        evidence: BftLocalInstallEvidenceV2,
        binding: &BftSignerBindingV2,
        pinned_provider_public_key: &str,
        requester_key: &SigningKey,
    ) -> Result<Self, BftSignerErrorV2> {
        intent.verify_for(binding, pinned_provider_public_key)?;
        evidence.validate_for(intent, binding)?;
        if hex::encode(requester_key.verifying_key().to_bytes())
            != binding.requester_auth_public_key
        {
            return Err(invalid("BFT V2 install ACK requester key mismatch"));
        }
        let body = BftTransitionInstallAckBodyV2 {
            schema_version: 2,
            binding_digest: binding.digest()?,
            intent_digest: intent.digest()?,
            evidence,
        };
        let preimage = framed_cbor(
            BFT_TRANSITION_INSTALL_ACK_DOMAIN_V2,
            &body,
            BFT_SIGNER_MAX_SIGNING_BYTES_V2,
        )?;
        Ok(Self {
            body,
            requester_public_key: binding.requester_auth_public_key.clone(),
            requester_signature_hex: hex::encode(requester_key.sign(&preimage).to_bytes()),
        })
    }

    pub fn verify_for(
        &self,
        intent: &BftTransitionInstallIntentV2,
        binding: &BftSignerBindingV2,
    ) -> Result<(), BftSignerErrorV2> {
        if self.body.schema_version != 2
            || self.body.binding_digest != binding.digest()?
            || self.body.intent_digest != intent.digest()?
            || self.requester_public_key != binding.requester_auth_public_key
        {
            return Err(invalid("BFT V2 install ACK binding mismatch"));
        }
        self.body.evidence.validate_for(intent, binding)?;
        verify_signature(
            BFT_TRANSITION_INSTALL_ACK_DOMAIN_V2,
            &self.body,
            &self.requester_signature_hex,
            &binding.requester_auth_public_key,
        )
    }

    pub fn digest(&self) -> Result<String, BftSignerErrorV2> {
        digest_hex(BFT_TRANSITION_INSTALL_ACK_DOMAIN_V2, self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftTransitionInstallReceiptBodyV2 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub request_id: String,
    pub intent_digest: String,
    pub acknowledgement_digest: String,
    pub source_height: u64,
    pub transition_source_parent_commit_ref: String,
    pub transition_block_id: String,
    pub transition_digest: String,
    pub confirmation_height: u64,
    pub confirmation_block_id: String,
    pub activation_height: u64,
    pub installed_head_height: u64,
    pub old_validator_set_hash: String,
    pub new_validator_set_hash: String,
    pub active_validator_set_hash: String,
    pub local_validator_id: String,
    pub local_signing_public_key: String,
    pub authority_epoch: u64,
    pub runtime_instance_id: String,
    pub installed_head_commit_ref: String,
    pub installed_state_root: String,
    pub installed_manifest_hash: String,
    pub wal_install_digest: String,
    pub driver_install_digest: String,
    pub lease_generation: u64,
    pub installed_revision: u64,
    pub pre_install_state_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftTransitionInstallReceiptV2 {
    pub body: BftTransitionInstallReceiptBodyV2,
    pub provider_public_key: String,
    pub provider_signature_hex: String,
}

impl BftTransitionInstallReceiptV2 {
    pub(crate) fn sign(
        body: BftTransitionInstallReceiptBodyV2,
        provider_key: &dyn BftSigningKeyHandleV2,
    ) -> Result<Self, BftSignerErrorV2> {
        let provider_public_key = provider_key.public_key_hex();
        decode_key(&provider_public_key)?;
        let preimage = framed_cbor(
            BFT_TRANSITION_INSTALL_RECEIPT_DOMAIN_V2,
            &body,
            BFT_SIGNER_MAX_SIGNING_BYTES_V2,
        )?;
        let signature = provider_key.sign_preimage(&preimage)?;
        Ok(Self {
            body,
            provider_public_key,
            provider_signature_hex: hex::encode(signature),
        })
    }

    pub fn digest(&self) -> Result<String, BftSignerErrorV2> {
        digest_hex(BFT_TRANSITION_INSTALL_RECEIPT_DOMAIN_V2, self)
    }

    pub fn verify_for(
        &self,
        binding: &BftSignerBindingV2,
        provider_public_key: &str,
    ) -> Result<(), BftSignerErrorV2> {
        let expected_confirmation = self.body.source_height.checked_add(1);
        let expected_activation = self.body.source_height.checked_add(2);
        if self.body.schema_version != 2
            || self.body.binding_digest != binding.digest()?
            || self.provider_public_key != provider_public_key
            || self.body.source_height == 0
            || self.body.lease_generation == 0
            || self.body.installed_revision == 0
            || self.body.runtime_instance_id != binding.operator_instance_id
            || expected_confirmation != Some(self.body.confirmation_height)
            || expected_activation != Some(self.body.activation_height)
            || self.body.installed_head_height < self.body.confirmation_height
            || self.body.active_validator_set_hash != self.body.new_validator_set_hash
            || self.body.local_validator_id != binding.local_validator_id
            || !binding.provisioned_signing_keys.iter().any(|key| {
                key.validator_id == self.body.local_validator_id
                    && key.public_key_hex == self.body.local_signing_public_key
            })
        {
            return Err(invalid("BFT V2 transition receipt binding mismatch"));
        }
        for (label, hash) in [
            ("receipt binding digest", self.body.binding_digest.as_str()),
            ("receipt intent digest", self.body.intent_digest.as_str()),
            (
                "receipt ACK digest",
                self.body.acknowledgement_digest.as_str(),
            ),
            (
                "receipt transition block id",
                self.body.transition_block_id.as_str(),
            ),
            (
                "receipt transition source parent",
                self.body.transition_source_parent_commit_ref.as_str(),
            ),
            (
                "receipt transition digest",
                self.body.transition_digest.as_str(),
            ),
            (
                "receipt confirmation block id",
                self.body.confirmation_block_id.as_str(),
            ),
            (
                "receipt active set hash",
                self.body.active_validator_set_hash.as_str(),
            ),
            (
                "receipt old set hash",
                self.body.old_validator_set_hash.as_str(),
            ),
            (
                "receipt new set hash",
                self.body.new_validator_set_hash.as_str(),
            ),
            (
                "receipt installed CommitRef",
                self.body.installed_head_commit_ref.as_str(),
            ),
            (
                "receipt state root",
                self.body.installed_state_root.as_str(),
            ),
            (
                "receipt manifest",
                self.body.installed_manifest_hash.as_str(),
            ),
            ("receipt WAL digest", self.body.wal_install_digest.as_str()),
            (
                "receipt driver digest",
                self.body.driver_install_digest.as_str(),
            ),
            (
                "receipt pre-install state digest",
                self.body.pre_install_state_digest.as_str(),
            ),
        ] {
            validate_hash(label, hash)?;
        }
        validate_identity("receipt request id", &self.body.request_id)?;
        validate_identity("receipt runtime instance", &self.body.runtime_instance_id)?;
        decode_key(&self.body.local_signing_public_key)?;
        verify_signature(
            BFT_TRANSITION_INSTALL_RECEIPT_DOMAIN_V2,
            &self.body,
            &self.provider_signature_hex,
            provider_public_key,
        )
    }
}

pub(crate) fn validate_intent_body(
    body: &BftTransitionInstallIntentBodyV2,
    binding: &BftSignerBindingV2,
) -> Result<(), BftSignerErrorV2> {
    let expected_confirmation = body
        .transition_source_height
        .checked_add(1)
        .ok_or_else(|| invalid("BFT V2 transition source height overflow"))?;
    let expected_activation = body
        .transition_source_height
        .checked_add(2)
        .ok_or_else(|| invalid("BFT V2 activation height overflow"))?;
    if body.schema_version != 2
        || body.binding_digest != binding.digest()?
        || body.lease_generation == 0
        || body.transition_source_height == 0
        || body.confirmation_height != expected_confirmation
        || body.activation_height != expected_activation
        || body.install_head_height < body.confirmation_height
        || body.install_active_set_hash != body.new_validator_set_hash
        || body.local_validator_id != binding.local_validator_id
        || body.runtime_instance_id != binding.operator_instance_id
        || !binding.provisioned_signing_keys.iter().any(|key| {
            key.validator_id == body.local_validator_id
                && key.public_key_hex == body.local_signing_public_key
        })
    {
        return Err(invalid("BFT V2 transition install intent fields mismatch"));
    }
    validate_hash("intent state digest", &body.expected_state_digest)?;
    validate_hash(
        "intent transition source parent",
        &body.transition_source_parent_commit_ref,
    )?;
    validate_hash("intent transition block id", &body.transition_block_id)?;
    validate_hash("intent transition digest", &body.transition_digest)?;
    validate_hash("intent confirmation block id", &body.confirmation_block_id)?;
    validate_hash("intent old set hash", &body.old_validator_set_hash)?;
    validate_hash("intent new set hash", &body.new_validator_set_hash)?;
    validate_hash(
        "intent install active set hash",
        &body.install_active_set_hash,
    )?;
    validate_hash("intent install parent", &body.install_parent_commit_ref)?;
    validate_hash("intent install state root", &body.install_state_root)?;
    validate_hash("intent install manifest", &body.install_manifest_hash)?;
    validate_identity("intent runtime instance", &body.runtime_instance_id)?;
    validate_identity("intent request id", &body.request_id)?;
    validate_key_identity(&body.local_signing_public_key)?;
    if body.intent_nonce.len() != 64
        || body
            .intent_nonce
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("BFT V2 transition intent nonce is not canonical"));
    }
    Ok(())
}

fn validate_key_identity(value: &str) -> Result<(), BftSignerErrorV2> {
    decode_key(value).map(|_| ())
}

fn validate_hash(label: &str, value: &str) -> Result<(), BftSignerErrorV2> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid(format!(
            "BFT V2 {label} must be canonical lowercase hash"
        )));
    }
    Ok(())
}

fn validate_identity(label: &str, value: &str) -> Result<(), BftSignerErrorV2> {
    if value.trim().is_empty() || value.len() > 256 {
        return Err(invalid(format!("BFT V2 {label} is outside its bound")));
    }
    Ok(())
}
