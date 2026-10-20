use super::*;
impl IdentityService {
    pub(super) fn native_complete(
        &mut self,
        sid: Option<&str>,
        owner: &str,
        p: &Value,
        origin: &str,
        now: u64,
    ) -> Result<(Value, Option<String>), StoreError> {
        let id = p["ceremony_id"]
            .as_str()
            .filter(|s| s.len() == 43)
            .ok_or_else(|| reject("invalid_request"))?;
        if p["operation_id"].as_str() != Some(id) {
            return Err(reject("invalid_request"));
        }
        let response_digest = digest(&self.config.session_key, "native-response", &p.to_string());
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
                json!({"api_version":2,"ok":true,"phase":"operation_committed","next_action":"fresh_authentication"}),
                None,
            ));
        }
        let ckey = format!("ceremony:{id}");
        let ceremony = self
            .store
            .transaction(|tx| tx.get::<CeremonyRecord>(&ckey))?
            .ok_or_else(|| reject("authentication_failed"))?
            .value;
        check_ceremony(&ceremony, owner, now)?;
        let state = ceremony.server_state.as_ref().ok_or(StoreError::Corrupt)?;
        if !["native_login", "native_bind", "native_reauth"].contains(&ceremony.kind.as_str())
            || ceremony.namespace != self.config.namespace
            || ceremony.rp_config_version != self.config.rp.version
            || state["audience"] != origin
        {
            return Err(reject("authentication_failed"));
        }
        let key = public_key(state)?;
        let verification = (|| {
            let sig = p["signature"]
                .as_str()
                .filter(|s| s.len() == 86)
                .and_then(|s| URL_SAFE_NO_PAD.decode(s).ok())
                .and_then(|b| Signature::from_slice(&b).ok())
                .ok_or_else(|| reject("authentication_failed"))?;
            let bytes = URL_SAFE_NO_PAD
                .decode(state["signed_bytes"].as_str().ok_or(StoreError::Corrupt)?)
                .map_err(|_| StoreError::Corrupt)?;
            VerifyingKey::from_bytes(&key)
                .map_err(|_| reject("authentication_failed"))?
                .verify_strict(&bytes, &sig)
                .map_err(|_| reject("authentication_failed"))
        })();
        if let Err(error) = verification {
            self.record_ceremony_failure(&ckey, owner, now)?;
            return Err(error);
        }
        let config = &self.config;
        let token = random_token();
        let skey = sid.map(|s| session_key(&config.session_key, s));
        self.store.transaction(|tx| {
            let mut c = tx.get::<CeremonyRecord>(&ckey)?.ok_or(StoreError::Corrupt)?;
            check_ceremony(&c.value, owner, now)?;
            // The entire verified challenge must still be the stored challenge.
            if c.value.server_state != ceremony.server_state || c.value.kind != ceremony.kind || c.value.namespace != config.namespace || c.value.rp_config_version != config.rp.version { return Err(reject("state_changed")); }
            let purpose = state["purpose"].as_str().ok_or(StoreError::Corrupt)?;
            let (account_id, bound_session) = if purpose == "login" {
                (native_credential(tx, &key)?.value.account_id, None)
            } else {
                let (a, s) = valid_session(tx, skey.as_deref().ok_or_else(|| reject("session_expired"))?, config.rp.identity_instance_epoch, now)?;
                if c.value.target_account_id.as_deref() != Some(a.account_id.as_str()) { return Err(reject("authentication_failed")); }
                (a.account_id.clone(), Some((a, s)))
            };
            let akey = format!("account:{account_id}");
            let mut account = tx.get::<AccountRecord>(&akey)?.ok_or(StoreError::Corrupt)?;
            if account.value.status != "active" { return Err(reject("authentication_failed")); }
            let credential = if purpose == "bind" {
                let (a, s) = bound_session.as_ref().ok_or(StoreError::Corrupt)?;
                let aid = c.value.target_digest.as_deref().ok_or(StoreError::Corrupt)?;
                let mut action = authorized_action(tx, aid, a, s, "bind", None, now)?;
                if active_keys(tx, &account_id)?.len() >= 10 { return Err(reject("credential_limit")); }
                if tx.get::<CredentialLookup>(&lookup_key(&key))?.is_some() { return Err(reject("credential_conflict")); }
                let cid = random_token();
                let cred = CredentialRecord { credential_record_id:cid.clone(), account_id:account_id.clone(), state:"active".into(), version:1, label:"导入密钥".into(), created_at:now, last_used_at:Some(now), public_material:CredentialMaterial::NativeKey { public_key:key }, registered_via:"public_key".into() };
                tx.create(&credential_key(&cid), "credential", Some(&account_id), None, &cred)?;
                tx.create(&lookup_key(&key), "lookup", Some(&account_id), None, &CredentialLookup { scope:"native:ed25519:v1".into(), key_digest:blake3::hash(&key).to_hex().to_string(), canonical_key:key.to_vec(), credential_record_id:cid, account_id:account_id.clone(), state:"active".into() })?;
                action.value.state = "consumed".into();
                tx.replace(&format!("action:{aid}"), action.row_version, "action", Some(&account_id), Some(action.value.expires_at), &action.value)?;
                account.value.policy_version += 1; account.value.has_ever_public_key = true;
                cred
            } else {
                let mut cred = native_credential(tx, &key)?;
                if cred.value.account_id != account_id { return Err(reject("authentication_failed")); }
                cred.value.last_used_at = Some(now);
                tx.replace(&credential_key(&cred.value.credential_record_id), cred.row_version, "credential", Some(&account_id), None, &cred.value)?;
                cred.value
            };
            if purpose == "reauth" {
                let (a, s) = bound_session.as_ref().ok_or(StoreError::Corrupt)?;
                let aid = c.value.target_digest.as_deref().ok_or(StoreError::Corrupt)?;
                let mut action = pending_action(tx, aid, a, s, now)?;
                action.value.state = "authorized".into(); action.value.proof_credential_id = Some(credential.credential_record_id.clone()); action.value.proof_time = Some(now); action.value.expires_at = now + 300_000;
                tx.replace(&format!("action:{aid}"), action.row_version, "action", Some(&account_id), Some(action.value.expires_at), &action.value)?;
            }
            let session_token = if purpose == "login" {
                account.value.last_authenticated_at = Some(now);
                create_session(tx, config, &account.value, &credential, &token, now)?; Some(token.clone())
            } else { None };
            tx.replace(&akey, account.row_version, "account", Some(&account_id), None, &account.value)?;
            c.value.status = "consumed".into(); c.value.server_state = None; c.value.result_ref = Some(account_id.clone());
            tx.replace(&ckey, c.row_version, "ceremony", Some(&account_id), Some(ceremony_storage_expiry(&c.value)), &c.value)?;
            tx.create(&format!("receipt:{id}"), "receipt", Some(&account_id), Some(now + 7 * 86_400_000), &OperationReceipt { operation_id:id.into(), kind:ceremony.kind.clone(), owner_context_digest:owner.into(), request_digest:response_digest.clone(), state:"committed".into(), result_ids:vec![account_id.clone()], committed_at:now })?;
            audit(tx, &account.value, if purpose == "bind" {"security_changed"} else {"credential_authenticated"}, now)?;
            Ok((json!({"api_version":2,"ok":true,"phase":if purpose == "login" {"authenticated"} else {"credential_verified"},"account":{"account_id":account_id,"display_name":account.value.display_name},"world":{"state":"not_requested"}}), session_token))
        })
    }
}
