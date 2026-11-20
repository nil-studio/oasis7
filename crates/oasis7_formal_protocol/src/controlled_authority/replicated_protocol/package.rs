use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_RECORD_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_OBJECTS: usize = 256;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ArtifactRole {
    Input,
    Result,
    Snapshot,
    Journal,
    NonceIndex,
    EffectOutbox,
    ExecutionManifest,
    Wasm,
    Rules,
    NativeExecutionEnvironment,
    IncrementalGenesisConfig,
    IncrementalRootSet,
    IncrementalCommitBody,
    IncrementalHistoryPrefix,
    IncrementalRetentionLedger,
}
const ROLES: [ArtifactRole; 9] = [
    ArtifactRole::Input,
    ArtifactRole::Result,
    ArtifactRole::Snapshot,
    ArtifactRole::Journal,
    ArtifactRole::NonceIndex,
    ArtifactRole::EffectOutbox,
    ArtifactRole::ExecutionManifest,
    ArtifactRole::Wasm,
    ArtifactRole::Rules,
];
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactObject {
    pub content_hash: String,
    #[serde(deserialize_with = "decode_bytes")]
    pub bytes: Vec<u8>,
    /// Only these explicitly declared references are checked. The protocol does
    /// not infer opaque JSON/WASM references or validate execution semantics.
    #[serde(deserialize_with = "decode_references")]
    pub references: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClosedRecord {
    pub payload_digest: String,
    pub before_state_root: String,
    pub after_state_root: String,
    pub roots: BTreeMap<ArtifactRole, String>,
    /// Canonical ordering by content hash; full bytes travel to both endpoints.
    #[serde(deserialize_with = "decode_objects")]
    pub objects: Vec<ArtifactObject>,
}
pub fn bounded_vec<'de, D, T, const N: usize>(decoder: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Bounded<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for Bounded<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("bounded typed sequence")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            if seq.size_hint().is_some_and(|n| n > N) {
                return Err(serde::de::Error::custom("typed sequence capacity limit"));
            }
            let mut result = Vec::new();
            while let Some(value) = seq.next_element()? {
                if result.len() == N {
                    return Err(serde::de::Error::custom("typed sequence count limit"));
                }
                result.push(value);
            }
            Ok(result)
        }
    }
    decoder.deserialize_seq(Bounded::<T, N>(std::marker::PhantomData))
}
fn decode_bytes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    bounded_vec::<D, u8, MAX_RECORD_BYTES>(d)
}
fn decode_references<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    bounded_vec::<D, String, MAX_OBJECTS>(d)
}
fn decode_objects<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<ArtifactObject>, D::Error> {
    bounded_vec::<D, ArtifactObject, MAX_OBJECTS>(d)
}
trait PackageValidationOps {
    type Error: From<ProtocolError>;
    type Native: std::borrow::Borrow<crate::native_environment::NativeExecutionEnvironmentV1>;
    fn work(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn before_closure(&mut self, record: &ClosedRecord) -> Result<(), Self::Error>;
    fn retention(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
    fn native(&mut self, bytes: &[u8]) -> Result<Self::Native, Self::Error>;
    fn hash(&mut self, bytes: &[u8]) -> Result<blake3::Hash, Self::Error>;
}
struct LegacyPackageOps;
impl PackageValidationOps for LegacyPackageOps {
    type Error = ProtocolError;
    type Native = crate::native_environment::NativeExecutionEnvironmentV1;
    fn work(&mut self, _: usize) -> Result<(), ProtocolError> {
        Ok(())
    }
    fn before_closure(&mut self, _: &ClosedRecord) -> Result<(), ProtocolError> {
        Ok(())
    }
    fn retention(&mut self, b: &[u8]) -> Result<(), ProtocolError> {
        crate::incremental_retention::RetentionLedgerManifestV1::decode_with_work(b, &mut |_, _| {
            Ok(())
        })
        .map(|_| ())
        .map_err(|_| invalid("retention ledger descriptor codec"))
    }
    fn native(&mut self, b: &[u8]) -> Result<Self::Native, ProtocolError> {
        crate::native_environment::NativeExecutionEnvironmentV1::decode(b).map_err(invalid)
    }
    fn hash(&mut self, b: &[u8]) -> Result<blake3::Hash, ProtocolError> {
        Ok(blake3::hash(b))
    }
}
impl ClosedRecord {
    /// Resolve an explicitly versioned Snapshot delta closure to its original
    /// canonical bytes. Legacy raw artifacts retain their exact representation.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        let root = self
            .roots
            .get(&ArtifactRole::Snapshot)
            .ok_or_else(|| invalid("Snapshot role missing"))?;
        let read = |hash: &str| -> Result<Vec<u8>, String> {
            let object = self
                .objects
                .iter()
                .find(|object| object.content_hash == hash)
                .ok_or("Snapshot dependency unavailable")?;
            if blake3::hash(&object.bytes).to_hex().as_str() != hash {
                return Err("Snapshot dependency hash mismatch".into());
            }
            if let Some(artifact) =
                crate::snapshot_delta::SnapshotArtifactV1::decode(&object.bytes)?
                && object.references != [artifact.base_ref]
            {
                return Err("Snapshot artifact declared base reference mismatch".into());
            }
            Ok(object.bytes.clone())
        };
        let bytes = read(root).map_err(invalid)?;
        if crate::snapshot_delta::SnapshotArtifactV1::decode(&bytes)
            .map_err(invalid)?
            .is_none()
        {
            return Ok(bytes);
        }
        crate::snapshot_delta::resolve_snapshot_artifact_bytes(root, &read).map_err(invalid)
    }

    /// The descriptor is signed material; its chunks remain explicit external dependencies.
    pub fn native_environment(
        &self,
    ) -> Result<Option<crate::native_environment::NativeExecutionEnvironmentV1>, ProtocolError>
    {
        self.native_environment_with(&mut LegacyPackageOps)
    }
    fn native_environment_with<O: PackageValidationOps>(
        &self,
        ops: &mut O,
    ) -> Result<Option<O::Native>, O::Error> {
        ops.work(0)?;
        let Some(id) = self.roots.get(&ArtifactRole::NativeExecutionEnvironment) else {
            return Ok(None);
        };
        let mut found = None;
        for object in &self.objects {
            ops.work(object.content_hash.len().min(id.len()))?;
            if &object.content_hash == id {
                found = Some(object);
                break;
            }
        }
        let object = found.ok_or_else(|| invalid("native descriptor object missing"))?;
        if !object.references.is_empty() {
            return Err(invalid("native external chunks are not embedded references").into());
        }
        ops.native(&object.bytes).map(Some)
    }

    /// Caller must supply an authorized, correctly executed record and declare
    /// every recovery dependency. This validates typed declared closure only,
    /// not player authorization, WASM behavior, result correctness or real fsync.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        crypto::hash_hex(&self.payload_digest)?;
        crypto::hash_hex(&self.before_state_root)?;
        crypto::hash_hex(&self.after_state_root)?;
        if !(ROLES.len()..=ROLES.len() + 1).contains(&self.roots.len())
            || ROLES.iter().any(|r| !self.roots.contains_key(r))
        {
            return Err(invalid(
                "record requires all nine roles and bounded objects",
            ));
        }
        self.native_environment()?;
        self.validate_object_closure()
    }

