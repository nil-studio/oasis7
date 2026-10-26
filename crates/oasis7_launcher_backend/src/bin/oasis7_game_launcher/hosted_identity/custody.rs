//! Native custody approval authority. Uses the existing identity records/transactions.
use super::{
    digest,
    model::*,
    random_token, reject,
    service::{
        IdentityService, active_keys, audit, consume_quota, credential_key, header, session_key,
        valid_session,
    },
    store::{IdentityTransaction, StoreError},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer, SigningKey};
use oasis7_custody_core::{DecimalU64, approval::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[path = "custody_complete.rs"]
mod complete;

pub(super) struct CustodyConfig {
    key: SigningKey,
    key_id: String,
    audience: String,
    adapter_secret: [u8; 32],
}
impl CustodyConfig {
    pub(super) fn from_env(session_secret: &[u8; 32]) -> Result<Option<Self>, String> {
        let names = [
            "OASIS7_CUSTODY_APPROVAL_SIGNING_KEY",
            "OASIS7_CUSTODY_APPROVAL_KEY_ID",
            "OASIS7_CUSTODY_ACCOUNT_AUTHORITY_AUDIENCE",
            "OASIS7_CUSTODY_NATIVE_ADAPTER_SECRET",
        ];
        if names.iter().all(|name| std::env::var_os(name).is_none()) {
            return Ok(None);
        }
        let values = names
            .iter()
            .map(|name| {
                std::env::var(name)
                    .ok()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| format!("{name} required for custody approvals"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let decode = |v: &str| -> Result<[u8; 32], String> {
            hex::decode(v)
                .map_err(|_| "custody secrets must be 32-byte hex".to_owned())?
                .try_into()
                .map_err(|_| "custody secrets must be 32-byte hex".to_owned())
        };
        if values[1].len() > 128 || values[2].len() > 256 {
            return Err("invalid custody key id/audience".into());
        }
        let seed = decode(&values[0])?;
        let adapter_secret = decode(&values[3])?;
        if &seed == session_secret || seed == adapter_secret || &adapter_secret == session_secret {
            return Err("custody signing, adapter and session secrets must be independent".into());
        }
        Ok(Some(Self {
            key: SigningKey::from_bytes(&seed),
            key_id: values[1].clone(),
            audience: values[2].clone(),
            adapter_secret,
        }))
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct ApprovalRecord {
    approval_id: String,
    request: ApprovalOptionsRequestV1,
    intent_digest: String,
    session_storage_key: String,
    manager_account_id: String,
    manager_session_ref: String,
    policy_version: u64,
    security_epoch: u64,
    origin_credential_id: String,
    origin_credential_version: u64,
    options: Value,
    state: Value,
    ceremony_expires_at: u64,
    failed_attempts: u32,
    complete_digest: Option<String>,
    proof_bytes: Option<Vec<u8>>,
}
fn approval_key(id: &str) -> String {
    format!("custody_approval:{id}")
}
fn record_response(record: &ApprovalRecord, now: u64) -> Result<Value, StoreError> {
    let phase = if record.proof_bytes.is_some() {
        ApprovalPhaseV1::Completed
    } else if record.ceremony_expires_at <= now {
        ApprovalPhaseV1::Expired
    } else if record.failed_attempts >= 5 {
        ApprovalPhaseV1::Exhausted
    } else {
        ApprovalPhaseV1::Pending
    };
    serde_json::to_value(ApprovalOptionsResponseV1 {
        api_version: 1,
        phase,
        approval_id: record.approval_id.clone(),
        ceremony_id: record.approval_id.clone(),
        operation_id: record.approval_id.clone(),
        options: (phase == ApprovalPhaseV1::Pending).then(|| record.options.clone()),
        intent_digest: record.intent_digest.clone(),
        audience: record.request.context.audience.clone(),
        ceremony_expires_at: DecimalU64(record.ceremony_expires_at),
    })
    .map_err(|_| StoreError::Corrupt)
}
fn management_session(
    tx: &IdentityTransaction<'_>,
    key: &str,
    epoch: u64,
    now: u64,
) -> Result<(AccountRecord, AccountSession), StoreError> {
    let (account, session) = valid_session(tx, key, epoch, now)?;
    let credential = tx
        .get::<CredentialRecord>(&credential_key(&session.origin_credential_id))?
        .ok_or(StoreError::Corrupt)?
        .value;
    if session.auth_method != "webauthn"
        || !matches!(
            credential.public_material,
            CredentialMaterial::WebAuthn { .. }
        )
    {
        return Err(reject("passkey_management_session_required"));
    }
    Ok((account, session))
}

impl IdentityService {
    pub(super) fn custody_browser_route(
        &mut self,
        method: &str,
        path: &str,
        sid: Option<&str>,
        payload: &Value,
        source: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        if self.custody.is_none() {
            return Err(reject("custody_disabled"));
        }
        match (method, path) {
            ("POST", "/custody/approvals/options") => {
                self.custody_options(sid, payload, source, now)
            }
            ("POST", "/custody/approvals/complete") => {
                self.custody_complete(sid, payload, source, now)
            }
            ("GET", "/custody/session-status")
            | ("GET" | "POST", "/custody/management-session-status") => {
                let (a, s) = if path == "/custody/management-session-status" {
                    if !payload.as_object().is_some_and(|v| v.is_empty()) {
                        return Err(reject("invalid_request"));
                    }
                    let key = session_key(
                        &self.config.session_key,
                        sid.ok_or_else(|| reject("session_expired"))?,
                    );
                    self.store.transaction(|tx| {
                        management_session(tx, &key, self.config.rp.identity_instance_epoch, now)
                    })?
                } else {
                    self.session(sid, now)?
                };
                serde_json::to_value(CustodySessionStatusV1 {
                    api_version: 1,
                    identity_namespace: self.config.namespace.key(),
                    identity_instance_epoch: DecimalU64(self.config.rp.identity_instance_epoch),
                    account_id: a.account_id,
                    session_ref: s.session_id,
                    credential_ref: s.origin_credential_id,
                    credential_version: DecimalU64(s.origin_credential_version),
                    security_version: DecimalU64(a.security_epoch),
                    policy_version: DecimalU64(a.policy_version),
                    server_time: DecimalU64(now),
                    expires_at: DecimalU64(s.expires_at),
                    idle_expires_at: DecimalU64(s.idle_expires_at),
                })
                .map_err(|_| StoreError::Corrupt)
            }
            _ => Err(reject("method_not_allowed")),
        }
    }
    fn custody_options(
        &mut self,
        sid: Option<&str>,
        payload: &Value,
        source: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        let request: ApprovalOptionsRequestV1 =
            serde_json::from_value(payload.clone()).map_err(|_| reject("invalid_request"))?;
        let config = self
            .custody
            .as_ref()
            .ok_or_else(|| reject("custody_disabled"))?;
        if request.context.audience != config.audience
            || request.context.environment != self.config.namespace.environment_id
        {
            return Err(reject("approval_target_mismatch"));
        }
        let key = session_key(
            &self.config.session_key,
            sid.ok_or_else(|| reject("session_expired"))?,
        );
        let epoch = self.config.rp.identity_instance_epoch;
        let (account, session, credentials) = self.store.transaction(|tx| {
            let (a, s) = management_session(tx, &key, epoch, now)?;
            let credentials = active_keys(tx, &a.account_id)?
                .into_iter()
                .filter_map(|c| match c.public_material {
                    CredentialMaterial::WebAuthn { library_record, .. } => Some(library_record),
                    _ => None,
                })
                .collect::<Vec<_>>();
            Ok((a, s, credentials))
        })?;
        let digest = request.intent_digest().map_err(|_| StoreError::Corrupt)?;
        let index = format!(
            "custody_request:{}",
            super::digest(
                &self.config.session_key,
                "custody-request",
                &format!(
                    "{}:{}:{}",
                    account.account_id, config.audience, request.approval_request_id
                )
            )
        );
        if let Some(saved) = self.store.transaction(|tx| tx.get::<String>(&index))? {
            let record = self
                .store
                .transaction(|tx| {
                    tx.get::<ApprovalRecord>(&approval_key(&saved.value))?
                        .ok_or(StoreError::Corrupt)
                })?
                .value;
            if record.intent_digest != digest || record.manager_session_ref != session.session_id {
                return Err(reject("idempotency_conflict"));
            }
            return record_response(&record, now);
        }
        request
            .validate(now)
            .map_err(|_| reject("invalid_request"))?;
        if credentials.is_empty() {
            return Err(reject("reauth_required"));
        }
        let (options, state) = self
            .webauthn
            .authentication_options(&credentials)
            .map_err(|_| reject("authentication_failed"))?;
        let record = ApprovalRecord {
            approval_id: random_token(),
            request,
            intent_digest: digest,
            session_storage_key: key.clone(),
            manager_account_id: account.account_id.clone(),
            manager_session_ref: session.session_id,
            policy_version: account.policy_version,
            security_epoch: account.security_epoch,
            origin_credential_id: session.origin_credential_id,
            origin_credential_version: session.origin_credential_version,
            options,
            state,
            ceremony_expires_at: now.checked_add(120_000).ok_or(StoreError::Corrupt)?,
            failed_attempts: 0,
            complete_digest: None,
            proof_bytes: None,
        };
        let secret = self.config.session_key;
        self.store.transaction(|tx| {
            let (a, _) = management_session(tx, &key, epoch, now)?;
            if a.policy_version != record.policy_version
                || a.security_epoch != record.security_epoch
            {
                return Err(reject("state_changed"));
            }
            consume_quota(
                tx,
                "custody-options-source",
                &super::digest(&secret, "source", source),
                now,
                60_000,
                60,
            )?;
            consume_quota(
                tx,
                "custody-options-manager",
                &a.account_id,
                now,
                60_000,
                10,
            )?;
            tx.create(
                &index,
                "custody_request",
                Some(&a.account_id),
                None,
                &record.approval_id,
            )?;
            tx.create(
                &approval_key(&record.approval_id),
                "custody_approval",
                Some(&a.account_id),
                None,
                &record,
            )?;
            audit(tx, &a, "custody_approval_requested", now)
        })?;
        record_response(&record, now)
    }
    pub(super) fn custody_verify_route(
        &mut self,
        method: &str,
        headers: &str,
        payload: &Value,
        _source: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        let config = self
            .custody
            .as_ref()
            .ok_or_else(|| reject("custody_disabled"))?;
        if method != "POST"
            || header(headers, "origin").is_some()
            || header(headers, "cookie").is_some()
            || header(headers, "content-type").is_none_or(|v| v != "application/json")
        {
            return Err(reject("service_auth_failed"));
        }
        let auth = header(headers, "authorization")
            .and_then(|v| v.strip_prefix("Bearer "))
            .and_then(|v| hex::decode(v).ok())
            .filter(|v| v.len() == 32)
            .ok_or_else(|| reject("service_auth_failed"))?;
        if blake3::keyed_hash(&self.config.session_key, &auth)
            != blake3::keyed_hash(&self.config.session_key, &config.adapter_secret)
        {
            return Err(reject("service_auth_failed"));
        }
        let request: ApprovalVerifyRequestV1 =
            serde_json::from_value(payload.clone()).map_err(|_| reject("invalid_request"))?;
        self.verify_approval(&request, now)
    }
    fn verify_approval(
        &mut self,
        request: &ApprovalVerifyRequestV1,
        now: u64,
    ) -> Result<Value, StoreError> {
        let config = self
            .custody
            .as_ref()
            .ok_or_else(|| reject("custody_disabled"))?;
        let proof = &request.approval.proof;
        if request.verify_nonce.is_empty()
            || request.verify_nonce.len() > 256
            || proof.version != 1
            || proof.key_id != config.key_id
            || proof.identity_namespace != self.config.namespace.key()
            || request.expected_audience != config.audience
            || proof.context.audience != request.expected_audience
            || proof.context.environment != self.config.namespace.environment_id
            || proof.intent_digest != request.expected_intent_digest
            || request.approval.digest().map_err(|_| StoreError::Corrupt)? != request.proof_digest
        {
            return Err(reject("approval_binding_mismatch"));
        }
        let signature = URL_SAFE_NO_PAD
            .decode(&request.approval.signature)
            .ok()
            .and_then(|v| Signature::from_slice(&v).ok())
            .ok_or_else(|| reject("approval_signature_invalid"))?;
        config
            .key
            .verifying_key()
            .verify_strict(
                &proof.signing_bytes().map_err(|_| StoreError::Corrupt)?,
                &signature,
            )
            .map_err(|_| reject("approval_signature_invalid"))?;
        let epoch = self.config.rp.identity_instance_epoch;
        let result = self.store.transaction(|tx| {
            consume_quota(tx, "custody-verify", "adapter", now, 60_000, 300)?;
            let saved = tx
                .get::<ApprovalRecord>(&approval_key(&proof.approval_id))?
                .ok_or_else(|| reject("approval_history_unavailable"))?
                .value;
            let original: SignedCustodyApprovalV1 =
                serde_json::from_slice(saved.proof_bytes.as_deref().ok_or(StoreError::Corrupt)?)
                    .map_err(|_| StoreError::Corrupt)?;
            if original != request.approval {
                return Err(reject("approval_original_mismatch"));
            }
            let source_session = tx.get::<AccountSession>(&saved.session_storage_key)?;
            let passkey =
                tx.get::<CredentialRecord>(&credential_key(&proof.passkey_credential_id))?;
            let account =
                tx.get::<AccountRecord>(&format!("account:{}", proof.manager_account_id))?;
            let origin_credential =
                tx.get::<CredentialRecord>(&credential_key(&proof.origin_credential_id))?;
            let origin_active =
                valid_session(tx, &saved.session_storage_key, epoch, now).is_ok_and(|(a, s)| {
                    a.policy_version == proof.manager_policy_version.0
                        && a.security_epoch == proof.manager_security_version.0
                        && s.session_id == proof.manager_session_ref
                        && s.origin_credential_id == proof.origin_credential_id
                        && s.origin_credential_version == proof.origin_credential_version.0
                }) && passkey.as_ref().is_some_and(|c| {
                    c.value.account_id == proof.manager_account_id
                        && c.value.state == "active"
                        && c.value.version == proof.passkey_credential_version.0
                        && matches!(c.value.public_material, CredentialMaterial::WebAuthn { .. })
                });
            let origin_state = if source_session.is_none()
                || account.is_none()
                || passkey.is_none()
                || origin_credential.is_none()
            {
                OriginStateV1::Unavailable
            } else if origin_active {
                OriginStateV1::Active
            } else {
                OriginStateV1::Revoked
            };
            let proof_state = if proof.expires_at.0 <= now {
                ProofStateV1::Expired
            } else {
                ProofStateV1::Active
            };
            if request.purpose == VerificationPurposeV1::ConsumeApproval
                && (proof_state != ProofStateV1::Active || origin_state != OriginStateV1::Active)
            {
                return Err(reject("approval_expired_or_revoked"));
            }
            Ok(ApprovalVerificationV1 {
                version: 1,
                key_id: config.key_id.clone(),
                identity_namespace: self.config.namespace.key(),
                purpose: request.purpose,
                proof_digest: request.proof_digest.clone(),
                approval_request_id: proof.approval_request_id.clone(),
                audience: proof.context.audience.clone(),
                intent_digest: proof.intent_digest.clone(),
                verify_nonce: request.verify_nonce.clone(),
                manager_security_version: DecimalU64(
                    account.as_ref().map_or(0, |a| a.value.security_epoch),
                ),
                manager_policy_version: DecimalU64(
                    account.as_ref().map_or(0, |a| a.value.policy_version),
                ),
                origin_credential_version: DecimalU64(
                    origin_credential.map_or(0, |c| c.value.version),
                ),
                identity_instance_epoch: DecimalU64(epoch),
                credential_version: DecimalU64(passkey.map_or(0, |c| c.value.version)),
                observed_at: DecimalU64(now),
                proof_expires_at: proof.expires_at,
                source_session_expires_at: DecimalU64(
                    source_session.as_ref().map_or(0, |s| s.value.expires_at),
                ),
                source_session_idle_expires_at: DecimalU64(
                    source_session
                        .as_ref()
                        .map_or(0, |s| s.value.idle_expires_at),
                ),
                proof_state,
                origin_state,
            })
        })?;
        let signed = SignedApprovalVerificationV1 {
            signature: URL_SAFE_NO_PAD.encode(
                config
                    .key
                    .sign(&result.signing_bytes().map_err(|_| StoreError::Corrupt)?)
                    .to_bytes(),
            ),
            verification: result,
        };
        serde_json::to_value(signed).map_err(|_| StoreError::Corrupt)
    }
}

#[cfg(test)]
#[path = "custody_tests.rs"]
mod tests;
