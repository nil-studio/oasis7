//! Native first-account registration in the existing identity transaction.
//! No world admission, session issuance or executor authority is implied.
use super::{
    digest,
    model::*,
    native_key, random_token, reject,
    service::{IdentityService, audit, check_ceremony, consume_quota, credential_key},
    store::StoreError,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, VerifyingKey};
use oasis7_agent_api::identity::{NativeIdentityChallengeV1, NativeIdentityPurposeV1};
use serde_json::{Value, json};

#[cfg(test)]
#[path = "machine_registration_tests.rs"]
mod tests;

fn operation(p: &Value) -> Result<&str, StoreError> {
    p["operation_id"]
        .as_str()
        .filter(|id| {
            URL_SAFE_NO_PAD
                .decode(id)
                .is_ok_and(|v| v.len() == 32 && URL_SAFE_NO_PAD.encode(v) == *id)
        })
        .ok_or_else(|| reject("invalid_request"))
}
fn fields(p: &Value, allowed: &[&str]) -> Result<(), StoreError> {
    if !p
        .as_object()
        .is_some_and(|p| p.keys().all(|k| allowed.contains(&k.as_str())))
    {
        return Err(reject("invalid_request"));
    }
    Ok(())
}
fn committed(receipt: &OperationReceipt) -> Value {
    json!({"api_version":2,"ok":true,"phase":"account_registered","operation_id":receipt.operation_id,"receipt":{"state":receipt.state,"account_id":receipt.result_ids.first()},"next_action":"fresh_authentication","world":{"state":"not_requested"}})
}
impl IdentityService {
    #[expect(
        clippy::too_many_arguments,
        reason = "Retain HTTP owner, source and audience boundaries"
    )]
    pub(super) fn machine_registration_route(
        &mut self,
        method: &str,
        path: &str,
        owner: &str,
        p: &Value,
        source: &str,
        origin: &str,
        now: u64,
    ) -> Option<Result<Value, StoreError>> {
        match (method, path) {
            ("POST", "/machine-registration/options") => {
                Some(self.machine_registration_options(owner, p, source, origin, now))
            }
            ("POST", "/machine-registration/complete") => {
                Some(self.machine_registration_complete(owner, p, origin, now))
            }
            _ => None,
        }
    }

    fn machine_registration_options(
        &mut self,
        owner: &str,
        p: &Value,
        source: &str,
        origin: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        fields(p, &["operation_id", "public_key", "display_name"])?;
        let id = operation(p)?;
        let key = native_key::public_key(p)?;
        if VerifyingKey::from_bytes(&key)
            .map_err(|_| reject("invalid_request"))?
            .is_weak()
        {
            return Err(reject("invalid_request"));
        }
        let name = p["display_name"]
            .as_str()
            .filter(|v| !v.trim().is_empty() && v.len() <= 128 && !v.chars().any(char::is_control))
            .ok_or_else(|| reject("invalid_request"))?;
        let request_digest = digest(
            &self.config.session_key,
            "machine-registration-options",
            &json!({"public_key":hex::encode(key),"display_name":name,"audience":origin})
                .to_string(),
        );
        let nonce = random_token();
        let nonce_bytes: [u8; 32] = URL_SAFE_NO_PAD
            .decode(&nonce)
            .map_err(|_| StoreError::Corrupt)?
            .try_into()
            .map_err(|_| StoreError::Corrupt)?;
        let expires = now
            .checked_add(120_000)
            .ok_or_else(|| reject("invalid_request"))?;
        let bytes = NativeIdentityChallengeV1 {
            identity_service_id: &self.config.namespace.identity_service_id,
            environment_id: &self.config.namespace.environment_id,
            audience: origin,
            purpose: NativeIdentityPurposeV1::Register,
            account_context: "",
            operation_id: id,
            public_key: &key,
            nonce: &nonce_bytes,
            issued_at: now,
            expires_at: expires,
        }
        .signing_bytes()
        .map_err(|_| reject("invalid_request"))?;
        let options = json!({"identity_service_id":self.config.namespace.identity_service_id,"environment_id":self.config.namespace.environment_id,"audience":origin,"purpose":"register","account_context":"","operation_id":id,"public_key":hex::encode(key),"nonce":nonce,"issued_at":now,"expires_at":expires,"signed_bytes":URL_SAFE_NO_PAD.encode(bytes)});
        let ceremony = CeremonyRecord {
            ceremony_id: id.into(),
            kind: "machine_register".into(),
            namespace: self.config.namespace.clone(),
            rp_config_version: self.config.rp.version,
            owner_context_digest: owner.into(),
            target_account_id: Some(random_token()),
            target_digest: Some(request_digest.clone()),
            expires_at: expires,
            failed_attempts: 0,
            status: "pending".into(),
            server_state: Some(
                json!({"options":options,"display_name":name,"source_digest":digest(&self.config.session_key,"source",source)}),
            ),
            result_ref: None,
        };
        let config = &self.config;
        self.store.transaction(|tx| {
            let ckey = format!("ceremony:{id}");
            if let Some(old) = tx.get::<CeremonyRecord>(&ckey)? {
                if old.value.kind != "machine_register" || old.value.owner_context_digest != owner || old.value.target_digest.as_deref() != Some(&request_digest) {
                    return Err(reject("idempotency_conflict"));
                }
                if let Some(receipt) = tx.get::<OperationReceipt>(&format!("receipt:{id}"))? {
                    return Ok(committed(&receipt.value));
                }
                check_ceremony(&old.value, owner, now)?;
                if old.value.namespace != config.namespace || old.value.rp_config_version != config.rp.version { return Err(reject("state_changed")); }
                return Ok(json!({"api_version":2,"ok":true,"ceremony_id":id,"operation_id":id,"options":old.value.server_state.as_ref().ok_or(StoreError::Corrupt)?["options"],"expires_at":old.value.expires_at}));
            }
            if tx.get::<OperationReceipt>(&format!("receipt:{id}"))?.is_some() { return Err(reject("idempotency_conflict")); }
            if !config.registration_enabled || !config.machine_registration_enabled { return Err(reject("registration_closed")); }
            consume_quota(tx,"register-source",&digest(&config.session_key,"source",source),now,60_000,60)?;
            consume_quota(tx,"register-total","deployment",now,60_000,300)?;
            // Retain the original operation identity across proof expiry and restart.
            tx.create(&ckey,"ceremony",None,None,&ceremony)?;
            Ok(json!({"api_version":2,"ok":true,"ceremony_id":id,"operation_id":id,"options":options,"expires_at":expires}))
        })
    }

    fn machine_registration_complete(
        &mut self,
        owner: &str,
        p: &Value,
        origin: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        fields(p, &["operation_id", "ceremony_id", "signature"])?;
        let id = operation(p)?;
        if p["ceremony_id"].as_str() != Some(id) {
            return Err(reject("invalid_request"));
        }
        let request_digest = digest(
            &self.config.session_key,
            "machine-registration-complete",
            &p.to_string(),
        );
        let ckey = format!("ceremony:{id}");
        // Receipt recovery is permitted with the exact original proof, even when
        // registration is closed or the original proof has expired. No session.
        if let Some(receipt) = self
            .store
            .transaction(|tx| tx.get::<OperationReceipt>(&format!("receipt:{id}")))?
        {
            if receipt.value.kind != "machine_register"
                || receipt.value.owner_context_digest != owner
                || receipt.value.request_digest != request_digest
            {
                return Err(reject("idempotency_conflict"));
            }
            return Ok(committed(&receipt.value));
        }
        let ceremony = self
            .store
            .transaction(|tx| tx.get::<CeremonyRecord>(&ckey))?
            .ok_or_else(|| reject("authentication_failed"))?
            .value;
        check_ceremony(&ceremony, owner, now)?;
        let state = ceremony.server_state.as_ref().ok_or(StoreError::Corrupt)?;
        let options = &state["options"];
        if ceremony.kind != "machine_register"
            || ceremony.namespace != self.config.namespace
            || ceremony.rp_config_version != self.config.rp.version
            || options["audience"] != origin
            || options["purpose"] != "register"
        {
            return Err(reject("authentication_failed"));
        }
        let key = native_key::public_key(options)?;
        let proof = (|| {
            let sig = p["signature"]
                .as_str()
                .filter(|s| s.len() == 86)
                .and_then(|s| URL_SAFE_NO_PAD.decode(s).ok())
                .and_then(|s| Signature::from_slice(&s).ok())
                .ok_or_else(|| reject("authentication_failed"))?;
            let bytes = URL_SAFE_NO_PAD
                .decode(
                    options["signed_bytes"]
                        .as_str()
                        .ok_or(StoreError::Corrupt)?,
                )
                .map_err(|_| StoreError::Corrupt)?;
            VerifyingKey::from_bytes(&key)
                .map_err(|_| reject("authentication_failed"))?
                .verify_strict(&bytes, &sig)
                .map_err(|_| reject("authentication_failed"))
        })();
        if let Err(error) = proof {
            self.record_ceremony_failure(&ckey, owner, now)?;
            return Err(error);
        }
        let config = &self.config;
        self.store.transaction(|tx| {
            let mut c = tx
                .get::<CeremonyRecord>(&ckey)?
                .ok_or(StoreError::Corrupt)?;
            if let Some(receipt) = tx.get::<OperationReceipt>(&format!("receipt:{id}"))? {
                if receipt.value.kind != "machine_register"
                    || receipt.value.owner_context_digest != owner
                    || receipt.value.request_digest != request_digest
                {
                    return Err(reject("idempotency_conflict"));
                }
                return Ok(committed(&receipt.value));
            }
            check_ceremony(&c.value, owner, now)?;
            if c.value.server_state != ceremony.server_state
                || c.value.kind != ceremony.kind
                || c.value.namespace != config.namespace
                || c.value.rp_config_version != config.rp.version
            {
                return Err(reject("state_changed"));
            }
            if !config.registration_enabled || !config.machine_registration_enabled {
                return Err(reject("registration_closed"));
            }
            let lookup_key = native_key::lookup_key(&key);
            if tx.get::<CredentialLookup>(&lookup_key)?.is_some() {
                return Err(reject("credential_conflict"));
            }
            consume_quota(
                tx,
                "accounts-source",
                state["source_digest"].as_str().ok_or(StoreError::Corrupt)?,
                now,
                3_600_000,
                10,
            )?;
            consume_quota(tx, "accounts-total", "deployment", now, 86_400_000, 100)?;
            let account_id = c
                .value
                .target_account_id
                .clone()
                .ok_or(StoreError::Corrupt)?;
            let account = AccountRecord {
                account_id: account_id.clone(),
                user_handle: uuid::Uuid::new_v4().as_bytes().to_vec(),
                display_name: state["display_name"]
                    .as_str()
                    .ok_or(StoreError::Corrupt)?
                    .into(),
                status: "active".into(),
                policy_version: 1,
                security_epoch: 1,
                has_ever_public_key: true,
                created_at: now,
                last_authenticated_at: None,
                last_recovery_id: None,
            };
            let cid = random_token();
            let credential = CredentialRecord {
                credential_record_id: cid.clone(),
                account_id: account_id.clone(),
                state: "active".into(),
                version: 1,
                label: "机器账号密钥".into(),
                created_at: now,
                last_used_at: None,
                public_material: CredentialMaterial::NativeKey { public_key: key },
                registered_via: "machine_registration".into(),
            };
            tx.create(
                &format!("account:{account_id}"),
                "account",
                Some(&account_id),
                None,
                &account,
            )?;
            tx.create(
                &credential_key(&cid),
                "credential",
                Some(&account_id),
                None,
                &credential,
            )?;
            tx.create(
                &lookup_key,
                "lookup",
                Some(&account_id),
                None,
                &CredentialLookup {
                    scope: "native:ed25519:v1".into(),
                    key_digest: blake3::hash(&key).to_hex().to_string(),
                    canonical_key: key.to_vec(),
                    credential_record_id: cid,
                    account_id: account_id.clone(),
                    state: "active".into(),
                },
            )?;
            c.value.status = "consumed".into();
            c.value.result_ref = Some(account_id.clone());
            tx.replace(
                &ckey,
                c.row_version,
                "ceremony",
                Some(&account_id),
                None,
                &c.value,
            )?;
            let receipt = OperationReceipt {
                operation_id: id.into(),
                kind: "machine_register".into(),
                owner_context_digest: owner.into(),
                request_digest: request_digest.clone(),
                state: "committed".into(),
                result_ids: vec![account_id.clone()],
                committed_at: now,
            };
            tx.create(
                &format!("receipt:{id}"),
                "receipt",
                Some(&account_id),
                None,
                &receipt,
            )?;
            audit(tx, &account, "account_registered", now)?;
            Ok(committed(&receipt))
        })
    }
}
