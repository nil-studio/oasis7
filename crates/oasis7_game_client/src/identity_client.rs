//! Explicit native identity operations with protected, immutable request originals.
//! Identity receipts do not prove world admission, ownership or task execution.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_agent_api::identity::{NativeIdentityChallengeV1, NativeIdentityPurposeV1};
use oasis7_client_storage::identity_store::IdentityStore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io::Read, path::PathBuf, time::Duration};

#[path = "identity_http.rs"]
mod http;
#[path = "participant_client.rs"]
mod participant;
pub use participant::{
    EntryOwnerProgress, ParticipantClient, ParticipantClientConfig, ParticipantOwnerObservation,
    ParticipantProgress,
};
#[cfg(test)]
#[path = "identity_client_tests.rs"]
mod tests;
pub const IDENTITY_PREFIX: &str = "/api/public/hosted-account/v2";
#[path = "identity_discovery.rs"]
mod discovery;

#[derive(Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct IdentityClientConfig {
    pub endpoint: String,
    pub identity_service_id: String,
    pub environment_id: String,
    pub audience: String,
    pub state_directory: PathBuf,
    #[cfg(test)]
    #[serde(skip)]
    pub(crate) test_loopback: bool,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct OriginalRequest {
    path: String,
    cookie: String,
    csrf: String,
    body: String,
}
#[derive(Serialize, Deserialize)]
struct IdentitySession {
    account_id: String,
    cookie: String,
}
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct IdentityProgress {
    pub stage: String,
    pub state: String,
    pub operation_id: String,
    pub account_id: Option<String>,
    pub next_action: String,
}
pub struct IdentityClient {
    config: IdentityClientConfig,
    store: IdentityStore,
    http: reqwest::blocking::Client,
    scope: String,
    signing_key: SigningKey,
}
impl IdentityClient {
    /// Only the trusted local entry adapters use current account authentication.
    /// This does not expose cookies, signing keys, or an arbitrary HTTP client.
    pub(crate) fn authenticated_entry_call(
        &self,
        login_operation: &str,
        path: &str,
        body: &Value,
    ) -> Result<Value, String> {
        validate_operation(login_operation)?;
        if ![
            "/participant/authorize",
            "/participant/lookup",
            "/participant/submit",
            "/participant/owner-view",
            "/participant/starter-eligibility",
            "/entry-owner/control",
            "/entry-executor/control",
        ]
        .contains(&path)
        {
            return Err("entry route not allowed".into());
        }
        let session = self
            .store
            .read_original::<IdentitySession>(&self.scope, login_operation, "login-session")?
            .ok_or("action_required: explicit fresh identity login")?;
        let mut original = OriginalRequest {
            path: "/session".into(),
            cookie: session.cookie,
            csrf: String::new(),
            body: String::new(),
        };
        let (current, _) = self.call("GET", "/session", Some(&original))?;
        if current["account"]["account_id"] != session.account_id {
            return Err("identity session subject changed".into());
        }
        original.path = "/capabilities".into();
        let (caps, _) = self.call("GET", "/capabilities", Some(&original))?;
        original.csrf = caps["csrf_token"]
            .as_str()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("identity CSRF unavailable")?
            .into();
        original.path = path.into();
        original.body = serde_json::to_string(body).map_err(|_| "entry request encoding failed")?;
        self.call("POST", path, Some(&original)).map(|(v, _)| v)
    }
    pub fn new(config: IdentityClientConfig, passphrase: &[u8]) -> Result<Self, String> {
        http::validate_config(&config)?;
        let store = IdentityStore::open(&config.state_directory, passphrase)?;
        let signing_key = SigningKey::from_bytes(store.login_seed());
        let scope = blake3::hash(
            serde_json::to_string(&(
                &config.endpoint,
                &config.identity_service_id,
                &config.environment_id,
                &config.audience,
            ))
            .map_err(|_| "identity configuration invalid")?
            .as_bytes(),
        )
        .to_hex()
        .to_string();
        let http = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .map_err(|_| "identity HTTP client unavailable")?;
        Ok(Self {
            config,
            store,
            http,
            scope,
            signing_key,
        })
    }
    pub fn public_key(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }

    /// Nonsecret historical progress only; never a second permissions ledger.
    pub fn status(&self, operation: &str) -> Result<IdentityProgress, String> {
        validate_operation(operation)?;
        if let Some(receipt) = self.store.read_original::<IdentityProgress>(
            &self.scope,
            operation,
            "registration-receipt",
        )? {
            return Ok(receipt);
        }
        if let Some(session) =
            self.store
                .read_original::<IdentitySession>(&self.scope, operation, "login-session")?
        {
            return Ok(IdentityProgress {
                stage: "identity_login".into(),
                state: "confirmed".into(),
                operation_id: operation.into(),
                account_id: Some(session.account_id),
                next_action: "revalidate_current_session_before_world_admission".into(),
            });
        }
        let complete = self.store.read_original::<OriginalRequest>(
            &self.scope,
            operation,
            "complete-request",
        )?;
        let options = self.store.read_original::<OriginalRequest>(
            &self.scope,
            operation,
            "options-request",
        )?;
        let registering = options
            .as_ref()
            .is_some_and(|v| v.path == "/machine-registration/options");
        Ok(IdentityProgress {
            stage: if registering {
                "identity_registration"
            } else {
                "identity_login"
            }
            .into(),
            state: if complete.is_some() {
                "unknown"
            } else if options.is_some() {
                "pending"
            } else {
                "not_started"
            }
            .into(),
            operation_id: operation.into(),
            account_id: None,
            next_action: if complete.is_some() {
                "replay_original"
            } else if options.is_some() {
                "resume_original"
            } else {
                "explicit_start"
            }
            .into(),
        })
    }

    /// Creates the identity key only by an explicit local user action. No network.
    pub fn initialize(config: &IdentityClientConfig, passphrase: &[u8]) -> Result<String, String> {
        http::validate_config(config)?;
        let store = IdentityStore::create(&config.state_directory, passphrase)?;
        Ok(hex::encode(
            SigningKey::from_bytes(store.login_seed())
                .verifying_key()
                .to_bytes(),
        ))
    }

    pub fn register(
        &self,
        operation: &str,
        display_name: &str,
    ) -> Result<IdentityProgress, String> {
        if !URL_SAFE_NO_PAD
            .decode(operation)
            .is_ok_and(|v| v.len() == 32 && URL_SAFE_NO_PAD.encode(v) == operation)
            || display_name.trim().is_empty()
            || display_name.len() > 128
            || display_name.chars().any(char::is_control)
        {
            return Err("invalid registration operation or display name".into());
        }
        let body=serde_json::to_string(&json!({"operation_id":operation,"public_key":self.public_key(),"display_name":display_name})).map_err(|_|"identity request encoding failed")?;
        let options =
            self.original_options(operation, "/machine-registration/options", &body, true)?;
        let finish = self.original_finish(
            operation,
            &options,
            NativeIdentityPurposeV1::Register,
            "/machine-registration/complete",
        )?;
        let (result, _) = self.call("POST", &finish.path, Some(&finish))?;
        if result["phase"] != "account_registered"
            || result["operation_id"] != operation
            || result["receipt"]["state"] != "committed"
            || result["next_action"] != "fresh_authentication"
        {
            return Err("registration outcome unknown; replay the original operation".into());
        }
        let account = result["receipt"]["account_id"]
            .as_str()
            .filter(|v| !v.is_empty() && v.len() <= 128)
            .ok_or("registration receipt invalid")?;
        let progress = IdentityProgress {
            stage: "identity_registration".into(),
            state: "confirmed".into(),
            operation_id: operation.into(),
            account_id: Some(account.into()),
            next_action: "login".into(),
        };
        let winner: IdentityProgress =
            self.store
                .put_original(&self.scope, operation, "registration-receipt", &progress)?;
        if winner != progress {
            return Err("identity receipt conflict".into());
        }
        Ok(progress)
    }

