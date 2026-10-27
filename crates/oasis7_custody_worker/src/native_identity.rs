#[path = "native_provision_transport.rs"]
pub(crate) mod provision_transport;
#[path = "native_source_revocation.rs"]
pub(crate) mod source_revocation;
// Fixed Identity-only native authority adapter. No caller chooses an upstream URL or headers.
use oasis7_custody_core::{DecimalU64, approval::*};
use serde::de::DeserializeOwned;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use worker::{
    AbortSignal, Env, Error, Fetch, Headers, Method, Request, RequestInit, RequestRedirect, Result,
    Url,
};

const PREFIX: &str = "/api/public/hosted-account/v2/custody";
const MAX_BYTES: usize = 32 * 1024;
const MAX_SKEW_MS: u64 = 5_000;
fn rejected() -> Error {
    Error::RustError("native identity unavailable or binding rejected".into())
}
fn checked(value: bool) -> Result<()> {
    if value { Ok(()) } else { Err(rejected()) }
}
fn clock() -> Result<u64> {
    let value = js_sys::Date::now();
    checked(
        value.is_finite()
            && value >= 0.0
            && value.fract() == 0.0
            && value <= 9_007_199_254_740_991.0,
    )?;
    Ok(value as u64)
}
fn identifier(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.bytes().any(|v| v.is_ascii_control())
}

pub(crate) struct NativeIdentityConfig {
    origin: String,
    namespace: String,
    epoch: DecimalU64,
    environment: String,
    audience: String,
    key_id: String,
    public_key: String,
    adapter_secret: String,
}
impl NativeIdentityConfig {
    pub(crate) fn from_env(env: &Env) -> Result<Option<Self>> {
        const NAMES: [&str; 8] = [
            "CUSTODY_NATIVE_ORIGIN",
            "CUSTODY_NATIVE_RP_ID",
            "CUSTODY_NATIVE_NAMESPACE",
            "CUSTODY_NATIVE_INSTANCE_EPOCH",
            "CUSTODY_NATIVE_ENVIRONMENT",
            "CUSTODY_NATIVE_AUDIENCE",
            "CUSTODY_NATIVE_APPROVAL_KEY_ID",
            "CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY",
        ];
        let present = NAMES.iter().any(|name| env.var(name).is_ok())
            || env.secret("CUSTODY_NATIVE_ADAPTER_SECRET").is_ok();
        if !present {
            return Ok(None);
        }
        let values = NAMES
            .iter()
            .map(|name| env.var(name).map(|v| v.to_string()).map_err(|_| rejected()))
            .collect::<Result<Vec<_>>>()?;
        let origin = Url::parse(&values[0]).map_err(|_| rejected())?;
        checked(
            origin.scheme() == "https"
                && origin.origin().ascii_serialization() == values[0]
                && origin.host_str() == Some(values[1].as_str())
                && origin.username().is_empty()
                && origin.password().is_none(),
        )?;
        let epoch: DecimalU64 =
            serde_json::from_value(serde_json::Value::String(values[3].clone()))
                .map_err(|_| rejected())?;
        checked(
            epoch.0 > 0
                && identifier(&values[2], 256)
                && identifier(&values[4], 64)
                && identifier(&values[5], 256)
                && identifier(&values[6], 128),
        )?;
        let key: [u8; 32] = hex::decode(&values[7])
            .map_err(|_| rejected())?
            .try_into()
            .map_err(|_| rejected())?;
        checked(
            hex::encode(key) == values[7]
                && !ed25519_dalek::VerifyingKey::from_bytes(&key)
                    .map_err(|_| rejected())?
                    .is_weak(),
        )?;
        let secret = env
            .secret("CUSTODY_NATIVE_ADAPTER_SECRET")
            .map_err(|_| rejected())?
            .to_string();
        let decoded: [u8; 32] = hex::decode(&secret)
            .map_err(|_| rejected())?
            .try_into()
            .map_err(|_| rejected())?;
        checked(hex::encode(decoded) == secret)?;
        Ok(Some(Self {
            origin: values[0].clone(),
            namespace: values[2].clone(),
            epoch,
            environment: values[4].clone(),
            audience: values[5].clone(),
            key_id: values[6].clone(),
            public_key: values[7].clone(),
            adapter_secret: secret,
        }))
    }
    fn identity(&self, namespace: &str, epoch: DecimalU64) -> Result<()> {
        checked(namespace == self.namespace && epoch == self.epoch)
    }

