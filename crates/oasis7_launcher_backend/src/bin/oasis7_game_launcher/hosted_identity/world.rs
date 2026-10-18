//! Account-cookie authorization and control fencing are separate from slot accounting.
use super::{
    model::*,
    now_ms, random_token, reject,
    service::{IdentityService, session_key, valid_session},
    store::StoreError,
};
use crate::hosted_player_session::HostedPlayerSessionIssuer;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use oasis7_client_api::viewer_auth::{HostedAuthContextV2, issue_hosted_authorization_grant};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[path = "world_runtime.rs"]
mod runtime;

#[derive(Clone, Serialize, Deserialize)]
struct WorldLease {
    parent_session_id: String,
    public_key: String,
    device_session_id: String,
    join_attempt_id: String,
    world_epoch: u64,
    created_at: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct WorldControl {
    account_id: String,
    player_id: String,
    world_id: String,
    encrypted_slot_token: String,
    current: Option<WorldLease>,
    pending: Option<WorldLease>,
    releasing: bool,
    retired_attempts: std::collections::BTreeMap<String, u64>,
    last_release: Option<WorldLease>,
}

impl IdentityService {
    /// Authenticated, read-only fencing metadata for the takeover confirmation.
    pub fn world_control_epoch(&mut self, sid: Option<&str>) -> Option<u64> {
        let (account, _) = self.session(sid, now_ms()).ok()?;
        let world_id = std::env::var("OASIS7_HOSTED_WORLD_ID").ok()?;
        let key = format!("binding:{}:{}", account.account_id, self.config.world_scope);
        let binding = self
            .store
            .transaction(|tx| tx.get::<PlayerBinding>(&key))
            .ok()??;
        if binding.value.account_id != account.account_id
            || binding.value.world_identity_scope != self.config.world_scope
        {
            return None;
        }
        runtime::query(
            &self.config.runtime_live_bind,
            &world_id,
            &binding.value.player_id,
        )
        .ok()
        .map(|fence| fence.epoch)
    }
    /// Caller must first execute the HTTP Origin/CSRF/JSON guards in `handle`.
    pub fn world_route(
        &mut self,
        method: &str,
        relative: &str,
        sid: Option<&str>,
        p: &Value,
        issuer: &mut HostedPlayerSessionIssuer,
        now: u64,
    ) -> Option<Result<Value, StoreError>> {
        if !matches!(
            relative,
            "/world/join" | "/world/refresh" | "/world/release"
        ) {
            return None;
        }
        if method != "POST" {
            return Some(Err(reject("method_not_allowed")));
        }
        Some(self.world_operation(relative, sid, p, issuer, now))
    }

