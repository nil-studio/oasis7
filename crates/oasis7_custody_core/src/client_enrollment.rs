//! Closed public pairing metadata. These DTOs do not attest OS isolation or
//! confer authority; deployment allowlists and same-Account approval are separate.
use crate::{DecimalU64, approval::bytes_digest, time_units::UnixMillis};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrollmentError {
    Invalid,
    Bounds,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostTransportProfileV1 {
    NativeBrokerCustodyV1,
    DotMcpCustodyV1,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostIsolationMechanismV1 {
    SeparateOsPrincipal,
    EnforcedSandbox,
    RemoteClosedToolHost,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostDeploymentDescriptorV1 {
    pub api_version: u8,
    pub environment: String,
    pub transport_profile: HostTransportProfileV1,
    pub tool_profile: String,
    pub host_instance_id: String,
    pub client_public_key: String,
    pub build_digest: String,
    pub skill_manifest_digest: String,
    pub tool_manifest_digest: String,
    pub https_origin: String,
    pub restricted_runtime_identity: String,
    pub isolation_mechanism: HostIsolationMechanismV1,
    pub isolation_acceptance_refs: Vec<String>,
}
impl HostDeploymentDescriptorV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        if self.api_version != 1
            || !id(&self.environment)
            || !id(&self.host_instance_id)
            || self.tool_profile != "custody-native-v1"
            || !hex64(&self.client_public_key)
            || ![
                &self.build_digest,
                &self.skill_manifest_digest,
                &self.tool_manifest_digest,
            ]
            .iter()
            .all(|s| hex64(s))
            || !text(&self.restricted_runtime_identity)
            || self.isolation_acceptance_refs.is_empty()
            || self.isolation_acceptance_refs.len() > 16
            || self.isolation_acceptance_refs.iter().any(|s| !text(s))
        {
            return Err(EnrollmentError::Invalid);
        }
        crate::http_pop::CanonicalHttpTarget::new(
            &self.https_origin,
            crate::http_pop::HttpPopRoute::ProofChallenges,
        )
        .map_err(|_| EnrollmentError::Invalid)?;
        Ok(())
    }
    pub fn digest(&self) -> Result<String, EnrollmentError> {
        self.validate()?;
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.host-deployment-descriptor.v1",
            self,
        ))
        .map_err(|_| EnrollmentError::Invalid)?;
        Ok(bytes_digest(&bytes))
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientEnrollmentCreateRequestV1 {
    pub target: PairingInviteTargetRefV1,
    pub api_version: u8,
    pub request_id: String,
    pub descriptor: HostDeploymentDescriptorV1,
}
impl ClientEnrollmentCreateRequestV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        self.descriptor.validate()?;
        self.target.validate()?;
        if self.api_version != 1 || !id(&self.request_id) {
            return Err(EnrollmentError::Invalid);
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientEnrollmentProofRequestV1 {
    pub target: PairingInviteTargetRefV1,
    pub client_public_key: String,
    pub server_nonce: String,
    pub challenge_expires_at: UnixMillis,
    pub api_version: u8,
    pub enrollment_id: String,
    pub request_id: String,
    pub descriptor_digest: String,
}
impl ClientEnrollmentProofRequestV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        self.target.validate()?;
        if self.api_version != 1
            || !id(&self.enrollment_id)
            || !id(&self.request_id)
            || !hex64(&self.descriptor_digest)
            || !hex64(&self.client_public_key)
            || !enrollment_nonce(&self.server_nonce)
            || self.challenge_expires_at.get() == 0
        {
            return Err(EnrollmentError::Invalid);
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestSessionPurposeV1 {
    Execute,
    Read,
    Cleanup,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientCredentialClaimRequestV1 {
    pub api_version: u8,
    pub enrollment_id: String,
    pub test_session_id: String,
    pub request_id: String,
    pub purposes: Vec<TestSessionPurposeV1>,
    pub replace: bool,
    pub expected_generation: Option<DecimalU64>,
}
impl ClientCredentialClaimRequestV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        if self.api_version != 1
            || ![&self.enrollment_id, &self.test_session_id, &self.request_id]
                .iter()
                .all(|s| id(s))
            || self.purposes.is_empty()
            || self.purposes.len() > 2
            || self.purposes.contains(&TestSessionPurposeV1::Cleanup)
            || self.purposes.len() == 2 && self.purposes[0] == self.purposes[1]
            || self.replace != self.expected_generation.is_some()
            || self.expected_generation.is_some_and(|v| v.0 == 0)
        {
            return Err(EnrollmentError::Invalid);
        }
        Ok(())
    }
}
/// Explicit millisecond limits, not a grant and not the legacy seconds window.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPurposeLimitsV1 {
    pub purpose: TestSessionPurposeV1,
    pub not_before: UnixMillis,
    pub deadline: UnixMillis,
    pub request_limit: DecimalU64,
    pub transport_limit: DecimalU64,
    pub business_limit: DecimalU64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreePurposeLimitsV1 {
    pub execute: TestSessionPurposeLimitsV1,
    pub read: TestSessionPurposeLimitsV1,
    pub cleanup: TestSessionPurposeLimitsV1,
}
impl ThreePurposeLimitsV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        if self.execute.purpose != TestSessionPurposeV1::Execute
            || self.read.purpose != TestSessionPurposeV1::Read
            || self.cleanup.purpose != TestSessionPurposeV1::Cleanup
            || self.read.business_limit.0 != 0
            || self.read.deadline.get() < self.execute.deadline.get()
            || self.cleanup.deadline.get() < self.execute.deadline.get()
        {
            return Err(EnrollmentError::Invalid);
        }
        for p in [&self.execute, &self.read, &self.cleanup] {
            if p.deadline.get() <= p.not_before.get()
                || p.request_limit.0 == 0
                || p.transport_limit.0 == 0
            {
                return Err(EnrollmentError::Invalid);
            }
        }
        Ok(())
    }
}
fn id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
}
fn text(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
fn hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl core::fmt::Display for EnrollmentError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid client enrollment metadata: {self:?}")
    }
}
impl std::error::Error for EnrollmentError {}

