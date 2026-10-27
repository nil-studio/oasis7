//! Fixed physical enrollment namespace and public deployment policy.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use oasis7_custody_core::{
    approval::bytes_digest, client_enrollment::*, http_pop::*, time_units::UnixMillis,
};
use wasm_bindgen::JsValue;
use worker::Env;
pub(crate) const BINDING: &str = "CLIENT_ENROLLMENTS";
pub(crate) fn denied() -> JsValue {
    JsValue::from_str("client enrollment unavailable")
}
pub(crate) fn now() -> Result<UnixMillis, JsValue> {
    let n = js_sys::Date::now();
    if !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n > 9_007_199_254_740_991.0 {
        return Err(denied());
    }
    Ok(UnixMillis::new(n as u64))
}
pub(crate) fn hash<T: serde::Serialize>(domain: &str, value: &T) -> Result<String, JsValue> {
    Ok(bytes_digest(
        &oasis7_wasm_abi::encode_canonical_cbor(&(domain, value)).map_err(|_| denied())?,
    ))
}
pub(crate) struct Config {
    pub origin: String,
    pub realm: String,
    pub deployments: HostDeploymentAllowlist,
}
impl Config {
    pub fn load(env: &Env) -> Result<Self, JsValue> {
        let get = |name| env.var(name).map(|v| v.to_string()).map_err(|_| denied());
        let origin = get("CUSTODY_PUBLIC_ORIGIN")?;
        CanonicalHttpTarget::new(
            &origin,
            HttpPopRoute::EnrollmentProve("origin-check".into()),
        )
        .map_err(|_| denied())?;
        let realm = get("CUSTODY_ENROLLMENT_REALM")?;
        if realm.is_empty() || realm.len() > 256 || realm.chars().any(char::is_control) {
            return Err(denied());
        }
        let pins = match env.var("CUSTODY_HOST_DEPLOYMENTS") {
            Ok(v) => serde_json::from_str::<Vec<HostDeploymentPinV1>>(&v.to_string())
                .map_err(|_| denied())?,
            Err(_) => Vec::new(),
        };
        let deployments = HostDeploymentAllowlist::from_fixed_config(pins).map_err(|_| denied())?;
        Ok(Self {
            origin,
            realm,
            deployments,
        })
    }
    pub fn enabled(env: &Env) -> Result<Self, JsValue> {
        if env
            .var("CUSTODY_CLIENT_ENROLLMENT_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        {
            return Err(denied());
        }
        Self::load(env)
    }
    pub fn enrollment_id(&self, r: &ClientEnrollmentCreateRequestV1) -> Result<String, JsValue> {
        Ok(format!(
            "enroll-{}",
            hash(
                "oasis7.custody.enrollment-id.v1",
                &(&self.realm, &r.request_id, &r.descriptor.client_public_key)
            )?
        ))
    }
    pub fn actor_name(&self, id: &str) -> Result<String, JsValue> {
        if !valid_id(id) {
            return Err(denied());
        }
        Ok(format!(
            "enrollment:{}",
            hash("oasis7.custody.enrollment-actor.v1", &(&self.realm, id))?
        ))
    }
    pub fn actor_id(&self, env: &Env, id: &str) -> Result<String, JsValue> {
        Ok(env
            .durable_object(BINDING)
            .map_err(|_| denied())?
            .id_from_name(&self.actor_name(id)?)
            .map_err(|_| denied())?
            .to_string())
    }
    pub fn bucket_name(&self, bucket: &str) -> Result<String, JsValue> {
        if bucket.len() != 64
            || !bucket
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(denied());
        }
        Ok(format!(
            "enrollment-abuse:{}",
            hash(
                "oasis7.custody.enrollment-abuse-actor.v1",
                &(&self.realm, bucket)
            )?
        ))
    }
}
pub(crate) fn valid_id(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 256
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
}
pub(crate) fn nonce() -> Result<String, JsValue> {
    let text = crate::approval_crypto::verification_nonce().map_err(|_| denied())?;
    let bytes = hex::decode(text).map_err(|_| denied())?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
pub(crate) struct Physical {
    pub actual_actor_id: String,
    pub actual_realm: String,
    pub expected_actor_id: String,
    pub expected_realm: String,
    pub origin: String,
}
impl Physical {
    pub fn for_id(env: &Env, id: &str, actual: &str, realm: &str) -> Result<Self, JsValue> {
        let c = Config::load(env)?;
        Ok(Self {
            actual_actor_id: actual.into(),
            actual_realm: realm.into(),
            expected_actor_id: c.actor_id(env, id)?,
            expected_realm: c.realm,
            origin: c.origin,
        })
    }
    pub fn core(&self) -> EnrollmentPhysicalCurrent<'_> {
        EnrollmentPhysicalCurrent {
            actual_actor_id: &self.actual_actor_id,
            expected_actor_id: &self.expected_actor_id,
            actual_realm: &self.actual_realm,
            expected_realm: &self.expected_realm,
            public_origin: &self.origin,
        }
    }
    pub fn validate(&self) -> Result<(), JsValue> {
        if self.actual_actor_id != self.expected_actor_id
            || self.actual_realm != self.expected_realm
        {
            return Err(denied());
        }
        Ok(())
    }
}
pub(crate) struct Exact(pub oasis7_custody_core::approval_admission::ExactSignatureChecks);
impl oasis7_custody_core::approval_admission::ApprovalSignatureVerifier for Exact {
    fn verify(
        &mut self,
        c: &oasis7_custody_core::approval_admission::ExactSignatureChecks,
    ) -> Result<(), oasis7_custody_core::approval_admission::ApprovalAdmissionError> {
        if c == &self.0 {
            Ok(())
        } else {
            Err(oasis7_custody_core::approval_admission::ApprovalAdmissionError::SignatureRejected)
        }
    }
}
pub(crate) async fn verify(
    c: oasis7_custody_core::approval_admission::ExactSignatureChecks,
) -> Result<Exact, JsValue> {
    for j in c.checks() {
        crate::approval_crypto::verify_ed25519(
            j.public_key_hex(),
            j.signature_base64url(),
            j.canonical_bytes(),
        )
        .await
        .map_err(|_| denied())?;
    }
    Ok(Exact(c))
}
