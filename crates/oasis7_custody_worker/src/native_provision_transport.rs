//! Fixed private native routes. No browser request, arbitrary headers, bearer token,
//! or caller-selected URL is accepted. Exact requests must be durably saved first.
use super::{NativeIdentityConfig, checked, rejected};
use serde::de::DeserializeOwned;
use worker::{Env, Headers, Method, Request, RequestInit, RequestRedirect, Result};
use zeroize::{Zeroize, Zeroizing};

const BASE: &str = "/api/public/hosted-account/v2";
const MAX_REQUEST: usize = 12 * 1024;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativePrivateRoute {
    Capabilities,
    RegistrationOptions,
    RegistrationComplete,
    LoginOptions,
    LoginComplete,
    BindingLookup,
    BindingOptions,
    BindingComplete,
    SessionStatus,
    ParticipantPolicyOriginal,
    ParticipantAuthorize,
    ParticipantSubmit,
    ParticipantLookup,
    ParticipantOwnerView,
}
impl NativePrivateRoute {
    fn path(self) -> &'static str {
        match self {
            Self::Capabilities => "/capabilities",
            Self::RegistrationOptions => "/machine-registration/options",
            Self::RegistrationComplete => "/machine-registration/complete",
            Self::LoginOptions => "/native-key/options",
            Self::LoginComplete => "/native-key/complete",
            Self::BindingLookup => "/custody/managed-bindings/lookup",
            Self::BindingOptions => "/custody/managed-bindings/options",
            Self::BindingComplete => "/custody/managed-bindings/complete",
            Self::SessionStatus => "/custody/managed-bindings/session-status",
            Self::ParticipantPolicyOriginal => "/participant/policy-original",
            Self::ParticipantAuthorize => "/participant/authorize",
            Self::ParticipantSubmit => "/participant/submit",
            Self::ParticipantLookup => "/participant/lookup",
            Self::ParticipantOwnerView => "/participant/owner-view",
        }
    }
    fn participant(self) -> bool {
        matches!(
            self,
            Self::ParticipantPolicyOriginal
                | Self::ParticipantAuthorize
                | Self::ParticipantSubmit
                | Self::ParticipantLookup
                | Self::ParticipantOwnerView
        )
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrivateCookieKind {
    Preauth,
    ManagedSession,
}
impl PrivateCookieKind {
    fn name(self) -> &'static str {
        match self {
            Self::Preauth => "__Host-oasis7_pre",
            Self::ManagedSession => "__Host-oasis7_sid",
        }
    }
}
/// Only decrypted private records or the direct original native response construct this.
/// It has no Debug/serialization and never crosses a public role RPC.
pub(crate) struct PrivateCookie {
    kind: PrivateCookieKind,
    token: Zeroizing<String>,
}
impl PrivateCookie {
    pub(crate) fn from_private(kind: PrivateCookieKind, token: String) -> Result<Self> {
        let token = Zeroizing::new(token);
        checked(
            token.len() == 43
                && token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        )?;
        Ok(Self { kind, token })
    }
    pub(crate) fn token(&self) -> &str {
        &self.token
    }
}
pub(crate) struct NativePrivateReply<T> {
    pub http_status: u16,
    pub body: T,
    pub cookie: Option<PrivateCookie>,
}
pub(crate) struct NativeProvisionTransport {
    config: NativeIdentityConfig,
}
impl NativeProvisionTransport {
    pub(crate) fn from_env(env: &Env) -> Result<Self> {
        checked(
            env.var("CUSTODY_NATIVE_PROVISION_ENABLED")
                .ok()
                .map(|v| v.to_string())
                .as_deref()
                == Some("true"),
        )?;
        // Dedicated private KEK must exist and be valid before any native I/O.
        let _private =
            crate::native_private_store::NativePrivateConfig::load(env).map_err(|_| rejected())?;
        let config = NativeIdentityConfig::from_env(env)?.ok_or_else(rejected)?;
        Ok(Self { config })
    }
    pub(crate) fn native_origin(&self) -> &str {
        &self.config.origin
    }
    pub(crate) fn fact_pin(&self) -> crate::native_provision_facts::NativeFactPin {
        crate::native_provision_facts::NativeFactPin {
            key_id: self.config.key_id.clone(),
            public_key: self.config.public_key.clone(),
        }
    }
    pub(crate) fn validate_binding(
        &self,
        receipt: &oasis7_custody_core::native_binding::NativeBindingReceiptV1,
    ) -> Result<()> {
        receipt.validate().map_err(|_| rejected())?;
        self.config
            .identity(&receipt.identity_namespace, receipt.identity_instance_epoch)
    }
    pub(crate) fn validate_registration(
        &self,
        request: &oasis7_custody_core::native_binding::NativeRegistrationRequestV1,
    ) -> Result<()> {
        request.validate().map_err(|_| rejected())?;
        self.config
            .identity(&request.identity_namespace, request.identity_instance_epoch)?;
        checked(request.audience == self.config.origin)
    }
    pub(crate) async fn request<T: DeserializeOwned>(
        &self,
        route: NativePrivateRoute,
        original_body: &str,
        cookie: Option<&PrivateCookie>,
        original_csrf: Option<&str>,
    ) -> Result<NativePrivateReply<T>> {
        checked(original_body.len() <= MAX_REQUEST)?;
        let get = route == NativePrivateRoute::Capabilities;
        checked(if get {
            original_body.is_empty() && original_csrf.is_none()
        } else {
            !original_body.is_empty()
        })?;
        if route == NativePrivateRoute::SessionStatus || route.participant() {
            checked(cookie.is_some_and(|v| v.kind == PrivateCookieKind::ManagedSession))?;
        } else if !get {
            checked(cookie.is_some_and(|v| v.kind == PrivateCookieKind::Preauth))?;
        }
        let headers = Headers::new();
        if let Some(cookie) = cookie {
            let header = Zeroizing::new(format!("{}={}", cookie.kind.name(), cookie.token()));
            headers.set("Cookie", &header)?;
        }
        if !get {
            let csrf = original_csrf.ok_or_else(rejected)?;
            checked(
                csrf.len() == 64
                    && csrf
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            )?;
            headers.set("Origin", &self.config.origin)?;
            headers.set("Sec-Fetch-Site", "same-origin")?;
            headers.set("X-Oasis7-CSRF", csrf)?;
            headers.set("Content-Type", "application/json")?;
        }
        let mut init = RequestInit::new();
        init.with_method(if get { Method::Get } else { Method::Post })
            .with_headers(headers)
            .with_redirect(RequestRedirect::Manual);
        if !get {
            init.with_body(Some(original_body.into()));
        }
        let request = Request::new_with_init(
            &format!("{}{BASE}{}", self.config.origin, route.path()),
            &init,
        )?;
        let response =
            super::send_native_status(request, route == NativePrivateRoute::BindingLookup).await?;
        let http_status = response.status_code();
        let wanted = if route == NativePrivateRoute::LoginComplete {
            PrivateCookieKind::ManagedSession
        } else {
            PrivateCookieKind::Preauth
        };
        let mut selected = None;
        let mut values = Zeroizing::new(response.headers().get_all("Set-Cookie")?);
        checked(values.len() <= 4 && values.iter().all(|v| v.len() <= 1024))?;
        // Participant forwarding cannot rotate, replace or recover a Cookie.
        if route.participant() {
            checked(values.is_empty())?;
        }
        for raw in values.iter_mut() {
            if let Some((name, token)) = raw.split(';').next().and_then(|v| v.split_once('=')) {
                if name == wanted.name() {
                    checked(selected.is_none())?;
                    // __Host cookies must retain the native origin's secure host-only scope.
                    let attrs: Vec<_> = raw.split(';').skip(1).map(str::trim).collect();
                    checked(
                        attrs.iter().any(|v| v.eq_ignore_ascii_case("Secure"))
                            && attrs.iter().any(|v| v.eq_ignore_ascii_case("HttpOnly"))
                            && attrs.contains(&"Path=/")
                            && !attrs.iter().any(|v| {
                                v.split_once('=').is_some_and(|(name, _)| {
                                    name.trim().eq_ignore_ascii_case("domain")
                                })
                            }),
                    )?;
                    selected = Some(PrivateCookie::from_private(wanted, token.to_owned())?);
                }
            }
            raw.zeroize();
        }
        let body = super::read_native_body(response).await?;
        Ok(NativePrivateReply {
            http_status,
            body,
            cookie: selected,
        })
    }
}
