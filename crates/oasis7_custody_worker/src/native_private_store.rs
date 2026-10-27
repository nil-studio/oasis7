//! Full private-record AAD context and fail-closed configuration.
//! The concrete Identity manifest factory owns crypto; record actors store only envelopes.
#[path = "native_provision_driver.rs"]
pub(crate) mod driver;
#[path = "native_private_manifest.rs"]
pub(crate) mod manifest;
use oasis7_custody_core::{
    DecimalU64, approval::bytes_digest, envelope::*, http::Role, role_key_slot::RoleKeySlotScopeV1,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use zeroize::Zeroizing;

type Result<T> = std::result::Result<T, JsValue>;
fn rejected() -> JsValue {
    JsValue::from_str("native private record rejected")
}
fn id(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}

/// Expected address comes from the original Identity task and current Account permit.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativePrivateScope {
    pub role_scope: RoleKeySlotScopeV1,
    pub identity_namespace: String,
    pub identity_instance_epoch: DecimalU64,
    pub record_id: String,
    pub record_version: DecimalU64,
    pub purpose: NativePrivatePurpose,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NativePrivatePurpose {
    Preauth,
    OriginalRequest,
    NativeSession,
}
impl NativePrivateScope {
    pub(crate) fn context(&self) -> Result<EnvelopeContextV1> {
        self.role_scope.validate().map_err(|_| rejected())?;
        if self.role_scope.context.identity.role != Role::Identity
            || !id(&self.identity_namespace)
            || !id(&self.record_id)
            || self.identity_instance_epoch.0 == 0
            || self.record_version.0 == 0
        {
            return Err(rejected());
        }
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.native-private-record.v1",
            self,
        ))
        .map_err(|_| rejected())?;
        let mut context = self.role_scope.context.clone();
        context.identity.key_id = bytes_digest(&bytes);
        context.identity.key_version = self.record_version;
        context.validate().map_err(|_| rejected())?;
        Ok(context)
    }
}
/// Production configuration remains fail-closed. The role-key KEK is separate;
/// neither key is accepted in a browser/RPC payload or created on first use.
pub(crate) struct NativePrivateConfig {
    pub kek: Zeroizing<[u8; 32]>,
    pub wrapping_version: DecimalU64,
}
impl NativePrivateConfig {
    pub(crate) fn load(env: &worker::Env) -> Result<Self> {
        if env
            .var("CUSTODY_NATIVE_PROVISION_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        {
            return Err(rejected());
        }
        let raw = Zeroizing::new(
            env.secret("CUSTODY_NATIVE_PRIVATE_KEK")
                .map_err(|_| rejected())?
                .to_string(),
        );
        if raw.len() != 64
            || !raw
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(rejected());
        }
        let mut kek = Zeroizing::new([0; 32]);
        hex::decode_to_slice(raw.as_bytes(), kek.as_mut_slice()).map_err(|_| rejected())?;
        let wrapping_version: DecimalU64 = serde_json::from_value(serde_json::Value::String(
            env.var("CUSTODY_NATIVE_PRIVATE_WRAPPING_KEY_VERSION")
                .map_err(|_| rejected())?
                .to_string(),
        ))
        .map_err(|_| rejected())?;
        if wrapping_version.0 == 0 {
            return Err(rejected());
        }
        Ok(Self {
            kek,
            wrapping_version,
        })
    }
}
