//! Immutable role-local encrypted Ed25519 slots. These structural contracts do
//! not authorize generation or authenticate ciphertext/public-key correspondence.
//! The role adapter must open the envelope and derive/check the public key.
use crate::{
    envelope::{EnvelopeContextV1, EnvelopeError, KeyEnvelopeV1},
    provisioning::{ProvisionRoleKeyAlgorithmV1, ProvisionRoleKeyTaskV1, PublicRoleKey},
};
use serde::{Deserialize, Serialize};

pub const ROLE_KEY_SLOT_SCHEMA_VERSION: u8 = 1;
pub const ROLE_KEY_SEED_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleKeySlotError {
    Envelope(EnvelopeError),
    UnsupportedSchema,
    InvalidIdentifier,
    InvalidPublicKey,
    InvalidSeedCiphertextLength,
    PublicReferenceMismatch,
    TaskDigestMismatch,
    ScopeMismatch,
    Conflict,
    Encoding,
}
impl core::fmt::Display for RoleKeySlotError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid role key slot: {self:?}")
    }
}
impl From<EnvelopeError> for RoleKeySlotError {
    fn from(error: EnvelopeError) -> Self {
        Self::Envelope(error)
    }
}
type Result<T> = core::result::Result<T, RoleKeySlotError>;
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || b"-_.:".contains(&v))
}
fn public_key_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}

/// Identity projection of a fixed task. Pending/cancelled state is deliberately
/// absent: current authorization belongs to the Account authority, not this scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawRoleKeySlotScopeV1")]
pub struct RoleKeySlotScopeV1 {
    pub provision_id: String,
    pub operation_id: String,
    pub context: EnvelopeContextV1,
    pub algorithm: ProvisionRoleKeyAlgorithmV1,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRoleKeySlotScopeV1 {
    provision_id: String,
    operation_id: String,
    context: EnvelopeContextV1,
    algorithm: ProvisionRoleKeyAlgorithmV1,
}
impl TryFrom<RawRoleKeySlotScopeV1> for RoleKeySlotScopeV1 {
    type Error = RoleKeySlotError;
    fn try_from(raw: RawRoleKeySlotScopeV1) -> Result<Self> {
        let scope = Self {
            provision_id: raw.provision_id,
            operation_id: raw.operation_id,
            context: raw.context,
            algorithm: raw.algorithm,
        };
        scope.validate()?;
        Ok(scope)
    }
}
impl RoleKeySlotScopeV1 {
    /// No seed, authorization boolean or imported key is accepted here. Even a
    /// cancelled task can be projected for historical identity comparison.
    pub fn from_task(task: &ProvisionRoleKeyTaskV1) -> Result<Self> {
        let scope = Self {
            provision_id: task.provision_id.clone(),
            operation_id: task.operation_id.clone(),
            context: EnvelopeContextV1 {
                schema_version: crate::envelope::ENVELOPE_SCHEMA_VERSION,
                identity: crate::envelope::KeyIdentity {
                    environment: task.environment.clone(),
                    account_id: task.custody_account_id.clone(),
                    role: task.role,
                    key_id: task.key_id.clone(),
                    key_version: task.key_version,
                },
                algorithm: crate::envelope::EnvelopeAlgorithmV1::Aes256Gcm,
            },
            algorithm: task.algorithm,
        };
        scope.validate()?;
        Ok(scope)
    }
    pub fn validate(&self) -> Result<()> {
        self.context.validate()?;
        if !valid_id(&self.provision_id)
            || !valid_id(&self.operation_id)
            || !valid_id(&self.context.identity.key_id)
        {
            return Err(RoleKeySlotError::InvalidIdentifier);
        }
        // The protocol algorithm is a closed Ed25519 enum; the encryption
        // algorithm is separately fixed by EnvelopeContextV1.
        Ok(())
    }
    pub fn task_digest(&self) -> Result<String> {
        self.validate()?;
        oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.role-key-slot-task.v1", self))
            .map(|bytes| crate::approval::bytes_digest(&bytes))
            .map_err(|_| RoleKeySlotError::Encoding)
    }
}

/// Private persisted record. Its public reference may be returned only after
/// the platform authenticates the envelope and derives the same public key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawRoleKeySlotV1")]
pub struct RoleKeySlotV1 {
    pub schema_version: u8,
    pub scope: RoleKeySlotScopeV1,
    pub task_digest: String,
    pub public_ref: PublicRoleKey,
    pub envelope: KeyEnvelopeV1,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRoleKeySlotV1 {
    schema_version: u8,
    scope: RoleKeySlotScopeV1,
    task_digest: String,
    public_ref: PublicRoleKey,
    envelope: KeyEnvelopeV1,
}
impl TryFrom<RawRoleKeySlotV1> for RoleKeySlotV1 {
    type Error = RoleKeySlotError;
    fn try_from(raw: RawRoleKeySlotV1) -> Result<Self> {
        let slot = Self {
            schema_version: raw.schema_version,
            scope: raw.scope,
            task_digest: raw.task_digest,
            public_ref: raw.public_ref,
            envelope: raw.envelope,
        };
        slot.validate()?;
        Ok(slot)
    }
}
impl RoleKeySlotV1 {
    pub fn new(
        scope: RoleKeySlotScopeV1,
        public_ref: PublicRoleKey,
        envelope: KeyEnvelopeV1,
    ) -> Result<Self> {
        let slot = Self {
            schema_version: ROLE_KEY_SLOT_SCHEMA_VERSION,
            task_digest: scope.task_digest()?,
            scope,
            public_ref,
            envelope,
        };
        slot.validate()?;
        Ok(slot)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != ROLE_KEY_SLOT_SCHEMA_VERSION {
            return Err(RoleKeySlotError::UnsupportedSchema);
        }
        self.scope.validate()?;
        self.envelope.validate_for(&self.scope.context)?;
        if self.envelope.ciphertext.len()
            != (ROLE_KEY_SEED_BYTES + crate::envelope::AES_GCM_TAG_BYTES) * 2
        {
            return Err(RoleKeySlotError::InvalidSeedCiphertextLength);
        }
        let identity = &self.scope.context.identity;
        if self.public_ref.key_id != identity.key_id
            || self.public_ref.key_version != identity.key_version
        {
            return Err(RoleKeySlotError::PublicReferenceMismatch);
        }
        if !public_key_hex(&self.public_ref.public_key) {
            return Err(RoleKeySlotError::InvalidPublicKey);
        }
        if self.task_digest != self.scope.task_digest()? {
            return Err(RoleKeySlotError::TaskDigestMismatch);
        }
        Ok(())
    }
    /// Expected scope must come from the trusted role-local storage address and
    /// fixed Account task, never from the stored envelope being authenticated.
    pub fn validate_for(&self, expected: &RoleKeySlotScopeV1) -> Result<()> {
        self.validate()?;
        expected.validate()?;
        if &self.scope != expected {
            return Err(RoleKeySlotError::ScopeMismatch);
        }
        Ok(())
    }
    /// Pure immutable insertion semantics, without storage or authorization.
    /// A racing platform writer should discard its unused candidate and first
    /// authenticate the actual stored winner before returning its public ref.
    pub fn ensure(existing: Option<&Self>, candidate: Self) -> Result<Self> {
        candidate.validate()?;
        if let Some(original) = existing {
            original.validate_for(&candidate.scope)?;
            if original != &candidate {
                return Err(RoleKeySlotError::Conflict);
            }
            return Ok(original.clone());
        }
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DecimalU64, envelope::*, http::Role, provisioning::ProvisionRoleKeyTaskStateV1};
    use serde_json::json;

