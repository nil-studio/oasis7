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
#[path = "custody_revocation_overflow.rs"]
pub(crate) mod overflow;

pub(crate) struct RevocationConfig {
    poll: [u8; 32],
    ack: [u8; 32],
    owner_key_id: String,
    owner_key: VerifyingKey,
    owner_audience: String,
    participants: Vec<String>,
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
    pub participants: Vec<String>,
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
            participants: vec!["a".into()],
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
            "OASIS7_CUSTODY_REVOCATION_PARTICIPANTS",
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
        if values.len() != 6 {
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
        let mut participants: Vec<String> =
            serde_json::from_str(&values[5]).map_err(|_| "invalid participants")?;
        participants.sort();
        if participants.len() > 256
            || participants
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 256)
            || participants.windows(2).any(|p| p[0] == p[1])
        {
            return Err("invalid participants".into());
        }
        Ok(Some(Self {
            participants,
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
            participants: self.participants.clone(),
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
        if !self.metadata.participants.contains(&manager) || !source.invalidates() {
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
        let original = SignedSourceRevocationEventV1 { event, signature };
        if tx
            .get::<overflow::OverflowRecord>(&overflow::record_key(&manager))?
            .is_some()
            || tx.count_kind("custody_source_revocation")? >= MAX_SOURCE_EVENTS
        {
            return self.record_overflow(tx, original);
        }
        tx.create(
            &format!("custody_source_revocation:{event_id}"),
            "custody_source_revocation",
            Some(&manager),
            None,
            &SourceOutbox {
                original,
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
    participant_registry_digest(
        &metadata.namespace,
        DecimalU64(metadata.epoch),
        &metadata.participants,
    )
    .map_err(|_| StoreError::Corrupt)?;
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
        || !metadata.participants.contains(&event.manager_account_id)
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
            let events = self
                .store
                .transaction(|tx| overflow::poll_records(tx, request.limit as usize))?;
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
            overflow::acknowledge(tx, &signed)?;
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
#[path = "custody_revocation_tests.rs"]
mod tests;
