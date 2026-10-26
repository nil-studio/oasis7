//! Native durable invalidation bridge. Verification credentials do not authorize this API.
use super::{
    super::{
        model::*,
        now_ms, reject,
        store::{IdentityTransaction, StoreError},
    },
    CustodyConfig,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use oasis7_custody_core::{DecimalU64, source_revocation::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const MAX_SOURCE_EVENTS: usize = 4096;
pub(crate) struct RevocationConfig {
    poll: [u8; 32],
    ack: [u8; 32],
    owner_key_id: String,
    owner_key: VerifyingKey,
    owner_audience: String,
}
#[derive(Clone)]
pub(crate) struct RevocationSigner {
    key: SigningKey,
    key_id: String,
    namespace: String,
    environment: String,
    epoch: u64,
    audience: String,
    pub(crate) metadata: BridgeMetadata,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BridgeMetadata {
    pub namespace: String,
    pub epoch: u64,
    pub owner_key_id: String,
    pub owner_public_key: String,
    pub native_key_id: String,
    pub native_public_key: String,
    pub owner_audience: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceOutbox {
    pub original: SignedSourceRevocationEventV1,
    pub acknowledgement: Option<SignedSourceClosureAckV1>,
    pub last_delivery: DecimalU64,
}
impl RevocationConfig {
    #[cfg(test)]
    pub(crate) fn http_fixture(audience: String) -> Self {
        Self {
            poll: [83; 32],
            ack: [84; 32],
            owner_key_id: "owner-source-test".into(),
            owner_key: SigningKey::from_bytes(&[92; 32]).verifying_key(),
            owner_audience: audience,
        }
    }

    pub(super) fn from_env(
        config: &CustodyConfig,
        session: &[u8; 32],
    ) -> Result<Option<Self>, String> {
        let names = [
            "OASIS7_CUSTODY_REVOCATION_POLL_SECRET",
            "OASIS7_CUSTODY_REVOCATION_ACK_SECRET",
            "OASIS7_CUSTODY_REVOCATION_OWNER_KEY_ID",
            "OASIS7_CUSTODY_REVOCATION_OWNER_PUBLIC_KEY",
            "OASIS7_CUSTODY_REVOCATION_OWNER_AUDIENCE",
        ];
        Self::from_values(
            config,
            session,
            names.iter().map(|name| std::env::var(name).ok()).collect(),
        )
    }
    fn from_values(
        config: &CustodyConfig,
        session: &[u8; 32],
        values: Vec<Option<String>>,
    ) -> Result<Option<Self>, String> {
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        let values = values
            .into_iter()
            .map(|v| {
                v.filter(|s| !s.is_empty())
                    .ok_or_else(|| "complete revocation configuration required".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values.len() != 5 {
            return Err("complete revocation configuration required".into());
        }
        let decode = |v: &str| -> Result<[u8; 32], String> {
            let bytes = hex::decode(v).map_err(|_| "invalid revocation key".to_string())?;
            if hex::encode(&bytes) != v {
                return Err("noncanonical revocation key".into());
            }
            bytes
                .try_into()
                .map_err(|_| "invalid revocation key length".into())
        };
        let poll = decode(&values[0])?;
        let ack = decode(&values[1])?;
        if poll == ack
            || [poll, ack]
                .iter()
                .any(|v| v == session || v == &config.adapter_secret || v == &config.key.to_bytes())
            || values[2].len() > 256
            || values[4] != config.audience
        {
            return Err("revocation credentials must be independent and audience pinned".into());
        }
        let owner_key = VerifyingKey::from_bytes(&decode(&values[3])?)
            .map_err(|_| "invalid Owner key".to_owned())?;
        if owner_key.is_weak() || owner_key == config.key.verifying_key() {
            return Err("weak Owner key".into());
        }
        Ok(Some(Self {
            poll,
            ack,
            owner_key_id: values[2].clone(),
            owner_key,
            owner_audience: values[4].clone(),
        }))
    }
    pub(crate) fn signer(
        &self,
        config: &CustodyConfig,
        namespace: String,
        environment: String,
        epoch: u64,
    ) -> RevocationSigner {
        let metadata = BridgeMetadata {
            namespace: namespace.clone(),
            epoch,
            owner_key_id: self.owner_key_id.clone(),
            owner_public_key: hex::encode(self.owner_key.to_bytes()),
            native_key_id: config.key_id.clone(),
            native_public_key: hex::encode(config.key.verifying_key().to_bytes()),
            owner_audience: self.owner_audience.clone(),
        };
        RevocationSigner {
            metadata,
            key: config.key.clone(),
            key_id: config.key_id.clone(),
            namespace,
            environment,
            epoch,
            audience: self.owner_audience.clone(),
        }
    }
    pub(super) fn authorize(
        &self,
        headers: &str,
        poll: bool,
        session_secret: &[u8; 32],
    ) -> Result<(), StoreError> {
        use super::super::service::header;
        if header(headers, "origin").is_some()
            || header(headers, "cookie").is_some()
            || header(headers, "content-type") != Some("application/json")
        {
            return Err(reject("service_auth_failed"));
        }
        let auth = header(headers, "authorization")
            .and_then(|v| v.strip_prefix("Bearer "))
            .and_then(|v| hex::decode(v).ok())
            .filter(|v| v.len() == 32)
            .ok_or_else(|| reject("service_auth_failed"))?;
        let expected = if poll { &self.poll } else { &self.ack };
        if blake3::keyed_hash(session_secret, &auth) != blake3::keyed_hash(session_secret, expected)
        {
            return Err(reject("service_auth_failed"));
        }
        Ok(())
    }
}
impl RevocationSigner {
    pub(crate) fn record(
        &self,
        tx: &mut IdentityTransaction<'_>,
        key: &str,
        row_version: u64,
        record_kind: &str,
        previous: &Value,
        current: &Value,
    ) -> Result<(), StoreError> {
        let pair = match record_kind {
            "account" => {
                let p: AccountRecord =
                    serde_json::from_value(previous.clone()).map_err(|_| StoreError::Corrupt)?;
                let c: AccountRecord =
                    serde_json::from_value(current.clone()).map_err(|_| StoreError::Corrupt)?;
                if p.account_id != c.account_id {
                    return Err(StoreError::Corrupt);
                }
                Some((
                    p.account_id.clone(),
                    RevokedSourceV1::Account {
                        previous: AccountSecurityStateV1 {
                            status: p.status,
                            policy_version: DecimalU64(p.policy_version),
                            security_version: DecimalU64(p.security_epoch),
                        },
                        current: AccountSecurityStateV1 {
                            status: c.status,
                            policy_version: DecimalU64(c.policy_version),
                            security_version: DecimalU64(c.security_epoch),
                        },
                    },
                ))
            }
            "credential" => {
                let p: CredentialRecord =
                    serde_json::from_value(previous.clone()).map_err(|_| StoreError::Corrupt)?;
                let c: CredentialRecord =
                    serde_json::from_value(current.clone()).map_err(|_| StoreError::Corrupt)?;
                if p.account_id != c.account_id || p.credential_record_id != c.credential_record_id
                {
                    return Err(StoreError::Corrupt);
                }
                Some((
                    p.account_id,
                    RevokedSourceV1::Credential {
                        credential_ref: p.credential_record_id,
                        previous: CredentialSecurityStateV1 {
                            state: p.state,
                            version: DecimalU64(p.version),
                        },
                        current: CredentialSecurityStateV1 {
                            state: c.state,
                            version: DecimalU64(c.version),
                        },
                    },
                ))
            }
            "session" => {
                let p: AccountSession =
                    serde_json::from_value(previous.clone()).map_err(|_| StoreError::Corrupt)?;
                let c: AccountSession =
                    serde_json::from_value(current.clone()).map_err(|_| StoreError::Corrupt)?;
                if p.account_id != c.account_id || p.session_id != c.session_id {
                    return Err(StoreError::Corrupt);
                }
                Some((
                    p.account_id.clone(),
                    RevokedSourceV1::Session {
                        session_ref: p.session_id.clone(),
                        previous: session_state(&p),
                        current: session_state(&c),
                    },
                ))
            }
            _ => None,
        };
        let Some((manager, source)) = pair else {
            return Ok(());
        };
        if !source.invalidates() {
            return Ok(());
        }
        let event_id = SourceRevocationEventV1::stable_id(
            &self.namespace,
            self.epoch,
            key,
            row_version,
            &source,
        )
        .map_err(|_| StoreError::Corrupt)?;
        let manager_security_version = DecimalU64(
            tx.get::<AccountRecord>(&format!("account:{manager}"))?
                .ok_or(StoreError::Corrupt)?
                .value
                .security_epoch,
        );
        let event = SourceRevocationEventV1 {
            version: 1,
            key_id: self.key_id.clone(),
            event_id: event_id.clone(),
            identity_namespace: self.namespace.clone(),
            environment: self.environment.clone(),
            identity_instance_epoch: DecimalU64(self.epoch),
            manager_account_id: manager.clone(),
            manager_security_version,
            owner_audience: self.audience.clone(),
            observed_at: DecimalU64(now_ms()),
            source,
        };
        let signature = URL_SAFE_NO_PAD.encode(
            self.key
                .sign(&event.signing_bytes().map_err(|_| StoreError::Corrupt)?)
                .to_bytes(),
        );
        tx.create(
            &format!("custody_source_revocation:{event_id}"),
            "custody_source_revocation",
            Some(&manager),
            None,
            &SourceOutbox {
                original: SignedSourceRevocationEventV1 { event, signature },
                acknowledgement: None,
                last_delivery: DecimalU64(0),
            },
        )
    }
}
pub(crate) fn verify_barrier_ack(
    record: &SourceOutbox,
    metadata: &BridgeMetadata,
) -> Result<String, StoreError> {
    let signed = record
        .acknowledgement
        .as_ref()
        .ok_or_else(|| reject("custody_revocation_pending"))?;
    let ack = &signed.ack;
    let event = &record.original.event;
    let native_key: [u8; 32] = hex::decode(&metadata.native_public_key)
        .map_err(|_| StoreError::Corrupt)?
        .try_into()
        .map_err(|_| StoreError::Corrupt)?;
    let native_key = VerifyingKey::from_bytes(&native_key).map_err(|_| StoreError::Corrupt)?;
    if native_key.is_weak()
        || event.key_id != metadata.native_key_id
        || event.owner_audience != metadata.owner_audience
    {
        return Err(StoreError::Corrupt);
    }
    native_key
        .verify_strict(
            &event.signing_bytes().map_err(|_| StoreError::Corrupt)?,
            &Signature::from_slice(
                &URL_SAFE_NO_PAD
                    .decode(&record.original.signature)
                    .map_err(|_| StoreError::Corrupt)?,
            )
            .map_err(|_| StoreError::Corrupt)?,
        )
        .map_err(|_| StoreError::Corrupt)?;
    let key: [u8; 32] = hex::decode(&metadata.owner_public_key)
        .map_err(|_| StoreError::Corrupt)?
        .try_into()
        .map_err(|_| StoreError::Corrupt)?;
    let key = VerifyingKey::from_bytes(&key).map_err(|_| StoreError::Corrupt)?;
    if key.is_weak()
        || ack.key_id != metadata.owner_key_id
        || ack.event_id != event.event_id
        || ack.event_digest != record.original.digest().map_err(|_| StoreError::Corrupt)?
        || ack.identity_namespace != metadata.namespace
        || ack.identity_namespace != event.identity_namespace
        || ack.environment != event.environment
        || ack.manager_account_id != event.manager_account_id
        || ack.owner_audience != event.owner_audience
        || ack.identity_instance_epoch != event.identity_instance_epoch
        || event.identity_instance_epoch.0 > metadata.epoch
        || ack.closed_at.0 < event.observed_at.0
        || ack.closed_at.0 > now_ms().saturating_add(5000)
    {
        return Err(StoreError::Corrupt);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(&signed.signature)
        .map_err(|_| StoreError::Corrupt)?;
    key.verify_strict(
        &ack.signing_bytes().map_err(|_| StoreError::Corrupt)?,
        &Signature::from_slice(&bytes).map_err(|_| StoreError::Corrupt)?,
    )
    .map_err(|_| StoreError::Corrupt)?;
    Ok(event.manager_account_id.clone())
}
fn session_state(s: &AccountSession) -> SessionSecurityStateV1 {
    SessionSecurityStateV1 {
        origin_credential_ref: s.origin_credential_id.clone(),
        origin_credential_version: DecimalU64(s.origin_credential_version),
        issued_security_version: DecimalU64(s.issued_security_epoch),
        identity_instance_epoch: DecimalU64(s.identity_instance_epoch),
        auth_method: s.auth_method.clone(),
        revoked_at: s.revoked_at.map(DecimalU64),
        expires_at: DecimalU64(s.expires_at),
        idle_expires_at: DecimalU64(s.idle_expires_at),
    }
}
impl super::super::service::IdentityService {
    pub(crate) fn custody_revocation_route(
        &mut self,
        method: &str,
        path: &str,
        headers: &str,
        payload: &Value,
        now: u64,
    ) -> Result<Value, StoreError> {
        if method != "POST"
            || !matches!(
                path,
                "/custody/revocations/poll" | "/custody/revocations/ack"
            )
        {
            return Err(reject("method_not_allowed"));
        }
        let config = self
            .custody
            .as_ref()
            .and_then(|c| c.revocation.as_ref())
            .ok_or_else(|| reject("custody_disabled"))?;
        let poll = path.ends_with("/poll");
        config.authorize(headers, poll, &self.config.session_key)?;
        if poll {
            let request: SourceRevocationsPollRequestV1 =
                serde_json::from_value(payload.clone()).map_err(|_| reject("invalid_request"))?;
            if request.api_version != 1 || request.limit == 0 || request.limit > 32 {
                return Err(reject("invalid_request"));
            }
            let events = self.store.transaction(|tx| {
                let mut all = tx.all::<SourceOutbox>("custody_source_revocation", None)?;
                let mut sequence = all
                    .iter()
                    .map(|(_, v)| v.value.last_delivery.0)
                    .max()
                    .unwrap_or(0);
                all.sort_by(|(ka, a), (kb, b)| {
                    a.value
                        .last_delivery
                        .0
                        .cmp(&b.value.last_delivery.0)
                        .then(ka.cmp(kb))
                });
                let mut events = Vec::new();
                for (key, mut record) in all
                    .into_iter()
                    .filter(|(_, v)| v.value.acknowledgement.is_none())
                    .take(request.limit as usize)
                {
                    sequence = sequence.checked_add(1).ok_or(StoreError::Corrupt)?;
                    record.value.last_delivery = DecimalU64(sequence);
                    tx.replace(
                        &key,
                        record.row_version,
                        "custody_source_revocation",
                        Some(&record.value.original.event.manager_account_id),
                        None,
                        &record.value,
                    )?;
                    events.push(record.value.original);
                }
                Ok(events)
            })?;
            return serde_json::to_value(SourceRevocationsPollResponseV1 {
                api_version: 1,
                events,
            })
            .map_err(|_| StoreError::Corrupt);
        }
        let signed: SignedSourceClosureAckV1 =
            serde_json::from_value(payload.clone()).map_err(|_| reject("invalid_request"))?;
        let ack = &signed.ack;
        let bytes = ack.signing_bytes().map_err(|_| reject("invalid_request"))?;
        if ack.key_id != config.owner_key_id
            || ack.identity_namespace != self.config.namespace.key()
            || ack.environment != self.config.namespace.environment_id
            || ack.identity_instance_epoch.0 != self.config.rp.identity_instance_epoch
            || ack.owner_audience != config.owner_audience
            || ack.closed_at.0 > now.saturating_add(5000)
        {
            return Err(reject("revocation_ack_binding_mismatch"));
        }
        let raw = URL_SAFE_NO_PAD
            .decode(&signed.signature)
            .map_err(|_| reject("revocation_ack_signature_invalid"))?;
        if URL_SAFE_NO_PAD.encode(&raw) != signed.signature {
            return Err(reject("revocation_ack_signature_invalid"));
        }
        config
            .owner_key
            .verify_strict(
                &bytes,
                &Signature::from_slice(&raw)
                    .map_err(|_| reject("revocation_ack_signature_invalid"))?,
            )
            .map_err(|_| reject("revocation_ack_signature_invalid"))?;
        self.store.transaction(|tx| {
            let key = format!("custody_source_revocation:{}", ack.event_id);
            let mut record = tx
                .get::<SourceOutbox>(&key)?
                .ok_or_else(|| reject("revocation_event_missing"))?;
            let event = &record.value.original;
            if event.digest().map_err(|_| StoreError::Corrupt)? != ack.event_digest
                || event.event.manager_account_id != ack.manager_account_id
                || ack.closed_at.0 < event.event.observed_at.0
            {
                return Err(reject("revocation_ack_binding_mismatch"));
            }
            let metadata = tx
                .get::<BridgeMetadata>("custody_revocation_bridge_meta")?
                .ok_or(StoreError::Corrupt)?;
            let mut candidate = record.value.clone();
            candidate.acknowledgement = Some(signed.clone());
            verify_barrier_ack(&candidate, &metadata.value)?;
            if let Some(original) = &record.value.acknowledgement {
                if original != &signed {
                    return Err(reject("idempotency_conflict"));
                }
            } else {
                record.value.acknowledgement = Some(signed.clone());
                tx.replace(
                    &key,
                    record.row_version,
                    "custody_source_revocation",
                    Some(&ack.manager_account_id),
                    None,
                    &record.value,
                )?;
            }
            serde_json::to_value(SourceRevocationAckResultV1 {
                api_version: 1,
                event_id: ack.event_id.clone(),
                event_digest: ack.event_digest.clone(),
                status: SourceClosureStateV1::Closed,
            })
            .map_err(|_| StoreError::Corrupt)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::{model_session::model_session_tests::Fixture, service::session_key};
    use super::*;
    fn fixture() -> Fixture {
        let (mut f, _) = super::super::tests::fixture();
        let owner = SigningKey::from_bytes(&[61; 32]);
        let custody = f.service.custody.as_mut().unwrap();
        custody.revocation = Some(RevocationConfig {
            poll: [83; 32],
            ack: [84; 32],
            owner_key_id: "owner-test".into(),
            owner_key: owner.verifying_key(),
            owner_audience: custody.audience.clone(),
        });
        custody
            .attach_revocation(
                &mut f.service.store,
                f.service.config.namespace.key(),
                f.service.config.namespace.environment_id.clone(),
                f.service.config.rp.identity_instance_epoch,
            )
            .unwrap();
        f
    }
    fn headers(poll: bool) -> String {
        format!(
            "POST / HTTP/1.1\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\n",
            hex::encode(if poll { [83; 32] } else { [84; 32] })
        )
    }
    fn poll(f: &mut Fixture, limit: u16) -> Vec<SignedSourceRevocationEventV1> {
        let now = now_ms();
        let value = f
            .service
            .custody_revocation_route(
                "POST",
                "/custody/revocations/poll",
                &headers(true),
                &serde_json::json!({"api_version":1,"limit":limit}),
                now,
            )
            .unwrap();
        serde_json::from_value::<SourceRevocationsPollResponseV1>(value)
            .unwrap()
            .events
    }
    fn change_account(f: &mut Fixture) {
        f.service
            .store
            .transaction(|tx| {
                let mut a = tx.get::<AccountRecord>("account:a")?.unwrap();
                a.value.policy_version += 1;
                tx.replace(
                    "account:a",
                    a.row_version,
                    "account",
                    Some("a"),
                    None,
                    &a.value,
                )
            })
            .unwrap();
    }
    fn ack(event: &SignedSourceRevocationEventV1) -> SignedSourceClosureAckV1 {
        let event = &event.event;
        let ack = SourceClosureAckV1 {
            version: 1,
            key_id: "owner-test".into(),
            event_id: event.event_id.clone(),
            event_digest: String::new(),
            identity_namespace: event.identity_namespace.clone(),
            environment: event.environment.clone(),
            identity_instance_epoch: event.identity_instance_epoch,
            manager_account_id: event.manager_account_id.clone(),
            owner_audience: event.owner_audience.clone(),
            closure_digest: "a".repeat(64),
            inventory_digest: "b".repeat(64),
            closed_at: DecimalU64(now_ms()),
            status: SourceClosureStateV1::Closed,
        };
        SignedSourceClosureAckV1 {
            ack,
            signature: String::new(),
        }
    }
    fn sign_ack(ack: &mut SignedSourceClosureAckV1) {
        ack.signature = URL_SAFE_NO_PAD.encode(
            SigningKey::from_bytes(&[61; 32])
                .sign(&ack.ack.signing_bytes().unwrap())
                .to_bytes(),
        );
    }
    #[test]
    fn source_writes_and_signed_originals_commit_or_roll_back_together() {
        let mut f = fixture();
        let before = f
            .service
            .store
            .transaction(|tx| {
                Ok(tx
                    .get::<AccountRecord>("account:a")?
                    .unwrap()
                    .value
                    .policy_version)
            })
            .unwrap();
        let failed: Result<(), StoreError> = f.service.store.transaction(|tx| {
            let mut a = tx.get::<AccountRecord>("account:a")?.unwrap();
            a.value.policy_version += 1;
            tx.replace(
                "account:a",
                a.row_version,
                "account",
                Some("a"),
                None,
                &a.value,
            )?;
            Err(StoreError::Conflict)
        });
        assert!(failed.is_err());
        assert!(poll(&mut f, 8).is_empty());
        assert_eq!(
            f.service
                .store
                .transaction(|tx| Ok(tx
                    .get::<AccountRecord>("account:a")?
                    .unwrap()
                    .value
                    .policy_version))
                .unwrap(),
            before
        );
        change_account(&mut f);
        let events = poll(&mut f, 8);
        assert_eq!(events.len(), 1);
        assert_eq!(poll(&mut f, 8), events);
        let config = f.service.custody.as_ref().unwrap();
        let event = &events[0];
        config
            .key
            .verifying_key()
            .verify_strict(
                &event.event.signing_bytes().unwrap(),
                &Signature::from_slice(&URL_SAFE_NO_PAD.decode(&event.signature).unwrap()).unwrap(),
            )
            .unwrap();
        assert_eq!(event.event.manager_security_version.0, 9);
    }
    #[test]
    fn ordinary_activity_and_extension_do_not_emit_but_logout_and_shortening_do() {
        let mut f = fixture();
        let key = session_key(&f.service.config.session_key, &f.token);
        f.service
            .store
            .transaction(|tx| {
                let mut s = tx.get::<AccountSession>(&key)?.unwrap();
                s.value.idle_expires_at += 1000;
                tx.replace(
                    &key,
                    s.row_version,
                    "session",
                    Some("a"),
                    Some(s.value.expires_at),
                    &s.value,
                )
            })
            .unwrap();
        assert!(poll(&mut f, 8).is_empty());
        f.service
            .store
            .transaction(|tx| {
                let mut a = tx.get::<AccountRecord>("account:a")?.unwrap();
                a.value.last_authenticated_at = Some(now_ms());
                tx.replace(
                    "account:a",
                    a.row_version,
                    "account",
                    Some("a"),
                    None,
                    &a.value,
                )
            })
            .unwrap();
        assert!(poll(&mut f, 8).is_empty());
        f.service
            .store
            .transaction(|tx| {
                let mut s = tx.get::<AccountSession>(&key)?.unwrap();
                s.value.idle_expires_at -= 1;
                tx.replace(
                    &key,
                    s.row_version,
                    "session",
                    Some("a"),
                    Some(s.value.expires_at),
                    &s.value,
                )
            })
            .unwrap();
        assert_eq!(poll(&mut f, 8).len(), 1);
        f.service.logout(Some(&f.token), now_ms()).unwrap();
        let original = poll(&mut f, 8);
        assert_eq!(original.len(), 2);
        f.service.logout(Some(&f.token), now_ms() + 1).unwrap();
        let retried = poll(&mut f, 8);
        assert_eq!(retried.len(), 2);
        for event in original {
            assert!(retried.contains(&event));
        }
    }
    #[test]
    fn poll_round_robin_preserves_signatures_and_does_not_starve_unacknowledged_events() {
        let mut f = fixture();
        for _ in 0..3 {
            change_account(&mut f)
        }
        let a = poll(&mut f, 1).pop().unwrap();
        let b = poll(&mut f, 1).pop().unwrap();
        let c = poll(&mut f, 1).pop().unwrap();
        assert_ne!(a.event.event_id, b.event.event_id);
        assert_ne!(b.event.event_id, c.event.event_id);
        assert_ne!(a.event.event_id, c.event.event_id);
        assert_eq!(poll(&mut f, 1), vec![a]);
    }
    #[test]
    fn acknowledgements_require_independent_auth_owner_signature_and_exact_event_binding() {
        let mut f = fixture();
        change_account(&mut f);
        let event = poll(&mut f, 8).pop().unwrap();
        let mut signed = ack(&event);
        signed.ack.event_digest = event.digest().unwrap();
        sign_ack(&mut signed);
        let path = "/custody/revocations/ack";
        let now = now_ms();
        let value = serde_json::to_value(&signed).unwrap();
        assert!(
            f.service
                .custody_revocation_route("POST", path, &headers(true), &value, now)
                .is_err()
        );
        for mutate in 0..4 {
            let mut wrong = signed.clone();
            match mutate {
                0 => wrong.ack.manager_account_id = "other".into(),
                1 => wrong.ack.event_digest = "c".repeat(64),
                2 => wrong.ack.closed_at = DecimalU64(event.event.observed_at.0 - 1),
                _ => wrong.ack.key_id = "other-key".into(),
            };
            sign_ack(&mut wrong);
            assert!(
                f.service
                    .custody_revocation_route(
                        "POST",
                        path,
                        &headers(false),
                        &serde_json::to_value(wrong).unwrap(),
                        now
                    )
                    .is_err()
            );
        }
        let original = f
            .service
            .custody_revocation_route("POST", path, &headers(false), &value, now)
            .unwrap();
        assert_eq!(
            f.service
                .custody_revocation_route("POST", path, &headers(false), &value, now)
                .unwrap(),
            original
        );
        assert!(poll(&mut f, 8).is_empty());
        assert!(
            f.service
                .custody_revocation_route(
                    "POST",
                    path,
                    &headers(false),
                    &serde_json::json!({"ack":true}),
                    now
                )
                .is_err()
        );
    }
    #[test]
    fn complete_configuration_requires_independent_credentials_and_role_keys() {
        let (f, _) = super::super::tests::fixture();
        let config = f.service.custody.as_ref().unwrap();
        let good = vec![
            Some(hex::encode([83; 32])),
            Some(hex::encode([84; 32])),
            Some("owner-test".into()),
            Some(hex::encode(
                SigningKey::from_bytes(&[61; 32]).verifying_key().to_bytes(),
            )),
            Some(config.audience.clone()),
        ];
        assert!(
            RevocationConfig::from_values(config, &f.service.config.session_key, vec![None; 5])
                .unwrap()
                .is_none()
        );
        assert!(
            RevocationConfig::from_values(config, &f.service.config.session_key, good.clone())
                .unwrap()
                .is_some()
        );
        for index in 0..5 {
            let mut bad = good.clone();
            bad[index] = None;
            assert!(
                RevocationConfig::from_values(config, &f.service.config.session_key, bad).is_err()
            );
        }
        let mut bad = good.clone();
        bad[1] = bad[0].clone();
        assert!(RevocationConfig::from_values(config, &f.service.config.session_key, bad).is_err());
        let mut bad = good.clone();
        bad[0] = Some(hex::encode(config.adapter_secret));
        assert!(RevocationConfig::from_values(config, &f.service.config.session_key, bad).is_err());
        let mut bad = good;
        bad[3] = Some(hex::encode(config.key.verifying_key().to_bytes()));
        assert!(RevocationConfig::from_values(config, &f.service.config.session_key, bad).is_err());
    }
    #[test]
    fn offline_epoch_requires_real_signed_closed_inventory_not_boolean_attestations() {
        use super::super::super::store::IdentityStore;
        let mut f = fixture();
        let path = f.service.config.sqlite_path.clone();
        let namespace = f.service.config.namespace.clone();
        let mut rp = f.service.config.rp.clone();
        rp.identity_instance_epoch += 1;
        assert!(IdentityStore::advance_instance_epoch(&path, &namespace, &rp, true, true).is_err());
        change_account(&mut f);
        let event = poll(&mut f, 8).pop().unwrap();
        assert!(IdentityStore::advance_instance_epoch(&path, &namespace, &rp, true, true).is_err());
        let mut signed = ack(&event);
        signed.ack.event_digest = event.digest().unwrap();
        sign_ack(&mut signed);
        f.service
            .custody_revocation_route(
                "POST",
                "/custody/revocations/ack",
                &headers(false),
                &serde_json::to_value(&signed).unwrap(),
                now_ms(),
            )
            .unwrap();
        IdentityStore::advance_instance_epoch(&path, &namespace, &rp, true, true).unwrap();
        assert!(
            IdentityStore::open(&path, namespace.clone(), f.service.config.rp.clone()).is_err()
        );
        assert!(IdentityStore::open(&path, namespace, rp).is_ok());
    }
    #[test]
    fn source_deletion_and_kind_reclassification_cannot_bypass_hook_or_block_ceremony_cleanup() {
        let mut f = fixture();
        let session = session_key(&f.service.config.session_key, &f.token);
        for key in ["account:a".to_owned(), session] {
            assert!(
                f.service
                    .store
                    .transaction(|tx| {
                        let record = tx.get::<Value>(&key)?.unwrap();
                        tx.delete(&key, record.row_version)
                    })
                    .is_err()
            );
            assert!(
                f.service
                    .store
                    .transaction(|tx| {
                        let record = tx.get::<Value>(&key)?.unwrap();
                        tx.replace(
                            &key,
                            record.row_version,
                            "unrelated",
                            Some("a"),
                            None,
                            &record.value,
                        )
                    })
                    .is_err()
            );
        }
        f.service
            .store
            .transaction(|tx| {
                tx.create(
                    "expired-test-ceremony",
                    "ceremony",
                    Some("a"),
                    Some(1),
                    &serde_json::json!({"test":true}),
                )
            })
            .unwrap();
        assert!(f.service.store.cleanup_expired(now_ms()).unwrap() > 0);
        assert!(poll(&mut f, 8).is_empty());
    }
    #[test]
    fn missing_base_configuration_does_not_ignore_revocation_group() {
        const CHILD: &str = "OASIS7_REVOCATION_MISSING_BASE_TEST_CHILD";
        if std::env::var_os(CHILD).is_some() {
            assert!(CustodyConfig::from_env(&[19; 32]).is_err());
            return;
        }
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["hosted_identity::custody::revocation::tests::missing_base_configuration_does_not_ignore_revocation_group","--exact"]);
        for name in [
            "OASIS7_CUSTODY_APPROVAL_SIGNING_KEY",
            "OASIS7_CUSTODY_APPROVAL_KEY_ID",
            "OASIS7_CUSTODY_ACCOUNT_AUTHORITY_AUDIENCE",
            "OASIS7_CUSTODY_NATIVE_ADAPTER_SECRET",
            "OASIS7_CUSTODY_REVOCATION_ACK_SECRET",
            "OASIS7_CUSTODY_REVOCATION_OWNER_KEY_ID",
            "OASIS7_CUSTODY_REVOCATION_OWNER_PUBLIC_KEY",
            "OASIS7_CUSTODY_REVOCATION_OWNER_AUDIENCE",
        ] {
            command.env_remove(name);
        }
        let output = command
            .env(CHILD, "1")
            .env(
                "OASIS7_CUSTODY_REVOCATION_POLL_SECRET",
                hex::encode([83; 32]),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated missing-base configuration test failed"
        );
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("1 passed")
        );
    }
    #[test]
    fn credential_security_mutation_emits_only_public_source_versions() {
        let mut f = fixture();
        f.service
            .store
            .transaction(|tx| {
                let (key, mut credential) = tx
                    .all::<CredentialRecord>("credential", Some("a"))?
                    .into_iter()
                    .find(|(_, c)| {
                        matches!(c.value.public_material, CredentialMaterial::WebAuthn { .. })
                    })
                    .unwrap();
                credential.value.state = "revoked".into();
                credential.value.version += 1;
                tx.replace(
                    &key,
                    credential.row_version,
                    "credential",
                    Some("a"),
                    None,
                    &credential.value,
                )
            })
            .unwrap();
        let events = poll(&mut f, 8);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].event.source,
            RevokedSourceV1::Credential { .. }
        ));
        let raw = serde_json::to_string(&events).unwrap();
        assert!(!raw.contains("library_record"));
        assert!(!raw.contains("token_digest"));
    }
    #[test]
    fn actual_source_event_capacity_rejects_security_write_without_pruning_ack_history() {
        let mut f = fixture();
        f.service
            .store
            .transaction(|tx| {
                for _ in 0..MAX_SOURCE_EVENTS {
                    let mut a = tx.get::<AccountRecord>("account:a")?.unwrap();
                    a.value.policy_version += 1;
                    tx.replace(
                        "account:a",
                        a.row_version,
                        "account",
                        Some("a"),
                        None,
                        &a.value,
                    )?;
                }
                Ok(())
            })
            .unwrap();
        let before = f
            .service
            .store
            .transaction(|tx| {
                Ok(tx
                    .get::<AccountRecord>("account:a")?
                    .unwrap()
                    .value
                    .policy_version)
            })
            .unwrap();
        assert!(
            f.service
                .store
                .transaction(|tx| {
                    let mut a = tx.get::<AccountRecord>("account:a")?.unwrap();
                    a.value.policy_version += 1;
                    tx.replace(
                        "account:a",
                        a.row_version,
                        "account",
                        Some("a"),
                        None,
                        &a.value,
                    )
                })
                .is_err()
        );
        assert_eq!(
            f.service
                .store
                .transaction(|tx| Ok(tx
                    .get::<AccountRecord>("account:a")?
                    .unwrap()
                    .value
                    .policy_version))
                .unwrap(),
            before
        );
        let event = poll(&mut f, 1).pop().unwrap();
        let mut signed = ack(&event);
        signed.ack.event_digest = event.digest().unwrap();
        sign_ack(&mut signed);
        f.service
            .custody_revocation_route(
                "POST",
                "/custody/revocations/ack",
                &headers(false),
                &serde_json::to_value(signed).unwrap(),
                now_ms(),
            )
            .unwrap();
        assert_eq!(
            f.service
                .store
                .transaction(|tx| tx.count_kind("custody_source_revocation"))
                .unwrap(),
            MAX_SOURCE_EVENTS
        );
        assert!(
            f.service
                .store
                .transaction(|tx| {
                    let mut a = tx.get::<AccountRecord>("account:a")?.unwrap();
                    a.value.policy_version += 1;
                    tx.replace(
                        "account:a",
                        a.row_version,
                        "account",
                        Some("a"),
                        None,
                        &a.value,
                    )
                })
                .is_err()
        );
    }
    #[test]
    fn registered_bridge_missing_metadata_is_not_a_legacy_unregistered_store() {
        use super::super::super::store::IdentityStore;
        let mut f = fixture();
        f.service
            .store
            .transaction(|tx| {
                let record = tx.get::<Value>("custody_revocation_bridge_meta")?.unwrap();
                tx.delete("custody_revocation_bridge_meta", record.row_version)
            })
            .unwrap();
        let mut rp = f.service.config.rp.clone();
        rp.identity_instance_epoch += 1;
        assert!(
            IdentityStore::advance_instance_epoch(
                &f.service.config.sqlite_path,
                &f.service.config.namespace,
                &rp,
                true,
                true
            )
            .is_err()
        );
        let mut reopened = IdentityStore::open(
            &f.service.config.sqlite_path,
            f.service.config.namespace.clone(),
            f.service.config.rp.clone(),
        )
        .unwrap();
        assert!(
            reopened
                .require_revocation_configuration_if_registered()
                .is_err()
        );
        assert!(
            f.service
                .custody
                .as_ref()
                .unwrap()
                .attach_revocation(
                    &mut reopened,
                    f.service.config.namespace.key(),
                    f.service.config.namespace.environment_id.clone(),
                    f.service.config.rp.identity_instance_epoch
                )
                .is_err()
        );
    }
}