    /// Validate the exact IWS role set without interpreting it as the legacy
    /// Snapshot/Journal package. Page trees are stored in the separate bounded
    /// IWS page store and are verified by the endpoint owner.
    pub fn validate_iws(&self) -> Result<(), ProtocolError> {
        self.validate_iws_with(&mut LegacyPackageOps)
    }
    fn validate_iws_with<O: PackageValidationOps>(&self, ops: &mut O) -> Result<(), O::Error> {
        const IWS_ROLES: [ArtifactRole; 9] = [
            ArtifactRole::Input,
            ArtifactRole::IncrementalGenesisConfig,
            ArtifactRole::IncrementalRootSet,
            ArtifactRole::IncrementalCommitBody,
            ArtifactRole::IncrementalHistoryPrefix,
            ArtifactRole::ExecutionManifest,
            ArtifactRole::Wasm,
            ArtifactRole::Rules,
            ArtifactRole::NativeExecutionEnvironment,
        ];
        ops.work(self.payload_digest.len())?;
        crypto::hash_hex(&self.payload_digest)?;
        ops.work(self.before_state_root.len())?;
        crypto::hash_hex(&self.before_state_root)?;
        ops.work(self.after_state_root.len())?;
        crypto::hash_hex(&self.after_state_root)?;
        if self.roots.len()
            != IWS_ROLES.len()
                + usize::from(
                    self.roots
                        .contains_key(&ArtifactRole::IncrementalRetentionLedger),
                )
            || {
                let mut missing = false;
                for role in IWS_ROLES {
                    ops.work(0)?;
                    if !self.roots.contains_key(&role) {
                        missing = true;
                        break;
                    }
                }
                missing
            }
            || {
                ops.work(
                    self.payload_digest.len()
                        + self.roots.get(&ArtifactRole::Input).map_or(0, String::len),
                )?;
                self.roots.get(&ArtifactRole::Input) != Some(&self.payload_digest)
            }
        {
            return Err(invalid("incremental record exact roles/input").into());
        }
        for value in self.roots.values() {
            ops.work(value.len())?;
            crypto::hash_hex(value)?;
        }
        if let Some(id) = self.roots.get(&ArtifactRole::IncrementalRetentionLedger) {
            let mut selected = None;
            for object in &self.objects {
                ops.work(object.content_hash.len() + id.len())?;
                if object.content_hash == *id {
                    if selected.is_some() {
                        return Err(invalid("duplicate retention ledger descriptor").into());
                    }
                    selected = Some(object);
                }
            }
            let object = selected.ok_or_else(|| invalid("retention ledger descriptor missing"))?;
            ops.retention(&object.bytes)?;
        }
        self.native_environment_with(ops)?;
        self.validate_object_closure_with(ops)
    }

