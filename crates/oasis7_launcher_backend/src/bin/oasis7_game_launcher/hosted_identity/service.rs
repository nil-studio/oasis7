use super::{
    config::IdentityConfig,
    digest,
    model::*,
    now_ms, random_token, reject,
    store::{IdentityStore, IdentityTransaction, StoreError},
    webauthn::WebAuthnAdapter,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};

pub(crate) struct IdentityReply {
    pub status: u16,
    pub body: Value,
    pub cookies: Vec<String>,
}
pub(crate) struct IdentityService {
    pub(super) custody: Option<super::custody::CustodyConfig>,
    pub config: IdentityConfig,
    pub store: IdentityStore,
    pub webauthn: WebAuthnAdapter,
    pub(super) email: super::email::EmailAdapter,
    pub(super) clock: super::ClockGuard,
    pub(super) participant_entry: Option<super::participant_entry::ParticipantEntry>,
}
pub(super) const PREFIX: &str = "/api/public/hosted-account/v2";
const SID: &str = "__Host-oasis7_sid";
const PREAUTH: &str = "__Host-oasis7_pre";
const DAY: u64 = 86_400_000;

// Keep native challenge status queryable beyond proof expiry. This only controls
// storage cleanup; check_ceremony still enforces the original 120-second expiry.
pub(super) fn ceremony_storage_expiry(ceremony: &CeremonyRecord) -> u64 {
    if ceremony.kind.starts_with("native_") {
        ceremony.expires_at.saturating_add(600_000)
    } else {
        ceremony.expires_at
    }
}

fn ceremony_storage_deadline(ceremony: &CeremonyRecord) -> Option<u64> {
    if ceremony.kind == "machine_register" {
        None // Permanent original operation binding, including failed/cancelled attempts.
    } else {
        Some(ceremony_storage_expiry(ceremony))
    }
}

impl IdentityReply {
    pub fn ok(body: Value) -> Self {
        Self {
            status: 200,
            body,
            cookies: vec![],
        }
    }
    pub fn error(error: StoreError) -> Self {
        let (status, code) = match error {
            StoreError::Rejected(code) => (
                match code.as_str() {
                    "invalid_request" => 400,
                    "registration_closed"
                    | "csrf_failed"
                    | "origin_rejected"
                    | "passkey_management_session_required"
                    | "reauth_required" => 403,
                    "custody_disabled" => 404,
                    "rate_limited" => 429,
                    "ceremony_expired" | "approval_expired" | "intent_expired" => 410,
                    "authentication_failed" | "session_expired" | "service_auth_failed" => 401,
                    _ => 409,
                },
                code,
            ),
            StoreError::Conflict => (409, "state_changed".into()),
            StoreError::OutcomeUnknown => (202, "operation_pending".into()),
            _ => (503, "identity_store_unavailable".into()),
        };
        Self {
            status,
            body: json!({"api_version":2,"ok":false,"error_code":code,"diagnostic_id":random_token(),"next_action":"refresh_state"}),
            cookies: vec![],
        }
    }
}
pub(super) fn cookie_value<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    let mut result = None;
    for line in headers.lines().skip(1) {
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':')
            && key.eq_ignore_ascii_case("cookie")
        {
            for item in value.split(';') {
                if let Some((k, v)) = item.trim().split_once('=')
                    && k == name
                {
                    if result.is_some() {
                        return None;
                    }
                    result = Some(v);
                }
            }
        }
    }
    result.filter(|v| {
        v.len() == 43
            && v.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
}
pub(crate) fn header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    let mut result = None;
    for line in headers.lines().skip(1) {
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && k.eq_ignore_ascii_case(name)
        {
            if result.is_some() {
                return None;
            }
            result = Some(v.trim());
        }
    }
    result
}
fn set_cookie(name: &str, token: &str, max_age: u64) -> String {
    format!("{name}={token}; Secure; HttpOnly; Path=/; SameSite=Lax; Max-Age={max_age}")
}
fn account_key(id: &str) -> String {
    format!("account:{id}")
}
pub(super) fn session_key(secret: &[u8; 32], token: &str) -> String {
    format!("session:{}", digest(secret, "session", token))
}
pub(super) fn credential_key(id: &str) -> String {
    format!("credential:{id}")
}