    fn task(role: Role) -> ProvisionRoleKeyTaskV1 {
        ProvisionRoleKeyTaskV1 {
            operation_id: "task-one".into(),
            provision_id: "provision-one".into(),
            environment: "test".into(),
            custody_account_id: "account-one".into(),
            role,
            key_id: "role-key-one".into(),
            key_version: DecimalU64(u64::MAX),
            algorithm: ProvisionRoleKeyAlgorithmV1::Ed25519,
            state: ProvisionRoleKeyTaskStateV1::Pending,
        }
    }
    // Opaque format fixture only. Platform tests must use authenticated AES-GCM
    // and prove that its decrypted seed derives this public reference.
    fn slot(role: Role) -> RoleKeySlotV1 {
        let scope = RoleKeySlotScopeV1::from_task(&task(role)).unwrap();
        let public = PublicRoleKey {
            key_id: scope.context.identity.key_id.clone(),
            key_version: scope.context.identity.key_version,
            public_key: "44".repeat(32),
        };
        let envelope = KeyEnvelopeV1 {
            context: scope.context.clone(),
            ciphertext: "11".repeat(48),
            value_nonce: "22".repeat(12),
            wrapped_dek: "33".repeat(48),
            wrap_nonce: "55".repeat(12),
            wrapping_key_version: DecimalU64(u64::MAX),
        };
        RoleKeySlotV1::new(scope, public, envelope).unwrap()
    }
    #[test]
    fn three_roles_restore_exactly_and_keep_full_width_versions() {
        for role in [Role::Identity, Role::Owner, Role::Executor] {
            let original = slot(role);
            let raw = serde_json::to_value(&original).unwrap();
            assert_eq!(raw["public_ref"]["key_version"], "18446744073709551615");
            let restored: RoleKeySlotV1 = serde_json::from_value(raw).unwrap();
            restored.validate_for(&original.scope).unwrap();
            assert_eq!(restored, original);
        }
        let mut cancelled = task(Role::Identity);
        let original = RoleKeySlotScopeV1::from_task(&cancelled).unwrap();
        cancelled.state = ProvisionRoleKeyTaskStateV1::Cancelled;
        assert_eq!(RoleKeySlotScopeV1::from_task(&cancelled).unwrap(), original);
        assert!(RoleKeySlotScopeV1::from_task(&task(Role::RecoveryFence)).is_err());
    }
    #[test]
    fn expected_scope_binds_task_provision_and_every_key_identity_field() {
        let original = slot(Role::Owner);
        for field in 0..7 {
            let mut other = original.scope.clone();
            match field {
                0 => other.provision_id = "different".into(),
                1 => other.operation_id = "other-task".into(),
                2 => other.context.identity.environment = "dev".into(),
                3 => other.context.identity.account_id = "other-account".into(),
                4 => other.context.identity.role = Role::Identity,
                5 => other.context.identity.key_id = "other-key".into(),
                _ => other.context.identity.key_version = DecimalU64(2),
            };
            assert_ne!(
                original.scope.task_digest().unwrap(),
                other.task_digest().unwrap()
            );
            assert_eq!(
                original.validate_for(&other),
                Err(RoleKeySlotError::ScopeMismatch)
            );
        }
    }
    #[test]
    fn immutable_insert_never_replaces_existing_ciphertext_or_public_reference() {
        let original = slot(Role::Executor);
        assert_eq!(
            RoleKeySlotV1::ensure(None, original.clone()).unwrap(),
            original
        );
        assert_eq!(
            RoleKeySlotV1::ensure(Some(&original), original.clone()).unwrap(),
            original
        );
        for part in 0..4 {
            let mut other = original.clone();
            match part {
                0 => other.public_ref.public_key = "66".repeat(32),
                1 => other.envelope.ciphertext = "77".repeat(48),
                2 => other.envelope.value_nonce = "88".repeat(12),
                _ => other.envelope.wrapped_dek = "99".repeat(48),
            };
            assert_eq!(
                RoleKeySlotV1::ensure(Some(&original), other),
                Err(RoleKeySlotError::Conflict)
            );
        }
        assert_eq!(original, slot(Role::Executor));
    }
    #[test]
    fn malformed_or_inconsistent_persisted_material_is_rejected() {
        let original = serde_json::to_value(slot(Role::Identity)).unwrap();
        for (field, value) in [
            ("schema_version", json!(2)),
            ("task_digest", json!("aa".repeat(32))),
            ("extra", json!(true)),
        ] {
            let mut bad = original.clone();
            bad[field] = value;
            assert!(serde_json::from_value::<RoleKeySlotV1>(bad).is_err());
        }
        for key in [
            "44".repeat(31),
            "44".repeat(33),
            "AA".repeat(32),
            "gg".repeat(32),
        ] {
            let mut bad = original.clone();
            bad["public_ref"]["public_key"] = json!(key);
            assert!(serde_json::from_value::<RoleKeySlotV1>(bad).is_err());
        }
        for bytes in [47, 49] {
            let mut bad = original.clone();
            bad["envelope"]["ciphertext"] = json!("11".repeat(bytes));
            assert!(serde_json::from_value::<RoleKeySlotV1>(bad).is_err());
        }
        for (field, value) in [
            ("key_id", json!("different")),
            ("key_version", json!("0")),
            ("key_version", json!("1")),
            ("key_version", json!(1)),
        ] {
            let mut bad = original.clone();
            bad["public_ref"][field] = value;
            assert!(serde_json::from_value::<RoleKeySlotV1>(bad).is_err());
        }
        let mut bad = original.clone();
        bad["envelope"]["context"]["identity"]["account_id"] = json!("other");
        assert!(serde_json::from_value::<RoleKeySlotV1>(bad).is_err());
        for (field, value) in [
            ("algorithm", json!("rsa")),
            ("operation_id", json!("")),
            ("provision_id", json!("unsafe/id")),
            ("extra", json!(true)),
        ] {
            let mut bad = original["scope"].clone();
            bad[field] = value;
            assert!(serde_json::from_value::<RoleKeySlotScopeV1>(bad).is_err());
        }
        let mut bad = original["scope"].clone();
        bad["context"]["identity"]["key_version"] = json!("0");
        assert!(serde_json::from_value::<RoleKeySlotScopeV1>(bad).is_err());
    }
}
