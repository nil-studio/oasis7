//! Durable native source invalidation and Owner closure acknowledgements.
use crate::{DecimalU64, approval::bytes_digest};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidSourceRevocation;
fn bounded(v: &str) -> bool {
    !v.trim().is_empty() && v.len() <= 256
}
fn hex(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn canonical<T: Serialize>(domain: &str, value: &T) -> Result<Vec<u8>, InvalidSourceRevocation> {
    oasis7_wasm_abi::encode_canonical_cbor(&(domain, value)).map_err(|_| InvalidSourceRevocation)
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountSecurityStateV1 {
    pub status: String,
    pub policy_version: DecimalU64,
    pub security_version: DecimalU64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialSecurityStateV1 {
    pub state: String,
    pub version: DecimalU64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionSecurityStateV1 {
    pub origin_credential_ref: String,
    pub origin_credential_version: DecimalU64,
    pub issued_security_version: DecimalU64,
    pub identity_instance_epoch: DecimalU64,
    pub auth_method: String,
    pub revoked_at: Option<DecimalU64>,
    pub expires_at: DecimalU64,
    pub idle_expires_at: DecimalU64,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RevokedSourceV1 {
    Account {
        previous: AccountSecurityStateV1,
        current: AccountSecurityStateV1,
    },
    Credential {
        credential_ref: String,
        previous: CredentialSecurityStateV1,
        current: CredentialSecurityStateV1,
    },
    Session {
        session_ref: String,
        previous: SessionSecurityStateV1,
        current: SessionSecurityStateV1,
    },
}
impl RevokedSourceV1 {
    pub fn invalidates(&self) -> bool {
        match self {
            Self::Account {
                previous: p,
                current: c,
            } => p != c,
            Self::Credential {
                previous: p,
                current: c,
                ..
            } => p != c,
            Self::Session {
                previous: p,
                current: c,
                ..
            } => {
                p.origin_credential_ref != c.origin_credential_ref
                    || p.origin_credential_version != c.origin_credential_version
                    || p.issued_security_version != c.issued_security_version
                    || p.identity_instance_epoch != c.identity_instance_epoch
                    || p.auth_method != c.auth_method
                    || p.revoked_at.is_some() != c.revoked_at.is_some()
                    || c.expires_at.0 < p.expires_at.0
                    || c.idle_expires_at.0 < p.idle_expires_at.0
            }
        }
    }
    fn valid(&self) -> bool {
        match self {
            Self::Account { previous, current } => [previous, current]
                .iter()
                .all(|v| bounded(&v.status) && v.policy_version.0 > 0 && v.security_version.0 > 0),
            Self::Credential {
                credential_ref,
                previous,
                current,
            } => {
                bounded(credential_ref)
                    && [previous, current]
                        .iter()
                        .all(|v| bounded(&v.state) && v.version.0 > 0)
            }
            Self::Session {
                session_ref,
                previous,
                current,
            } => {
                bounded(session_ref)
                    && [previous, current].iter().all(|v| {
                        bounded(&v.origin_credential_ref)
                            && bounded(&v.auth_method)
                            && v.origin_credential_version.0 > 0
                            && v.issued_security_version.0 > 0
                            && v.identity_instance_epoch.0 > 0
                    })
            }
        }
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRevocationEventV1 {
    pub version: u8,
    pub key_id: String,
    pub event_id: String,
    pub identity_namespace: String,
    pub environment: String,
    pub identity_instance_epoch: DecimalU64,
    pub manager_account_id: String,
    pub manager_security_version: DecimalU64,
    pub owner_audience: String,
    pub observed_at: DecimalU64,
    pub source: RevokedSourceV1,
}
impl SourceRevocationEventV1 {
    pub fn stable_id(
        namespace: &str,
        epoch: u64,
        record_ref: &str,
        row_version: u64,
        source: &RevokedSourceV1,
    ) -> Result<String, InvalidSourceRevocation> {
        Ok(bytes_digest(&canonical(
            "oasis7.custody.source-event-id.v1",
            &(namespace, epoch, record_ref, row_version, source),
        )?))
    }
    pub fn validate(&self) -> Result<(), InvalidSourceRevocation> {
        if self.version != 1
            || self.manager_security_version.0 == 0
            || !hex(&self.event_id)
            || self.identity_instance_epoch.0 == 0
            || [
                &self.key_id,
                &self.identity_namespace,
                &self.environment,
                &self.manager_account_id,
                &self.owner_audience,
            ]
            .iter()
            .any(|v| !bounded(v))
            || !self.source.valid()
            || !self.source.invalidates()
        {
            return Err(InvalidSourceRevocation);
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, InvalidSourceRevocation> {
        self.validate()?;
        canonical("oasis7.custody.source-revocation.v1", self)
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedSourceRevocationEventV1 {
    pub event: SourceRevocationEventV1,
    pub signature: String,
}
impl SignedSourceRevocationEventV1 {
    pub fn digest(&self) -> Result<String, InvalidSourceRevocation> {
        self.event.validate()?;
        if !bounded(&self.signature) {
            return Err(InvalidSourceRevocation);
        }
        Ok(bytes_digest(&canonical(
            "oasis7.custody.signed-source-revocation.v1",
            self,
        )?))
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceClosureStateV1 {
    Closed,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceClosureAckV1 {
    pub version: u8,
    pub key_id: String,
    pub event_id: String,
    pub event_digest: String,
    pub identity_namespace: String,
    pub environment: String,
    pub identity_instance_epoch: DecimalU64,
    pub manager_account_id: String,
    pub owner_audience: String,
    pub closure_digest: String,
    pub inventory_digest: String,
    pub closed_at: DecimalU64,
    pub status: SourceClosureStateV1,
}
impl SourceClosureAckV1 {
    pub fn validate(&self) -> Result<(), InvalidSourceRevocation> {
        if self.version != 1
            || self.identity_instance_epoch.0 == 0
            || [
                &self.event_id,
                &self.event_digest,
                &self.closure_digest,
                &self.inventory_digest,
            ]
            .iter()
            .any(|v| !hex(v))
            || [
                &self.key_id,
                &self.identity_namespace,
                &self.environment,
                &self.manager_account_id,
                &self.owner_audience,
            ]
            .iter()
            .any(|v| !bounded(v))
        {
            return Err(InvalidSourceRevocation);
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, InvalidSourceRevocation> {
        self.validate()?;
        canonical("oasis7.custody.source-closure-ack.v1", self)
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedSourceClosureAckV1 {
    pub ack: SourceClosureAckV1,
    pub signature: String,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRevocationsPollRequestV1 {
    pub api_version: u8,
    pub limit: u8,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRevocationsPollResponseV1 {
    pub api_version: u8,
    pub events: Vec<SignedSourceRevocationEventV1>,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRevocationAckResultV1 {
    pub api_version: u8,
    pub event_id: String,
    pub event_digest: String,
    pub status: SourceClosureStateV1,
}

/// Signed, permanently fenced summary after the bounded full-event journal fills.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceCheckpointStateV1 {
    PermanentFence,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRevocationCheckpointV1 {
    pub version: u8,
    pub key_id: String,
    pub identity_namespace: String,
    pub environment: String,
    pub identity_instance_epoch: DecimalU64,
    pub manager_account_id: String,
    pub owner_audience: String,
    pub participant_registry_digest: String,
    pub first_event_id: String,
    pub first_event_digest: String,
    pub previous_checkpoint_digest: String,
    pub latest_event_digest: String,
    pub watermark: DecimalU64,
    pub chain_digest: String,
    pub observed_at: DecimalU64,
    pub state: SourceCheckpointStateV1,
}
impl SourceRevocationCheckpointV1 {
    pub fn validate(&self) -> Result<(), InvalidSourceRevocation> {
        if self.version != 1
            || self.identity_instance_epoch.0 == 0
            || self.watermark.0 == 0
            || [
                &self.key_id,
                &self.identity_namespace,
                &self.environment,
                &self.manager_account_id,
                &self.owner_audience,
                &self.first_event_id,
            ]
            .iter()
            .any(|v| !bounded(v))
            || [
                &self.participant_registry_digest,
                &self.first_event_digest,
                &self.previous_checkpoint_digest,
                &self.latest_event_digest,
                &self.chain_digest,
            ]
            .iter()
            .any(|v| !hex(v))
        {
            return Err(InvalidSourceRevocation);
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, InvalidSourceRevocation> {
        self.validate()?;
        canonical("oasis7.custody.source-revocation-checkpoint.v1", self)
    }
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedSourceRevocationCheckpointV1 {
    pub checkpoint: SourceRevocationCheckpointV1,
    pub signature: String,
}
impl SignedSourceRevocationCheckpointV1 {
    pub fn digest(&self) -> Result<String, InvalidSourceRevocation> {
        self.checkpoint.validate()?;
        canonical(
            "oasis7.custody.signed-source-revocation-checkpoint.v1",
            self,
        )
        .map(|v| bytes_digest(&v))
    }
}
/// Registration is immutable within an identity epoch; callers pass sorted, unique IDs.
pub fn participant_registry_digest(
    namespace: &str,
    epoch: DecimalU64,
    participants: &[String],
) -> Result<String, InvalidSourceRevocation> {
    if !bounded(namespace)
        || epoch.0 == 0
        || participants.len() > 256
        || participants.iter().any(|v| !bounded(v))
        || participants.windows(2).any(|p| p[0] >= p[1])
    {
        return Err(InvalidSourceRevocation);
    }
    canonical(
        "oasis7.custody.source-participants.v1",
        &(namespace, epoch, participants),
    )
    .map(|v| bytes_digest(&v))
}
pub fn checkpoint_seed() -> String {
    bytes_digest(b"oasis7.custody.source-checkpoint-seed.v1")
}
pub fn checkpoint_chain(
    previous: &str,
    watermark: DecimalU64,
    event_digest: &str,
) -> Result<String, InvalidSourceRevocation> {
    if !hex(previous) || !hex(event_digest) || watermark.0 == 0 {
        return Err(InvalidSourceRevocation);
    }
    canonical(
        "oasis7.custody.source-checkpoint-chain.v1",
        &(previous, watermark, event_digest),
    )
    .map(|v| bytes_digest(&v))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_contract_is_closed_full_width_and_signing_domains_are_distinct() {
        let source = RevokedSourceV1::Account {
            previous: AccountSecurityStateV1 {
                status: "active".into(),
                policy_version: DecimalU64(9_007_199_254_740_993),
                security_version: DecimalU64(1),
            },
            current: AccountSecurityStateV1 {
                status: "frozen".into(),
                policy_version: DecimalU64(u64::MAX),
                security_version: DecimalU64(2),
            },
        };
        let event = SourceRevocationEventV1 {
            version: 1,
            key_id: "native".into(),
            event_id: SourceRevocationEventV1::stable_id("身份:test", 3, "account:one", 1, &source)
                .unwrap(),
            identity_namespace: "身份:test".into(),
            environment: "test".into(),
            identity_instance_epoch: DecimalU64(3),
            manager_account_id: "manager".into(),
            manager_security_version: DecimalU64(2),
            owner_audience: "owner".into(),
            observed_at: DecimalU64(1),
            source,
        };
        let raw = serde_json::to_value(&event).unwrap();
        let restored: SourceRevocationEventV1 = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(restored.signing_bytes(), event.signing_bytes());
        let mut wrong = raw.clone();
        wrong["unknown"] = true.into();
        assert!(serde_json::from_value::<SourceRevocationEventV1>(wrong).is_err());
        let mut wrong = raw;
        wrong["manager_security_version"] = serde_json::json!(2);
        assert!(serde_json::from_value::<SourceRevocationEventV1>(wrong).is_err());
        let signed = SignedSourceRevocationEventV1 {
            event,
            signature: "fixture".into(),
        };
        let ack = SourceClosureAckV1 {
            version: 1,
            key_id: "owner".into(),
            event_id: signed.event.event_id.clone(),
            event_digest: signed.digest().unwrap(),
            identity_namespace: signed.event.identity_namespace.clone(),
            environment: "test".into(),
            identity_instance_epoch: DecimalU64(3),
            manager_account_id: "manager".into(),
            owner_audience: "owner".into(),
            closure_digest: "a".repeat(64),
            inventory_digest: "b".repeat(64),
            closed_at: DecimalU64(2),
            status: SourceClosureStateV1::Closed,
        };
        assert_ne!(
            signed.event.signing_bytes().unwrap(),
            ack.signing_bytes().unwrap()
        );
    }
    #[test]
    fn session_activity_renewal_is_not_an_invalidation_but_each_security_change_is() {
        let previous = SessionSecurityStateV1 {
            origin_credential_ref: "credential".into(),
            origin_credential_version: DecimalU64(1),
            issued_security_version: DecimalU64(2),
            identity_instance_epoch: DecimalU64(3),
            auth_method: "webauthn".into(),
            revoked_at: None,
            expires_at: DecimalU64(1000),
            idle_expires_at: DecimalU64(900),
        };
        let mut renewal = previous.clone();
        renewal.expires_at = DecimalU64(2000);
        renewal.idle_expires_at = DecimalU64(1900);
        assert!(
            !RevokedSourceV1::Session {
                session_ref: "session".into(),
                previous: previous.clone(),
                current: renewal
            }
            .invalidates()
        );
        for field in 0..8 {
            let mut current = previous.clone();
            match field {
                0 => current.origin_credential_ref = "other".into(),
                1 => current.origin_credential_version = DecimalU64(2),
                2 => current.issued_security_version = DecimalU64(3),
                3 => current.identity_instance_epoch = DecimalU64(4),
                4 => current.auth_method = "native_key".into(),
                5 => current.revoked_at = Some(DecimalU64(10)),
                6 => current.expires_at = DecimalU64(999),
                _ => current.idle_expires_at = DecimalU64(899),
            };
            assert!(
                RevokedSourceV1::Session {
                    session_ref: "session".into(),
                    previous: previous.clone(),
                    current
                }
                .invalidates()
            );
        }
    }
    #[test]
    fn checkpoint_canonical_binding_is_closed_and_preserves_u64_limits() {
        let registry =
            participant_registry_digest("identity:test", DecimalU64(3), &["a".into(), "b".into()])
                .unwrap();
        assert!(
            participant_registry_digest("identity:test", DecimalU64(3), &["b".into(), "a".into()])
                .is_err()
        );
        assert!(
            participant_registry_digest("identity:test", DecimalU64(3), &["a".into(), "a".into()])
                .is_err()
        );
        let checkpoint = SourceRevocationCheckpointV1 {
            version: 1,
            key_id: "native".into(),
            identity_namespace: "identity:test".into(),
            environment: "test".into(),
            identity_instance_epoch: DecimalU64(3),
            manager_account_id: "a".into(),
            owner_audience: "owner".into(),
            participant_registry_digest: registry,
            first_event_id: "first".into(),
            first_event_digest: "a".repeat(64),
            previous_checkpoint_digest: checkpoint_seed(),
            latest_event_digest: "b".repeat(64),
            watermark: DecimalU64(u64::MAX),
            chain_digest: checkpoint_chain(
                &checkpoint_seed(),
                DecimalU64(u64::MAX),
                &"b".repeat(64),
            )
            .unwrap(),
            observed_at: DecimalU64(9007199254740993),
            state: SourceCheckpointStateV1::PermanentFence,
        };
        let bytes = checkpoint.signing_bytes().unwrap();
        let json = serde_json::to_value(&checkpoint).unwrap();
        assert_eq!(json["watermark"], "18446744073709551615");
        assert_eq!(
            serde_json::from_value::<SourceRevocationCheckpointV1>(json.clone())
                .unwrap()
                .signing_bytes()
                .unwrap(),
            bytes
        );
        for field in [
            "participant_registry_digest",
            "first_event_digest",
            "previous_checkpoint_digest",
            "latest_event_digest",
            "chain_digest",
        ] {
            let mut other = json.clone();
            other[field] = serde_json::json!("c".repeat(64));
            assert_ne!(
                serde_json::from_value::<SourceRevocationCheckpointV1>(other)
                    .unwrap()
                    .signing_bytes()
                    .unwrap(),
                bytes
            );
        }
        for (field, value) in [
            ("state", serde_json::json!("active")),
            ("watermark", serde_json::json!(18446744073709551615_u64)),
            ("extra", serde_json::json!(true)),
        ] {
            let mut other = json.clone();
            other[field] = value;
            assert!(serde_json::from_value::<SourceRevocationCheckpointV1>(other).is_err());
        }
    }
}
