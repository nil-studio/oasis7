use super::*;

fn return_original(record: &ApprovalRecord, now: u64) -> Result<Value, StoreError> {
    let bytes = record.proof_bytes.as_ref().ok_or(StoreError::Corrupt)?;
    let signed: SignedCustodyApprovalV1 =
        serde_json::from_slice(bytes).map_err(|_| StoreError::Corrupt)?;
    if signed.proof.expires_at.0 <= now {
        return Err(reject("approval_expired"));
    }
    serde_json::to_value(ApprovalCompleteResponseV1 {
        api_version: 1,
        phase: ApprovalPhaseV1::Completed,
        approval: signed,
        proof_bytes: URL_SAFE_NO_PAD.encode(bytes),
    })
    .map_err(|_| StoreError::Corrupt)
}

impl IdentityService {
    pub(super) fn custody_complete(
        &mut self,
        sid: Option<&str>,
        payload: &Value,
        source: &str,
        now: u64,
    ) -> Result<Value, StoreError> {
        let request: ApprovalCompleteRequestV1 =
            serde_json::from_value(payload.clone()).map_err(|_| reject("invalid_request"))?;
        if request.approval_id.is_empty()
            || request.approval_id.len() > 128
            || request.ceremony_id != request.approval_id
            || request.operation_id != request.approval_id
        {
            return Err(reject("invalid_request"));
        }
        let record_key = approval_key(&request.approval_id);
        let session_key = session_key(
            &self.config.session_key,
            sid.ok_or_else(|| reject("session_expired"))?,
        );
        let epoch = self.config.rp.identity_instance_epoch;
        let complete_digest = complete_body_digest(&request).map_err(|_| StoreError::Corrupt)?;
        let secret = self.config.session_key;
        let saved = self.store.transaction(|tx| {
            consume_quota(
                tx,
                "custody-complete-source",
                &digest(&secret, "source", source),
                now,
                60_000,
                60,
            )?;
            let (account, session) = management_session(tx, &session_key, epoch, now)?;
            let mut saved = tx
                .get::<ApprovalRecord>(&record_key)?
                .ok_or_else(|| reject("approval_not_found"))?;
            check_record(
                &saved.value,
                &account,
                &session,
                &session_key,
                &request.intent_digest,
            )?;
            if saved
                .value
                .complete_digest
                .as_ref()
                .is_some_and(|v| v != &complete_digest)
            {
                return Err(reject("idempotency_conflict"));
            }
            if saved.value.complete_digest.is_none() {
                saved.value.complete_digest = Some(complete_digest.clone());
                tx.replace(
                    &record_key,
                    saved.row_version,
                    "custody_approval",
                    Some(&account.account_id),
                    None,
                    &saved.value,
                )?;
                return tx
                    .get::<ApprovalRecord>(&record_key)?
                    .ok_or(StoreError::Corrupt);
            }
            Ok(saved)
        })?;
        if saved.value.proof_bytes.is_some() {
            // The approving Passkey may have been revoked independently of the
            // session's originating credential since the first completion.
            let original: SignedCustodyApprovalV1 = serde_json::from_slice(
                saved
                    .value
                    .proof_bytes
                    .as_deref()
                    .ok_or(StoreError::Corrupt)?,
            )
            .map_err(|_| StoreError::Corrupt)?;
            self.store.transaction(|tx| {
                let c = tx
                    .get::<CredentialRecord>(&credential_key(
                        &original.proof.passkey_credential_id,
                    ))?
                    .ok_or_else(|| reject("approval_revoked"))?
                    .value;
                if c.state != "active" || c.version != original.proof.passkey_credential_version.0 {
                    return Err(reject("approval_revoked"));
                }
                Ok(())
            })?;
            return return_original(&saved.value, now);
        }
        if saved.value.ceremony_expires_at <= now || saved.value.failed_attempts >= 5 {
            return Err(reject("ceremony_expired"));
        }
        saved
            .value
            .request
            .validate(now)
            .map_err(|_| reject("intent_expired"))?;
        let verification = (|| {
            let (handle, raw_id) = self
                .webauthn
                .identify_authentication(&request.response)
                .map_err(|_| reject("authentication_failed"))?;
            let credential = self.store.transaction(|tx| {
                let lookup = tx
                    .get::<CredentialLookup>(&format!("lookup:passkey:{raw_id}"))?
                    .ok_or_else(|| reject("authentication_failed"))?
                    .value;
                if lookup.state != "active" || lookup.account_id != saved.value.manager_account_id {
                    return Err(reject("authentication_failed"));
                }
                tx.get::<CredentialRecord>(&credential_key(&lookup.credential_record_id))?
                    .ok_or_else(|| reject("authentication_failed"))
            })?;
            if credential.value.account_id != saved.value.manager_account_id
                || credential.value.state != "active"
            {
                return Err(reject("authentication_failed"));
            }
            let updated = match &credential.value.public_material {
                CredentialMaterial::WebAuthn {
                    user_handle,
                    library_record,
                    rp_id,
                    ..
                } if rp_id == &self.config.rp.rp_id
                    && uuid::Uuid::from_slice(user_handle)
                        .is_ok_and(|id| id.to_string() == handle) =>
                {
                    self.webauthn
                        .finish_authentication(
                            &request.response,
                            &saved.value.state,
                            library_record,
                        )
                        .map_err(|_| reject("authentication_failed"))?
                }
                _ => return Err(reject("authentication_failed")),
            };
            Ok((credential, updated))
        })();
        let (credential, updated) = match verification {
            Ok(value) => value,
            Err(error) => {
                if matches!(&error, StoreError::Rejected(code) if code == "authentication_failed") {
                    self.store.transaction(|tx| {
                        let mut record = tx
                            .get::<ApprovalRecord>(&record_key)?
                            .ok_or(StoreError::Corrupt)?;
                        if record.value.proof_bytes.is_none() && record.value.failed_attempts < 5 {
                            record.value.failed_attempts += 1;
                            if record.value.failed_attempts >= 5 {
                                record.value.state = Value::Null;
                            }
                            tx.replace(
                                &record_key,
                                record.row_version,
                                "custody_approval",
                                Some(&record.value.manager_account_id),
                                None,
                                &record.value,
                            )?;
                        }
                        Ok(())
                    })?;
                }
                return Err(error);
            }
        };
        let config = self
            .custody
            .as_ref()
            .ok_or_else(|| reject("custody_disabled"))?;
        let proof = CustodyApprovalProofV1 {
            version: 1,
            key_id: config.key_id.clone(),
            identity_namespace: self.config.namespace.key(),
            approval_id: saved.value.approval_id.clone(),
            approval_request_id: saved.value.request.approval_request_id.clone(),
            context: saved.value.request.context.clone(),
            intent_digest: saved.value.intent_digest.clone(),
            manager_account_id: saved.value.manager_account_id.clone(),
            manager_session_ref: saved.value.manager_session_ref.clone(),
            origin_credential_id: saved.value.origin_credential_id.clone(),
            origin_credential_version: DecimalU64(saved.value.origin_credential_version),
            passkey_credential_id: credential.value.credential_record_id.clone(),
            passkey_credential_version: DecimalU64(credential.value.version),
            manager_policy_version: DecimalU64(saved.value.policy_version),
            manager_security_version: DecimalU64(saved.value.security_epoch),
            identity_instance_epoch: DecimalU64(epoch),
            nonce: random_token(),
            issued_at: DecimalU64(now),
            expires_at: DecimalU64(now.checked_add(30_000).ok_or(StoreError::Corrupt)?),
        };
        let signed = SignedCustodyApprovalV1 {
            signature: URL_SAFE_NO_PAD.encode(
                config
                    .key
                    .sign(&proof.signing_bytes().map_err(|_| StoreError::Corrupt)?)
                    .to_bytes(),
            ),
            proof,
        };
        let proof_bytes = serde_json::to_vec(&signed).map_err(|_| StoreError::Corrupt)?;
        let completed = self.store.transaction(|tx| {
            let (account, session) = management_session(tx, &session_key, epoch, now)?;
            let mut current = tx
                .get::<ApprovalRecord>(&record_key)?
                .ok_or(StoreError::Corrupt)?;
            check_record(
                &current.value,
                &account,
                &session,
                &session_key,
                &request.intent_digest,
            )?;
            if current.row_version != saved.row_version
                || current.value.proof_bytes.is_some()
                || current.value.failed_attempts >= 5
                || current.value.ceremony_expires_at <= now
            {
                return Err(reject("state_changed"));
            }
            let mut c = tx
                .get::<CredentialRecord>(&credential_key(&credential.value.credential_record_id))?
                .ok_or(StoreError::Corrupt)?;
            if c.row_version != credential.row_version
                || c.value.state != "active"
                || c.value.version != credential.value.version
                || c.value.account_id != account.account_id
            {
                return Err(reject("state_changed"));
            }
            if let CredentialMaterial::WebAuthn { library_record, .. } =
                &mut c.value.public_material
            {
                *library_record = updated;
            } else {
                return Err(reject("state_changed"));
            }
            c.value.last_used_at = Some(now);
            tx.replace(
                &credential_key(&c.value.credential_record_id),
                c.row_version,
                "credential",
                Some(&account.account_id),
                None,
                &c.value,
            )?;
            current.value.complete_digest = Some(complete_digest.clone());
            current.value.proof_bytes = Some(proof_bytes.clone());
            current.value.state = Value::Null;
            tx.replace(
                &record_key,
                current.row_version,
                "custody_approval",
                Some(&account.account_id),
                None,
                &current.value,
            )?;
            audit(tx, &account, "custody_approval_completed", now)?;
            Ok(current.value)
        })?;
        return_original(&completed, now)
    }
}

fn check_record(
    record: &ApprovalRecord,
    account: &AccountRecord,
    session: &AccountSession,
    key: &str,
    intent_digest: &str,
) -> Result<(), StoreError> {
    if record.manager_account_id != account.account_id
        || record.manager_session_ref != session.session_id
        || record.session_storage_key != key
        || record.policy_version != account.policy_version
        || record.security_epoch != account.security_epoch
        || record.origin_credential_id != session.origin_credential_id
        || record.origin_credential_version != session.origin_credential_version
        || record.intent_digest != intent_digest
    {
        return Err(reject("approval_source_changed"));
    }
    Ok(())
}