    fn validate_object_closure(&self) -> Result<(), ProtocolError> {
        self.validate_object_closure_with(&mut LegacyPackageOps)
    }
    fn validate_object_closure_with<O: PackageValidationOps>(
        &self,
        ops: &mut O,
    ) -> Result<(), O::Error> {
        if self.objects.is_empty() || self.objects.len() > MAX_OBJECTS {
            return Err(invalid("record bounded objects").into());
        }
        ops.before_closure(self)?;
        let mut total = 0usize;
        let mut objects = BTreeMap::new();
        let mut previous: Option<&str> = None;
        for object in &self.objects {
            ops.work(object.content_hash.len() * 2)?;
            crypto::hash_hex(&object.content_hash)?;
            if previous.is_some_and(|p| p >= object.content_hash.as_str()) {
                return Err(invalid("objects must be strictly hash ordered").into());
            }
            previous = Some(&object.content_hash);
            total = total
                .checked_add(object.bytes.len())
                .ok_or_else(|| invalid("byte overflow"))?;
            if total > MAX_RECORD_BYTES || object.references.len() > MAX_OBJECTS {
                return Err(invalid("record byte/reference limit").into());
            }
            if ops.hash(&object.bytes)?.to_hex().as_str() != object.content_hash {
                return Err(invalid("artifact byte hash mismatch").into());
            }
            let mut last: Option<&str> = None;
            for reference in &object.references {
                ops.work(reference.len() * 2)?;
                crypto::hash_hex(reference)?;
                if last.is_some_and(|p| p >= reference.as_str()) {
                    return Err(invalid("references must be strictly ordered").into());
                }
                last = Some(reference);
            }
            ops.work(
                object
                    .content_hash
                    .len()
                    .saturating_mul(11)
                    .saturating_mul((objects.len() + 1).ilog2() as usize + 1),
            )?;
            objects.insert(object.content_hash.as_str(), object);
        }
        ops.work(self.roots[&ArtifactRole::Input].len() + self.payload_digest.len())?;
        if self.roots[&ArtifactRole::Input] != self.payload_digest {
            return Err(invalid("payload digest must identify complete input bytes").into());
        }
        let mut reached = BTreeSet::new();
        let mut queue = Vec::with_capacity(self.roots.len());
        for value in self.roots.values() {
            ops.work(0)?;
            queue.push(value.as_str());
        }
        while let Some(hash) = queue.pop() {
            ops.work(
                hash.len()
                    .saturating_mul(11)
                    .saturating_mul((reached.len() + 1).ilog2() as usize + 1),
            )?;
            if !reached.insert(hash) {
                continue;
            }
            ops.work(
                hash.len()
                    .saturating_mul(11)
                    .saturating_mul((objects.len() + 1).ilog2() as usize + 1),
            )?;
            let object = objects
                .get(hash)
                .ok_or_else(|| invalid("declared artifact missing"))?;
            for reference in &object.references {
                ops.work(0)?;
                queue.push(reference.as_str());
            }
        }
        if reached.len() != objects.len() {
            return Err(invalid("unreachable undeclared artifact").into());
        }
        Ok(())
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn byte_len(&self) -> usize {
        self.objects.iter().map(|o| o.bytes.len()).sum()
    }
}

#[path = "package_bounded.rs"]
mod bounded;

pub use bounded::{BoundedIwsValidationErrorV1, PaidIwsValidationRefusalV1};
