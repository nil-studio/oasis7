//! Independently issued permission for original, possibly uncommitted input.
//! Existing full-state read grants never imply this purpose.
use super::binding::{BftSignerErrorV3, BftSigningKeyHandleV3, decode_key, invalid};
use super::private_retention_input::BftPrivateRetentionAdmissionInputRefV1;
use serde::{Deserialize, Serialize};
const DOMAIN: &str = "oasis7.bft.private-retention.original-input-read-grant.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BftPrivateRetentionAdmissionReadPurposeV1 {
    OriginalAdmissionInput,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionAdmissionReadGrantBodyV1 {
    pub schema_version: u32,
    pub purpose: BftPrivateRetentionAdmissionReadPurposeV1,
    pub scope: oasis7_proto::distributed_bft::v2::BftScopeV2,
    pub reference: BftPrivateRetentionAdmissionInputRefV1,
    pub subject_public_key: String,
    pub requester_peer_id: String,
    pub serving_peer_id: String,
    pub schedule_digest: String,
    pub view_generation: u64,
    pub request_nonce: String,
    pub grant_epoch: u64,
    pub not_before_unix_ms: u64,
    pub expires_unix_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionAdmissionReadGrantV1 {
    pub issuer_public_key: String,
    pub body: BftPrivateRetentionAdmissionReadGrantBodyV1,
    pub signature_hex: String,
}
impl BftPrivateRetentionAdmissionReadGrantBodyV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.reference.validate()?;
        decode_key(&self.subject_public_key)?;
        super::state::validate_digest(&self.schedule_digest)?;
        super::state::validate_digest(&self.request_nonce)?;
        super::state::validate_digest(&self.scope.genesis_hash)?;
        if self.schema_version != 1
            || self.grant_epoch == 0
            || self.expires_unix_ms <= self.not_before_unix_ms
            || self.scope.world_id.is_empty()
            || self.scope.world_id.len() > 256
            || self.scope.chain_id.is_empty()
            || self.scope.chain_id.len() > 256
            || [&self.requester_peer_id, &self.serving_peer_id]
                .into_iter()
                .any(|peer| peer.is_empty() || peer.len() > 128 || peer.trim() != peer)
            || self.requester_peer_id == self.serving_peer_id
        {
            return Err(invalid(
                "admission read grant purpose/scope/endpoint/validity bounds",
            ));
        }
        Ok(())
    }
}
impl BftPrivateRetentionAdmissionReadGrantV1 {
    pub fn sign(
        body: BftPrivateRetentionAdmissionReadGrantBodyV1,
        issuer: &dyn BftSigningKeyHandleV3,
    ) -> Result<Self, BftSignerErrorV3> {
        body.validate()?;
        let issuer_public_key = issuer.public_key_hex();
        decode_key(&issuer_public_key)?;
        let signature_hex = super::crypto::sign_preimage(issuer, DOMAIN, &body)?;
        Ok(Self {
            issuer_public_key,
            body,
            signature_hex,
        })
    }
    /// Signature verification alone is insufficient. The consumer must also
    /// sample the issuer's fresh witness/revocation state and compare the exact
    /// current view, Noise endpoints, subject, input, nonce and request signature.
    pub fn verify_signature(&self, trusted_issuer: &str) -> Result<(), BftSignerErrorV3> {
        self.body.validate()?;
        if self.issuer_public_key != trusted_issuer {
            return Err(invalid("admission read grant untrusted issuer"));
        }
        super::crypto::verify_signature(DOMAIN, &self.body, &self.signature_hex, trusted_issuer)
    }
}
