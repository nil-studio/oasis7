//! Email recovery fixes the candidate before waiting and replaces credentials atomically.
use super::{
    digest,
    email::{consume_otp, otp_fields, read_identity_receipt, read_otp},
    model::*,
    random_token, reject,
    service::{
        IdentityService, active_keys, audit, authorized_action, check_ceremony, consume_quota,
        create_session, credential_key, session_key, valid_session,
    },
    store::{IdentityTransaction, StoreError, Versioned},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
const DAY: u64 = 86_400_000;
#[derive(Clone, Serialize, Deserialize)]
struct RecoveryContext {
    id: String,
    owner: String,
    account_id: String,
    email_id: String,
    email_version: u64,
    policy_version: u64,
    security_epoch: u64,
    email_verified_at: u64,
    expires_at: u64,
    candidate_id: Option<String>,
    candidate_digest: Option<String>,
    recovery_id: Option<String>,
    #[serde(default)]
    operation_id: Option<String>,
    #[serde(default)]
    cancel_operation_id: Option<String>,
}
impl IdentityService {
    #[expect(
        clippy::too_many_arguments,
        reason = "Recovery routing preserves authenticated session, ceremony owner, source quota and authoritative time separately."
    )]
    pub(super) fn recovery_route(
        &mut self,
        method: &str,
        path: &str,
        sid: Option<&str>,
        owner: &str,
        p: &Value,
        source: &str,
        now: u64,
    ) -> Option<Result<(Value, Option<String>), StoreError>> {
        if !(path == "/recovery" || path.starts_with("/recovery/")) {
            return None;
        }
        Some(self.recovery_dispatch(method, path, sid, owner, p, source, now))
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Recovery dispatch must retain every route authentication and rate-limit binding without deriving one from another."
    )]
    fn recovery_dispatch(
        &mut self,
        method: &str,
        path: &str,
        sid: Option<&str>,
        owner: &str,
        p: &Value,
        source: &str,
        now: u64,
    ) -> Result<(Value, Option<String>), StoreError> {
        if !self.email.recovery_enabled {
            return Err(reject("recovery_unavailable"));
        }
        let parts: Vec<_> = path.trim_matches('/').split('/').collect();
        if method == "POST" && path == "/recovery/eligibility/options" {
            let address = p["email"]
                .as_str()
                .ok_or_else(|| reject("invalid_request"))?;
            let target = p["recovery_id"].as_str();
            return self
                .issue_email_otp("recovery_start", owner, address, None, target, source, now)
                .map(|v| (v, None));
        }
        if method == "POST" && path == "/recovery/eligibility" {
            return self.recovery_eligibility(owner, p, now).map(|v| (v, None));
        }
        if method == "POST" && path == "/recovery/candidate/options" {
            return self
                .recovery_candidate_options(owner, p, now)
                .map(|v| (v, None));
        }
        if method == "POST" && path == "/recovery/candidate/complete" {
            return self
                .recovery_candidate_complete(owner, p, now)
                .map(|v| (v, None));
        }
        if method == "POST" && path == "/recovery" {
            return self
                .recovery_create(owner, p, source, now)
                .map(|v| (v, None));
        }
        if parts.len() < 2 || parts[1].len() != 43 {
            return Err(reject("invalid_request"));
        }
        let id = parts[1];
        if method == "GET" && parts.len() == 2 {
            let context_id = p["context_id"].as_str();
            let holder = self.session(sid, now).ok().map(|(a, _)| a.account_id);
            return self.store.transaction(|tx| {
                let r = tx
                    .get::<RecoveryRequest>(&format!("recovery:{id}"))?
                    .ok_or_else(|| reject("authentication_failed"))?;
                if holder.as_deref() != Some(r.value.account_id.as_str()) {
                    let context = recovery_context(
                        tx,
                        context_id.ok_or_else(|| reject("authentication_failed"))?,
                        owner,
                        now,
                    )?;
                    if context.value.recovery_id.as_deref() != Some(id)
                        || context.value.account_id != r.value.account_id
                    {
                        return Err(reject("authentication_failed"));
                    }
                }
                Ok((projection(&r), None))
            });
        }
        if method == "POST" && parts.len() == 3 && matches!(parts[2], "approve" | "cancel") {
            return self
                .recovery_decision(id, parts[2], sid, owner, p, now)
                .map(|v| (v, None));
        }
        if method == "POST" && parts.len() == 3 && parts[2] == "notification/retry" {
            return Err(reject("invalid_request"));
        }
        if method == "POST" && parts.len() == 4 && parts[2] == "completion" && parts[3] == "options"
        {
            return self
                .recovery_completion_options(id, owner, p, source, now)
                .map(|v| (v, None));
        }
        if method == "POST" && parts.len() == 3 && parts[2] == "complete" {
            return self.recovery_complete(id, owner, p, now);
        }
        Err(reject("route_not_available"))
    }
    fn recovery_eligibility(
        &mut self,
        owner: &str,
        p: &Value,
        now: u64,
    ) -> Result<Value, StoreError> {
        let (id, code) = otp_fields(p)?;
        let recovery_id = p["recovery_id"].as_str();
        self.otp_precheck(id, code, "recovery_start", owner, None, recovery_id, now)?;
        let email = &self.email;
        let context_id = random_token();
        self.store.transaction(|tx|{let mut otp=read_otp(tx,email,id,code,"recovery_start",owner,None,recovery_id,now)?;let l=tx.get::<CredentialLookup>(&format!("email_lookup:{}",otp.value.lookup))?.ok_or_else(||reject("authentication_failed"))?.value;let c=tx.get::<CredentialRecord>(&credential_key(&l.credential_record_id))?.ok_or(StoreError::Corrupt)?.value;let a=tx.get::<AccountRecord>(&format!("account:{}",l.account_id))?.ok_or(StoreError::Corrupt)?.value;check_email_recovery(&a,&c,now)?;let(mut candidate_id,mut candidate_digest)=(None,None);if let Some(rid)=recovery_id{let r=tx.get::<RecoveryRequest>(&format!("recovery:{rid}"))?.ok_or_else(||reject("authentication_failed"))?.value;check_recovery_state(&r,&a,&c,now)?;candidate_id=Some(r.candidate_credential_id);candidate_digest=Some(r.candidate_digest);}
  consume_otp(tx,&mut otp)?;let context=RecoveryContext{id:context_id.clone(),owner:owner.into(),account_id:a.account_id.clone(),email_id:c.credential_record_id,email_version:c.version,policy_version:a.policy_version,security_epoch:a.security_epoch,email_verified_at:now,expires_at:now+7*DAY,candidate_id,candidate_digest,recovery_id:recovery_id.map(str::to_owned),operation_id:recovery_id.map(str::to_owned),cancel_operation_id:Some(random_token())};tx.create(&format!("recovery_context:{context_id}"),"recovery_context",Some(&a.account_id),Some(context.expires_at),&context)?;Ok(json!({"api_version":2,"ok":true,"context_id":context_id,"eligible":true,"phase":"eligibility_verified","cancel_operation_id":context.cancel_operation_id}))})
    }
    fn recovery_candidate_options(
        &mut self,
        owner: &str,
        p: &Value,
        now: u64,
    ) -> Result<Value, StoreError> {
        let id = p["context_id"]
            .as_str()
            .ok_or_else(|| reject("invalid_request"))?;
        let (context, a) = self.store.transaction(|tx| {
            let c = recovery_context(tx, id, owner, now)?;
            if c.value.email_verified_at + 300_000 <= now
                || c.value.candidate_id.is_some()
                || c.value.recovery_id.is_some()
            {
                return Err(reject("state_changed"));
            }
            let a = tx
                .get::<AccountRecord>(&format!("account:{}", c.value.account_id))?
                .ok_or(StoreError::Corrupt)?
                .value;
            check_context_account(&c.value, &a)?;
            Ok((c.value, a))
        })?;
        let handle = uuid::Uuid::from_slice(&a.user_handle).map_err(|_| StoreError::Corrupt)?;
        let (options, state) = self
            .webauthn
            .registration_options(&handle.to_string(), &a.display_name, &[])
            .map_err(|_| reject("authentication_failed"))?;
        let cid = random_token();
        let ceremony = CeremonyRecord {
            ceremony_id: cid.clone(),
            kind: "recovery_candidate".into(),
            namespace: self.config.namespace.clone(),
            rp_config_version: self.config.rp.version,
            owner_context_digest: owner.into(),
            target_account_id: Some(a.account_id),
            target_digest: Some(context.id),
            expires_at: now + 120_000,
            failed_attempts: 0,
            status: "pending".into(),
            server_state: Some(state),
            result_ref: None,
        };
        self.store.transaction(|tx| {
            tx.create(
                &format!("ceremony:{cid}"),
                "ceremony",
                ceremony.target_account_id.as_deref(),
                Some(ceremony.expires_at),
                &ceremony,
            )
        })?;
        Ok(json!({"api_version":2,"ok":true,"ceremony_id":cid,"options":options}))
    }
    fn recovery_candidate_complete(
        &mut self,
        owner: &str,
        p: &Value,
        now: u64,
    ) -> Result<Value, StoreError> {
        let context_id = p["context_id"]
            .as_str()
            .ok_or_else(|| reject("invalid_request"))?;
        let ceremony_id = p["ceremony_id"]
            .as_str()
            .ok_or_else(|| reject("invalid_request"))?;
        let key = format!("ceremony:{ceremony_id}");
        let ceremony = self.store.transaction(|tx| {
            tx.get::<CeremonyRecord>(&key)?
                .ok_or_else(|| reject("authentication_failed"))
        })?;
        check_ceremony(&ceremony.value, owner, now)?;
        if ceremony.value.kind != "recovery_candidate"
            || ceremony.value.target_digest.as_deref() != Some(context_id)
        {
            return Err(reject("authentication_failed"));
        }
        let record = match self.webauthn.finish_registration(
            &p["response"],
            ceremony
                .value
                .server_state
                .as_ref()
                .ok_or(StoreError::Corrupt)?,
        ) {
            Ok(record) => record,
            Err(_) => {
                self.record_ceremony_failure(&key, owner, now)?;
                return Err(reject("authentication_failed"));
            }
        };
        let raw_id = self
            .webauthn
            .credential_id(&record)
            .map_err(|_| reject("authentication_failed"))?;
        let raw_bytes = URL_SAFE_NO_PAD
            .decode(&raw_id)
            .map_err(|_| reject("authentication_failed"))?;
        let cid = random_token();
        let candidate_digest = digest(
            &self.config.session_key,
            "recovery-candidate",
            &record.to_string(),
        );
        let config = &self.config;
        self.store.transaction(|tx|{let mut context=recovery_context(tx,context_id,owner,now)?;if context.value.email_verified_at+300_000<=now||context.value.candidate_id.is_some(){return Err(reject("state_changed"));}let a=tx.get::<AccountRecord>(&format!("account:{}",context.value.account_id))?.ok_or(StoreError::Corrupt)?.value;check_context_account(&context.value,&a)?;let mut ceremony=tx.get::<CeremonyRecord>(&key)?.ok_or(StoreError::Corrupt)?;check_ceremony(&ceremony.value,owner,now)?;for(key,mut previous)in tx.all::<CredentialRecord>("credential",Some(&a.account_id))?{if previous.value.state=="recovery_candidate"{if previous.value.created_at.saturating_add(7*DAY)>now{return Err(reject("recovery_in_progress"));}previous.value.state="revoked".into();previous.value.version+=1;tx.replace(&key,previous.row_version,"credential",Some(&a.account_id),None,&previous.value)?;}}let lkey=format!("lookup:passkey:{raw_id}");if tx.get::<CredentialLookup>(&lkey)?.is_some(){return Err(reject("credential_conflict"));}
   let candidate=CredentialRecord{credential_record_id:cid.clone(),account_id:a.account_id.clone(),state:"recovery_candidate".into(),version:1,label:"恢复通行密钥".into(),created_at:now,last_used_at:None,public_material:CredentialMaterial::WebAuthn{rp_id:config.rp.rp_id.clone(),credential_id:raw_bytes.clone(),user_handle:a.user_handle.clone(),library_record:record.clone(),library_record_version:1},registered_via:format!("recovery_context:{context_id}")};tx.create(&credential_key(&cid),"credential",Some(&a.account_id),None,&candidate)?;tx.create(&lkey,"lookup",Some(&a.account_id),None,&CredentialLookup{scope:config.rp.rp_id.clone(),key_digest:blake3::hash(&raw_bytes).to_hex().to_string(),canonical_key:raw_bytes,credential_record_id:cid.clone(),account_id:a.account_id.clone(),state:"recovery_candidate".into()})?;context.value.candidate_id=Some(cid.clone());context.value.candidate_digest=Some(candidate_digest.clone());context.value.operation_id=Some(ceremony_id.into());tx.replace(&format!("recovery_context:{context_id}"),context.row_version,"recovery_context",Some(&a.account_id),Some(context.value.expires_at),&context.value)?;ceremony.value.status="consumed".into();ceremony.value.server_state=None;tx.replace(&key,ceremony.row_version,"ceremony",Some(&a.account_id),Some(ceremony.value.expires_at),&ceremony.value)?;Ok(json!({"api_version":2,"ok":true,"phase":"candidate_prepared","candidate_digest":candidate_digest,"operation_id":ceremony_id}))})
    }
    fn recovery_create(
        &mut self,
        owner: &str,
        p: &Value,
        source: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        let context_id = p["context_id"]
            .as_str()
            .ok_or_else(|| reject("invalid_request"))?;
        let id = p["operation_id"]
            .as_str()
            .filter(|v| v.len() == 43)
            .ok_or_else(|| reject("invalid_request"))?;
        let secret = self.config.session_key;
        let request_digest = digest(&secret, "recovery-create", &p.to_string());
        if let Some(value) = self.committed_identity_receipt(id, owner, &request_digest)? {
            return Ok(value);
        }

        let email = &self.email;
        let response = self.store.transaction(|tx| {
            if let Some(value) = read_identity_receipt(tx, id, owner, &request_digest)? {
                return Ok(value);
            }
            let mut context = recovery_context(tx, context_id, owner, now)?;
            if context.value.operation_id.as_deref() != Some(id) {
                return Err(reject("invalid_request"));
            }
            if context.value.email_verified_at + 300_000 <= now {
                return Err(reject("reauth_required"));
            }
            let a = tx
                .get::<AccountRecord>(&format!("account:{}", context.value.account_id))?
                .ok_or(StoreError::Corrupt)?
                .value;
            check_context_account(&context.value, &a)?;
            let c = tx
                .get::<CredentialRecord>(&credential_key(&context.value.email_id))?
                .ok_or(StoreError::Corrupt)?
                .value;
            check_email_recovery(&a, &c, now)?;
            let cid = context.value.candidate_id.as_ref()
                .ok_or_else(|| reject("candidate_required"))?;
            let digest = context.value.candidate_digest.as_ref().ok_or(StoreError::Corrupt)?;
            if let Some(rid) = &context.value.recovery_id {
                if rid != id {
                    return Err(reject("state_changed"));
                }
                let existing = tx
                    .get::<RecoveryRequest>(&format!("recovery:{id}"))?
                    .ok_or(StoreError::Corrupt)?;
                return Ok(projection(&existing));
            }
            if tx.all::<RecoveryRequest>("recovery", Some(&a.account_id))?
                .iter().any(|(_, r)| {
                    matches!(r.value.state.as_str(), "waiting" | "approved")
                        && r.value.expires_at > now
                })
            {
                return Err(reject("recovery_in_progress"));
            }
            consume_quota(tx, "recovery-source", &super::digest(&secret, "source", source), now, DAY, 5)?;
            consume_quota(tx, "recovery-account", &a.account_id, now, DAY, 2)?;
            let r = RecoveryRequest {
                recovery_id: id.into(),
                account_id: a.account_id.clone(),
                email_credential_id: c.credential_record_id,
                email_credential_version: c.version,
                candidate_credential_id: cid.clone(),
                candidate_digest: digest.clone(),
                account_policy_version_at_start: a.policy_version,
                security_epoch_at_start: a.security_epoch,
                created_at: now,
                not_before: now + DAY,
                expires_at: now + 7 * DAY,
                state: "waiting".into(),
                approved_by: None,
                notification_accepted_at: None,
                result_ref: None,
            };
            tx.create(&format!("recovery:{id}"), "recovery", Some(&a.account_id), Some(r.expires_at), &r)?;
            context.value.recovery_id = Some(id.into());
            tx.replace(
                &format!("recovery_context:{context_id}"), context.row_version,
                "recovery_context", Some(&a.account_id), Some(context.value.expires_at), &context.value,
            )?;
            tx.create(
                &format!("outbox:recovery-notice:{id}"), "recovery_notice", Some(&a.account_id), Some(now + 90 * DAY),
                &OutboxEvent {
                    event_id: format!("recovery-notice:{id}"),
                    account_id: a.account_id.clone(),
                    event_type: "recovery_requested".into(),
                    state_version: a.policy_version,
                    safe_payload: json!({"recovery_id": id, "candidate_digest": digest}),
                    attempts: 0,
                    delivery_state: "pending".into(),
                },
            )?;
            tx.create(
                &format!("receipt:{id}"), "receipt", Some(&a.account_id), Some(now + 7 * DAY),
                &OperationReceipt {
                    operation_id: id.into(),
                    kind: "recovery_create".into(),
                    owner_context_digest: owner.into(),
                    request_digest: request_digest.clone(),
                    state: "committed".into(),
                    result_ids: vec![a.account_id.clone(), id.into()],
                    committed_at: now,
                },
            )?;
            audit(tx, &a, "recovery_requested", now)?;
            let _ = email;
            Ok(json!({"api_version": 2, "ok": true, "recovery_id": id, "state": "waiting", "not_before": r.not_before, "expires_at": r.expires_at}))
        })?;
        // SMTP acceptance is recorded after the durable request, never before commit.
        let _ = self.send_recovery_notification(id, now);
        Ok(response)
    }
    /// Called by the bounded background worker, never by a GET request.
    pub(super) fn retry_recovery_notifications(&mut self, now: u64) -> Result<(), StoreError> {
        if !self.email.available() {
            return Ok(());
        }
        let next = self.store.transaction(|tx| {
            Ok(tx
                .list::<OutboxEvent>("recovery_notice", None, None, 100)?
                .into_iter()
                .find_map(|(_, event)| {
                    let e = event.value;
                    if matches!(
                        e.event_type.as_str(),
                        "recovery_requested" | "recovery_completed"
                    ) && e.delivery_state == "pending"
                        && e.safe_payload["next_attempt_at"].as_u64().unwrap_or(0) <= now
                    {
                        e.safe_payload["recovery_id"]
                            .as_str()
                            .map(|id| (id.to_owned(), e.event_type == "recovery_completed"))
                    } else {
                        None
                    }
                }))
        })?;
        if let Some((id, completed)) = next {
            if completed {
                let _ = self.send_completed_recovery_notification(&id, now);
            } else {
                let _ = self.send_recovery_notification(&id, now);
            }
        }
        Ok(())
    }
    fn send_completed_recovery_notification(
        &mut self,
        id: &str,
        now: u64,
    ) -> Result<(), StoreError> {
        let (r, c) = self.store.transaction(|tx| {
            let r = tx
                .get::<RecoveryRequest>(&format!("recovery:{id}"))?
                .ok_or(StoreError::Corrupt)?
                .value;
            if r.state != "completed" {
                return Err(reject("state_changed"));
            }
            let c = tx
                .get::<CredentialRecord>(&credential_key(&r.email_credential_id))?
                .ok_or(StoreError::Corrupt)?
                .value;
            Ok((r, c))
        })?;
        let address = match &c.public_material {
            CredentialMaterial::Email {
                encrypted_address, ..
            } if c.state == "active" && c.version == r.email_credential_version => {
                Some(self.email.decrypt(encrypted_address)?)
            }
            _ => None,
        };
        let delivered = if let Some(address) = address {
            self.email.send(&address,"Oasis7 账号恢复已完成",&format!("恢复申请：{id}\n账号恢复已完成，旧通行密钥和旧登录会话已失效。若不是你发起，请使用受限支持渠道核查。"))
        } else {
            Ok(())
        };
        self.store.transaction(|tx| {
            let key = format!("outbox:recovery-complete:{id}");
            let mut e = tx.get::<OutboxEvent>(&key)?.ok_or(StoreError::Corrupt)?;
            e.value.attempts += 1;
            e.value.safe_payload["next_attempt_at"] =
                json!(now + 30_000 * (1u64 << e.value.attempts.min(5)));
            if delivered.is_ok() {
                e.value.delivery_state = "accepted_or_channel_removed".into();
            }
            tx.replace(
                &key,
                e.row_version,
                if delivered.is_ok() {
                    "recovery_notice_done"
                } else {
                    "recovery_notice"
                },
                Some(&e.value.account_id),
                Some(now + 90 * DAY),
                &e.value,
            )
        })?;
        delivered
    }
    pub(super) fn send_recovery_notification(
        &mut self,
        id: &str,
        now: u64,
    ) -> Result<(), StoreError> {
        let (r, c) = self.store.transaction(|tx| {
            let r = tx
                .get::<RecoveryRequest>(&format!("recovery:{id}"))?
                .ok_or(StoreError::Corrupt)?
                .value;
            if !matches!(r.state.as_str(), "waiting" | "approved") || r.expires_at <= now {
                return Err(reject("recovery_expired"));
            }
            let c = tx
                .get::<CredentialRecord>(&credential_key(&r.email_credential_id))?
                .ok_or(StoreError::Corrupt)?
                .value;
            Ok((r, c))
        })?;
        if r.notification_accepted_at.is_some() {
            return Ok(());
        }
        let address = if let CredentialMaterial::Email {
            encrypted_address, ..
        } = c.public_material
        {
            self.email.decrypt(&encrypted_address)?
        } else {
            return Err(StoreError::Corrupt);
        };
        let delivered=self.email.send(&address,"Oasis7 账号恢复申请",&format!("恢复申请：{}\n固定候选：{}\n若不是你发起，请使用已有通行密钥登录并取消。邮件服务接受后至少等待24小时；这封邮件不包含登录链接或验证码。",r.recovery_id,r.candidate_digest));
        #[cfg(not(test))]
        let accepted_at = now.max(super::now_ms());
        #[cfg(test)]
        let accepted_at = now;
        self.store.transaction(|tx| {
            let mut current = tx
                .get::<RecoveryRequest>(&format!("recovery:{id}"))?
                .ok_or(StoreError::Corrupt)?;
            if !matches!(current.value.state.as_str(), "waiting" | "approved") {
                return Ok(());
            }
            let mut outbox = tx
                .get::<OutboxEvent>(&format!("outbox:recovery-notice:{id}"))?
                .ok_or(StoreError::Corrupt)?;
            outbox.value.attempts += 1;
            let delay = (30_000u64).saturating_mul(1u64 << outbox.value.attempts.min(5));
            outbox.value.safe_payload["next_attempt_at"] = json!(now.saturating_add(delay));
            if delivered.is_ok() {
                outbox.value.delivery_state = "accepted".into();
                if current.value.notification_accepted_at.is_none() {
                    current.value.notification_accepted_at = Some(accepted_at);
                    current.value.not_before = current.value.not_before.max(accepted_at + DAY);
                    tx.replace(
                        &format!("recovery:{id}"),
                        current.row_version,
                        "recovery",
                        Some(&current.value.account_id),
                        Some(current.value.expires_at),
                        &current.value,
                    )?;
                }
            }
            tx.replace(
                &format!("outbox:recovery-notice:{id}"),
                outbox.row_version,
                if delivered.is_ok() {
                    "recovery_notice_done"
                } else {
                    "recovery_notice"
                },
                Some(&outbox.value.account_id),
                Some(now + 90 * DAY),
                &outbox.value,
            )?;
            Ok(())
        })?;
        delivered
    }
    fn recovery_decision(
        &mut self,
        id: &str,
        decision: &str,
        sid: Option<&str>,
        owner: &str,
        p: &Value,
        now: u64,
    ) -> Result<Value, StoreError> {
        let operation_id = p["operation_id"]
            .as_str()
            .filter(|v| v.len() == 43)
            .ok_or_else(|| reject("invalid_request"))?;
        let request_digest = digest(
            &self.config.session_key,
            "recovery-decision",
            &p.to_string(),
        );
        if let Some(value) =
            self.committed_identity_receipt(operation_id, owner, &request_digest)?
        {
            return Ok(value);
        }
        let holder = self.session(sid, now).ok();
        let skey = sid.map(|v| session_key(&self.config.session_key, v));
        let epoch = self.config.rp.identity_instance_epoch;
        self.store.transaction(|tx| {
            if let Some(value) = read_identity_receipt(tx, operation_id, owner, &request_digest)? {
                return Ok(value);
            }
            let key = format!("recovery:{id}");
            let mut r = tx
                .get::<RecoveryRequest>(&key)?
                .ok_or_else(|| reject("authentication_failed"))?;
            if r.row_version != p["expected_version"].as_u64().unwrap_or(0)
                || !matches!(r.value.state.as_str(), "waiting" | "approved")
                || r.value.expires_at <= now
            {
                return Err(reject("state_changed"));
            }
            let a = tx
                .get::<AccountRecord>(&format!("account:{}", r.value.account_id))?
                .ok_or(StoreError::Corrupt)?
                .value;
            let target = format!("{id}:{}:{}", r.value.candidate_digest, r.row_version);
            if let Some((holder, _)) = &holder {
                if holder.account_id != a.account_id {
                    return Err(reject("authentication_failed"));
                }
                let (a, s) = valid_session(
                    tx,
                    skey.as_ref().ok_or_else(|| reject("session_expired"))?,
                    epoch,
                    now,
                )?;
                let action_id = p["action_id"]
                    .as_str()
                    .ok_or_else(|| reject("reauth_required"))?;
                if operation_id != action_id {
                    return Err(reject("invalid_request"));
                }
                let kind = if decision == "approve" {
                    "recovery_approve"
                } else {
                    "recovery_cancel"
                };
                let mut action =
                    authorized_action(tx, action_id, &a, &s, kind, Some(&target), now)?;
                let proof = tx
                    .get::<CredentialRecord>(&credential_key(
                        action
                            .value
                            .proof_credential_id
                            .as_deref()
                            .ok_or_else(|| reject("reauth_required"))?,
                    ))?
                    .ok_or(StoreError::Corrupt)?
                    .value;
                if !proof.public_material.is_public_key() {
                    return Err(reject("reauth_required"));
                }
                r.value.approved_by = if decision == "approve" {
                    Some(proof.credential_record_id)
                } else {
                    None
                };
                action.value.state = "consumed".into();
                tx.replace(
                    &format!("action:{action_id}"),
                    action.row_version,
                    "action",
                    Some(&a.account_id),
                    Some(action.value.expires_at),
                    &action.value,
                )?;
            } else {
                if decision != "cancel" {
                    return Err(reject("reauth_required"));
                }
                let context = recovery_context(
                    tx,
                    p["context_id"]
                        .as_str()
                        .ok_or_else(|| reject("authentication_failed"))?,
                    owner,
                    now,
                )?;
                if context.value.account_id != a.account_id
                    || context.value.cancel_operation_id.as_deref() != Some(operation_id)
                    || context.value.recovery_id.as_deref() != Some(id)
                {
                    return Err(reject("authentication_failed"));
                }
            }
            r.value.state = if decision == "approve" {
                "approved"
            } else {
                "cancelled"
            }
            .into();
            if decision == "cancel" {
                let mut c = tx
                    .get::<CredentialRecord>(&credential_key(&r.value.candidate_credential_id))?
                    .ok_or(StoreError::Corrupt)?;
                c.value.state = "revoked".into();
                c.value.version += 1;
                tx.replace(
                    &credential_key(&c.value.credential_record_id),
                    c.row_version,
                    "credential",
                    Some(&a.account_id),
                    None,
                    &c.value,
                )?;
            }
            tx.replace(
                &key,
                r.row_version,
                "recovery",
                Some(&a.account_id),
                Some(r.value.expires_at),
                &r.value,
            )?;
            if decision == "cancel"
                && let Some(mut notice) =
                    tx.get::<OutboxEvent>(&format!("outbox:recovery-notice:{id}"))?
            {
                notice.value.delivery_state = "cancelled".into();
                tx.replace(
                    &format!("outbox:recovery-notice:{id}"),
                    notice.row_version,
                    "recovery_notice_done",
                    Some(&a.account_id),
                    Some(now + 90 * DAY),
                    &notice.value,
                )?;
            }
            tx.create(
                &format!("receipt:{operation_id}"),
                "receipt",
                Some(&a.account_id),
                Some(now + 7 * DAY),
                &OperationReceipt {
                    operation_id: operation_id.into(),
                    kind: format!("recovery_{decision}"),
                    owner_context_digest: owner.into(),
                    request_digest,
                    state: "committed".into(),
                    result_ids: vec![a.account_id.clone(), id.into()],
                    committed_at: now,
                },
            )?;
            audit(
                tx,
                &a,
                if decision == "approve" {
                    "recovery_approved"
                } else {
                    "recovery_cancelled"
                },
                now,
            )?;
            Ok(json!({"api_version":2,"ok":true,"state":r.value.state}))
        })
    }
    fn recovery_completion_options(
        &mut self,
        id: &str,
        owner: &str,
        p: &Value,
        source: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        let context_id = p["context_id"]
            .as_str()
            .ok_or_else(|| reject("invalid_request"))?;
        let (r, c, email) = self.store.transaction(|tx| {
            let context = recovery_context(tx, context_id, owner, now)?;
            if context.value.recovery_id.as_deref() != Some(id) {
                return Err(reject("authentication_failed"));
            }
            let r = tx
                .get::<RecoveryRequest>(&format!("recovery:{id}"))?
                .ok_or(StoreError::Corrupt)?
                .value;
            let a = tx
                .get::<AccountRecord>(&format!("account:{}", r.account_id))?
                .ok_or(StoreError::Corrupt)?
                .value;
            let e = tx
                .get::<CredentialRecord>(&credential_key(&r.email_credential_id))?
                .ok_or(StoreError::Corrupt)?
                .value;
            check_recovery_state(&r, &a, &e, now)?;
            check_ready(&r, self.email.clock_healthy, now)?;
            let c = tx
                .get::<CredentialRecord>(&credential_key(&r.candidate_credential_id))?
                .ok_or(StoreError::Corrupt)?
                .value;
            Ok((r, c, e))
        })?;
        let address = if let CredentialMaterial::Email {
            encrypted_address, ..
        } = email.public_material
        {
            self.email.decrypt(&encrypted_address)?
        } else {
            return Err(StoreError::Corrupt);
        };
        let otp = self.issue_email_otp(
            "recovery_complete",
            owner,
            &address,
            Some(&r.account_id),
            Some(id),
            source,
            now,
        )?;
        let record = if let CredentialMaterial::WebAuthn { library_record, .. } = c.public_material
        {
            library_record
        } else {
            return Err(StoreError::Corrupt);
        };
        let (options, state) = self
            .webauthn
            .authentication_options(&[record])
            .map_err(|_| reject("authentication_failed"))?;
        let cid = random_token();
        let ceremony = CeremonyRecord {
            ceremony_id: cid.clone(),
            kind: "recovery_complete".into(),
            namespace: self.config.namespace.clone(),
            rp_config_version: self.config.rp.version,
            owner_context_digest: owner.into(),
            target_account_id: Some(r.account_id),
            target_digest: Some(format!("{id}:{}", r.candidate_digest)),
            expires_at: now + 120_000,
            failed_attempts: 0,
            status: "pending".into(),
            server_state: Some(state),
            result_ref: None,
        };
        self.store.transaction(|tx| {
            tx.create(
                &format!("ceremony:{cid}"),
                "ceremony",
                ceremony.target_account_id.as_deref(),
                Some(ceremony.expires_at),
                &ceremony,
            )
        })?;
        Ok(
            json!({"api_version":2,"ok":true,"otp_id":otp["otp_id"],"ceremony_id":cid,"operation_id":cid,"options":options}),
        )
    }
    fn recovery_complete(
        &mut self,
        id: &str,
        owner: &str,
        p: &Value,
        now: u64,
    ) -> Result<(Value, Option<String>), StoreError> {
        let (otp_id, code) = otp_fields(p)?;
        let ceremony_id = p["ceremony_id"]
            .as_str()
            .filter(|v| v.len() == 43)
            .ok_or_else(|| reject("invalid_request"))?;
        if p["operation_id"].as_str() != Some(ceremony_id) {
            return Err(reject("invalid_request"));
        }
        let request_digest = digest(
            &self.config.session_key,
            "recovery-response",
            &p.to_string(),
        );
        if let Some(value) = self.committed_identity_receipt(ceremony_id, owner, &request_digest)? {
            return Ok((value, None));
        }
        let r = self.store.transaction(|tx| {
            tx.get::<RecoveryRequest>(&format!("recovery:{id}"))?
                .ok_or_else(|| reject("authentication_failed"))
        })?;
        self.otp_precheck(
            otp_id,
            code,
            "recovery_complete",
            owner,
            Some(&r.value.account_id),
            Some(id),
            now,
        )?;
        let c = self.store.transaction(|tx| {
            tx.get::<CredentialRecord>(&credential_key(&r.value.candidate_credential_id))?
                .ok_or(StoreError::Corrupt)
        })?;
        let ceremony = self.store.transaction(|tx| {
            tx.get::<CeremonyRecord>(&format!("ceremony:{ceremony_id}"))?
                .ok_or_else(|| reject("authentication_failed"))
        })?;
        check_ceremony(&ceremony.value, owner, now)?;
        let target = format!("{id}:{}", r.value.candidate_digest);
        if ceremony.value.kind != "recovery_complete"
            || ceremony.value.target_digest.as_deref() != Some(&target)
        {
            return Err(reject("authentication_failed"));
        }
        let record =
            if let CredentialMaterial::WebAuthn { library_record, .. } = &c.value.public_material {
                library_record
            } else {
                return Err(StoreError::Corrupt);
            };
        let updated = match self.webauthn.finish_authentication(
            &p["response"],
            ceremony
                .value
                .server_state
                .as_ref()
                .ok_or(StoreError::Corrupt)?,
            record,
        ) {
            Ok(updated) => updated,
            Err(_) => {
                self.record_ceremony_failure(&format!("ceremony:{ceremony_id}"), owner, now)?;
                return Err(reject("authentication_failed"));
            }
        };
        let token = random_token();
        let config = &self.config;
        let email = &self.email;
        self.store.transaction(|tx|{if let Some(v)=read_identity_receipt(tx,ceremony_id,owner,&request_digest)?{return Ok((v,None));}let mut r=tx.get::<RecoveryRequest>(&format!("recovery:{id}"))?.ok_or(StoreError::Corrupt)?;let mut a=tx.get::<AccountRecord>(&format!("account:{}",r.value.account_id))?.ok_or(StoreError::Corrupt)?;let e=tx.get::<CredentialRecord>(&credential_key(&r.value.email_credential_id))?.ok_or(StoreError::Corrupt)?.value;check_recovery_state(&r.value,&a.value,&e,now)?;check_ready(&r.value,email.clock_healthy,now)?;let mut candidate=tx.get::<CredentialRecord>(&credential_key(&r.value.candidate_credential_id))?.ok_or(StoreError::Corrupt)?;if candidate.row_version!=c.row_version||candidate.value.state!="recovery_candidate"{return Err(reject("state_changed"));}let mut ceremony=tx.get::<CeremonyRecord>(&format!("ceremony:{ceremony_id}"))?.ok_or(StoreError::Corrupt)?;check_ceremony(&ceremony.value,owner,now)?;let mut otp=read_otp(tx,email,otp_id,code,"recovery_complete",owner,Some(&a.value.account_id),Some(id),now)?;
   for old in active_keys(tx,&a.value.account_id)? {let key=credential_key(&old.credential_record_id);let mut old=tx.get::<CredentialRecord>(&key)?.ok_or(StoreError::Corrupt)?;old.value.state="revoked".into();old.value.version+=1;tx.replace(&key,old.row_version,"credential",Some(&a.value.account_id),None,&old.value)?;}
   candidate.value.state="active".into();candidate.value.version+=1;candidate.value.last_used_at=Some(now);if let CredentialMaterial::WebAuthn{library_record,..}=&mut candidate.value.public_material{*library_record=updated.clone();}tx.replace(&credential_key(&candidate.value.credential_record_id),candidate.row_version,"credential",Some(&a.value.account_id),None,&candidate.value)?;
   let raw_id=if let CredentialMaterial::WebAuthn{credential_id,..}=&candidate.value.public_material{URL_SAFE_NO_PAD.encode(credential_id)}else{return Err(StoreError::Corrupt)};let lkey=format!("lookup:passkey:{raw_id}");let mut lookup=tx.get::<CredentialLookup>(&lkey)?.ok_or(StoreError::Corrupt)?;if lookup.value.credential_record_id!=candidate.value.credential_record_id||lookup.value.state!="recovery_candidate"{return Err(reject("state_changed"));}lookup.value.state="active".into();tx.replace(&lkey,lookup.row_version,"lookup",Some(&a.value.account_id),None,&lookup.value)?;
   a.value.policy_version+=1;a.value.security_epoch+=1;a.value.has_ever_public_key=true;a.value.last_recovery_id=Some(id.into());a.value.last_authenticated_at=Some(now);tx.replace(&format!("account:{}",a.value.account_id),a.row_version,"account",Some(&a.value.account_id),None,&a.value)?;r.value.state="completed".into();r.value.result_ref=Some(a.value.account_id.clone());tx.replace(&format!("recovery:{id}"),r.row_version,"recovery",Some(&a.value.account_id),Some(r.value.expires_at),&r.value)?;consume_otp(tx,&mut otp)?;ceremony.value.status="consumed".into();ceremony.value.server_state=None;tx.replace(&format!("ceremony:{ceremony_id}"),ceremony.row_version,"ceremony",Some(&a.value.account_id),Some(ceremony.value.expires_at),&ceremony.value)?;create_session(tx,config,&a.value,&candidate.value,&token,now)?;let skey=session_key(&config.session_key,&token);let mut s=tx.get::<AccountSession>(&skey)?.ok_or(StoreError::Corrupt)?;s.value.recovery_context=Some(format!("email_recovery:{id}"));tx.replace(&skey,s.row_version,"session",Some(&a.value.account_id),Some(s.value.expires_at),&s.value)?;
   tx.create(&format!("receipt:{ceremony_id}"),"receipt",Some(&a.value.account_id),Some(now+7*DAY),&OperationReceipt{operation_id:ceremony_id.into(),kind:"email_recovery_complete".into(),owner_context_digest:owner.into(),request_digest:digest(&config.session_key,"recovery-response",&p.to_string()),state:"committed".into(),result_ids:vec![a.value.account_id.clone(),id.into()],committed_at:now})?;tx.create(&format!("outbox:recovery-complete:{id}"),"recovery_notice",Some(&a.value.account_id),Some(now+90*DAY),&OutboxEvent{event_id:format!("recovery-complete:{id}"),account_id:a.value.account_id.clone(),event_type:"recovery_completed".into(),state_version:a.value.policy_version,safe_payload:json!({"recovery_id":id}),attempts:0,delivery_state:"pending".into()})?;if let Some(mut notice)=tx.get::<OutboxEvent>(&format!("outbox:recovery-notice:{id}"))?{notice.value.delivery_state="superseded".into();tx.replace(&format!("outbox:recovery-notice:{id}"),notice.row_version,"recovery_notice_done",Some(&a.value.account_id),Some(now+90*DAY),&notice.value)?;}audit(tx,&a.value,"recovery_completed",now)?;Ok((json!({"api_version":2,"ok":true,"phase":"recovery_completed","account":{"account_id":a.value.account_id,"display_name":a.value.display_name},"world":{"state":"not_requested"},"old_sessions_revoked":true}),Some(token)))})
    }
}
fn recovery_context(
    tx: &IdentityTransaction<'_>,
    id: &str,
    owner: &str,
    now: u64,
) -> Result<Versioned<RecoveryContext>, StoreError> {
    let context = tx
        .get::<RecoveryContext>(&format!("recovery_context:{id}"))?
        .ok_or_else(|| reject("authentication_failed"))?;
    if context.value.owner != owner || context.value.expires_at <= now {
        return Err(reject("authentication_failed"));
    }
    Ok(context)
}
fn check_context_account(c: &RecoveryContext, a: &AccountRecord) -> Result<(), StoreError> {
    if c.account_id != a.account_id
        || c.policy_version != a.policy_version
        || c.security_epoch != a.security_epoch
        || a.status != "active"
    {
        return Err(reject("state_changed"));
    }
    Ok(())
}
fn check_email_recovery(
    a: &AccountRecord,
    c: &CredentialRecord,
    now: u64,
) -> Result<(), StoreError> {
    if a.status != "active" || c.account_id != a.account_id || c.state != "active" {
        return Err(reject("authentication_failed"));
    }
    if let CredentialMaterial::Email {
        recovery_enabled,
        recovery_eligible_since,
        bound_at,
        ..
    } = c.public_material
    {
        if !recovery_enabled
            || now < recovery_eligible_since.saturating_add(DAY)
            || now < bound_at.saturating_add(DAY)
        {
            return Err(reject("recovery_not_ready"));
        }
    } else {
        return Err(reject("authentication_failed"));
    }
    Ok(())
}
fn check_recovery_state(
    r: &RecoveryRequest,
    a: &AccountRecord,
    e: &CredentialRecord,
    now: u64,
) -> Result<(), StoreError> {
    check_email_recovery(a, e, now)?;
    if !matches!(r.state.as_str(), "waiting" | "approved") || r.expires_at <= now {
        return Err(reject("recovery_expired"));
    }
    if r.account_id != a.account_id
        || r.account_policy_version_at_start != a.policy_version
        || r.security_epoch_at_start != a.security_epoch
        || r.email_credential_id != e.credential_record_id
        || r.email_credential_version != e.version
    {
        return Err(reject("state_changed"));
    }
    Ok(())
}
fn check_ready(r: &RecoveryRequest, clock_healthy: bool, now: u64) -> Result<(), StoreError> {
    if !clock_healthy {
        return Err(reject("clock_unhealthy"));
    }
    if r.state == "approved" && r.approved_by.is_some() {
        return Ok(());
    }
    if r.notification_accepted_at.is_none() || r.not_before > now {
        return Err(reject("recovery_not_ready"));
    }
    Ok(())
}
fn projection(r: &Versioned<RecoveryRequest>) -> Value {
    json!({"api_version":2,"ok":true,"recovery_id":r.value.recovery_id,"state":r.value.state,"candidate_digest":r.value.candidate_digest,"not_before":r.value.not_before,"expires_at":r.value.expires_at,"version":r.row_version,"notification_accepted":r.value.notification_accepted_at.is_some()})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn account() -> AccountRecord {
        AccountRecord {
            account_id: "legacy-account".into(),
            user_handle: vec![1; 16],
            display_name: "Player".into(),
            status: "active".into(),
            policy_version: 3,
            security_epoch: 2,
            has_ever_public_key: true,
            created_at: 0,
            last_authenticated_at: None,
            last_recovery_id: None,
        }
    }
    fn email() -> CredentialRecord {
        CredentialRecord {
            credential_record_id: "email-1".into(),
            account_id: "legacy-account".into(),
            state: "active".into(),
            version: 4,
            label: "Email".into(),
            created_at: 0,
            last_used_at: None,
            public_material: CredentialMaterial::Email {
                encrypted_address: "not-read-by-policy".into(),
                lookup_hmac: "lookup".into(),
                normalization_version: 1,
                verified_at: 0,
                bound_at: 0,
                recovery_enabled: true,
                recovery_eligible_since: 0,
            },
            registered_via: "legacy".into(),
        }
    }
    fn request() -> RecoveryRequest {
        RecoveryRequest {
            recovery_id: "request".into(),
            account_id: "legacy-account".into(),
            email_credential_id: "email-1".into(),
            email_credential_version: 4,
            candidate_credential_id: "candidate".into(),
            candidate_digest: "fixed".into(),
            account_policy_version_at_start: 3,
            security_epoch_at_start: 2,
            created_at: DAY,
            not_before: 2 * DAY,
            expires_at: 8 * DAY,
            state: "waiting".into(),
            approved_by: None,
            notification_accepted_at: None,
            result_ref: None,
        }
    }
    #[test]
    fn recovery_requires_mature_current_email_and_never_unfreezes() {
        let a = account();
        let e = email();
        assert!(check_email_recovery(&a, &e, DAY - 1).is_err());
        assert!(check_email_recovery(&a, &e, DAY).is_ok());
        let mut frozen = a.clone();
        frozen.status = "frozen".into();
        assert!(check_email_recovery(&frozen, &e, 2 * DAY).is_err());
        let mut newer = e.clone();
        newer.version += 1;
        assert!(check_recovery_state(&request(), &a, &newer, 2 * DAY).is_err());
        let mut changed = a.clone();
        changed.policy_version += 1;
        assert!(check_recovery_state(&request(), &changed, &e, 2 * DAY).is_err());
    }
    #[test]
    fn wait_requires_smtp_acceptance_clockhealth_and_fixed_terminal_state() {
        let mut r = request();
        assert!(check_ready(&r, true, 3 * DAY).is_err());
        r.notification_accepted_at = Some(DAY);
        assert!(check_ready(&r, true, 2 * DAY - 1).is_err());
        assert!(check_ready(&r, false, 2 * DAY).is_err());
        assert!(check_ready(&r, true, 2 * DAY).is_ok());
        r.state = "cancelled".into();
        assert!(check_recovery_state(&r, &account(), &email(), 2 * DAY).is_err());
        r.state = "completed".into();
        assert!(check_recovery_state(&r, &account(), &email(), 2 * DAY).is_err());
        r.state = "waiting".into();
        assert!(check_recovery_state(&r, &account(), &email(), 8 * DAY).is_err());
    }
    #[test]
    fn approval_can_shorten_wait_but_never_bypasses_clock_or_account_versions() {
        let mut r = request();
        r.state = "approved".into();
        r.approved_by = Some("retained-active-public-key".into());
        assert!(check_ready(&r, true, DAY).is_ok());
        assert!(check_ready(&r, false, DAY).is_err());
        let mut a = account();
        a.security_epoch += 1;
        assert!(check_recovery_state(&r, &a, &email(), DAY).is_err());
    }
    #[test]
    fn invalid_candidate_responses_exhaust_persistent_ceremony_without_creating_key() {
        let config = super::super::config::IdentityConfig {
            namespace: IdentityNamespace {
                identity_service_id: "recovery-test".into(),
                environment_id: "isolated".into(),
            },
            rp: RpConfig {
                rp_id: "example.test".into(),
                allowed_origins: vec!["https://example.test".into()],
                version: 1,
                identity_instance_epoch: 1,
            },
            sqlite_path: std::env::temp_dir()
                .join(format!("oasis7-recovery-{}.sqlite", uuid::Uuid::new_v4())),
            registration_enabled: false,
            session_key: [7; 32],
            world_scope: "world:test".into(),
            runtime_live_bind: "127.0.0.1:5023".into(),
            tablestore: None,
            trusted_proxy_loopback: false,
        };
        let mut service = IdentityService::new(config).unwrap();
        let mut a = account();
        a.user_handle = uuid::Uuid::new_v4().as_bytes().to_vec();
        let context = RecoveryContext {
            id: "context".into(),
            owner: "owner".into(),
            account_id: a.account_id.clone(),
            email_id: "email-1".into(),
            email_version: 4,
            policy_version: a.policy_version,
            security_epoch: a.security_epoch,
            email_verified_at: DAY,
            expires_at: 8 * DAY,
            candidate_id: None,
            candidate_digest: None,
            recovery_id: None,
            operation_id: None,
            cancel_operation_id: Some("cancel-test".into()),
        };
        service
            .store
            .transaction(|tx| {
                tx.create(
                    "account:legacy-account",
                    "account",
                    Some(&a.account_id),
                    None,
                    &a,
                )?;
                tx.create(
                    "recovery_context:context",
                    "recovery_context",
                    Some(&a.account_id),
                    Some(context.expires_at),
                    &context,
                )
            })
            .unwrap();
        let options = service
            .recovery_candidate_options("owner", &json!({"context_id":"context"}), DAY)
            .unwrap();
        let cid = options["ceremony_id"].as_str().unwrap();
        let payload = json!({"context_id":"context","ceremony_id":cid,"response":null});
        for _ in 0..5 {
            assert!(
                service
                    .recovery_candidate_complete("owner", &payload, DAY + 1)
                    .is_err()
            );
        }
        let record = service
            .store
            .transaction(|tx| tx.get::<CeremonyRecord>(&format!("ceremony:{cid}")))
            .unwrap()
            .unwrap()
            .value;
        assert_eq!(record.failed_attempts, 5);
        assert_eq!(record.status, "exhausted");
        assert!(record.server_state.is_none());
        assert!(
            service
                .store
                .transaction(|tx| tx.all::<CredentialRecord>("credential", Some(&a.account_id)))
                .unwrap()
                .is_empty()
        );
    }
}
