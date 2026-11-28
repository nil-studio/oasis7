//! Closed public pairing metadata. These DTOs do not attest OS isolation or
//! confer authority; deployment allowlists and same-Account approval are separate.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrollmentError {
    Invalid,
    Bounds,
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

fn id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
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
