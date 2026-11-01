//! Fixed public deployment metadata only: never a Current observation or key loader.
#[cfg(feature = "owner")]
use oasis7_custody_core::provisioning::NativeStageScope;
use oasis7_custody_core::{
    DecimalU64,
    http_pop::{CanonicalHttpTarget, HttpPopRoute},
    native_binding::NativeBindingPinV1,
};
use wasm_bindgen::JsValue;
use worker::Env;
pub(crate) struct NativeDispatchHistoryPinsV2 {
    #[cfg(feature = "owner")]
    pub origin: String,
    pub key_id: String,
    pub public_key: String,
    #[cfg(feature = "owner")]
    pub owner_pin: NativeBindingPinV1,
}
fn denied() -> JsValue {
    JsValue::from_str("execution dispatch configuration unavailable")
}
impl NativeDispatchHistoryPinsV2 {
    pub(crate) fn load(env: &Env) -> Result<Self, JsValue> {
        let v = |name| env.var(name).map(|v| v.to_string()).map_err(|_| denied());
        if v("CUSTODY_TEST_SESSION_V2_ENABLED")? != "true"
            || v("CUSTODY_NATIVE_PROVISION_ENABLED")? != "true"
        {
            return Err(denied());
        }
        let origin = v("CUSTODY_NATIVE_ORIGIN")?;
        CanonicalHttpTarget::new(&origin, HttpPopRoute::ProofChallenges).map_err(|_| denied())?;
        let key_id = v("CUSTODY_NATIVE_APPROVAL_KEY_ID")?;
        let public_key = v("CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY")?;
        if key_id.is_empty()
            || key_id.len() > 256
            || key_id.chars().any(char::is_control)
            || public_key.len() != 64
            || !public_key
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(denied());
        }
        let owner_pin: NativeBindingPinV1 =
            serde_json::from_str(&v("CUSTODY_NATIVE_BINDING_OWNER_PIN")?).map_err(|_| denied())?;
        owner_pin.validate().map_err(|_| denied())?;
        let epoch: DecimalU64 =
            serde_json::from_value(v("CUSTODY_SERVICE_EPOCH")?.into()).map_err(|_| denied())?;
        if owner_pin.environment != v("CUSTODY_ROLE_KEY_ENVIRONMENT")?
            || owner_pin.service_epoch != epoch
            || owner_pin.key_id == key_id
            || owner_pin.public_key == public_key
        {
            return Err(denied());
        }
        Ok(Self {
            #[cfg(feature = "owner")]
            origin,
            key_id,
            public_key,
            #[cfg(feature = "owner")]
            owner_pin,
        })
    }
    #[cfg(feature = "owner")]
    pub(crate) fn as_core(&self) -> NativeStageScope<'_> {
        NativeStageScope {
            http_origin: &self.origin,
            native_root_key_id: &self.key_id,
            native_root_public_key: &self.public_key,
            owner_binding_pin: &self.owner_pin,
        }
    }
}