    /// Only the caller's current host-only management cookie is forwarded. No managed login.
    pub(crate) async fn management_session(
        &self,
        browser: &Request,
    ) -> Result<CustodySessionStatusV1> {
        let url = browser.url()?;
        checked(
            matches!(browser.method(), Method::Get | Method::Post)
                && url.origin().ascii_serialization() == self.origin
                && url.query().is_none(),
        )?;
        let headers = browser.headers();
        checked(
            headers.get("Authorization")?.is_none()
                && headers.get("Origin")?.is_none_or(|v| v == self.origin)
                && headers
                    .get("Sec-Fetch-Site")?
                    .is_none_or(|v| v == "same-origin"),
        )?;
        let cookies = headers.get("Cookie")?.ok_or_else(rejected)?;
        let mut selected = None;
        for item in cookies.split(';') {
            if let Some((name, value)) = item.trim().split_once('=')
                && name == "__Host-oasis7_sid"
            {
                checked(
                    selected.is_none()
                        && value.len() == 43
                        && value
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
                )?;
                selected = Some(value);
            }
        }
        let token = selected.ok_or_else(rejected)?;
        let outbound = Headers::new();
        outbound.set("Cookie", &format!("__Host-oasis7_sid={token}"))?;
        if let Some(origin) = headers.get("Origin")? {
            outbound.set("Origin", &origin)?;
        }
        if let Some(site) = headers.get("Sec-Fetch-Site")? {
            outbound.set("Sec-Fetch-Site", &site)?;
        }
        if browser.method() == Method::Post {
            checked(headers.get("Origin")?.as_deref() == Some(self.origin.as_str()))?;
            let csrf = headers.get("X-Oasis7-CSRF")?.ok_or_else(rejected)?;
            checked(
                csrf.len() == 64
                    && csrf
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            )?;
            outbound.set("X-Oasis7-CSRF", &csrf)?;
            outbound.set("Content-Type", "application/json")?;
        }
        let status: CustodySessionStatusV1 = self
            .get_management_status(outbound, browser.method())
            .await?;
        let now = clock()?;
        self.identity(&status.identity_namespace, status.identity_instance_epoch)?;
        checked(
            status.api_version == 1
                && identifier(&status.account_id, 256)
                && identifier(&status.session_ref, 256)
                && identifier(&status.credential_ref, 256)
                && status.credential_version.0 > 0
                && status.expires_at.0 > now
                && status.idle_expires_at.0 > now
                && status.server_time.0.abs_diff(now) <= MAX_SKEW_MS,
        )?;
        Ok(status)
    }
    async fn get_management_status(
        &self,
        headers: Headers,
        method: Method,
    ) -> Result<CustodySessionStatusV1> {
        let mut init = RequestInit::new();
        init.with_method(method.clone())
            .with_headers(headers)
            .with_redirect(RequestRedirect::Manual);
        if method == Method::Post {
            init.with_body(Some("{}".into()));
        }
        let request = Request::new_with_init(
            &format!("{}{PREFIX}/management-session-status", self.origin),
            &init,
        )?;
        read_native(request).await
    }

    /// Private typed Owner consumer only; the native service credential never leaves Identity.
    pub(crate) async fn verify_approval(
        &self,
        request: &ApprovalVerifyRequestV1,
    ) -> Result<SignedApprovalVerificationV1> {
        let proof = &request.approval.proof;
        self.identity(&proof.identity_namespace, proof.identity_instance_epoch)?;
        checked(
            proof.version == 1
                && proof.key_id == self.key_id
                && proof.context.environment == self.environment
                && proof.context.audience == self.audience
                && request.expected_audience == self.audience
                && request.expected_intent_digest == proof.intent_digest
                && request.proof_digest == request.approval.digest().map_err(|_| rejected())?
                && identifier(&request.verify_nonce, 256),
        )?;
        crate::approval_crypto::verify_ed25519(
            &self.public_key,
            &request.approval.signature,
            &proof.signing_bytes().map_err(|_| rejected())?,
        )
        .await
        .map_err(|_| rejected())?;
        let body = serde_json::to_string(request).map_err(|_| rejected())?;
        checked(body.len() <= MAX_BYTES)?;
        let headers = Headers::new();
        headers.set("Content-Type", "application/json")?;
        headers.set("Authorization", &format!("Bearer {}", self.adapter_secret))?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_redirect(RequestRedirect::Manual)
            .with_body(Some(body.into()));
        let outbound =
            Request::new_with_init(&format!("{}{PREFIX}/approvals/verify", self.origin), &init)?;
        let signed: SignedApprovalVerificationV1 = read_native(outbound).await?;
        let value = &signed.verification;
        self.identity(&value.identity_namespace, value.identity_instance_epoch)?;
        checked(
            value.version == 1
                && value.key_id == self.key_id
                && value.purpose == request.purpose
                && value.proof_digest == request.proof_digest
                && value.approval_request_id == proof.approval_request_id
                && value.audience == self.audience
                && value.intent_digest == request.expected_intent_digest
                && value.verify_nonce == request.verify_nonce
                && value.proof_expires_at == proof.expires_at,
        )?;
        let now = clock()?;
        checked(value.observed_at.0 <= now && now - value.observed_at.0 <= MAX_SKEW_MS)?;
        if request.purpose == VerificationPurposeV1::ConsumeApproval {
            checked(
                value.manager_security_version == proof.manager_security_version
                    && value.manager_policy_version == proof.manager_policy_version
                    && value.origin_credential_version == proof.origin_credential_version
                    && value.credential_version == proof.passkey_credential_version
                    && value.proof_state == ProofStateV1::Active
                    && value.origin_state == OriginStateV1::Active
                    && proof.issued_at.0 <= now
                    && proof.expires_at.0 > now
                    && proof.expires_at.0.saturating_sub(proof.issued_at.0) <= 30_000
                    && value.source_session_expires_at.0 > now
                    && value.source_session_idle_expires_at.0 > now,
            )?;
        }
        crate::approval_crypto::verify_ed25519(
            &self.public_key,
            &signed.signature,
            &value.signing_bytes().map_err(|_| rejected())?,
        )
        .await
        .map_err(|_| rejected())?;
        Ok(signed)
    }
}