impl IdentityService {
    pub fn new(mut config: IdentityConfig) -> Result<Self, String> {
        let webauthn = WebAuthnAdapter::new(&config.rp.rp_id, &config.rp.allowed_origins)
            .map_err(|_| "invalid WebAuthn configuration".to_string())?;
        let mut store = if let Some(table) = config.tablestore.take() {
            IdentityStore::open_tablestore(table, config.namespace.clone(), config.rp.clone())
        } else {
            IdentityStore::open(
                &config.sqlite_path,
                config.namespace.clone(),
                config.rp.clone(),
            )
        }
        .map_err(|e| e.to_string())?;
        let email = super::email::EmailAdapter::from_env(&config.session_key)?;
        email
            .verify_key_binding(&mut store)
            .map_err(|e| e.to_string())?;
        if let Some(path) = std::env::var_os("OASIS7_HOSTED_IDENTITY_LEGACY_SNAPSHOT") {
            let fenced = std::env::var("OASIS7_HOSTED_IDENTITY_LEGACY_WRITERS_FENCED")
                .ok()
                .as_deref()
                == Some("1");
            store
                .migrate_legacy_json(
                    std::path::Path::new(&path),
                    &config.world_scope,
                    now_ms(),
                    fenced,
                    |address| email.material(address, now_ms()),
                )
                .map_err(|e| e.to_string())?;
        } else if std::env::var_os("OASIS7_HOSTED_ACCOUNT_STORE_PATH")
            .is_some_and(|p| std::path::Path::new(&p).exists())
        {
            return Err(
                "existing account store requires an explicit fenced migration snapshot".into(),
            );
        }
        let custody = super::custody::CustodyConfig::from_env(&config.session_key)?;
        if let Some(custody) = &custody {
            custody
                .attach_revocation(
                    &mut store,
                    config.namespace.key(),
                    config.namespace.environment_id.clone(),
                    config.rp.identity_instance_epoch,
                )
                .map_err(|e| e.to_string())?;
        }
        store
            .require_revocation_configuration_if_registered()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            custody,
            participant_entry: super::participant_entry::ParticipantEntry::from_env()?,
            config,
            store,
            webauthn,
            email,
            clock: super::ClockGuard::new(),
        })
    }
    pub fn handle(
        &mut self,
        method: &str,
        path: &str,
        headers: &str,
        mut payload: Value,
        source: &str,
        issuer: &mut super::super::hosted_player_session::HostedPlayerSessionIssuer,
    ) -> IdentityReply {
        let now = now_ms();
        if !self.clock.observe(now) {
            self.email.clock_healthy = false;
            return IdentityReply::error(reject("clock_unhealthy"));
        }
        let pre = cookie_value(headers, PREAUTH)
            .map(str::to_owned)
            .unwrap_or_else(random_token);
        let sid = cookie_value(headers, SID).map(str::to_owned);
        let owner = digest(&self.config.session_key, "preauth", &pre);
        let csrf = digest(
            &self.config.session_key,
            "csrf",
            sid.as_deref().unwrap_or(&pre),
        );
        let relative = path.strip_prefix(PREFIX).unwrap_or("");
        // Dedicated service-auth route, never a browser Cookie/CSRF exemption for
        // other endpoints. It exposes only original approval source verification.
        if relative.starts_with("/custody/revocations/") {
            return match self.custody_revocation_route(method, relative, headers, &payload, now) {
                Ok(value) => IdentityReply::ok(value),
                Err(error) => IdentityReply::error(error),
            };
        }
        if relative == "/custody/approvals/verify" {
            return match self.custody_verify_route(method, headers, &payload, source, now) {
                Ok(value) => IdentityReply::ok(value),
                Err(error) => IdentityReply::error(error),
            };
        }
        if let Some(context) = cookie_value(headers, "__Host-oasis7_recovery") {
            if !payload.is_object() {
                payload = json!({});
            }
            payload["context_id"] = json!(context);
        } else if payload.get("context_id").is_some() {
            return IdentityReply::error(reject("authentication_failed"));
        }
        let writing = method != "GET";
        if writing && let Err(error) = self.validate_json_write(headers, &csrf) {
            return IdentityReply::error(error);
        }
        if relative.starts_with("/custody/managed-bindings/") {
            let origin = header(headers, "origin").unwrap_or("");
            let result = match (method, relative) {
                ("POST", "/custody/managed-bindings/session-status") => {
                    self.managed_session_status(sid.as_deref(), &payload, now)
                }
                ("POST", "/custody/managed-bindings/invalidation") => {
                    self.managed_invalidation(&owner, &payload, origin)
                }
                _ => self.managed_binding_route(method, relative, &owner, &payload, origin, now),
            };
            return match result {
                Ok(value) => IdentityReply::ok(value),
                Err(StoreError::Rejected(code))
                    if relative == "/custody/managed-bindings/lookup" && code == "not_found" =>
                {
                    let mut reply = IdentityReply::error(StoreError::Rejected(code));
                    reply.status = 404;
                    reply
                }
                Err(error) => IdentityReply::error(error),
            };
        }
        if relative.starts_with("/custody/") {
            return match self.custody_browser_route(
                method,
                relative,
                sid.as_deref(),
                &payload,
                source,
                now,
            ) {
                Ok(value) => IdentityReply::ok(value),
                Err(error) => IdentityReply::error(error),
            };
        }
        if relative == "/entry/discovery" {
            return match if method == "GET" {
                self.entry_discovery(source, now)
            } else {
                Err(reject("method_not_allowed"))
            } {
                Ok(v) => IdentityReply::ok(v),
                Err(e) => IdentityReply::error(e),
            };
        }
        if let Some(result) =
            self.entry_executor_route(method, relative, sid.as_deref(), &payload, now)
        {
            return match result {
                Ok(v) => IdentityReply::ok(v),
                Err(e) => IdentityReply::error(e),
            };
        }
        if let Some(result) =
            self.entry_owner_route(method, relative, sid.as_deref(), &payload, now)
        {
            return match result {
                Ok(v) => IdentityReply::ok(v),
                Err(e) => IdentityReply::error(e),
            };
        }
        if let Some(result) =
            self.participant_route(method, relative, sid.as_deref(), &payload, now)
        {
            return match result {
                Ok(v) => IdentityReply::ok(v),
                Err(e) => IdentityReply::error(e),
            };
        }
        if let Some(result) =
            self.world_route(method, relative, sid.as_deref(), &payload, issuer, now)
        {
            return match result {
                Ok(v) => IdentityReply::ok(v),
                Err(e) => {
                    let mut r = IdentityReply::error(e);
                    if r.body["error_code"] == "control_elsewhere"
                        && let Some(epoch) = self.world_control_epoch(sid.as_deref())
                    {
                        r.body["world_session_epoch"] = json!(epoch);
                    }
                    r
                }
            };
        }
        if let Some(result) = self.email_route(
            method,
            relative,
            sid.as_deref(),
            &owner,
            &payload,
            source,
            now,
        ) {
            return token_reply(result);
        }
        if let Some(result) = self.recovery_route(
            method,
            relative,
            sid.as_deref(),
            &owner,
            &payload,
            source,
            now,
        ) {
            let mut r = token_reply(result);
            if let Some(context) = r
                .body
                .get("context_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
            {
                r.cookies
                    .push(set_cookie("__Host-oasis7_recovery", &context, 7 * 86400));
                r.body.as_object_mut().unwrap().remove("context_id");
            }
            return r;
        }
        if let Some(result) = self.machine_registration_route(
            method,
            relative,
            &owner,
            &payload,
            source,
            header(headers, "origin").unwrap_or(""),
            now,
        ) {
            return match result {
                Ok(value) => IdentityReply::ok(value),
                Err(error) => IdentityReply::error(error),
            };
        }
        if let Some(result) = self.native_key_route(
            method,
            relative,
            sid.as_deref(),
            &owner,
            &payload,
            source,
            header(headers, "origin").unwrap_or(""),
            now,
        ) {
            return token_reply(result);
        }
        let result:Result<Value,StoreError>=match (method,relative) {
            ("GET","/capabilities")=>Ok(json!({"api_version":2,"ok":true,"csrf_token":csrf,"registration_enabled":self.config.registration_enabled,"passkey":{"available":true},"email":{"available":self.email.available()},"email_recovery":{"available":self.email.recovery_available()},"machine_registration":{"available":self.config.registration_enabled && self.config.machine_registration_enabled,"algorithm":"Ed25519","options_path":format!("{PREFIX}/machine-registration/options"),"complete_path":format!("{PREFIX}/machine-registration/complete")},"native_key":{"available":true,"mode":"import_only","algorithm":"Ed25519","identity_service_id":self.config.namespace.identity_service_id,"environment_id":self.config.namespace.environment_id}})),
            ("GET","/session")=>self.session(sid.as_deref(),now).map(|(a,s)|json!({"api_version":2,"ok":true,"csrf_token":csrf,"account":{"account_id":a.account_id,"display_name":a.display_name},"authentication":{"state":"authenticated","method":s.auth_method},"world":{"state":"not_requested"}})),
            ("POST","/passkeys/registration/options")=>self.registration_options(&owner,sid.as_deref(),&payload,source,now),
            ("POST","/passkeys/authentication/options")=>self.authentication_options(&owner,&payload,source,now),
            ("POST","/passkeys/registration/complete")|( "POST","/passkeys/authentication/complete")=>{
                let registering=relative.contains("registration");
                match self.complete(&owner,sid.as_deref(),&payload,registering,source,now) {
                    Ok((value,token))=>{let mut reply=IdentityReply::ok(value);if let Some(token)=token{reply.cookies.push(set_cookie(SID,&token,30*86400));}return reply;},
                    Err(e)=>Err(e),
                }
            },
            ("POST","/logout")=>self.logout(sid.as_deref(),now),
            ("GET","/credentials")|("GET","/sessions")=>self.list_account_records(sid.as_deref(),relative,&payload,now),
            ("POST","/actions")=>self.create_action(sid.as_deref(),&payload,now),
            ("POST",r) if r.starts_with("/ceremonies/")&&r.ends_with("/cancel")=>self.cancel_ceremony(&owner,r.trim_start_matches("/ceremonies/").trim_end_matches("/cancel"),sid.as_deref(),now),
            ("GET",r) if r.starts_with("/operations/")=>self.operation_status(&owner,r.trim_start_matches("/operations/"),sid.as_deref(),now),
            _=>self.handle_credentials(method,relative,sid.as_deref(),&payload,now,&owner),
        };
        let mut reply = match result {
            Ok(body) => IdentityReply::ok(body),
            Err(e) => IdentityReply::error(e),
        };
        if !writing && cookie_value(headers, PREAUTH).is_none() {
            reply.cookies.push(set_cookie(PREAUTH, &pre, 600));
        }
        if relative == "/logout" && reply.status == 200 {
            reply.cookies.push(set_cookie(SID, "", 0));
        }
        reply
    }
    pub(super) fn validate_json_write(&self, headers: &str, csrf: &str) -> Result<(), StoreError> {
        if header(headers, "origin")
            .is_none_or(|v| !self.config.rp.allowed_origins.iter().any(|o| o == v))
            || header(headers, "sec-fetch-site").is_some_and(|v| v != "same-origin")
        {
            return Err(reject("origin_rejected"));
        }
        if header(headers, "x-oasis7-csrf") != Some(csrf)
            || header(headers, "content-type")
                .is_none_or(|v| v.split(';').next() != Some("application/json"))
        {
            return Err(reject("csrf_failed"));
        }
        Ok(())
    }
    pub fn session(
        &mut self,
        token: Option<&str>,
        now: u64,
    ) -> Result<(AccountRecord, AccountSession), StoreError> {
        let token = token.ok_or_else(|| reject("session_expired"))?;
        let key = session_key(&self.config.session_key, token);
        let epoch = self.config.rp.identity_instance_epoch;
        self.store
            .transaction(|tx| valid_session(tx, &key, epoch, now))
    }
    fn registration_options(
        &mut self,
        owner: &str,
        sid: Option<&str>,
        p: &Value,
        source: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        let intent = p["intent"].as_str().unwrap_or("new_account");
        let (account_id, handle, name, action, existing) = if intent == "new_account" {
            if !self.config.registration_enabled {
                return Err(reject("registration_closed"));
            }
            if sid.is_some() {
                return Err(reject("already_authenticated"));
            }
            let id = format!("account-{}", random_token());
            let handle = uuid::Uuid::new_v4();
            let name = format!("玩家 {}", &id[id.len() - 6..]);
            (id, handle, name, None, vec![])
        } else if intent == "bind" {
            let (a, s) = self.session(sid, now)?;
            let action_id = p["action_id"]
                .as_str()
                .ok_or_else(|| reject("reauth_required"))?
                .to_owned();
            let existing = self.store.transaction(|tx| {
                let action = authorized_action(tx, &action_id, &a, &s, "bind", None, now)?;
                let keys = active_keys(tx, &a.account_id)?;
                if keys.len() >= 10 {
                    return Err(reject("credential_limit"));
                }
                let _ = action;
                Ok(keys
                    .into_iter()
                    .filter_map(|c| {
                        if let CredentialMaterial::WebAuthn { library_record, .. } =
                            c.public_material
                        {
                            Some(library_record)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>())
            })?;
            (
                a.account_id,
                uuid::Uuid::from_slice(&a.user_handle).map_err(|_| StoreError::Corrupt)?,
                a.display_name,
                Some(action_id),
                existing,
            )
        } else {
            return Err(reject("unsupported_intent"));
        };
        let (options, state) = self
            .webauthn
            .registration_options(&handle.to_string(), &name, &existing)
            .map_err(|_| reject("authentication_failed"))?;
        let id = random_token();
        let mut ceremony = CeremonyRecord {
            ceremony_id: id.clone(),
            kind: format!("register_{intent}"),
            namespace: self.config.namespace.clone(),
            rp_config_version: self.config.rp.version,
            owner_context_digest: owner.into(),
            target_account_id: Some(account_id),
            target_digest: action,
            expires_at: now + 120_000,
            failed_attempts: 0,
            status: "pending".into(),
            server_state: Some(state),
            result_ref: None,
        };
        // Immutable registration identity lives alongside server verifier state, never client supplied.
        ceremony.server_state =
            Some(json!({"webauthn":ceremony.server_state,"handle":handle.to_string(),"name":name}));
        let secret = self.config.session_key;
        self.store.transaction(|tx| {
            consume_quota(
                tx,
                "register-source",
                &digest(&secret, "source", source),
                now,
                60_000,
                60,
            )?;
            consume_quota(tx, "register-total", "deployment", now, 60_000, 300)?;
            tx.create(
                &format!("ceremony:{id}"),
                "ceremony",
                ceremony.target_account_id.as_deref(),
                Some(ceremony.expires_at),
                &ceremony,
            )
        })?;
        Ok(
            json!({"api_version":2,"ok":true,"ceremony_id":id,"operation_id":id,"options":options,"expires_at":ceremony.expires_at}),
        )
    }
    fn authentication_options(
        &mut self,
        owner: &str,
        p: &Value,
        source: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        if p["intent"]
            .as_str()
            .is_some_and(|v| v != "login" && v != "switch_account")
        {
            return Err(reject("invalid_request"));
        }
        let (options, state) = self
            .webauthn
            .authentication_options(&[])
            .map_err(|_| reject("authentication_failed"))?;
        let id = random_token();
        let ceremony = CeremonyRecord {
            ceremony_id: id.clone(),
            kind: "authenticate".into(),
            namespace: self.config.namespace.clone(),
            rp_config_version: self.config.rp.version,
            owner_context_digest: owner.into(),
            target_account_id: None,
            target_digest: None,
            expires_at: now + 120_000,
            failed_attempts: 0,
            status: "pending".into(),
            server_state: Some(state),
            result_ref: None,
        };
        let secret = self.config.session_key;
        self.store.transaction(|tx| {
            consume_quota(
                tx,
                "login-source",
                &digest(&secret, "source", source),
                now,
                60_000,
                60,
            )?;
            consume_quota(tx, "login-total", "deployment", now, 60_000, 300)?;
            tx.create(
                &format!("ceremony:{id}"),
                "ceremony",
                None,
                Some(ceremony.expires_at),
                &ceremony,
            )
        })?;
        Ok(
            json!({"api_version":2,"ok":true,"ceremony_id":id,"operation_id":id,"options":options,"expires_at":ceremony.expires_at}),
        )
    }
    fn complete(
        &mut self,
        owner: &str,
        sid: Option<&str>,
        p: &Value,
        registering: bool,
        source: &str,
        now: u64,
    ) -> Result<(Value, Option<String>), StoreError> {
        let id = p["ceremony_id"]
            .as_str()
            .filter(|v| v.len() == 43)
            .ok_or_else(|| reject("invalid_request"))?;
        if p["operation_id"].as_str() != Some(id) {
            return Err(reject("invalid_request"));
        }
        let key = format!("ceremony:{id}");
        let response_digest = digest(
            &self.config.session_key,
            "response",
            &p["response"].to_string(),
        );
        if let Some(receipt) = self
            .store
            .transaction(|tx| tx.get::<OperationReceipt>(&format!("receipt:{id}")))?
        {
            if receipt.value.owner_context_digest != owner
                || receipt.value.request_digest != response_digest
            {
                return Err(reject("idempotency_conflict"));
            }
            return Ok((
                json!({"api_version":2,"ok":true,"phase":"operation_committed","account_retained":true,"next_action":"fresh_authentication"}),
                None,
            ));
        }
        let ceremony = self
            .store
            .transaction(|tx| {
                tx.get::<CeremonyRecord>(&key)?
                    .ok_or_else(|| reject("authentication_failed"))
            })?
            .value;
        check_ceremony(&ceremony, owner, now)?;
        if registering != ceremony.kind.starts_with("register_") {
            return Err(reject("authentication_failed"));
        }
        let response = &p["response"];
        let mut verified_credential_version = None;
        let verification = (|| -> Result<Value, StoreError> {
            if registering {
                self.webauthn
                    .finish_registration(
                        response,
                        &ceremony.server_state.as_ref().ok_or(StoreError::Corrupt)?["webauthn"],
                    )
                    .map_err(|_| reject("authentication_failed"))
            } else {
                let (handle, raw_id) = self
                    .webauthn
                    .identify_authentication(response)
                    .map_err(|_| reject("authentication_failed"))?;
                let lookup_key = format!("lookup:passkey:{raw_id}");
                let stored = self.store.transaction(|tx| {
                    let l = tx
                        .get::<CredentialLookup>(&lookup_key)?
                        .ok_or_else(|| reject("authentication_failed"))?
                        .value;
                    tx.get::<CredentialRecord>(&credential_key(&l.credential_record_id))?
                        .ok_or_else(|| reject("authentication_failed"))
                })?;
                verified_credential_version = Some(stored.row_version);
                let c = stored.value;
                match &c.public_material {
                    CredentialMaterial::WebAuthn {
                        library_record,
                        user_handle,
                        ..
                    } if uuid::Uuid::from_slice(user_handle)
                        .is_ok_and(|u| u.to_string() == handle) =>
                    {
                        self.webauthn
                            .finish_authentication(
                                response,
                                ceremony.server_state.as_ref().ok_or(StoreError::Corrupt)?,
                                library_record,
                            )
                            .map_err(|_| reject("authentication_failed"))
                    }
                    _ => Err(reject("authentication_failed")),
                }
            }
        })();
        let record = match verification {
            Ok(record) => record,
            Err(error) => {
                if matches!(&error,StoreError::Rejected(code) if code=="authentication_failed") {
                    self.record_ceremony_failure(&key, owner, now)?;
                }
                return Err(error);
            }
        };
        let raw_id = self
            .webauthn
            .credential_id(&record)
            .map_err(|_| reject("authentication_failed"))?;
        let raw_bytes = URL_SAFE_NO_PAD
            .decode(&raw_id)
            .map_err(|_| reject("authentication_failed"))?;
        let token = random_token();
        let secret = self.config.session_key;
        let bound_session = if ceremony.kind == "register_bind" {
            Some(self.session(sid, now)?)
        } else {
            None
        };
        let config = &self.config;
        self.store.transaction(|tx|{
            let mut c=tx.get::<CeremonyRecord>(&key)?.ok_or_else(||reject("authentication_failed"))?;check_ceremony(&c.value,owner,now)?;
            let lookup_key=format!("lookup:passkey:{raw_id}");
            let (mut account,mut credential,account_version,credential_version)=if registering {
                if tx.get::<CredentialLookup>(&lookup_key)?.is_some(){return Err(reject("credential_conflict"));}
                let state=c.value.server_state.as_ref().ok_or(StoreError::Corrupt)?;
                let handle=uuid::Uuid::parse_str(state["handle"].as_str().ok_or(StoreError::Corrupt)?).map_err(|_|StoreError::Corrupt)?.as_bytes().to_vec();
                let account_id=c.value.target_account_id.as_ref().ok_or(StoreError::Corrupt)?.clone();
                let (a,av)=if let Some((a,_s))=&bound_session {let current=valid_session(tx,&session_key(&secret,sid.ok_or_else(||reject("session_expired"))?),config.rp.identity_instance_epoch,now)?;if current.0.account_id!=a.account_id{return Err(reject("state_changed"));}let aid=c.value.target_digest.as_ref().ok_or(StoreError::Corrupt)?;let mut action=authorized_action(tx,aid,&current.0,&current.1,"bind",None,now)?;if active_keys(tx,&a.account_id)?.len()>=10{return Err(reject("credential_limit"));}action.value.state="consumed".into();tx.replace(&format!("action:{aid}"),action.row_version,"action",Some(&a.account_id),Some(action.value.expires_at),&action.value)?;let a=tx.get::<AccountRecord>(&account_key(&account_id))?.ok_or(StoreError::Corrupt)?;(a.value,Some(a.row_version))}else{
                    if !config.registration_enabled{return Err(reject("registration_closed"));}
                    consume_quota(tx,"accounts-source",&digest(&secret,"source",source),now,3_600_000,10)?;consume_quota(tx,"accounts-total","deployment",now,DAY,100)?;
                    (AccountRecord {account_id:account_id.clone(),user_handle:handle.clone(),display_name:state["name"].as_str().ok_or(StoreError::Corrupt)?.into(),status:"active".into(),policy_version:1,security_epoch:1,has_ever_public_key:true,created_at:now,last_authenticated_at:Some(now),last_recovery_id:None},None)
                };
                let cred_id=random_token();let credential=CredentialRecord {credential_record_id:cred_id.clone(),account_id:account_id.clone(),state:"active".into(),version:1,label:"通行密钥".into(),created_at:now,last_used_at:Some(now),public_material:CredentialMaterial::WebAuthn{rp_id:config.rp.rp_id.clone(),credential_id:raw_bytes.clone(),user_handle:handle,library_record:record.clone(),library_record_version:1},registered_via:if av.is_some(){"public_key"}else{"new_account"}.into()};
                tx.create(&lookup_key,"lookup",Some(&account_id),None,&CredentialLookup{scope:config.rp.rp_id.clone(),key_digest:blake3::hash(&raw_bytes).to_hex().to_string(),canonical_key:raw_bytes.clone(),credential_record_id:cred_id,account_id:account_id.clone(),state:"active".into()})?;
                (a,credential,av,None)
            }else{
                let l=tx.get::<CredentialLookup>(&lookup_key)?.ok_or_else(||reject("authentication_failed"))?.value;
                if l.state!="active"||l.canonical_key!=raw_bytes{return Err(reject("authentication_failed"));}
                let c=tx.get::<CredentialRecord>(&credential_key(&l.credential_record_id))?.ok_or(StoreError::Corrupt)?;
                if Some(c.row_version)!=verified_credential_version{return Err(reject("state_changed"));}
                let a=tx.get::<AccountRecord>(&account_key(&l.account_id))?.ok_or(StoreError::Corrupt)?;
                (a.value,c.value,Some(a.row_version),Some(c.row_version))
            };
            if account.status!="active"||credential.state!="active"{return Err(reject("authentication_failed"));}
            if let CredentialMaterial::WebAuthn{library_record,..}=&mut credential.public_material{*library_record=record.clone();}
            account.last_authenticated_at=Some(now);account.has_ever_public_key=true;if bound_session.is_some(){account.policy_version+=1;}
            credential.last_used_at=Some(now);
            if let Some(v)=account_version{tx.replace(&account_key(&account.account_id),v,"account",Some(&account.account_id),None,&account)?;}else{tx.create(&account_key(&account.account_id),"account",Some(&account.account_id),None,&account)?;}
            if let Some(v)=credential_version{tx.replace(&credential_key(&credential.credential_record_id),v,"credential",Some(&account.account_id),None,&credential)?;}else{tx.create(&credential_key(&credential.credential_record_id),"credential",Some(&account.account_id),None,&credential)?;}
            let session_id=if bound_session.is_some(){None}else{Some(create_session(tx,config,&account,&credential,&token,now)?)};
            c.value.status="consumed".into();c.value.server_state=None;c.value.result_ref=Some(account.account_id.clone());tx.replace(&key,c.row_version,"ceremony",Some(&account.account_id),Some(ceremony_storage_expiry(&c.value)),&c.value)?;
            tx.create(&format!("receipt:{id}"),"receipt",Some(&account.account_id),Some(now+7*DAY),&OperationReceipt{operation_id:id.into(),kind:ceremony.kind.clone(),owner_context_digest:owner.into(),request_digest:response_digest.clone(),state:"committed".into(),result_ids:vec![account.account_id.clone()],committed_at:now})?;
            audit(tx,&account,"credential_authenticated",now)?;
            Ok((json!({"api_version":2,"ok":true,"phase":if registering{"account_registered"}else{"authenticated"},"account":{"account_id":account.account_id,"display_name":account.display_name},"world":{"state":"not_requested"}}),session_id.map(|_|token.clone())))
        })
    }
    pub(super) fn record_ceremony_failure(
        &mut self,
        key: &str,
        owner: &str,
        now: u64,
    ) -> Result<(), StoreError> {
        self.store.transaction(|tx| {
            let mut c = tx
                .get::<CeremonyRecord>(key)?
                .ok_or_else(|| reject("authentication_failed"))?;
            check_ceremony(&c.value, owner, now)?;
            c.value.failed_attempts += 1;
            if c.value.failed_attempts >= 5 {
                c.value.status = "exhausted".into();
                c.value.server_state = None;
            }
            tx.replace(
                key,
                c.row_version,
                "ceremony",
                c.value.target_account_id.as_deref(),
                ceremony_storage_deadline(&c.value),
                &c.value,
            )
        })
    }
    pub(super) fn logout(&mut self, sid: Option<&str>, now: u64) -> Result<Value, StoreError> {
        if let Some(token) = sid {
            let key = session_key(&self.config.session_key, token);
            self.store.transaction(|tx| {
                if let Some(mut s) = tx.get::<AccountSession>(&key)? {
                    s.value.revoked_at = Some(now);
                    tx.replace(
                        &key,
                        s.row_version,
                        "session",
                        Some(&s.value.account_id),
                        Some(s.value.expires_at),
                        &s.value,
                    )?;
                }
                Ok(())
            })?;
        }
        Ok(json!({"api_version":2,"ok":true}))
    }
    fn operation_status(
        &mut self,
        owner: &str,
        id: &str,
        sid: Option<&str>,
        now: u64,
    ) -> Result<Value, StoreError> {
        let action_owner = self
            .session(sid, now)
            .ok()
            .map(|(_, s)| digest(&self.config.session_key, "action-owner", &s.session_id));
        self.store.transaction(|tx|{
            if let Some(receipt)=tx.get::<OperationReceipt>(&format!("receipt:{id}"))?{if receipt.value.owner_context_digest!=owner{return Err(reject("authentication_failed"));}return Ok(json!({"api_version":2,"ok":true,"phase":"consumed","account_retained":true,"next_action":"fresh_authentication"}));}
            let c=tx.get::<CeremonyRecord>(&format!("ceremony:{id}"))?.ok_or_else(||reject("authentication_failed"))?;
            if c.value.owner_context_digest!=owner&&action_owner.as_deref()!=Some(c.value.owner_context_digest.as_str()){return Err(reject("authentication_failed"));}
            let phase=if c.value.status=="pending"&&c.value.expires_at<=now{"expired"}else{&c.value.status};
            Ok(json!({"api_version":2,"ok":true,"phase":phase,"next_action":if phase=="consumed"{"fresh_authentication"}else{"retry_original_ceremony"}}))
        })
    }
    fn cancel_ceremony(
        &mut self,
        owner: &str,
        id: &str,
        sid: Option<&str>,
        now: u64,
    ) -> Result<Value, StoreError> {
        let action_owner = self
            .session(sid, now)
            .ok()
            .map(|(_, s)| digest(&self.config.session_key, "action-owner", &s.session_id));
        self.store.transaction(|tx| {
            let key = format!("ceremony:{id}");
            let mut c = tx
                .get::<CeremonyRecord>(&key)?
                .ok_or_else(|| reject("authentication_failed"))?;
            if c.value.owner_context_digest != owner
                && action_owner.as_deref() != Some(c.value.owner_context_digest.as_str())
            {
                return Err(reject("authentication_failed"));
            }
            if c.value.status == "pending" {
                c.value.status = "cancelled".into();
                c.value.server_state = None;
                tx.replace(
                    &key,
                    c.row_version,
                    "ceremony",
                    c.value.target_account_id.as_deref(),
                    ceremony_storage_deadline(&c.value),
                    &c.value,
                )?;
            }
            Ok(json!({"api_version":2,"ok":true,"phase":c.value.status}))
        })
    }
}
pub(super) fn valid_session(
    tx: &IdentityTransaction<'_>,
    key: &str,
    epoch: u64,
    now: u64,
) -> Result<(AccountRecord, AccountSession), StoreError> {
    let s = tx
        .get::<AccountSession>(key)?
        .ok_or_else(|| reject("session_expired"))?
        .value;
    let a = tx
        .get::<AccountRecord>(&account_key(&s.account_id))?
        .ok_or(StoreError::Corrupt)?
        .value;
    let c = tx
        .get::<CredentialRecord>(&credential_key(&s.origin_credential_id))?
        .ok_or(StoreError::Corrupt)?
        .value;
    if s.revoked_at.is_some()
        || s.expires_at <= now
        || s.idle_expires_at <= now
        || s.identity_instance_epoch != epoch
        || a.account_id != s.account_id
        || a.status != "active"
        || a.security_epoch != s.issued_security_epoch
        || c.account_id != a.account_id
        || c.state != "active"
        || c.version != s.origin_credential_version
    {
        return Err(reject("session_expired"));
    }
    Ok((a, s))
}
pub(super) fn active_keys(
    tx: &IdentityTransaction<'_>,
    account: &str,
) -> Result<Vec<CredentialRecord>, StoreError> {
    Ok(tx
        .all::<CredentialRecord>("credential", Some(account))?
        .into_iter()
        .map(|(_, v)| v.value)
        .filter(|v| v.state == "active" && v.public_material.is_public_key())
        .collect())
}
pub(super) fn check_ceremony(c: &CeremonyRecord, owner: &str, now: u64) -> Result<(), StoreError> {
    if c.owner_context_digest != owner || c.status != "pending" || c.failed_attempts >= 5 {
        return Err(reject("authentication_failed"));
    }
    if c.expires_at <= now {
        return Err(reject("ceremony_expired"));
    }
    Ok(())
}
pub(super) fn authorized_action(
    tx: &IdentityTransaction<'_>,
    id: &str,
    a: &AccountRecord,
    s: &AccountSession,
    kind: &str,
    target: Option<&str>,
    now: u64,
) -> Result<super::store::Versioned<SensitiveAction>, StoreError> {
    let action = tx
        .get::<SensitiveAction>(&format!("action:{id}"))?
        .ok_or_else(|| reject("reauth_required"))?;
    let v = &action.value;
    if v.account_id != a.account_id
        || v.session_id != s.session_id
        || v.action_type != kind
        || target.is_some_and(|t| v.target_digest != t)
        || v.account_policy_version != a.policy_version
        || v.expires_at <= now
        || v.state != "authorized"
    {
        return Err(reject("reauth_required"));
    }
    let proof = tx
        .get::<CredentialRecord>(&credential_key(
            v.proof_credential_id
                .as_deref()
                .ok_or_else(|| reject("reauth_required"))?,
        ))?
        .ok_or(StoreError::Corrupt)?
        .value;
    if proof.state != "active"
        || proof.account_id != a.account_id
        || !(proof.public_material.is_public_key()
            || (!a.has_ever_public_key
                && kind != "freeze"
                && matches!(proof.public_material, CredentialMaterial::Email { .. })))
    {
        return Err(reject("reauth_required"));
    }
    Ok(action)
}
pub(super) fn create_session(
    tx: &mut IdentityTransaction<'_>,
    config: &IdentityConfig,
    a: &AccountRecord,
    c: &CredentialRecord,
    token: &str,
    now: u64,
) -> Result<String, StoreError> {
    let id = random_token();
    let token_digest = digest(&config.session_key, "session", token);
    let s = AccountSession {
        session_id: id.clone(),
        token_digest,
        account_id: a.account_id.clone(),
        origin_credential_id: c.credential_record_id.clone(),
        origin_credential_version: c.version,
        issued_security_epoch: a.security_epoch,
        identity_instance_epoch: config.rp.identity_instance_epoch,
        auth_method: match c.public_material {
            CredentialMaterial::WebAuthn { .. } => "webauthn",
            CredentialMaterial::NativeKey { .. } => "native_key",
            CredentialMaterial::Email { .. } => "email",
        }
        .into(),
        auth_time: now,
        expires_at: now + 30 * DAY,
        idle_expires_at: now + 7 * DAY,
        revoked_at: None,
        recovery_context: None,
        csrf_digest: digest(&config.session_key, "csrf", token),
    };
    tx.create(
        &session_key(&config.session_key, token),
        "session",
        Some(&a.account_id),
        Some(s.expires_at),
        &s,
    )?;
    Ok(id)
}
pub(super) fn audit(
    tx: &mut IdentityTransaction<'_>,
    a: &AccountRecord,
    kind: &str,
    now: u64,
) -> Result<(), StoreError> {
    let id = random_token();
    tx.create(
        &format!("outbox:{id}"),
        "outbox",
        Some(&a.account_id),
        Some(now + 90 * DAY),
        &OutboxEvent {
            event_id: id,
            account_id: a.account_id.clone(),
            event_type: kind.into(),
            state_version: a.policy_version,
            safe_payload: json!({"at":now}),
            attempts: 0,
            delivery_state: "pending".into(),
        },
    )
}
pub(super) fn consume_quota(
    tx: &mut IdentityTransaction<'_>,
    kind: &str,
    subject: &str,
    now: u64,
    window: u64,
    limit: u64,
) -> Result<(), StoreError> {
    let start = now / window * window;
    let key = format!("quota:{kind}:{subject}:{start}");
    let old = tx.get::<QuotaRecord>(&key)?;
    let mut q = old
        .as_ref()
        .map(|v| v.value.clone())
        .unwrap_or(QuotaRecord {
            bucket: key.clone(),
            window_start: start,
            count: 0,
        });
    if q.count >= limit {
        return Err(reject("rate_limited"));
    }
    q.count += 1;
    if let Some(v) = old {
        tx.replace(&key, v.row_version, "quota", None, Some(start + window), &q)
    } else {
        tx.create(&key, "quota", None, Some(start + window), &q)
    }
}

fn token_reply(result: Result<(Value, Option<String>), StoreError>) -> IdentityReply {
    match result {
        Ok((body, token)) => {
            let mut r = IdentityReply::ok(body);
            if let Some(t) = token {
                r.cookies.push(set_cookie(SID, &t, 30 * 86400));
            }
            r
        }
        Err(e) => IdentityReply::error(e),
    }
}
