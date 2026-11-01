//! Closed billing attestation codec, not an issuer or a route-enabling flag.
use crate::world_service::{SignedReadRequest, WorldIdentity};
use oasis7_agent_api::game::DecimalU64;
use serde::{Deserialize, Serialize};
pub const PAID_CLEANUP_TRANSPORT_DOMAIN_V1: &str = "oasis7.custody.paid-cleanup-transport.v1";
pub const CLEANUP_NATIVE_PATH_V2: &str = "/api/hosted-identity/entry/task-cleanup-v2";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupHttpOperationV1 {
    Submit,
    Lookup,
}
impl CleanupHttpOperationV1 {
    pub fn world_path(self) -> &'static str {
        match self {
            Self::Submit => "/v1/world/task-cleanup-submit-v2",
            Self::Lookup => "/v1/world/task-cleanup-lookup-v2",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupConsumerRoleV1 {
    Native,
    World,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupPeerInstanceV1 {
    pub origin: String,
    pub incarnation: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidCleanupTransportV1 {
    pub api_revision: u8,
    pub issuer_namespace: String,
    pub issuer_service_id: String,
    pub issuer_environment: String,
    pub issuer_key_id: String,
    pub issuer_epoch: DecimalU64,
    pub issuer_config_generation: DecimalU64,
    pub native: CleanupPeerInstanceV1,
    pub world_peer: CleanupPeerInstanceV1,
    pub world: WorldIdentity,
    pub branch_id: String,
    pub reorg_epoch: DecimalU64,
    pub operation: CleanupHttpOperationV1,
    pub custody_account_id: String,
    pub provision_id: String,
    pub setup_id: String,
    pub session_id: String,
    pub approval_operation_id: String,
    pub approval_input_digest: String,
    pub approved_intent_digest: String,
    pub test_session_plan_digest: String,
    pub termination_plan_digest: String,
    pub original_digest: String,
    pub scope_digest: String,
    pub canonical_operation_id: String,
    pub source_operation_id: String,
    pub source_record_digest: String,
    pub source_security_epoch: DecimalU64,
    pub directory_generation: DecimalU64,
    pub resource_flag_generation: DecimalU64,
    pub attempt_id: String,
    pub native_transport_id: String,
    pub world_transport_id: String,
    pub paid_record_digest: String,
    pub native_body_digest: String,
    pub world_body_digest: String,
    pub native_paid_index: DecimalU64,
    pub world_paid_index: DecimalU64,
    pub native_http_units: DecimalU64,
    pub world_http_units: DecimalU64,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub cleanup_until: DecimalU64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidCleanupTransportProofV1 {
    pub request: PaidCleanupTransportV1,
    pub subject_public_key: String,
    pub signature_hex: String,
}
impl core::fmt::Debug for PaidCleanupTransportProofV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PaidCleanupTransportProofV1(<private>)")
    }
}
impl From<SignedReadRequest<PaidCleanupTransportV1>> for PaidCleanupTransportProofV1 {
    fn from(value: SignedReadRequest<PaidCleanupTransportV1>) -> Self {
        Self {
            request: value.request,
            subject_public_key: value.subject_public_key,
            signature_hex: value.signature_hex,
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupIssuerPinV1 {
    pub namespace: String,
    pub service_id: String,
    pub environment: String,
    pub key_id: String,
    pub public_key: String,
    pub epoch: DecimalU64,
    pub config_generation: DecimalU64,
    pub not_before: DecimalU64,
    pub not_after: DecimalU64,
    pub native_origin: String,
    pub world_origin: String,
    pub world: WorldIdentity,
}
/// Values must come from actual Account/configuration recovery authority.
/// This DTO does not attest protected storage or arbitrary VM rollback safety.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupIssuerFloorV1 {
    pub epoch: DecimalU64,
    pub config_generation: DecimalU64,
}
fn id(v: &str) -> bool {
    !v.trim().is_empty() && v.len() <= 256
}
fn hex(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn billing_digest(domain: &str, value: &impl Serialize) -> Result<String, String> {
    let value = crate::world_service::request_digest(domain, value)?;
    let hex = value
        .strip_prefix("blake3:")
        .ok_or("cleanup digest encoding")?;
    if !self::hex(hex) {
        return Err("cleanup digest encoding".into());
    }
    Ok(hex.into())
}
impl PaidCleanupTransportV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.world.validate().map_err(|e| e.to_string())?;
        if self.api_revision != 1
            || !crate::canonical_https_origin::is_canonical_https_origin(&self.native.origin)
            || !crate::canonical_https_origin::is_canonical_https_origin(&self.world_peer.origin)
            || ![
                &self.issuer_namespace,
                &self.issuer_service_id,
                &self.issuer_environment,
                &self.issuer_key_id,
                &self.branch_id,
                &self.custody_account_id,
                &self.provision_id,
                &self.setup_id,
                &self.session_id,
                &self.approval_operation_id,
                &self.canonical_operation_id,
                &self.source_operation_id,
                &self.attempt_id,
                &self.native_transport_id,
                &self.world_transport_id,
            ]
            .iter()
            .all(|v| id(v))
            || ![
                &self.native.incarnation,
                &self.world_peer.incarnation,
                &self.approval_input_digest,
                &self.approved_intent_digest,
                &self.test_session_plan_digest,
                &self.termination_plan_digest,
                &self.original_digest,
                &self.scope_digest,
                &self.source_record_digest,
                &self.paid_record_digest,
                &self.native_body_digest,
                &self.world_body_digest,
            ]
            .iter()
            .all(|v| hex(v))
            || self.issuer_epoch.0 == 0
            || self.issuer_config_generation.0 == 0
            || self.source_security_epoch.0 == 0
            || self.native_transport_id == self.world_transport_id
            || self.native_transport_id == self.attempt_id
            || self.world_transport_id == self.attempt_id
            || self.native_http_units.0 != 1
            || self.world_http_units.0 != 1
            || self.native_paid_index.0 >= 24
            || self.world_paid_index.0 >= 24
            || self.native_paid_index == self.world_paid_index
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > self.cleanup_until.0
            || self.expires_at.0 - self.issued_at.0 > 5000
        {
            return Err("invalid paid cleanup transport shape".into());
        }
        Ok(())
    }
    pub fn attempt_key(&self, role: CleanupConsumerRoleV1) -> Result<String, String> {
        self.validate()?;
        billing_digest(
            "oasis7.custody.cleanup-consume-key.v1",
            &(
                &self.issuer_namespace,
                &self.issuer_service_id,
                &self.issuer_environment,
                &self.issuer_key_id,
                self.issuer_epoch,
                self.issuer_config_generation,
                &self.custody_account_id,
                &self.session_id,
                &self.termination_plan_digest,
                role,
                match role {
                    CleanupConsumerRoleV1::Native => &self.native_transport_id,
                    CleanupConsumerRoleV1::World => &self.world_transport_id,
                },
            ),
        )
    }
}
impl CleanupIssuerPinV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.world.validate().map_err(|e| e.to_string())?;
        let bytes: [u8; 32] = ::hex::decode(&self.public_key)
            .map_err(|_| "invalid issuer key")?
            .try_into()
            .map_err(|_| "invalid issuer key")?;
        let key =
            ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid issuer key")?;
        if key.is_weak()
            || !hex(&self.public_key)
            || self.epoch.0 == 0
            || self.config_generation.0 == 0
            || self.not_before.0 >= self.not_after.0
            || ![
                &self.namespace,
                &self.service_id,
                &self.environment,
                &self.key_id,
            ]
            .iter()
            .all(|v| id(v))
            || !crate::canonical_https_origin::is_canonical_https_origin(&self.native_origin)
            || !crate::canonical_https_origin::is_canonical_https_origin(&self.world_origin)
        {
            return Err("invalid cleanup issuer pin".into());
        }
        Ok(())
    }
}
/// Signature and binding checks only. Caller must durably consume before work.
/// Actual role/origin/path/body/incarnation are provided by the receiving adapter,
/// never taken back from the envelope as asserted authority.
pub struct CleanupTransportReceiveV1<'a> {
    pub role: CleanupConsumerRoleV1,
    pub origin: &'a str,
    pub method: &'a str,
    pub path: &'a str,
    pub body_digest: &'a str,
    pub incarnation: &'a str,
    pub now: u64,
}
pub fn verify_paid_cleanup_transport_v1(
    proof: &PaidCleanupTransportProofV1,
    pin: &CleanupIssuerPinV1,
    floor: &CleanupIssuerFloorV1,
    receive: &CleanupTransportReceiveV1<'_>,
) -> Result<(), String> {
    proof.request.validate()?;
    pin.validate()?;
    let p = &proof.request;
    let (peer, path, body) = match receive.role {
        CleanupConsumerRoleV1::Native => (&p.native, CLEANUP_NATIVE_PATH_V2, &p.native_body_digest),
        CleanupConsumerRoleV1::World => (
            &p.world_peer,
            p.operation.world_path(),
            &p.world_body_digest,
        ),
    };
    if proof.subject_public_key != pin.public_key
        || p.issuer_namespace != pin.namespace
        || p.issuer_service_id != pin.service_id
        || p.issuer_environment != pin.environment
        || p.issuer_key_id != pin.key_id
        || p.issuer_epoch != pin.epoch
        || p.issuer_config_generation != pin.config_generation
        || floor.epoch.0 == 0
        || floor.config_generation.0 == 0
        || p.issuer_epoch.0 < floor.epoch.0
        || p.issuer_config_generation.0 < floor.config_generation.0
        || p.world != pin.world
        || p.native.origin != pin.native_origin
        || p.world_peer.origin != pin.world_origin
        || p.issued_at.0 < pin.not_before.0
        || p.expires_at.0 > pin.not_after.0
        || receive.now < p.issued_at.0
        || receive.now >= p.expires_at.0
        || receive.origin != peer.origin
        || receive.method != "POST"
        || receive.path != path
        || receive.body_digest != body
        || receive.incarnation != peer.incarnation
    {
        return Err("paid cleanup transport binding mismatch".into());
    }
    crate::world_service::private_goal::verify_owner_signature_strict(
        PAID_CLEANUP_TRANSPORT_DOMAIN_V1,
        p,
        &pin.public_key,
        &proof.signature_hex,
    )
}

#[cfg(test)]
#[path = "paid_cleanup_transport_v1_tests.rs"]
mod tests;