/// Manual redirect and bounded streaming body apply to both fixed routes.
async fn read_native<T: DeserializeOwned>(request: Request) -> Result<T> {
    read_native_body(send_native(request).await?).await
}
async fn send_native(request: Request) -> Result<worker::Response> {
    send_native_status(request, false).await
}
async fn send_native_status(request: Request, allow_not_found: bool) -> Result<worker::Response> {
    let constructor =
        js_sys::Reflect::get(&js_sys::global(), &"AbortSignal".into()).map_err(|_| rejected())?;
    let timeout: js_sys::Function = js_sys::Reflect::get(&constructor, &"timeout".into())
        .map_err(|_| rejected())?
        .dyn_into()
        .map_err(|_| rejected())?;
    let signal: web_sys::AbortSignal = timeout
        .call1(&constructor, &5_000.into())
        .map_err(|_| rejected())?
        .dyn_into()
        .map_err(|_| rejected())?;
    let response = Fetch::Request(request)
        .send_with_signal(&AbortSignal::from(signal))
        .await
        .map_err(|_| rejected())?;
    checked(
        (response.status_code() == 200 || (allow_not_found && response.status_code() == 404))
            && response.headers().get("Content-Type")?.is_some_and(|v| {
                v.split(';')
                    .next()
                    .is_some_and(|v| v.eq_ignore_ascii_case("application/json"))
            }),
    )?;
    Ok(response)
}
async fn read_native_body<T: DeserializeOwned>(response: worker::Response) -> Result<T> {
    let response: web_sys::Response = response.into();
    let body = response.body().ok_or_else(rejected)?;
    let reader_fn: js_sys::Function = js_sys::Reflect::get(body.as_ref(), &"getReader".into())
        .map_err(|_| rejected())?
        .dyn_into()
        .map_err(|_| rejected())?;
    let reader = reader_fn.call0(body.as_ref()).map_err(|_| rejected())?;
    let read: js_sys::Function = js_sys::Reflect::get(&reader, &"read".into())
        .map_err(|_| rejected())?
        .dyn_into()
        .map_err(|_| rejected())?;
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    loop {
        let promise: js_sys::Promise = read
            .call0(&reader)
            .map_err(|_| rejected())?
            .dyn_into()
            .map_err(|_| rejected())?;
        let part = JsFuture::from(promise).await.map_err(|_| rejected())?;
        if js_sys::Reflect::get(&part, &"done".into())
            .map_err(|_| rejected())?
            .as_bool()
            == Some(true)
        {
            break;
        }
        let value: js_sys::Uint8Array = js_sys::Reflect::get(&part, &"value".into())
            .map_err(|_| rejected())?
            .dyn_into()
            .map_err(|_| rejected())?;
        if bytes.len().saturating_add(value.length() as usize) > MAX_BYTES {
            if let Ok(cancel) = js_sys::Reflect::get(&reader, &"cancel".into())
                && let Ok(cancel) = cancel.dyn_into::<js_sys::Function>()
                && let Ok(promise) = cancel.call0(&reader)
                && let Ok(promise) = promise.dyn_into::<js_sys::Promise>()
            {
                let _ = JsFuture::from(promise).await;
            }
            return Err(rejected());
        }
        let part_bytes = zeroize::Zeroizing::new(value.to_vec());
        bytes.extend_from_slice(part_bytes.as_slice());
        value.fill(0, 0, value.length());
    }
    serde_json::from_slice(&bytes).map_err(|_| rejected())
}
