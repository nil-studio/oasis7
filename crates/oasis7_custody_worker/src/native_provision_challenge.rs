//! Closed login/register challenge validation. Never signs an upstream supplied preimage.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use oasis7_agent_api::identity::{NativeIdentityChallengeV1, NativeIdentityPurposeV1};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;

type Result<T> = std::result::Result<T, JsValue>;
fn rejected() -> JsValue {
    JsValue::from_str("native provision challenge rejected")
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NativeProvisionPurpose {
    Register,
    Login,
}
impl NativeProvisionPurpose {
    fn codec(self) -> NativeIdentityPurposeV1 {
        match self {
            Self::Register => NativeIdentityPurposeV1::Register,
            Self::Login => NativeIdentityPurposeV1::Login,
        }
    }
}
/// Pins are built from static configuration and the exact Account native-stage permit.
pub(crate) struct NativeChallengeScope {
    pub identity_service_id: String,
    pub environment_id: String,
    pub audience: String,
    pub operation_id: String,
    pub public_key: String,
    pub purpose: NativeProvisionPurpose,
}
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeProvisionChallenge {
    pub identity_service_id: String,
    pub environment_id: String,
    pub audience: String,
    pub purpose: NativeProvisionPurpose,
    pub account_context: String,
    pub operation_id: String,
    pub public_key: String,
    pub nonce: String,
    pub issued_at: u64,
    pub expires_at: u64,
    pub signed_bytes: String,
}
impl NativeProvisionChallenge {
    pub(crate) fn signing_bytes(
        &self,
        expected: &NativeChallengeScope,
        now: u64,
    ) -> Result<Vec<u8>> {
        if self.identity_service_id != expected.identity_service_id
            || self.environment_id != expected.environment_id
            || self.audience != expected.audience
            || self.operation_id != expected.operation_id
            || self.public_key != expected.public_key
            || self.purpose != expected.purpose
            || !self.account_context.is_empty()
            || self.operation_id.is_empty()
            || self.operation_id.len() > 128
            || !self
                .operation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || self.issued_at > now
            || self.expires_at <= now
            || self.expires_at.checked_sub(self.issued_at) != Some(120_000)
        {
            return Err(rejected());
        }
        let public_key: [u8; 32] = hex::decode(&self.public_key)
            .map_err(|_| rejected())?
            .try_into()
            .map_err(|_| rejected())?;
        if hex::encode(public_key) != self.public_key {
            return Err(rejected());
        }
        let nonce: [u8; 32] = URL_SAFE_NO_PAD
            .decode(&self.nonce)
            .map_err(|_| rejected())?
            .try_into()
            .map_err(|_| rejected())?;
        if URL_SAFE_NO_PAD.encode(nonce) != self.nonce {
            return Err(rejected());
        }
        let bytes = NativeIdentityChallengeV1 {
            identity_service_id: &expected.identity_service_id,
            environment_id: &expected.environment_id,
            audience: &expected.audience,
            purpose: expected.purpose.codec(),
            account_context: "",
            operation_id: &expected.operation_id,
            public_key: &public_key,
            nonce: &nonce,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
        }
        .signing_bytes()
        .map_err(|_| rejected())?;
        if URL_SAFE_NO_PAD.encode(&bytes) != self.signed_bytes {
            return Err(rejected());
        }
        Ok(bytes)
    }
}