/// Public deployment pin, loaded only from existing fixed configuration.
/// Matching metadata is not an isolation attestation or a pairing grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostDeploymentPinV1 {
    pub environment: String,
    pub host_instance_id: String,
    pub descriptor_digest: String,
    pub client_public_key: String,
}
#[derive(Debug, Default)]
pub struct HostDeploymentAllowlist {
    pins: Vec<HostDeploymentPinV1>,
}
impl HostDeploymentAllowlist {
    pub fn from_fixed_config(pins: Vec<HostDeploymentPinV1>) -> Result<Self, EnrollmentError> {
        if pins.len() > 256
            || pins.iter().any(|p| {
                !id(&p.environment)
                    || !id(&p.host_instance_id)
                    || !hex64(&p.descriptor_digest)
                    || !hex64(&p.client_public_key)
            })
        {
            return Err(EnrollmentError::Invalid);
        }
        for (i, p) in pins.iter().enumerate() {
            if pins
                .iter()
                .skip(i + 1)
                .any(|q| q.environment == p.environment && q.host_instance_id == p.host_instance_id)
            {
                return Err(EnrollmentError::Invalid);
            }
        }
        Ok(Self { pins })
    }
    pub fn configured_metadata<'a>(
        &'a self,
        descriptor: &HostDeploymentDescriptorV1,
    ) -> Result<Option<&'a HostDeploymentPinV1>, EnrollmentError> {
        let digest = descriptor.digest()?;
        Ok(self.pins.iter().find(|p| {
            p.environment == descriptor.environment
                && p.host_instance_id == descriptor.host_instance_id
                && p.client_public_key == descriptor.client_public_key
                && p.descriptor_digest == digest
        }))
    }
}

#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingInviteTargetRefV1 {
    pub custody_account_id: String,
    pub provision_id: String,
    pub invite_id: String,
    pub invite_digest: String,
}
impl PairingInviteTargetRefV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        if ![
            &self.custody_account_id,
            &self.provision_id,
            &self.invite_id,
        ]
        .iter()
        .all(|v| id(v))
            || !hex64(&self.invite_digest)
        {
            return Err(EnrollmentError::Invalid);
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientPairingBindingV1 {
    pub api_version: u8,
    pub target: PairingInviteTargetRefV1,
    pub enrollment_id: String,
    pub enrollment_digest: String,
    pub host_instance_id: String,
    pub descriptor_digest: String,
    pub client_key_id: String,
    pub client_public_key: String,
    pub connector_public_key: Option<String>,
}
impl ClientPairingBindingV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        self.target.validate()?;
        if self.api_version != 1
            || ![
                &self.enrollment_id,
                &self.host_instance_id,
                &self.client_key_id,
            ]
            .iter()
            .all(|v| id(v))
            || ![
                &self.enrollment_digest,
                &self.descriptor_digest,
                &self.client_public_key,
            ]
            .iter()
            .all(|v| hex64(v))
            || self
                .connector_public_key
                .as_ref()
                .is_some_and(|v| !hex64(v) || v == &self.client_public_key)
        {
            return Err(EnrollmentError::Invalid);
        }
        Ok(())
    }
}
fn enrollment_nonce(s: &str) -> bool {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .is_ok_and(|v| {
            v.len() == 32 && base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v) == s
        })
}
#[path = "client_enrollment_state.rs"]
mod state;
pub use state::*;