    fn world_operation(
        &mut self,
        route: &str,
        sid: Option<&str>,
        p: &Value,
        issuer: &mut HostedPlayerSessionIssuer,
        checked_at: u64,
    ) -> Result<Value, StoreError> {
        if p.get("player_id").is_some()
            || p.get("account_id").is_some()
            || p.get("release_token").is_some()
        {
            return Err(reject("invalid_request"));
        }
        let (account, session) = self.session(sid, checked_at)?;
        if route == "/world/join"
            && std::env::var("OASIS7_HOSTED_WORLD_ADMISSION_MODE")
                .ok()
                .as_deref()
                != Some("open_preview")
        {
            return Err(reject("world_admission_unavailable"));
        }
        let world_id = std::env::var("OASIS7_HOSTED_WORLD_ID")
            .ok()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| reject("world_configuration_missing"))?;
        let binding_key = format!("binding:{}:{}", account.account_id, self.config.world_scope);
        let binding = self.store.transaction(|tx| {
            if let Some(binding) = tx.get::<PlayerBinding>(&binding_key)? {
                if binding.value.account_id != account.account_id
                    || binding.value.world_identity_scope != self.config.world_scope
                {
                    return Err(StoreError::Corrupt);
                }
                Ok(binding.value)
            } else {
                let binding = PlayerBinding {
                    account_id: account.account_id.clone(),
                    world_identity_scope: self.config.world_scope.clone(),
                    player_id: format!("hosted-player-{}", random_token()),
                    source: "identity_v2".into(),
                    binding_version: 1,
                };
                tx.create(
                    &binding_key,
                    "binding",
                    Some(&account.account_id),
                    None,
                    &binding,
                )?;
                Ok(binding)
            }
        })?;
        let control_key = format!(
            "world-control:{}:{}",
            account.account_id, self.config.world_scope
        );
        let fence = runtime::query(
            &self.config.runtime_live_bind,
            &world_id,
            &binding.player_id,
        )?;
        let public_key = p["public_key"]
            .as_str()
            .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| reject("invalid_session_key"))?
            .to_ascii_lowercase();
        let mut control = self.store.transaction(|tx| {
            let token_key = session_key(
                &self.config.session_key,
                sid.ok_or_else(|| reject("session_expired"))?,
            );
            valid_session(
                tx,
                &token_key,
                self.config.rp.identity_instance_epoch,
                checked_at,
            )?;
            let old = tx.get::<WorldControl>(&control_key)?;
            let mut control = if let Some(old) = &old {
                old.value.clone()
            } else {
                let token = random_token();
                WorldControl {
                    account_id: account.account_id.clone(),
                    player_id: binding.player_id.clone(),
                    world_id: world_id.clone(),
                    encrypted_slot_token: seal_slot_token(
                        &self.config.session_key,
                        &control_key,
                        &token,
                    )?,
                    current: None,
                    pending: None,
                    releasing: false,
                    retired_attempts: Default::default(),
                    last_release: None,
                }
            };
            if control.account_id != account.account_id
                || control.player_id != binding.player_id
                || control.world_id != world_id
            {
                return Err(StoreError::Corrupt);
            }
            // Completion comes from the runtime itself, never from a browser ack.
            if control.pending.as_ref().is_some_and(|pending| {
                fence.key.as_deref() == Some(pending.public_key.as_str())
                    && fence.epoch == pending.world_epoch
            }) {
                if let Some(old) = control.current.take() {
                    control.retired_attempts.insert(
                        old.join_attempt_id,
                        checked_at.saturating_add(7 * 86_400_000),
                    );
                }
                control.current = control.pending.take();
            }
            if route == "/world/join" {
                prepare_join(&mut control, &session, p, &public_key, &fence, checked_at)?;
            } else {
                validate_control_request(&control, &session, p, &public_key)?;
                let current = control
                    .current
                    .as_ref()
                    .or_else(|| {
                        if route == "/world/release" && control.pending.is_none() {
                            control.last_release.as_ref()
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| reject("world_registration_pending"))?;
                if (fence.key.as_deref() != Some(current.public_key.as_str())
                    || fence.epoch != current.world_epoch)
                    && !(route == "/world/release"
                        && (control.releasing || control.current.is_none())
                        && fence.key.is_none()
                        && fence.epoch == current.world_epoch.saturating_add(1))
                {
                    return Err(reject("control_replaced"));
                }
                if route == "/world/release" {
                    if control.current.is_some() {
                        control.releasing = true;
                    }
                } else if control.releasing {
                    return Err(reject("world_release_pending"));
                }
            }
            if let Some(old) = old {
                tx.replace(
                    &control_key,
                    old.row_version,
                    "world_control",
                    Some(&account.account_id),
                    None,
                    &control,
                )?;
            } else {
                tx.create(
                    &control_key,
                    "world_control",
                    Some(&account.account_id),
                    None,
                    &control,
                )?;
            }
            Ok(control)
        })?;
        let token = open_slot_token(
            &self.config.session_key,
            &control_key,
            &control.encrypted_slot_token,
        )?;
        if route == "/world/release" {
            if control.current.is_none() {
                return Ok(json!({"api_version":2,"ok":true,"world":{"state":"released"}}));
            }
            let lease = control
                .current
                .as_ref()
                .ok_or_else(|| reject("control_not_found"))?;
            if fence.key.is_some() {
                let grant = self.authorization(sid, &control, lease, checked_at, true)?;
                runtime::release(
                    &self.config.runtime_live_bind,
                    &world_id,
                    grant,
                    &binding.player_id,
                    lease.world_epoch,
                )?;
            }
            issuer
                .release_v2_slot(&binding.player_id, &token)
                .map_err(|e| reject(&e))?;
            self.store.transaction(|tx| {
                valid_session(
                    tx,
                    &session_key(
                        &self.config.session_key,
                        sid.ok_or_else(|| reject("session_expired"))?,
                    ),
                    self.config.rp.identity_instance_epoch,
                    checked_at,
                )?;
                let old = tx
                    .get::<WorldControl>(&control_key)?
                    .ok_or(StoreError::Corrupt)?;
                if let Some(old) = control.current.take() {
                    control.last_release = Some(old.clone());
                    control.retired_attempts.insert(
                        old.join_attempt_id,
                        checked_at.saturating_add(7 * 86_400_000),
                    );
                }
                if let Some(old) = control.pending.take() {
                    control.retired_attempts.insert(
                        old.join_attempt_id,
                        checked_at.saturating_add(7 * 86_400_000),
                    );
                }
                control.releasing = false;
                tx.replace(
                    &control_key,
                    old.row_version,
                    "world_control",
                    Some(&account.account_id),
                    None,
                    &control,
                )
            })?;
            return Ok(json!({"api_version":2,"ok":true,"world":{"state":"released"}}));
        }
        issuer
            .reserve_v2_slot(&binding.player_id, &token, checked_at)
            .map_err(|e| reject(&e))?;
        let lease = control
            .pending
            .as_ref()
            .filter(|v| v.parent_session_id == session.session_id && v.public_key == public_key)
            .or(control.current.as_ref())
            .ok_or_else(|| reject("control_not_found"))?;
        let grant = self.authorization(sid, &control, lease, checked_at, false)?;
        let bound = fence.key.as_deref() == Some(lease.public_key.as_str())
            && fence.epoch == lease.world_epoch;
        Ok(
            json!({"api_version":2,"ok":true,"phase":"world_authorized","account":{"account_id":account.account_id},"world":{"state":if bound{"bound"}else{"registration_pending"}},"grant":{"player_id":binding.player_id,"device_session_id":lease.device_session_id,"world_session_epoch":lease.world_epoch,"registration_grant":grant},"join_attempt_id":lease.join_attempt_id}),
        )
    }

    fn authorization(
        &mut self,
        sid: Option<&str>,
        control: &WorldControl,
        lease: &WorldLease,
        checked_at: u64,
        release: bool,
    ) -> Result<String, StoreError> {
        let (account, session) = self.session(sid, now_ms())?;
        if account.account_id != control.account_id || session.session_id != lease.parent_session_id
        {
            return Err(reject("control_replaced"));
        }
        let issued_at = now_ms();
        let expires_at = checked_at
            .saturating_add(30_000)
            .min(session.expires_at)
            .min(session.idle_expires_at);
        let context = HostedAuthContextV2 {
            version: 2,
            identity_service_id: self.config.namespace.identity_service_id.clone(),
            environment_id: self.config.namespace.environment_id.clone(),
            identity_instance_epoch: self.config.rp.identity_instance_epoch,
            account_id: account.account_id,
            account_session_id: session.session_id,
            credential_record_id: session.origin_credential_id,
            credential_version: session.origin_credential_version,
            account_security_epoch: session.issued_security_epoch,
            auth_method: session.auth_method,
            auth_time: session.auth_time,
            recovery_context: session.recovery_context,
            world_identity_scope: self.config.world_scope.clone(),
            world_id: control.world_id.clone(),
            player_id: control.player_id.clone(),
            device_session_id: lease.device_session_id.clone(),
            session_public_key: lease.public_key.clone(),
            world_session_epoch: lease.world_epoch,
            capabilities: vec![
                if release {
                    "player_session_release"
                } else {
                    "player_session"
                }
                .into(),
            ],
            checked_at,
            issued_at,
            expires_at,
            nonce: random_token(),
            issuer_key_id: "hosted-identity-v2".into(),
        };
        let private = std::env::var("OASIS7_HOSTED_AUTHORIZATION_ISSUER_PRIVATE_KEY")
            .map_err(|_| reject("authorization_issuer_missing"))?;
        issue_hosted_authorization_grant(&context, &private)
            .map_err(|_| reject("authorization_issue_failed"))
    }
}

fn prepare_join(
    control: &mut WorldControl,
    session: &AccountSession,
    p: &Value,
    key: &str,
    fence: &runtime::RuntimeFence,
    now: u64,
) -> Result<(), StoreError> {
    if control.releasing {
        return Err(reject("world_release_pending"));
    }
    let attempt = p["join_attempt_id"]
        .as_str()
        .filter(|v| {
            v.len() == 43
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .ok_or_else(|| reject("invalid_join_attempt"))?;
    control.retired_attempts.retain(|_, expiry| *expiry > now);
    if control.retired_attempts.contains_key(attempt) {
        return Err(reject("join_attempt_retired"));
    }
    if let Some(existing) = control
        .pending
        .as_ref()
        .or(control.current.as_ref())
        .filter(|v| v.join_attempt_id == attempt)
    {
        if existing.parent_session_id != session.session_id || existing.public_key != key {
            return Err(reject("idempotency_conflict"));
        }
        return Ok(());
    }
    if let Some(pending) = &control.pending
        && now < pending.created_at.saturating_add(30_000)
    {
        return Err(reject("operation_pending"));
    }
    if fence.key.is_some() {
        if p["takeover"].as_bool() != Some(true) {
            return Err(reject("control_elsewhere"));
        }
        if p["expected_epoch"].as_u64() != Some(fence.epoch) {
            return Err(reject("state_changed"));
        }
        if now.saturating_sub(session.auth_time) > 300_000 {
            return Err(reject("reauth_required"));
        }
        if fence.key.as_deref() == Some(key) {
            return Err(reject("new_control_key_required"));
        }
    } else if p["expected_epoch"]
        .as_u64()
        .is_some_and(|e| e != fence.epoch)
    {
        return Err(reject("state_changed"));
    }
    let epoch = fence
        .epoch
        .checked_add(1)
        .ok_or_else(|| reject("world_epoch_exhausted"))?;
    if let Some(old) = control.pending.take() {
        control
            .retired_attempts
            .insert(old.join_attempt_id, now.saturating_add(7 * 86_400_000));
    }
    control.pending = Some(WorldLease {
        parent_session_id: session.session_id.clone(),
        public_key: key.into(),
        device_session_id: random_token(),
        join_attempt_id: attempt.into(),
        world_epoch: epoch,
        created_at: now,
    });
    Ok(())
}

fn validate_control_request(
    control: &WorldControl,
    session: &AccountSession,
    p: &Value,
    key: &str,
) -> Result<(), StoreError> {
    let lease = control
        .current
        .as_ref()
        .or_else(|| {
            if control.pending.is_none() {
                control.last_release.as_ref()
            } else {
                None
            }
        })
        .ok_or_else(|| reject("world_registration_pending"))?;
    if lease.parent_session_id != session.session_id
        || lease.public_key != key
        || p["device_session_id"].as_str() != Some(lease.device_session_id.as_str())
        || p["expected_epoch"].as_u64() != Some(lease.world_epoch)
    {
        return Err(reject("control_replaced"));
    }
    Ok(())
}

fn seal_slot_token(secret: &[u8; 32], aad: &str, token: &str) -> Result<String, StoreError> {
    let key = blake3::keyed_hash(secret, b"hosted-v2-slot-encryption");
    let cipher =
        XChaCha20Poly1305::new_from_slice(key.as_bytes()).map_err(|_| StoreError::Corrupt)?;
    let mut nonce = [0; 24];
    getrandom::fill(&mut nonce)
        .map_err(|_| StoreError::Unavailable("random source unavailable".into()))?;
    let nonce = XNonce::from(nonce);
    let encrypted = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: token.as_bytes(),
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| StoreError::Corrupt)?;
    Ok(format!("{}.{}", hex::encode(nonce), hex::encode(encrypted)))
}
fn open_slot_token(secret: &[u8; 32], aad: &str, encoded: &str) -> Result<String, StoreError> {
    let (nonce, bytes) = encoded.split_once('.').ok_or(StoreError::Corrupt)?;
    let nonce: [u8; 24] = hex::decode(nonce)
        .map_err(|_| StoreError::Corrupt)?
        .try_into()
        .map_err(|_| StoreError::Corrupt)?;
    let bytes = hex::decode(bytes).map_err(|_| StoreError::Corrupt)?;
    let key = blake3::keyed_hash(secret, b"hosted-v2-slot-encryption");
    let cipher =
        XChaCha20Poly1305::new_from_slice(key.as_bytes()).map_err(|_| StoreError::Corrupt)?;
    let nonce = XNonce::from(nonce);
    let plain = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &bytes,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| StoreError::Corrupt)?;
    String::from_utf8(plain).map_err(|_| StoreError::Corrupt)
}

#[cfg(test)]
#[path = "world_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "world_runtime_timeout_tests.rs"]
mod runtime_timeout_tests;
