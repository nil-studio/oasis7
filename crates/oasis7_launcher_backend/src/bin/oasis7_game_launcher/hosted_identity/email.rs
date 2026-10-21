//! Optional email adapter: purpose-bound persistent OTPs and encrypted factor data.
use super::{
    digest,
    model::*,
    random_token, reject,
    service::{
        IdentityService, active_keys, audit, authorized_action, consume_quota, create_session,
        credential_key, session_key, valid_session,
    },
    store::{IdentityTransaction, StoreError, Versioned},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
#[cfg(not(test))]
use lettre::{Message, Transport};
use lettre::{SmtpTransport, transport::smtp::authentication::Credentials};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{env, time::Duration};
const DAY: u64 = 86_400_000;

pub(crate) struct EmailAdapter {
    key: [u8; 32],
    key_id: String,
    key_configured: bool,
    smtp: Option<SmtpTransport>,
    sender: Option<lettre::message::Mailbox>,
    pub recovery_enabled: bool,
    pub clock_healthy: bool,
    #[cfg(test)]
    pub delivered: Vec<(String, String, String)>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct EmailOtp {
    pub otp_id: String,
    pub purpose: String,
    pub owner: String,
    pub account_id: Option<String>,
    pub target_digest: Option<String>,
    pub credential_id: Option<String>,
    pub credential_version: Option<u64>,
    pub encrypted_address: String,
    pub lookup: String,
    pub verifier: String,
    pub expires_at: u64,
    pub failed_attempts: u32,
    pub state: String,
    pub issued_at: u64,
}
impl EmailAdapter {
    pub fn from_env(secret: &[u8; 32]) -> Result<Self, String> {
        #[cfg(test)]
        let fallback_key =
            Some(*blake3::keyed_hash(secret, b"oasis7.identity.email.key.v1").as_bytes());
        #[cfg(not(test))]
        let fallback_key: Option<[u8; 32]> = None;
        let _ = secret;
        let configured_key = env::var("OASIS7_HOSTED_IDENTITY_EMAIL_KEY")
            .ok()
            .map(|v| {
                hex::decode(v)
                    .map_err(|_| "invalid identity email key".to_string())
                    .and_then(|v| {
                        v.try_into()
                            .map_err(|_| "identity email key must contain 32 bytes".into())
                    })
            })
            .transpose()?;
        let key_configured = configured_key.is_some() || fallback_key.is_some();
        let key = configured_key.or(fallback_key).unwrap_or([0; 32]);
        let key_id =
            env::var("OASIS7_HOSTED_IDENTITY_EMAIL_KEY_ID").unwrap_or_else(|_| "email-v1".into());
        if key_id.is_empty()
            || key_id.len() > 40
            || !key_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("invalid email key id".into());
        }
        let enabled = env::var("OASIS7_HOSTED_EMAIL_ENABLED").ok().as_deref() == Some("1");
        if (enabled || env::var("OASIS7_HOSTED_IDENTITY_LEGACY_SNAPSHOT").is_ok())
            && !key_configured
        {
            return Err(
                "OASIS7_HOSTED_IDENTITY_EMAIL_KEY is required for email or legacy migration".into(),
            );
        }
        let configured = (|| {
            if !enabled {
                return None;
            }
            let from = env::var("OASIS7_HOSTED_LOGIN_SMTP_FROM_EMAIL")
                .ok()?
                .parse()
                .ok()?;
            let password = env::var("OASIS7_HOSTED_LOGIN_SMTP_PASSWORD").ok()?;
            if password.is_empty() {
                return None;
            }
            let username = env::var("OASIS7_HOSTED_LOGIN_SMTP_USERNAME")
                .ok()
                .or_else(|| env::var("OASIS7_HOSTED_LOGIN_SMTP_FROM_EMAIL").ok())?;
            let host = env::var("OASIS7_HOSTED_LOGIN_SMTP_HOST")
                .unwrap_or_else(|_| "smtpdm.aliyun.com".into());
            let port = env::var("OASIS7_HOSTED_LOGIN_SMTP_PORT")
                .ok()
                .map(|v| v.parse::<u16>())
                .transpose()
                .ok()?
                .unwrap_or(465);
            let smtp = SmtpTransport::relay(&host)
                .ok()?
                .port(port)
                .timeout(Some(Duration::from_secs(15)))
                .credentials(Credentials::new(username, password))
                .build();
            Some((smtp, from))
        })();
        let (smtp, sender) = configured
            .map(|(s, f)| (Some(s), Some(f)))
            .unwrap_or((None, None));
        Ok(Self {
            key,
            key_id,
            key_configured,
            smtp,
            sender,
            recovery_enabled: env::var("OASIS7_HOSTED_EMAIL_RECOVERY_ENABLED")
                .ok()
                .as_deref()
                == Some("1"),
            clock_healthy: env::var("OASIS7_HOSTED_IDENTITY_CLOCK_HEALTHY")
                .ok()
                .as_deref()
                == Some("1"),
            #[cfg(test)]
            delivered: vec![],
        })
    }
    pub fn verify_key_binding(
        &self,
        store: &mut super::store::IdentityStore,
    ) -> Result<(), StoreError> {
        if !self.key_configured {
            return Ok(());
        }
        let binding = format!("{}:{}", self.key_id, blake3::hash(&self.key).to_hex());
        store.transaction(|tx| {
            if let Some(old) = tx.get::<String>("config:email-key-v1")? {
                if old.value != binding {
                    return Err(reject("email_key_rotation_requires_migration"));
                }
            } else {
                tx.create("config:email-key-v1", "configuration", None, None, &binding)?;
            }
            Ok(())
        })
    }
    pub fn available(&self) -> bool {
        self.smtp.is_some() && self.sender.is_some()
    }
    pub fn recovery_available(&self) -> bool {
        self.available() && self.recovery_enabled && self.clock_healthy
    }
    pub fn lookup(&self, address: &str) -> String {
        format!(
            "v1:{}:{}",
            self.key_id,
            digest(&self.key, "email-lookup-v1", address)
        )
    }
    pub fn encrypt(&self, address: &str) -> Result<String, StoreError> {
        if !self.key_configured {
            return Err(reject("email_unavailable"));
        }
        let mut nonce = [0; 24];
        getrandom::fill(&mut nonce).map_err(|_| StoreError::Unavailable("entropy".into()))?;
        let cipher = XChaCha20Poly1305::new((&self.key).into())
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: address.as_bytes(),
                    aad: b"oasis7.identity.email.v1",
                },
            )
            .map_err(|_| StoreError::Corrupt)?;
        let mut bytes = nonce.to_vec();
        bytes.extend(cipher);
        Ok(format!(
            "v1:{}:{}",
            self.key_id,
            URL_SAFE_NO_PAD.encode(bytes)
        ))
    }
    pub fn decrypt(&self, cipher: &str) -> Result<String, StoreError> {
        let bytes = URL_SAFE_NO_PAD
            .decode(
                cipher
                    .strip_prefix(&format!("v1:{}:", self.key_id))
                    .ok_or(StoreError::Corrupt)?,
            )
            .map_err(|_| StoreError::Corrupt)?;
        if bytes.len() < 40 {
            return Err(StoreError::Corrupt);
        }
        let nonce: [u8; 24] = bytes[..24].try_into().map_err(|_| StoreError::Corrupt)?;
        let plain = XChaCha20Poly1305::new((&self.key).into())
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &bytes[24..],
                    aad: b"oasis7.identity.email.v1",
                },
            )
            .map_err(|_| StoreError::Corrupt)?;
        String::from_utf8(plain).map_err(|_| StoreError::Corrupt)
    }
    pub fn material(&self, address: &str, now: u64) -> Result<CredentialMaterial, StoreError> {
        Ok(CredentialMaterial::Email {
            encrypted_address: self.encrypt(address)?,
            lookup_hmac: self.lookup(address),
            normalization_version: 1,
            verified_at: now,
            bound_at: now,
            recovery_enabled: self.recovery_enabled,
            recovery_eligible_since: now,
        })
    }
    pub(super) fn verifier(&self, otp: &EmailOtp, code: &str) -> String {
        digest(
            &self.key,
            "email-otp-v1",
            &format!(
                "{}:{}:{}:{}:{}:{}",
                otp.otp_id,
                otp.purpose,
                otp.owner,
                otp.account_id.as_deref().unwrap_or(""),
                otp.target_digest.as_deref().unwrap_or(""),
                code
            ),
        )
    }
    pub(super) fn send(
        &mut self,
        address: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), StoreError> {
        #[cfg(test)]
        {
            self.delivered
                .push((address.into(), subject.into(), body.into()));
            Ok(())
        }
        #[cfg(not(test))]
        {
            let smtp = self
                .smtp
                .as_ref()
                .ok_or_else(|| reject("email_unavailable"))?;
            let from = self
                .sender
                .as_ref()
                .ok_or_else(|| reject("email_unavailable"))?;
            let target = address.parse().map_err(|_| reject("invalid_request"))?;
            let message = Message::builder()
                .from(from.clone())
                .to(target)
                .subject(subject)
                .body(body.to_owned())
                .map_err(|_| reject("email_unavailable"))?;
            smtp.send(&message)
                .map_err(|_| reject("email_unavailable"))?;
            Ok(())
        }
    }
}
pub(super) fn normalize_email(raw: &str) -> Result<String, StoreError> {
    let normalized = raw.trim().to_ascii_lowercase();
    let (local, domain) = normalized
        .split_once('@')
        .ok_or_else(|| reject("invalid_request"))?;
    if normalized.len() > 254
        || local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || normalized
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
        || normalized.parse::<lettre::Address>().is_err()
    {
        return Err(reject("invalid_request"));
    }
    Ok(normalized)
}
fn constant_equal(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}
#[expect(
    clippy::too_many_arguments,
    reason = "OTP verification independently binds purpose, owner, account, target and expiry within one transaction."
)]
pub(super) fn read_otp(
    tx: &IdentityTransaction<'_>,
    email: &EmailAdapter,
    id: &str,
    code: &str,
    purpose: &str,
    owner: &str,
    account: Option<&str>,
    target: Option<&str>,
    now: u64,
) -> Result<Versioned<EmailOtp>, StoreError> {
    let otp = tx
        .get::<EmailOtp>(&format!("otp:{id}"))?
        .ok_or_else(|| reject("authentication_failed"))?;
    let v = &otp.value;
    if v.state != "pending"
        || v.expires_at <= now
        || v.failed_attempts >= 5
        || v.purpose != purpose
        || v.owner != owner
        || v.account_id.as_deref() != account
        || v.target_digest.as_deref() != target
        || code.len() != 6
        || !constant_equal(&v.verifier, &email.verifier(v, code))
    {
        return Err(reject("authentication_failed"));
    }
    if let Some(cid) = &v.credential_id {
        let c = tx
            .get::<CredentialRecord>(&credential_key(cid))?
            .ok_or_else(|| reject("authentication_failed"))?
            .value;
        if c.state != "active"
            || Some(c.version) != v.credential_version
            || Some(c.account_id.as_str()) != account
        {
            return Err(reject("authentication_failed"));
        }
    }
    Ok(otp)
}
pub(super) fn consume_otp(
    tx: &mut IdentityTransaction<'_>,
    otp: &mut Versioned<EmailOtp>,
) -> Result<(), StoreError> {
    otp.value.state = "consumed".into();
    otp.value.verifier.clear();
    tx.replace(
        &format!("otp:{}", otp.value.otp_id),
        otp.row_version,
        "email_otp",
        otp.value.account_id.as_deref(),
        Some(otp.value.expires_at),
        &otp.value,
    )
}
impl IdentityService {
    pub(super) fn committed_identity_receipt(
        &mut self,
        id: &str,
        owner: &str,
        request_digest: &str,
    ) -> Result<Option<Value>, StoreError> {
        self.store
            .transaction(|tx| read_identity_receipt(tx, id, owner, request_digest))
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "OTP issuance retains distinct account, recovery target, owner and source quota bindings."
    )]
    pub(super) fn issue_email_otp(
        &mut self,
        purpose: &str,
        owner: &str,
        address: &str,
        account: Option<&str>,
        target: Option<&str>,
        source: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        #[cfg(not(test))]
        if !self.email.available() {
            return Err(reject("email_unavailable"));
        }
        let address = normalize_email(address)?;
        let id = random_token();
        let mut random = [0; 4];
        let code = loop {
            getrandom::fill(&mut random).map_err(|_| StoreError::Unavailable("entropy".into()))?;
            let n = u32::from_le_bytes(random);
            if n < 4_294_000_000 {
                break format!("{:06}", n % 1_000_000);
            }
        };
        let lookup = self.email.lookup(&address);
        let mut otp = EmailOtp {
            otp_id: id.clone(),
            purpose: purpose.into(),
            owner: owner.into(),
            account_id: account.map(str::to_owned),
            target_digest: target.map(str::to_owned),
            credential_id: None,
            credential_version: None,
            encrypted_address: self.email.encrypt(&address)?,
            lookup: lookup.clone(),
            verifier: String::new(),
            expires_at: now + 120_000,
            failed_attempts: 0,
            state: "preparing".into(),
            issued_at: now,
        };
        otp.verifier = self.email.verifier(&otp, &code);
        let secret = self.config.session_key;
        self.store.transaction(|tx|{consume_quota(tx,"email-source",&digest(&secret,"source",source),now,60_000,5)?;consume_quota(tx,"email-address",&lookup,now,60_000,1)?;consume_quota(tx,"email-total","deployment",now,60_000,300)?;if let Some(a)=account&& let Some(c)=tx.all::<CredentialRecord>("credential",Some(a))?.into_iter().map(|(_,v)|v.value).find(|c|c.state=="active"&&matches!(&c.public_material,CredentialMaterial::Email{lookup_hmac,..} if lookup_hmac==&lookup)){otp.credential_id=Some(c.credential_record_id);otp.credential_version=Some(c.version);}tx.create(&format!("otp:{id}"),"email_otp",account,Some(otp.expires_at),&otp)})?;
        let delivered = self.email.send(
            &address,
            "Oasis7 验证码",
            &format!("操作：{purpose}\n验证码：{code}\n两分钟内有效；此验证码仅用于本次指定操作。"),
        );
        self.store.transaction(|tx| {
            let mut saved = tx
                .get::<EmailOtp>(&format!("otp:{id}"))?
                .ok_or(StoreError::Corrupt)?;
            saved.value.state = if delivered.is_ok() {
                "pending"
            } else {
                "cancelled"
            }
            .into();
            tx.replace(
                &format!("otp:{id}"),
                saved.row_version,
                "email_otp",
                account,
                Some(saved.value.expires_at),
                &saved.value,
            )
        })?;
        delivered?;
        Ok(
            json!({"api_version":2,"ok":true,"challenge_id":id,"otp_id":id,"operation_id":id,"expires_at":otp.expires_at}),
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Failure accounting must verify the same purpose, owner, account, target and time as OTP consumption."
    )]
    pub(super) fn otp_precheck(
        &mut self,
        id: &str,
        code: &str,
        purpose: &str,
        owner: &str,
        account: Option<&str>,
        target: Option<&str>,
        now: u64,
    ) -> Result<(), StoreError> {
        let email = &self.email;
        let valid = self.store.transaction(|tx| {
            if read_otp(tx, email, id, code, purpose, owner, account, target, now).is_ok() {
                return Ok(true);
            }
            if let Some(mut otp) = tx.get::<EmailOtp>(&format!("otp:{id}"))?
                && otp.value.owner == owner
                && otp.value.purpose == purpose
                && otp.value.state == "pending"
            {
                otp.value.failed_attempts += 1;
                if otp.value.failed_attempts >= 5 {
                    otp.value.state = "exhausted".into();
                    otp.value.verifier.clear();
                }
                tx.replace(
                    &format!("otp:{id}"),
                    otp.row_version,
                    "email_otp",
                    otp.value.account_id.as_deref(),
                    Some(otp.value.expires_at),
                    &otp.value,
                )?;
            }
            Ok(false)
        })?;
        if valid {
            Ok(())
        } else {
            Err(reject("authentication_failed"))
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "The email route carries separate authenticated session, ceremony owner, source quota and time boundaries."
    )]
    pub(super) fn email_route(
        &mut self,
        method: &str,
        path: &str,
        sid: Option<&str>,
        owner: &str,
        p: &Value,
        source: &str,
        now: u64,
    ) -> Option<Result<(Value, Option<String>), StoreError>> {
        let handled = matches!(
            (method, path),
            ("POST", "/email/login/options")
                | ("POST", "/email/login/complete")
                | ("POST", "/email/binding/options")
                | ("POST", "/email/binding/complete")
                | ("POST", "/email/unbind")
        ) || (method == "POST"
            && path.starts_with("/actions/")
            && (path.ends_with("/email/options") || path.ends_with("/email/complete")));
        if !handled {
            return None;
        }
        Some(self.email_dispatch(path, sid, owner, p, source, now))
    }
    fn email_dispatch(
        &mut self,
        path: &str,
        sid: Option<&str>,
        owner: &str,
        p: &Value,
        source: &str,
        now: u64,
    ) -> Result<(Value, Option<String>), StoreError> {
        if path == "/email/login/options" {
            let address = p["email"]
                .as_str()
                .ok_or_else(|| reject("invalid_request"))?;
            return self
                .issue_email_otp("login", owner, address, None, None, source, now)
                .map(|v| (v, None));
        }
        if path == "/email/login/complete" {
            return self.email_login(owner, p, source, now);
        }
        if path == "/email/binding/complete" {
            let (id, _) = otp_fields(p)?;
            if p["operation_id"].as_str() != Some(id) {
                return Err(reject("invalid_request"));
            }
            let d = digest(&self.config.session_key, "email-binding", &p.to_string());
            if let Some(v) = self.committed_identity_receipt(id, owner, &d)? {
                return Ok((v, None));
            }
        }
        if path == "/email/unbind" {
            let id = p["action_id"]
                .as_str()
                .ok_or_else(|| reject("invalid_request"))?;
            if p["operation_id"].as_str() != Some(id) {
                return Err(reject("invalid_request"));
            }
            let d = digest(&self.config.session_key, "email-unbind", &p.to_string());
            if let Some(v) = self.committed_identity_receipt(id, owner, &d)? {
                return Ok((v, None));
            }
        }
        let (a, s) = self.session(sid, now)?;
        let skey = session_key(
            &self.config.session_key,
            sid.ok_or_else(|| reject("session_expired"))?,
        );
        let epoch = self.config.rp.identity_instance_epoch;
        if path.starts_with("/actions/") {
            let parts: Vec<_> = path.trim_matches('/').split('/').collect();
            if parts.len() != 4 {
                return Err(reject("invalid_request"));
            }
            let action_id = parts[1];
            let action = self.store.transaction(|tx| {
                tx.get::<SensitiveAction>(&format!("action:{action_id}"))?
                    .ok_or_else(|| reject("reauth_required"))
            })?;
            if a.has_ever_public_key
                || action.value.account_id != a.account_id
                || action.value.session_id != s.session_id
                || action.value.account_policy_version != a.policy_version
                || action.value.state != "pending"
                || action.value.expires_at <= now
            {
                return Err(reject("reauth_required"));
            }
            let email_credential = self
                .store
                .transaction(|tx| tx.all::<CredentialRecord>("credential", Some(&a.account_id)))?
                .into_iter()
                .map(|(_, v)| v.value)
                .find(|c| {
                    c.state == "active"
                        && matches!(c.public_material, CredentialMaterial::Email { .. })
                })
                .ok_or_else(|| reject("reauth_required"))?;
            if parts[3] == "options" {
                let address = if let CredentialMaterial::Email {
                    encrypted_address, ..
                } = &email_credential.public_material
                {
                    self.email.decrypt(encrypted_address)?
                } else {
                    unreachable!()
                };
                return self
                    .issue_email_otp(
                        "action",
                        owner,
                        &address,
                        Some(&a.account_id),
                        Some(action_id),
                        source,
                        now,
                    )
                    .map(|v| (v, None));
            }
            let (id, code) = otp_fields(p)?;
            self.otp_precheck(
                id,
                code,
                "action",
                owner,
                Some(&a.account_id),
                Some(action_id),
                now,
            )?;
            let email = &self.email;
            return self.store.transaction(|tx| {
                let (a, s) = valid_session(tx, &skey, epoch, now)?;
                if a.has_ever_public_key {
                    return Err(reject("reauth_required"));
                }
                let mut action = tx
                    .get::<SensitiveAction>(&format!("action:{action_id}"))?
                    .ok_or_else(|| reject("reauth_required"))?;
                if action.value.session_id != s.session_id
                    || action.value.state != "pending"
                    || action.value.account_policy_version != a.policy_version
                    || action.value.expires_at <= now
                {
                    return Err(reject("state_changed"));
                }
                let mut otp = read_otp(
                    tx,
                    email,
                    id,
                    code,
                    "action",
                    owner,
                    Some(&a.account_id),
                    Some(action_id),
                    now,
                )?;
                action.value.state = "authorized".into();
                action.value.proof_credential_id = otp.value.credential_id.clone();
                action.value.proof_time = Some(now);
                action.value.expires_at = now + 300_000;
                consume_otp(tx, &mut otp)?;
                tx.replace(
                    &format!("action:{action_id}"),
                    action.row_version,
                    "action",
                    Some(&a.account_id),
                    Some(action.value.expires_at),
                    &action.value,
                )?;
                Ok((
                    json!({"api_version":2,"ok":true,"action_id":action_id}),
                    None,
                ))
            });
        }
        let operation_id = p["operation_id"].as_str();
        let action_id = p["action_id"]
            .as_str()
            .ok_or_else(|| reject("reauth_required"))?;
        if path == "/email/binding/options" {
            let address = normalize_email(
                p["email"]
                    .as_str()
                    .ok_or_else(|| reject("invalid_request"))?,
            )?;
            let lookup = self.email.lookup(&address);
            self.store.transaction(|tx| {
                authorized_action(tx, action_id, &a, &s, "bind_email", Some(&lookup), now)?;
                Ok(())
            })?;
            return self
                .issue_email_otp(
                    "bind_email",
                    owner,
                    &address,
                    Some(&a.account_id),
                    Some(action_id),
                    source,
                    now,
                )
                .map(|v| (v, None));
        }
        if path == "/email/unbind" {
            if operation_id != Some(action_id) {
                return Err(reject("invalid_request"));
            }
            let request_digest = digest(&self.config.session_key, "email-unbind", &p.to_string());
            return self.store.transaction(|tx| {
                let (a, s) = valid_session(tx, &skey, epoch, now)?;
                let cid = p["credential_id"]
                    .as_str()
                    .ok_or_else(|| reject("invalid_request"))?;
                let mut action =
                    authorized_action(tx, action_id, &a, &s, "unbind_email", Some(cid), now)?;
                if active_keys(tx, &a.account_id)?.is_empty() {
                    return Err(reject("last_credential_protected"));
                }
                let mut c = tx
                    .get::<CredentialRecord>(&credential_key(cid))?
                    .ok_or_else(|| reject("invalid_request"))?;
                if c.value.account_id != a.account_id
                    || c.value.version != p["expected_version"].as_u64().unwrap_or(0)
                    || !matches!(c.value.public_material, CredentialMaterial::Email { .. })
                {
                    return Err(reject("state_changed"));
                }
                remove_email(tx, &mut c, now)?;
                complete_email_action(tx, &mut action, &a, now)?;
                tx.create(
                    &format!("receipt:{action_id}"),
                    "receipt",
                    Some(&a.account_id),
                    Some(now + 7 * DAY),
                    &OperationReceipt {
                        operation_id: action_id.into(),
                        kind: "email_unbind".into(),
                        owner_context_digest: owner.into(),
                        request_digest,
                        state: "committed".into(),
                        result_ids: vec![a.account_id.clone()],
                        committed_at: now,
                    },
                )?;
                Ok((json!({"api_version":2,"ok":true}), None))
            });
        }
        let (id, code) = otp_fields(p)?;
        if operation_id != Some(id) {
            return Err(reject("invalid_request"));
        }
        let request_digest = digest(&self.config.session_key, "email-binding", &p.to_string());
        self.otp_precheck(
            id,
            code,
            "bind_email",
            owner,
            Some(&a.account_id),
            Some(action_id),
            now,
        )?;
        let email = &self.email;
        self.store.transaction(|tx| {
            let (a, s) = valid_session(tx, &skey, epoch, now)?;
            let mut otp = read_otp(
                tx,
                email,
                id,
                code,
                "bind_email",
                owner,
                Some(&a.account_id),
                Some(action_id),
                now,
            )?;
            let mut action = authorized_action(
                tx,
                action_id,
                &a,
                &s,
                "bind_email",
                Some(&otp.value.lookup),
                now,
            )?;
            let lkey = format!("email_lookup:{}", otp.value.lookup);
            if let Some(l) = tx.get::<CredentialLookup>(&lkey)? {
                if l.value.account_id != a.account_id {
                    return Err(reject("credential_conflict"));
                }
                return Err(reject("state_changed"));
            }
            let old = tx.all::<CredentialRecord>("credential", Some(&a.account_id))?;
            for (_, mut c) in old {
                if c.value.state == "active"
                    && matches!(c.value.public_material, CredentialMaterial::Email { .. })
                {
                    remove_email(tx, &mut c, now)?;
                }
            }
            let address = email.decrypt(&otp.value.encrypted_address)?;
            let cid = random_token();
            let c = CredentialRecord {
                credential_record_id: cid.clone(),
                account_id: a.account_id.clone(),
                state: "active".into(),
                version: 1,
                label: "邮箱".into(),
                created_at: now,
                last_used_at: None,
                public_material: email.material(&address, now)?,
                registered_via: "authorized_email_binding".into(),
            };
            tx.create(
                &credential_key(&cid),
                "credential",
                Some(&a.account_id),
                None,
                &c,
            )?;
            tx.create(
                &lkey,
                "lookup",
                Some(&a.account_id),
                None,
                &CredentialLookup {
                    scope: "email:v1".into(),
                    key_digest: otp.value.lookup.clone(),
                    canonical_key: Vec::new(),
                    credential_record_id: cid.clone(),
                    account_id: a.account_id.clone(),
                    state: "active".into(),
                },
            )?;
            consume_otp(tx, &mut otp)?;
            complete_email_action(tx, &mut action, &a, now)?;
            tx.create(
                &format!("receipt:{id}"),
                "receipt",
                Some(&a.account_id),
                Some(now + 7 * DAY),
                &OperationReceipt {
                    operation_id: id.into(),
                    kind: "email_binding".into(),
                    owner_context_digest: owner.into(),
                    request_digest,
                    state: "committed".into(),
                    result_ids: vec![a.account_id.clone(), cid.clone()],
                    committed_at: now,
                },
            )?;
            Ok((json!({"api_version":2,"ok":true,"credential_id":cid}), None))
        })
    }
    fn email_login(
        &mut self,
        owner: &str,
        p: &Value,
        source: &str,
        now: u64,
    ) -> Result<(Value, Option<String>), StoreError> {
        let (id, code) = otp_fields(p)?;
        if p["operation_id"].as_str() != Some(id) {
            return Err(reject("invalid_request"));
        }
        let request_digest = digest(
            &self.config.session_key,
            "email-response",
            &format!("{id}:{code}"),
        );
        if let Some(value) = self.committed_identity_receipt(id, owner, &request_digest)? {
            return Ok((value, None));
        }
        self.otp_precheck(id, code, "login", owner, None, None, now)?;
        let token = random_token();
        let email = &self.email;
        let config = &self.config;
        self.store.transaction(|tx|{if let Some(v)=read_identity_receipt(tx,id,owner,&request_digest)?{return Ok((v,None));}let mut otp=read_otp(tx,email,id,code,"login",owner,None,None,now)?;let lkey=format!("email_lookup:{}",otp.value.lookup);let lookup=tx.get::<CredentialLookup>(&lkey)?;let(mut a,c,av)=if let Some(l)=lookup{if l.value.state!="active"{return Err(reject("authentication_failed"));}let c=tx.get::<CredentialRecord>(&credential_key(&l.value.credential_record_id))?.ok_or(StoreError::Corrupt)?.value;let a=tx.get::<AccountRecord>(&format!("account:{}",l.value.account_id))?.ok_or(StoreError::Corrupt)?;if c.state!="active"||!matches!(&c.public_material,CredentialMaterial::Email{lookup_hmac,..} if lookup_hmac==&otp.value.lookup){return Err(reject("authentication_failed"));}(a.value,c,Some(a.row_version))}else{if !config.registration_enabled{return Err(reject("authentication_failed"));}let account_id=format!("account-{}",random_token());let cid=random_token();let address=email.decrypt(&otp.value.encrypted_address)?;let a=AccountRecord{account_id:account_id.clone(),user_handle:uuid::Uuid::new_v4().as_bytes().to_vec(),display_name:format!("玩家 {}",&account_id[account_id.len()-6..]),status:"active".into(),policy_version:1,security_epoch:1,has_ever_public_key:false,created_at:now,last_authenticated_at:Some(now),last_recovery_id:None};let c=CredentialRecord{credential_record_id:cid.clone(),account_id:account_id.clone(),state:"active".into(),version:1,label:"邮箱".into(),created_at:now,last_used_at:Some(now),public_material:email.material(&address,now)?,registered_via:"email_registration".into()};tx.create(&credential_key(&cid),"credential",Some(&account_id),None,&c)?;tx.create(&lkey,"lookup",Some(&account_id),None,&CredentialLookup{scope:"email:v1".into(),key_digest:otp.value.lookup.clone(),canonical_key:vec![],credential_record_id:cid,account_id:account_id.clone(),state:"active".into()})?;(a,c,None)};if a.status!="active"{return Err(reject("authentication_failed"));}a.last_authenticated_at=Some(now);if let Some(v)=av{tx.replace(&format!("account:{}",a.account_id),v,"account",Some(&a.account_id),None,&a)?;}else{consume_quota(tx,"accounts-source",&digest(&config.session_key,"source",source),now,3_600_000,10)?;consume_quota(tx,"accounts-total","deployment",now,DAY,100)?;tx.create(&format!("account:{}",a.account_id),"account",Some(&a.account_id),None,&a)?;}create_session(tx,config,&a,&c,&token,now)?;let skey=session_key(&config.session_key,&token);let mut session=tx.get::<AccountSession>(&skey)?.ok_or(StoreError::Corrupt)?;session.value.auth_method="email".into();tx.replace(&skey,session.row_version,"session",Some(&a.account_id),Some(session.value.expires_at),&session.value)?;consume_otp(tx,&mut otp)?;tx.create(&format!("receipt:{id}"),"receipt",Some(&a.account_id),Some(now+7*DAY),&OperationReceipt{operation_id:id.into(),kind:"email_login".into(),owner_context_digest:owner.into(),request_digest:digest(&config.session_key,"email-response",&format!("{id}:{code}")),state:"committed".into(),result_ids:vec![a.account_id.clone()],committed_at:now})?;audit(tx,&a,"email_authenticated",now)?;Ok((json!({"api_version":2,"ok":true,"phase":"authenticated","account":{"account_id":a.account_id,"display_name":a.display_name},"world":{"state":"not_requested"}}),Some(token)))})
    }
}
pub(super) fn read_identity_receipt(
    tx: &IdentityTransaction<'_>,
    id: &str,
    owner: &str,
    request_digest: &str,
) -> Result<Option<Value>, StoreError> {
    let Some(receipt) = tx.get::<OperationReceipt>(&format!("receipt:{id}"))? else {
        return Ok(None);
    };
    let r = receipt.value;
    if r.owner_context_digest != owner {
        return Err(reject("authentication_failed"));
    }
    if r.request_digest != request_digest {
        return Err(reject("idempotency_conflict"));
    }
    Ok(Some(
        json!({"api_version":2,"ok":true,"phase":"operation_committed","operation_id":id,"result_ids":r.result_ids,"account_retained":true,"next_action":"fresh_authentication","world":{"state":"not_requested"}}),
    ))
}
pub(super) fn otp_fields(p: &Value) -> Result<(&str, &str), StoreError> {
    let id = p["otp_id"]
        .as_str()
        .or_else(|| p["challenge_id"].as_str())
        .filter(|id| id.len() == 43)
        .ok_or_else(|| reject("invalid_request"))?;
    let code = p["code"]
        .as_str()
        .or_else(|| p["otp_code"].as_str())
        .filter(|v| v.len() == 6 && v.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| reject("invalid_request"))?;
    Ok((id, code))
}
fn remove_email(
    tx: &mut IdentityTransaction<'_>,
    c: &mut Versioned<CredentialRecord>,
    now: u64,
) -> Result<(), StoreError> {
    if let CredentialMaterial::Email { lookup_hmac, .. } = &c.value.public_material {
        let lkey = format!("email_lookup:{lookup_hmac}");
        if let Some(l) = tx.get::<CredentialLookup>(&lkey)? {
            tx.delete(&lkey, l.row_version)?;
        }
    }
    c.value.state = "revoked".into();
    c.value.version += 1;
    tx.replace(
        &credential_key(&c.value.credential_record_id),
        c.row_version,
        "credential",
        Some(&c.value.account_id),
        None,
        &c.value,
    )?;
    for (key, mut r) in tx.all::<RecoveryRequest>("recovery", Some(&c.value.account_id))? {
        if matches!(r.value.state.as_str(), "waiting" | "approved") {
            r.value.state = "cancelled".into();
            tx.replace(
                &key,
                r.row_version,
                "recovery",
                Some(&r.value.account_id),
                Some(r.value.expires_at),
                &r.value,
            )?;
        }
    }
    let _ = now;
    Ok(())
}
fn complete_email_action(
    tx: &mut IdentityTransaction<'_>,
    action: &mut Versioned<SensitiveAction>,
    a: &AccountRecord,
    now: u64,
) -> Result<(), StoreError> {
    action.value.state = "consumed".into();
    tx.replace(
        &format!("action:{}", action.value.action_id),
        action.row_version,
        "action",
        Some(&a.account_id),
        Some(action.value.expires_at),
        &action.value,
    )?;
    let mut account = tx
        .get::<AccountRecord>(&format!("account:{}", a.account_id))?
        .ok_or(StoreError::Corrupt)?;
    account.value.policy_version += 1;
    tx.replace(
        &format!("account:{}", a.account_id),
        account.row_version,
        "account",
        Some(&a.account_id),
        None,
        &account.value,
    )?;
    audit(tx, &account.value, "email_factor_changed", now)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn service() -> IdentityService {
        IdentityService::new(super::super::config::IdentityConfig {
            namespace: IdentityNamespace {
                identity_service_id: "email-test".into(),
                environment_id: "isolated".into(),
            },
            rp: RpConfig {
                rp_id: "example.test".into(),
                allowed_origins: vec!["https://example.test".into()],
                version: 1,
                identity_instance_epoch: 1,
            },
            sqlite_path: std::env::temp_dir()
                .join(format!("oasis7-email-{}.sqlite", uuid::Uuid::new_v4())),
            registration_enabled: true,
            machine_registration_enabled: false,
            session_key: [7; 32],
            world_scope: "world:test".into(),
            runtime_live_bind: "127.0.0.1:5023".into(),
            tablestore: None,
            trusted_proxy_loopback: false,
        })
        .unwrap()
    }
    fn issued(s: &mut IdentityService, owner: &str, at: u64) -> (String, String) {
        let result = s
            .issue_email_otp(
                "login",
                owner,
                " A+Tag@Example.test ",
                None,
                None,
                "test-source",
                at,
            )
            .unwrap();
        let body = &s.email.delivered.last().unwrap().2;
        let code = body.split("验证码：").nth(1).unwrap()[..6].to_owned();
        (result["otp_id"].as_str().unwrap().into(), code)
    }
    #[test]
    fn otp_context_purpose_expiry_and_failure_limit_are_persistent() {
        let mut s = service();
        let (id, code) = issued(&mut s, "owner-a", 1000);
        assert!(
            s.otp_precheck(&id, &code, "login", "other-browser", None, None, 1001)
                .is_err()
        );
        assert!(
            s.otp_precheck(&id, &code, "recovery_start", "owner-a", None, None, 1001)
                .is_err()
        );
        assert!(
            s.otp_precheck(&id, &code, "login", "owner-a", None, None, 121000)
                .is_err()
        );
        let wrong_code = if code == "999999" { "000000" } else { "999999" };
        for _ in 0..5 {
            assert!(
                s.otp_precheck(&id, wrong_code, "login", "owner-a", None, None, 1002)
                    .is_err()
            );
        }
        let otp = s
            .store
            .transaction(|tx| tx.get::<EmailOtp>(&format!("otp:{id}")))
            .unwrap()
            .unwrap()
            .value;
        assert_eq!(otp.failed_attempts, 5);
        assert_eq!(otp.state, "exhausted");
        assert!(otp.verifier.is_empty());
        assert!(
            s.otp_precheck(&id, &code, "login", "owner-a", None, None, 1003)
                .is_err()
        );
    }
    #[test]
    fn email_login_preserves_account_when_registration_closed_and_does_not_allocate_world() {
        let mut s = service();
        let (id, code) = issued(&mut s, "owner", 1000);
        let payload = json!({"otp_id":id,"code":code,"operation_id":id});
        let (body, token) = s
            .email_login("owner", &payload, "test-source", 1001)
            .unwrap();
        let account = body["account"]["account_id"].as_str().unwrap().to_owned();
        assert_eq!(body["world"]["state"], "not_requested");
        assert!(token.is_some());
        let (retry, reissued) = s
            .email_login("owner", &payload, "test-source", 1002)
            .unwrap();
        assert!(reissued.is_none());
        assert_eq!(retry["phase"], "operation_committed");
        assert_eq!(retry["next_action"], "fresh_authentication");
        let different =
            json!({"otp_id":id,"code":if code=="999999"{"000000"}else{"999999"},"operation_id":id});
        assert!(
            matches!(s.email_login("owner",&different,"test-source",1002),Err(StoreError::Rejected(code)) if code=="idempotency_conflict")
        );
        s.config.registration_enabled = false;
        let (id, code) = issued(&mut s, "owner", 62000);
        let (body, token) = s
            .email_login(
                "owner",
                &json!({"otp_id":id,"code":code,"operation_id":id}),
                "test-source",
                62001,
            )
            .unwrap();
        assert_eq!(body["account"]["account_id"], account);
        let (a, session) = s.session(token.as_deref(), 62001).unwrap();
        assert!(!a.has_ever_public_key);
        assert_eq!(session.auth_method, "email");
        assert!(
            s.store
                .transaction(|tx| tx.list::<PlayerBinding>(
                    "player_binding",
                    Some(&account),
                    None,
                    10
                ))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn factor_encryption_authenticates_ciphertext_and_preserves_historical_normalization() {
        let e = EmailAdapter::from_env(&[7; 32]).unwrap();
        let address = normalize_email("A.B+Tag@Example.test").unwrap();
        assert_eq!(address, "a.b+tag@example.test");
        let cipher = e.encrypt(&address).unwrap();
        assert!(!cipher.contains(&address));
        assert_eq!(e.decrypt(&cipher).unwrap(), address);
        assert!(
            EmailAdapter::from_env(&[8; 32])
                .unwrap()
                .decrypt(&cipher)
                .is_err()
        );
        assert_ne!(e.lookup("ab@example.test"), e.lookup(&address));
    }
}
