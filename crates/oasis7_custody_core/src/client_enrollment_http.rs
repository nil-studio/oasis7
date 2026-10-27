//! Public metadata responses only; neither shape nor status is proof of authority.
use super::*;
use crate::time_units::UnixMillis;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientEnrollmentCreateResponseV1 {
    pub api_version: u8,
    pub enrollment_id: String,
    pub key_id: String,
    pub server_nonce: String,
    pub challenge_expires_at: UnixMillis,
    pub expires_at: UnixMillis,
    pub descriptor_digest: String,
}
impl ClientEnrollmentCreateResponseV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        if self.api_version != 1
            || !id(&self.enrollment_id)
            || !id(&self.key_id)
            || !enrollment_nonce(&self.server_nonce)
            || !hex64(&self.descriptor_digest)
            || self.challenge_expires_at.get() == 0
            || self.challenge_expires_at.get() > self.expires_at.get()
        {
            return Err(EnrollmentError::Invalid);
        }
        Ok(())
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientEnrollmentProofStatusV1 {
    Proved,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientEnrollmentProofResponseV1 {
    pub api_version: u8,
    pub enrollment_id: String,
    pub status: ClientEnrollmentProofStatusV1,
}
impl ClientEnrollmentProofResponseV1 {
    pub fn validate(&self) -> Result<(), EnrollmentError> {
        if self.api_version != 1 || !id(&self.enrollment_id) {
            return Err(EnrollmentError::Invalid);
        }
        Ok(())
    }
}