    pub fn login(&self, operation: &str) -> Result<IdentityProgress, String> {
        validate_operation(operation)?;
        let body = serde_json::to_string(&json!({"public_key":self.public_key(),"intent":"login"}))
            .map_err(|_| "identity request encoding failed")?;
        let options = self.original_options(operation, "/native-key/options", &body, false)?;
        let finish = self.original_finish(
            operation,
            &options,
            NativeIdentityPurposeV1::Login,
            "/native-key/complete",
        )?;
        // Reconcile the original complete request after an unknown response. A
        // consumed login without its original cookie requires explicit fresh login.
        if let Some(session) =
            self.store
                .read_original::<IdentitySession>(&self.scope, operation, "login-session")?
        {
            let request = OriginalRequest {
                path: "/session".into(),
                cookie: session.cookie,
                csrf: String::new(),
                body: String::new(),
            };
            let (reply, _) = self.call("GET", "/session", Some(&request))?;
            if reply["account"]["account_id"] != session.account_id {
                return Err("identity session changed".into());
            }
            return Ok(IdentityProgress {
                stage: "identity_login".into(),
                state: "confirmed".into(),
                operation_id: operation.into(),
                account_id: Some(session.account_id),
                next_action: "world_admission".into(),
            });
        }
        let (reply, cookie) = self.call("POST", &finish.path, Some(&finish))?;
        if reply["phase"] == "operation_committed" {
            return Ok(IdentityProgress {
                stage: "identity_login".into(),
                state: "action_required".into(),
                operation_id: operation.into(),
                account_id: None,
                next_action: "explicit_fresh_login".into(),
            });
        }
        if reply["phase"] != "authenticated" {
            return Err("login outcome unknown; query the original operation".into());
        }
        let account = reply["account"]["account_id"]
            .as_str()
            .filter(|v| !v.is_empty() && v.len() <= 128)
            .ok_or("identity account missing")?;
        let cookie = cookie.ok_or("login cookie missing; reconcile the original login")?;
        let session = IdentitySession {
            account_id: account.into(),
            cookie,
        };
        let winner: IdentitySession =
            self.store
                .put_original(&self.scope, operation, "login-session", &session)?;
        if winner.account_id != session.account_id || winner.cookie != session.cookie {
            return Err("identity session conflict".into());
        }
        Ok(IdentityProgress {
            stage: "identity_login".into(),
            state: "confirmed".into(),
            operation_id: operation.into(),
            account_id: Some(account.into()),
            next_action: "world_admission".into(),
        })
    }
    fn original_options(
        &self,
        operation: &str,
        path: &str,
        body: &str,
        registering: bool,
    ) -> Result<OriginalRequest, String> {
        validate_operation(operation)?;
        if let Some(original) = self.store.read_original::<OriginalRequest>(
            &self.scope,
            operation,
            "options-request",
        )? {
            if original.path != path || original.body != body {
                return Err("identity operation content conflict".into());
            }
            return Ok(original);
        }
        let (caps, cookie) = self.call("GET", "/capabilities", None)?;
        let native = &caps["native_key"];
        if native["identity_service_id"] != self.config.identity_service_id
            || native["environment_id"] != self.config.environment_id
            || native["algorithm"] != "Ed25519"
            || native["available"] != true
        {
            return Err("identity discovery differs from pinned trust".into());
        }
        if registering && caps["machine_registration"]["available"] != true {
            return Err(
                "machine registration unavailable; preserve identity and await authorized entry"
                    .into(),
            );
        }
        let csrf = caps["csrf_token"]
            .as_str()
            .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("identity CSRF missing")?;
        let original = OriginalRequest {
            path: path.into(),
            cookie: cookie.ok_or("identity preauth cookie missing")?,
            csrf: csrf.into(),
            body: body.into(),
        };
        let winner: OriginalRequest =
            self.store
                .put_original(&self.scope, operation, "options-request", &original)?;
        if winner.path != original.path || winner.body != original.body {
            return Err("identity operation content conflict".into());
        }
        Ok(winner)
    }
    fn original_finish(
        &self,
        operation: &str,
        options: &OriginalRequest,
        purpose: NativeIdentityPurposeV1,
        path: &str,
    ) -> Result<OriginalRequest, String> {
        if let Some(original) = self.store.read_original::<OriginalRequest>(
            &self.scope,
            operation,
            "complete-request",
        )? {
            if original.path != path
                || original.cookie != options.cookie
                || original.csrf != options.csrf
            {
                return Err("identity original binding conflict".into());
            }
            return Ok(original);
        }
        let (reply, _) = self.call("POST", &options.path, Some(options))?;
        // Freeze the received challenge before signing. A racing response can
        // never replace the winner or cause a second signed complete request.
        let reply: Value = self
            .store
            .put_original(&self.scope, operation, "challenge", &reply)?;
        if purpose == NativeIdentityPurposeV1::Register && reply["operation_id"] != operation {
            return Err("identity challenge operation mismatch".into());
        }
        let body = self.proof(&reply, purpose, http::now_ms())?;
        let original = OriginalRequest {
            path: path.into(),
            cookie: options.cookie.clone(),
            csrf: options.csrf.clone(),
            body: serde_json::to_string(&body).map_err(|_| "identity request encoding failed")?,
        };
        let winner: OriginalRequest =
            self.store
                .put_original(&self.scope, operation, "complete-request", &original)?;
        if winner != original {
            return Err("identity proof conflict; preserve original operation".into());
        }
        Ok(winner)
    }
    fn proof(
        &self,
        reply: &Value,
        purpose: NativeIdentityPurposeV1,
        now: u64,
    ) -> Result<Value, String> {
        let o = &reply["options"];
        let text = |name: &str| {
            o[name]
                .as_str()
                .ok_or_else(|| "identity challenge malformed".to_string())
        };
        let id = text("operation_id")?;
        validate_operation(id)?;
        if o["identity_service_id"] != self.config.identity_service_id
            || o["environment_id"] != self.config.environment_id
            || o["audience"] != self.config.audience
            || o["purpose"] != purpose.as_str()
            || o["account_context"] != ""
            || o["public_key"] != self.public_key()
            || reply["operation_id"] != id
            || reply["ceremony_id"] != id
            || reply["expires_at"] != o["expires_at"]
        {
            return Err("identity challenge trust or purpose mismatch".into());
        }
        let issued = o["issued_at"]
            .as_u64()
            .ok_or("identity challenge time malformed")?;
        let expires = o["expires_at"]
            .as_u64()
            .ok_or("identity challenge time malformed")?;
        if expires <= now
            || issued > now.saturating_add(5000)
            || expires.checked_sub(issued) != Some(120000)
        {
            return Err("identity challenge expired or invalid".into());
        }
        let nonce: [u8; 32] = URL_SAFE_NO_PAD
            .decode(text("nonce")?)
            .map_err(|_| "identity nonce invalid")?
            .try_into()
            .map_err(|_| "identity nonce invalid")?;
        let key = self.signing_key.verifying_key().to_bytes();
        let bytes = NativeIdentityChallengeV1 {
            identity_service_id: &self.config.identity_service_id,
            environment_id: &self.config.environment_id,
            audience: &self.config.audience,
            purpose,
            account_context: "",
            operation_id: id,
            public_key: &key,
            nonce: &nonce,
            issued_at: issued,
            expires_at: expires,
        }
        .signing_bytes()?;
        if URL_SAFE_NO_PAD.encode(&bytes) != text("signed_bytes")? {
            return Err("identity signed bytes mismatch".into());
        }
        Ok(
            json!({"operation_id":id,"ceremony_id":id,"signature":URL_SAFE_NO_PAD.encode(self.signing_key.sign(&bytes).to_bytes())}),
        )
    }
}
fn validate_operation(operation: &str) -> Result<(), String> {
    if operation.is_empty()
        || operation.len() > 128
        || !operation
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("invalid identity client operation".into());
    }
    Ok(())
}
