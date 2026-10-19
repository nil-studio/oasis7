use super::{
    model::*,
    random_token, reject,
    service::{
        IdentityService, active_keys, audit, authorized_action, credential_key, session_key,
        valid_session,
    },
    store::StoreError,
};
use serde_json::{Value, json};
#[cfg(test)]
#[path = "credential_contract_tests.rs"]
mod tests;
impl IdentityService {
    pub(super) fn list_account_records(
        &mut self,
        sid: Option<&str>,
        path: &str,
        p: &Value,
        now: u64,
    ) -> Result<Value, StoreError> {
        let (a, s) = self.session(sid, now)?;
        let cursor = p["cursor"].as_str().unwrap_or("");
        if cursor.len() > 128 {
            return Err(reject("invalid_request"));
        }
        self.store.transaction(|tx|if path=="/credentials"{
            let mut records=tx.all::<CredentialRecord>("credential",Some(&a.account_id))?.into_iter().map(|(_,v)|v.value).filter(|c|c.credential_record_id.as_str()>cursor).collect::<Vec<_>>();
            records.sort_by(|a,b|a.credential_record_id.cmp(&b.credential_record_id));let more=records.len()>100;records.truncate(100);let next=if more{records.last().map(|c|c.credential_record_id.clone())}else{None};
            Ok(json!({"api_version":2,"ok":true,"next_cursor":next,"credentials":records.into_iter().map(|c|json!({"id":c.credential_record_id,"label":c.label,"state":c.state,"version":c.version,"type":c.public_material.credential_type()})).collect::<Vec<_>>()}))
        }else{
            let mut records=tx.all::<AccountSession>("session",Some(&a.account_id))?.into_iter().map(|(_,v)|v.value).filter(|c|c.session_id.as_str()>cursor).collect::<Vec<_>>();
            records.sort_by(|a,b|a.session_id.cmp(&b.session_id));let more=records.len()>100;records.truncate(100);let next=if more{records.last().map(|s|s.session_id.clone())}else{None};
            Ok(json!({"api_version":2,"ok":true,"next_cursor":next,"sessions":records.into_iter().map(|r|json!({"id":r.session_id,"current":r.session_id==s.session_id,"auth_method":r.auth_method,"created_at":r.auth_time,"revoked":r.revoked_at.is_some(),"expired":r.expires_at<=now||r.idle_expires_at<=now,"device":"unknown"})).collect::<Vec<_>>()}))
        })
    }
    pub(super) fn create_action(
        &mut self,
        sid: Option<&str>,
        p: &Value,
        now: u64,
    ) -> Result<Value, StoreError> {
        let (a, s) = self.session(sid, now)?;
        let kind = p["action_type"]
            .as_str()
            .ok_or_else(|| reject("invalid_request"))?;
        if ![
            "bind",
            "revoke_credential",
            "revoke_session",
            "freeze",
            "bind_email",
            "unbind_email",
            "recovery_approve",
            "recovery_cancel",
        ]
        .contains(&kind)
        {
            return Err(reject("invalid_request"));
        }
        let resolved_target = if kind == "bind_email" {
            let email = super::email::normalize_email(
                p["email"]
                    .as_str()
                    .ok_or_else(|| reject("invalid_request"))?,
            )?;
            self.email.lookup(&email)
        } else {
            p["target"].as_str().unwrap_or("").to_owned()
        };
        let target = resolved_target.as_str();
        if target.len() > 128 {
            return Err(reject("invalid_request"));
        }
        if p["expected_version"]
            .as_u64()
            .is_some_and(|v| v != a.policy_version)
        {
            return Err(reject("state_changed"));
        }
        let id = random_token();
        let action = SensitiveAction {
            action_id: id.clone(),
            account_id: a.account_id.clone(),
            session_id: s.session_id,
            action_type: kind.into(),
            target_digest: target.into(),
            account_policy_version: a.policy_version,
            proof_credential_id: None,
            proof_time: None,
            expires_at: now + 300_000,
            state: "pending".into(),
        };
        self.store.transaction(|tx| {
            super::service::consume_quota(tx, "actions-account", &a.account_id, now, 60_000, 30)?;
            tx.create(
                &format!("action:{id}"),
                "action",
                Some(&a.account_id),
                Some(action.expires_at),
                &action,
            )
        })?;
        Ok(json!({"api_version":2,"ok":true,"action_id":id,"expires_at":action.expires_at}))
    }
    pub(super) fn handle_credentials(
        &mut self,
        method: &str,
        path: &str,
        sid: Option<&str>,
        p: &Value,
        now: u64,
        owner: &str,
    ) -> Result<Value, StoreError> {
        let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
        if method == "POST" && parts.len() == 4 && parts[0] == "actions" && parts[2] == "reauth" {
            return self.reauth(parts[1], parts[3], sid, p, now);
        }
        let (a, s) = self.session(sid, now)?;
        let request_digest = super::digest(
            &self.config.session_key,
            "operation-request",
            &format!("{method}:{path}:{}", p),
        );
        if method == "POST" && (path.ends_with("/revoke") || path == "/account/freeze") {
            let operation = p["operation_id"]
                .as_str()
                .ok_or_else(|| reject("invalid_request"))?;
            let expected = p["action_id"]
                .as_str()
                .unwrap_or_else(|| parts.get(1).copied().unwrap_or(""));
            if operation != expected {
                return Err(reject("invalid_request"));
            }
            if let Some(receipt) = self
                .store
                .transaction(|tx| tx.get::<OperationReceipt>(&format!("receipt:{operation}")))?
            {
                if receipt.value.result_ids.first() != Some(&a.account_id)
                    || receipt.value.request_digest != request_digest
                {
                    return Err(reject("idempotency_conflict"));
                }
                return Ok(json!({"api_version":2,"ok":true,"phase":"operation_committed"}));
            }
        }
        let secret = self.config.session_key;
        let epoch = self.config.rp.identity_instance_epoch;
        let skey = session_key(&secret, sid.ok_or_else(|| reject("session_expired"))?);
        if method == "POST" && path == "/account/freeze" {
            if p["confirmed"].as_bool() != Some(true) {
                return Err(reject("invalid_request"));
            }
            let action_id = p["action_id"]
                .as_str()
                .ok_or_else(|| reject("reauth_required"))?;
            return self.store.transaction(|tx| {
                let (a, s) = valid_session(tx, &skey, epoch, now)?;
                let mut action = authorized_action(tx, action_id, &a, &s, "freeze", Some(""), now)?;
                action.value.state = "consumed".into();
                tx.replace(
                    &format!("action:{action_id}"),
                    action.row_version,
                    "action",
                    Some(&a.account_id),
                    Some(action.value.expires_at),
                    &action.value,
                )?;
                let key = format!("account:{}", a.account_id);
                let mut account = tx.get::<AccountRecord>(&key)?.ok_or(StoreError::Corrupt)?;
                account.value.status = "frozen".into();
                account.value.security_epoch += 1;
                account.value.policy_version += 1;
                tx.replace(
                    &key,
                    account.row_version,
                    "account",
                    Some(&a.account_id),
                    None,
                    &account.value,
                )?;
                audit(tx, &account.value, "account_frozen", now)?;
                record_action_receipt(tx, action_id, &a, owner, &request_digest, now)?;
                Ok(json!({"api_version":2,"ok":true,"phase":"account_frozen"}))
            });
        }
        if method == "POST" && parts.len() == 3 && parts[0] == "credentials" && parts[2] == "revoke"
        {
            let id = parts[1];
            let action_id = p["action_id"]
                .as_str()
                .ok_or_else(|| reject("reauth_required"))?;
            return self.store.transaction(|tx| {
                let (a, s) = valid_session(tx, &skey, epoch, now)?;
                let mut action =
                    authorized_action(tx, action_id, &a, &s, "revoke_credential", Some(id), now)?;
                if action.value.proof_credential_id.as_deref() == Some(id) {
                    return Err(reject("last_credential_protected"));
                }
                let key = credential_key(id);
                let mut c = tx
                    .get::<CredentialRecord>(&key)?
                    .ok_or_else(|| reject("invalid_request"))?;
                if c.value.account_id != a.account_id
                    || c.value.version != p["expected_version"].as_u64().unwrap_or(0)
                {
                    return Err(reject("state_changed"));
                }
                if !c.value.public_material.is_public_key()
                    || active_keys(tx, &a.account_id)?.len() <= 1
                {
                    return Err(reject("last_credential_protected"));
                }
                c.value.state = "revoked".into();
                c.value.version += 1;
                tx.replace(
                    &key,
                    c.row_version,
                    "credential",
                    Some(&a.account_id),
                    None,
                    &c.value,
                )?;
                finish_action(tx, action_id, &mut action, &a, now, owner, &request_digest)?;
                Ok(json!({"api_version":2,"ok":true,"revocation_enforcement_max_seconds":60}))
            });
        }
        if method == "PATCH" && parts.len() == 2 && parts[0] == "credentials" {
            let label = p["label"]
                .as_str()
                .filter(|v| {
                    !v.trim().is_empty() && v.len() <= 80 && !v.chars().any(char::is_control)
                })
                .ok_or_else(|| reject("invalid_request"))?;
            return self.store.transaction(|tx| {
                valid_session(tx, &skey, epoch, now)?;
                let key = credential_key(parts[1]);
                let mut c = tx
                    .get::<CredentialRecord>(&key)?
                    .ok_or_else(|| reject("invalid_request"))?;
                if c.value.account_id != a.account_id
                    || c.value.version != p["expected_version"].as_u64().unwrap_or(0)
                {
                    return Err(reject("state_changed"));
                }
                c.value.label = label.into();
                tx.replace(
                    &key,
                    c.row_version,
                    "credential",
                    Some(&a.account_id),
                    None,
                    &c.value,
                )?;
                Ok(json!({"api_version":2,"ok":true}))
            });
        }
        if method == "POST" && parts.len() == 3 && parts[0] == "sessions" && parts[2] == "revoke" {
            return self.store.transaction(|tx| {
                let (a, s) = valid_session(tx, &skey, epoch, now)?;
                let record = tx
                    .all::<AccountSession>("session", Some(&a.account_id))?
                    .into_iter()
                    .find(|(_, v)| v.value.session_id == parts[1])
                    .ok_or_else(|| reject("invalid_request"))?;
                let (key, mut target) = record;
                if target.value.session_id != s.session_id {
                    let action_id = p["action_id"]
                        .as_str()
                        .ok_or_else(|| reject("reauth_required"))?;
                    let mut action = authorized_action(
                        tx,
                        action_id,
                        &a,
                        &s,
                        "revoke_session",
                        Some(parts[1]),
                        now,
                    )?;
                    finish_action(tx, action_id, &mut action, &a, now, owner, &request_digest)?;
                }
                target.value.revoked_at = Some(now);
                tx.replace(
                    &key,
                    target.row_version,
                    "session",
                    Some(&a.account_id),
                    Some(target.value.expires_at),
                    &target.value,
                )?;
                audit(tx, &a, "session_revoked", now)?;
                if target.value.session_id == s.session_id {
                    record_action_receipt(tx, &s.session_id, &a, owner, &request_digest, now)?;
                }
                Ok(json!({"api_version":2,"ok":true}))
            });
        }
        let _ = s;
        Err(reject("route_not_available"))
    }
    fn reauth(
        &mut self,
        id: &str,
        stage: &str,
        sid: Option<&str>,
        p: &Value,
        now: u64,
    ) -> Result<Value, StoreError> {
        let (a, s) = self.session(sid, now)?;
        let key = format!("action:{id}");
        let action = self.store.transaction(|tx| {
            tx.get::<SensitiveAction>(&key)?
                .ok_or_else(|| reject("reauth_required"))
        })?;
        if action.value.account_id != a.account_id
            || action.value.session_id != s.session_id
            || action.value.state != "pending"
            || action.value.expires_at <= now
            || action.value.account_policy_version != a.policy_version
        {
            return Err(reject("reauth_required"));
        }
        let owner = super::digest(&self.config.session_key, "action-owner", &s.session_id);
        if stage == "options" {
            let credentials = self
                .store
                .transaction(|tx| active_keys(tx, &a.account_id))?;
            let records: Vec<Value> = credentials
                .iter()
                .filter_map(|c| {
                    if let CredentialMaterial::WebAuthn { library_record, .. } = &c.public_material
                    {
                        Some(library_record.clone())
                    } else {
                        None
                    }
                })
                .collect();
            let (options, state) = self
                .webauthn
                .authentication_options(&records)
                .map_err(|_| reject("authentication_failed"))?;
            let ceremony_id = random_token();
            let c = CeremonyRecord {
                ceremony_id: ceremony_id.clone(),
                kind: "reauth".into(),
                namespace: self.config.namespace.clone(),
                rp_config_version: self.config.rp.version,
                owner_context_digest: owner,
                target_account_id: Some(a.account_id.clone()),
                target_digest: Some(id.into()),
                expires_at: now + 120_000,
                failed_attempts: 0,
                status: "pending".into(),
                server_state: Some(state),
                result_ref: None,
            };
            self.store.transaction(|tx| {
                super::service::consume_quota(
                    tx,
                    "reauth-account",
                    &a.account_id,
                    now,
                    60_000,
                    60,
                )?;
                tx.create(
                    &format!("ceremony:{ceremony_id}"),
                    "ceremony",
                    Some(&a.account_id),
                    Some(c.expires_at),
                    &c,
                )
            })?;
            return Ok(
                json!({"api_version":2,"ok":true,"ceremony_id":ceremony_id,"options":options}),
            );
        }
        if stage != "complete" {
            return Err(reject("invalid_request"));
        }
        let ceremony_id = p["ceremony_id"]
            .as_str()
            .ok_or_else(|| reject("invalid_request"))?;
        let ckey = format!("ceremony:{ceremony_id}");
        let ceremony = self.store.transaction(|tx| {
            tx.get::<CeremonyRecord>(&ckey)?
                .ok_or_else(|| reject("authentication_failed"))
        })?;
        super::service::check_ceremony(&ceremony.value, &owner, now)?;
        if ceremony.value.kind != "reauth" || ceremony.value.target_digest.as_deref() != Some(id) {
            return Err(reject("authentication_failed"));
        }
        let response = &p["response"];
        let verification = (|| {
            let raw_id = response["id"]
                .as_str()
                .ok_or_else(|| reject("authentication_failed"))?;
            let c = self.store.transaction(|tx| {
                let l = tx
                    .get::<CredentialLookup>(&format!("lookup:passkey:{raw_id}"))?
                    .ok_or_else(|| reject("authentication_failed"))?
                    .value;
                tx.get::<CredentialRecord>(&credential_key(&l.credential_record_id))?
                    .ok_or_else(|| reject("authentication_failed"))
            })?;
            if c.value.account_id != a.account_id || c.value.state != "active" {
                return Err(reject("authentication_failed"));
            }
            let library_record = if let CredentialMaterial::WebAuthn { library_record, .. } =
                &c.value.public_material
            {
                library_record
            } else {
                return Err(reject("authentication_failed"));
            };
            let updated = self
                .webauthn
                .finish_authentication(
                    response,
                    ceremony
                        .value
                        .server_state
                        .as_ref()
                        .ok_or(StoreError::Corrupt)?,
                    library_record,
                )
                .map_err(|_| reject("authentication_failed"))?;
            Ok((c, updated))
        })();
        let (c, updated) = match verification {
            Ok(result) => result,
            Err(error) => {
                if matches!(&error,StoreError::Rejected(code) if code=="authentication_failed") {
                    self.record_ceremony_failure(&ckey, &owner, now)?;
                }
                return Err(error);
            }
        };
        let skey = session_key(
            &self.config.session_key,
            sid.ok_or_else(|| reject("session_expired"))?,
        );
        let epoch = self.config.rp.identity_instance_epoch;
        self.store.transaction(|tx| {
            let (a, s) = valid_session(tx, &skey, epoch, now)?;
            let mut current = tx
                .get::<SensitiveAction>(&key)?
                .ok_or_else(|| reject("reauth_required"))?;
            if current.row_version != action.row_version
                || current.value.account_policy_version != a.policy_version
                || current.value.session_id != s.session_id
                || current.value.state != "pending"
            {
                return Err(reject("state_changed"));
            }
            let mut credential = tx
                .get::<CredentialRecord>(&credential_key(&c.value.credential_record_id))?
                .ok_or(StoreError::Corrupt)?;
            if credential.row_version != c.row_version || credential.value.state != "active" {
                return Err(reject("state_changed"));
            }
            let mut ceremony = tx
                .get::<CeremonyRecord>(&ckey)?
                .ok_or(StoreError::Corrupt)?;
            super::service::check_ceremony(&ceremony.value, &owner, now)?;
            if let CredentialMaterial::WebAuthn { library_record, .. } =
                &mut credential.value.public_material
            {
                *library_record = updated.clone();
            }
            credential.value.last_used_at = Some(now);
            tx.replace(
                &credential_key(&credential.value.credential_record_id),
                credential.row_version,
                "credential",
                Some(&a.account_id),
                None,
                &credential.value,
            )?;
            current.value.state = "authorized".into();
            current.value.proof_credential_id = Some(credential.value.credential_record_id);
            current.value.proof_time = Some(now);
            current.value.expires_at = now + 300_000;
            tx.replace(
                &key,
                current.row_version,
                "action",
                Some(&a.account_id),
                Some(current.value.expires_at),
                &current.value,
            )?;
            ceremony.value.status = "consumed".into();
            ceremony.value.server_state = None;
            tx.replace(
                &ckey,
                ceremony.row_version,
                "ceremony",
                Some(&a.account_id),
                Some(ceremony.value.expires_at),
                &ceremony.value,
            )?;
            Ok(json!({"api_version":2,"ok":true,"action_id":id}))
        })
    }
}
fn finish_action(
    tx: &mut super::store::IdentityTransaction<'_>,
    id: &str,
    action: &mut super::store::Versioned<SensitiveAction>,
    a: &AccountRecord,
    now: u64,
    owner: &str,
    request_digest: &str,
) -> Result<(), StoreError> {
    action.value.state = "consumed".into();
    tx.replace(
        &format!("action:{id}"),
        action.row_version,
        "action",
        Some(&a.account_id),
        Some(action.value.expires_at),
        &action.value,
    )?;
    let key = format!("account:{}", a.account_id);
    let mut account = tx.get::<AccountRecord>(&key)?.ok_or(StoreError::Corrupt)?;
    account.value.policy_version += 1;
    tx.replace(
        &key,
        account.row_version,
        "account",
        Some(&a.account_id),
        None,
        &account.value,
    )?;
    audit(tx, &account.value, "security_changed", now)?;
    record_action_receipt(tx, id, a, owner, request_digest, now)
}

fn record_action_receipt(
    tx: &mut super::store::IdentityTransaction<'_>,
    id: &str,
    a: &AccountRecord,
    owner: &str,
    request_digest: &str,
    now: u64,
) -> Result<(), StoreError> {
    tx.create(
        &format!("receipt:{id}"),
        "receipt",
        Some(&a.account_id),
        Some(now + 7 * 86400000),
        &OperationReceipt {
            operation_id: id.into(),
            kind: "sensitive_action".into(),
            owner_context_digest: owner.into(),
            request_digest: request_digest.into(),
            state: "committed".into(),
            result_ids: vec![a.account_id.clone()],
            committed_at: now,
        },
    )
}
