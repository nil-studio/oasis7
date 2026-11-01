//! Account-owned Native rollback floor. It is independent of Native SQLite;
//! accepting history here cannot authorize a World request or a signing slot.
use super::*;
use crate::provisioning::NativeStageScope;
use std::collections::BTreeMap;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "proof",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum NativeFloorEvidenceV2 {
    Registration(Box<SignedNativeRegistrationAckV2>),
    Activation(Box<SignedNativeActivationAckV2>),
    Current(Box<SignedNativeMachineCurrentFactV2>),
    TermCurrent(Box<SignedNativeTermCurrentFactV2>),
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeFloorEntryV2 {
    native_origin: String,
    identity_namespace: String,
    native_key_id: String,
    floor: NativeExternalFloorSnapshotV2,
    evidence: NativeFloorEvidenceV2,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountNativeFloorStoreV2 {
    schema_revision: u8,
    entries: BTreeMap<String, NativeFloorEntryV2>,
}
impl core::fmt::Debug for AccountNativeFloorStoreV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AccountNativeFloorStoreV2(<private>)")
    }
}
impl core::fmt::Debug for NativeFloorEvidenceV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("NativeFloorEvidenceV2(<private>)")
    }
}
impl NativeFloorEvidenceV2 {
    /// Historical correlation only. This does not verify payment or issue a grant.
    pub(crate) fn session_binding(&self) -> (&str, &str) {
        match self {
            Self::Registration(v) => (&v.ack.session_id, &v.ack.approved_input_digest),
            Self::Activation(v) => (&v.ack.session_id, &v.ack.approved_input_digest),
            Self::Current(v) => (&v.fact.session_id, &v.fact.approved_input_digest),
            Self::TermCurrent(v) => (&v.fact.session_id, &v.fact.approved_input_digest),
        }
    }
    fn registration_ref(&self) -> &str {
        match self {
            Self::Registration(v) => &v.ack.opaque_registration_ref,
            Self::Activation(v) => &v.ack.opaque_registration_ref,
            Self::Current(v) => &v.fact.opaque_registration_ref,
            Self::TermCurrent(v) => &v.fact.opaque_registration_ref,
        }
    }
    fn identity(&self) -> (&str, &str, DecimalU64, &NativeStorageHeadV2, DecimalU64) {
        match self {
            Self::Registration(v) => (
                &v.ack.native_key_id,
                &v.ack.identity_namespace,
                v.ack.identity_epoch,
                &v.ack.storage_head,
                v.ack.registry_cursor,
            ),
            Self::Activation(v) => (
                &v.ack.native_key_id,
                &v.ack.identity_namespace,
                v.ack.identity_epoch,
                &v.ack.storage_head,
                v.ack.registry_cursor,
            ),
            Self::Current(v) => (
                &v.fact.native_key_id,
                &v.fact.identity_namespace,
                v.fact.identity_epoch,
                &v.fact.storage_head,
                v.fact.registry_cursor,
            ),
            Self::TermCurrent(v) => (
                &v.fact.native_key_id,
                &v.fact.identity_namespace,
                v.fact.identity_epoch,
                &v.fact.storage_head,
                v.fact.registry_cursor,
            ),
        }
    }
    /// Historical signature validation only. Phase/Book/body correlation is
    /// checked by the actual registration/current receiver before an effect.
    fn verify_history(&self, scope: &NativeStageScope<'_>, namespace: &str) -> Result<(), String> {
        let (key, ns, epoch, head, cursor) = self.identity();
        head.validate()?;
        if key != scope.native_root_key_id || ns != namespace || epoch.0 == 0 || cursor.0 == 0 {
            return Err(invalid());
        }
        match self {
            Self::Registration(v) => {
                if v.ack.api_version != 2
                    || !hex64(&v.ack.pending_certificate_digest)
                    || !hex64(&v.ack.approved_input_digest)
                    || !hex64(&v.ack.owner_read_response_digest)
                    || !id(&v.ack.session_id)
                    || !id(&v.ack.opaque_registration_ref)
                {
                    return Err(invalid());
                }
                verify(
                    NATIVE_REGISTRATION_ACK_V2_DOMAIN,
                    &v.ack,
                    &v.signature_hex,
                    scope.native_root_public_key,
                )
            }
            Self::Activation(v) => {
                if v.ack.api_version != 2
                    || !hex64(&v.ack.pending_certificate_digest)
                    || !hex64(&v.ack.active_certificate_digest)
                    || !hex64(&v.ack.root_birth_digest)
                    || !hex64(&v.ack.approved_input_digest)
                    || !id(&v.ack.session_id)
                {
                    return Err(invalid());
                }
                verify(
                    NATIVE_ACTIVATION_ACK_V2_DOMAIN,
                    &v.ack,
                    &v.signature_hex,
                    scope.native_root_public_key,
                )
            }
            Self::Current(v) => v.verify_native(
                scope.native_root_key_id,
                scope.native_root_public_key,
                v.fact.issued_at.0,
            ),
            Self::TermCurrent(v) => v.verify_native(
                scope.native_root_key_id,
                scope.native_root_public_key,
                v.fact.issued_at.0,
            ),
        }
    }
    fn snapshot(&self) -> NativeExternalFloorSnapshotV2 {
        let (key, _, epoch, head, cursor) = self.identity();
        NativeExternalFloorSnapshotV2 {
            native_store_instance_id: head.instance_id.clone(),
            minimum_native_revision: head.revision,
            native_revision_commitment: head.commitment.clone(),
            registry_cursor: cursor,
            native_identity_epoch: epoch,
            native_key_id: key.into(),
        }
    }
}
impl AccountNativeFloorStoreV2 {
    /// Structural restore only; fixed external Native pins must still be supplied
    /// to consume_history/check_evidence before any current use.
    pub fn validate_structure(&self) -> Result<(), String> {
        if self.schema_revision != 2 {
            return Err(invalid());
        }
        for (key, entry) in &self.entries {
            if Self::key(
                &entry.native_origin,
                &entry.identity_namespace,
                &entry.native_key_id,
            )? != *key
                || entry.floor != entry.evidence.snapshot()
                || entry.floor.native_identity_epoch.0 == 0
                || entry.floor.registry_cursor.0 == 0
            {
                return Err(invalid());
            }
            entry.evidence.identity().3.validate()?;
        }
        Ok(())
    }
    pub fn empty() -> Self {
        Self {
            schema_revision: 2,
            entries: BTreeMap::new(),
        }
    }
    fn key(origin: &str, namespace: &str, key: &str) -> Result<String, String> {
        crate::http_pop::CanonicalHttpTarget::new(
            origin,
            crate::http_pop::HttpPopRoute::ProofChallenges,
        )
        .map_err(|_| invalid())?;
        if !id(namespace) || !id(key) {
            return Err(invalid());
        }
        hash(&(
            "oasis7.custody.account-native-floor-location.v2",
            origin,
            namespace,
            key,
        ))
    }
    /// The Account storage transaction commits this before returning any new
    /// OwnerVault or sealed World capability. No Native->Account callback exists.
    pub fn consume_history(
        &mut self,
        scope: &NativeStageScope<'_>,
        namespace: &str,
        evidence: NativeFloorEvidenceV2,
    ) -> Result<NativeExternalFloorSnapshotV2, String> {
        if self.schema_revision != 2 {
            return Err(invalid());
        }
        evidence.verify_history(scope, namespace)?;
        let key = Self::key(scope.http_origin, namespace, scope.native_root_key_id)?;
        let next = evidence.snapshot();
        if let Some(old) = self.entries.get(&key) {
            old.evidence.verify_history(scope, namespace)?;
            if old.native_origin != scope.http_origin
                || old.identity_namespace != namespace
                || old.native_key_id != scope.native_root_key_id
                || old.floor != old.evidence.snapshot()
                || next.native_store_instance_id != old.floor.native_store_instance_id
                || next.native_identity_epoch != old.floor.native_identity_epoch
                || next.minimum_native_revision.0 < old.floor.minimum_native_revision.0
                || evidence.registration_ref() == old.evidence.registration_ref()
                    && next.registry_cursor.0 < old.floor.registry_cursor.0
                || next.minimum_native_revision == old.floor.minimum_native_revision
                    && (next.native_revision_commitment != old.floor.native_revision_commitment
                        || next.registry_cursor != old.floor.registry_cursor)
            {
                return Err(invalid());
            }
        }
        self.entries.insert(
            key,
            NativeFloorEntryV2 {
                native_origin: scope.http_origin.into(),
                identity_namespace: namespace.into(),
                native_key_id: scope.native_root_key_id.into(),
                floor: next.clone(),
                evidence,
            },
        );
        Ok(next)
    }
    pub fn check_evidence(
        &self,
        scope: &NativeStageScope<'_>,
        namespace: &str,
        evidence: &NativeFloorEvidenceV2,
    ) -> Result<NativeExternalFloorSnapshotV2, String> {
        evidence.verify_history(scope, namespace)?;
        let key = Self::key(scope.http_origin, namespace, scope.native_root_key_id)?;
        let old = self.entries.get(&key).ok_or_else(invalid)?;
        old.evidence.verify_history(scope, namespace)?;
        let next = evidence.snapshot();
        if self.schema_revision != 2
            || old.floor != old.evidence.snapshot()
            || old.native_origin != scope.http_origin
            || old.identity_namespace != namespace
            || old.native_key_id != scope.native_root_key_id
            || next.native_store_instance_id != old.floor.native_store_instance_id
            || next.native_identity_epoch != old.floor.native_identity_epoch
            || next.minimum_native_revision.0 < old.floor.minimum_native_revision.0
            || evidence.registration_ref() == old.evidence.registration_ref()
                && next.registry_cursor.0 < old.floor.registry_cursor.0
            || next.minimum_native_revision == old.floor.minimum_native_revision
                && (next.native_revision_commitment != old.floor.native_revision_commitment
                    || next.registry_cursor != old.floor.registry_cursor)
        {
            return Err(invalid());
        }
        Ok(old.floor.clone())
    }
    pub fn validate_native_scope(
        &self,
        scope: &NativeStageScope<'_>,
        namespace: &str,
    ) -> Result<(), String> {
        if self.schema_revision != 2 {
            return Err(invalid());
        }
        let key = Self::key(scope.http_origin, namespace, scope.native_root_key_id)?;
        let entry = self.entries.get(&key).ok_or_else(invalid)?;
        entry.evidence.verify_history(scope, namespace)?;
        if entry.floor != entry.evidence.snapshot()
            || entry.native_origin != scope.http_origin
            || entry.identity_namespace != namespace
            || entry.native_key_id != scope.native_root_key_id
        {
            return Err(invalid());
        }
        Ok(())
    }
}
