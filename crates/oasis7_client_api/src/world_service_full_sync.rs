//! Strict wire contracts for grant-authorized full-state synchronization.
//!
//! These values are claims and transport metadata only. A consumer must verify
//! the grant against its externally pinned activation issuer and independently
//! verify the requested cut before accepting any returned bytes.
use crate::world_service::WorldIdentity;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const FORMAL_FULL_SYNC_MANIFEST_PATH: &str = "/v1/formal/full-sync/manifest";
pub const FORMAL_FULL_SYNC_OBJECT_PATH: &str = "/v1/formal/full-sync/object";
pub const MAX_FULL_SYNC_CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_FULL_SYNC_RECORD_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_FULL_SYNC_OBJECTS: usize = 256;
pub const MAX_FULL_SYNC_HISTORY_PATH: usize = 257;
const GRANT_DOMAIN: &str = "oasis7.formal-node-read-grant.v1";
const MAX_GRANT_LIFETIME_MS: u64 = 30 * 24 * 60 * 60 * 1000;

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn grant_bytes(
    issuer_public_key: &str,
    body: &FormalNodeReadGrantBodyV1,
) -> Result<Vec<u8>, String> {
    let mut bytes = GRANT_DOMAIN.as_bytes().to_vec();
    bytes.push(0);
    bytes.extend(serde_json::to_vec(&(issuer_public_key, body)).map_err(|e| e.to_string())?);
    Ok(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormalNodeReadRoleV1 {
    StateSync,
    FullReadOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalNodeReadGrantBodyV1 {
    pub schema_version: u32,
    pub world: WorldIdentity,
    pub chain_id: String,
    pub node_public_key: String,
    pub role: FormalNodeReadRoleV1,
    /// Lease generation issued by the trusted activation issuer.
    pub grant_epoch: u64,
    /// Exact formal authority epoch for which this grant is valid. A lawful
    /// recovery advances the authority epoch and therefore requires a newly
    /// issued grant; it never legitimizes an old-epoch reader automatically.
    pub authority_epoch: u64,
    pub not_before_unix_ms: u64,
    pub expires_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalNodeReadGrantV1 {
    pub issuer_public_key: String,
    pub body: FormalNodeReadGrantBodyV1,
    pub signature_hex: String,
}

impl FormalNodeReadGrantV1 {
    /// Build a grant using an issuer key that the consumer will independently
    /// compare with InitialActivationTrustV1.issuer_public_key.
    pub fn sign(
        body: FormalNodeReadGrantBodyV1,
        issuer_private_key_hex: &str,
    ) -> Result<Self, String> {
        validate_grant_body(&body)?;
        let private_bytes: [u8; 32] = hex::decode(issuer_private_key_hex)
            .map_err(|error| error.to_string())?
            .try_into()
            .map_err(|_| "formal node grant issuer key length".to_owned())?;
        let signing_key = SigningKey::from_bytes(&private_bytes);
        let issuer_public_key = hex::encode(signing_key.verifying_key().to_bytes());
        let signature_hex = hex::encode(
            signing_key
                .sign(&grant_bytes(&issuer_public_key, &body)?)
                .to_bytes(),
        );
        Ok(Self {
            issuer_public_key,
            body,
            signature_hex,
        })
    }

    /// Verify an issuer-signed, time-bounded lease against all local identity
    /// inputs. The caller supplies its independently verified current epoch.
    #[expect(
        clippy::too_many_arguments,
        reason = "Independent issuer, world, authority, audience and time bindings are required to verify this signed grant"
    )]
    pub fn verify(
        &self,
        expected_issuer_public_key: &str,
        expected_world: &WorldIdentity,
        expected_chain_id: &str,
        expected_node_public_key: &str,
        expected_role: &FormalNodeReadRoleV1,
        current_authority_epoch: u64,
        now_unix_ms: u64,
    ) -> Result<(), String> {
        validate_grant_body(&self.body)?;
        if self.issuer_public_key != expected_issuer_public_key
            || !key(expected_issuer_public_key)
            || self.body.world != *expected_world
            || self.body.chain_id != expected_chain_id
            || self.body.node_public_key != expected_node_public_key
            || self.body.role != *expected_role
            || self.body.authority_epoch != current_authority_epoch
            || now_unix_ms < self.body.not_before_unix_ms
            || now_unix_ms >= self.body.expires_unix_ms
        {
            return Err("formal node read grant identity, epoch, role, or lease mismatch".into());
        }
        let signature_bytes: [u8; 64] = hex::decode(&self.signature_hex)
            .map_err(|error| error.to_string())?
            .try_into()
            .map_err(|_| "formal node grant signature length".to_owned())?;
        let issuer_bytes: [u8; 32] = hex::decode(expected_issuer_public_key)
            .map_err(|error| error.to_string())?
            .try_into()
            .map_err(|_| "formal node grant issuer key length".to_owned())?;
        let verifying_key =
            VerifyingKey::from_bytes(&issuer_bytes).map_err(|error| error.to_string())?;
        verifying_key
            .verify(
                &grant_bytes(&self.issuer_public_key, &self.body)?,
                &Signature::from_bytes(&signature_bytes),
            )
            .map_err(|error| format!("formal node grant signature: {error}"))
    }
}

fn validate_grant_body(body: &FormalNodeReadGrantBodyV1) -> Result<(), String> {
    body.world.validate().map_err(|error| error.to_string())?;
    if body.schema_version != 1
        || body.chain_id.trim().is_empty()
        || body.chain_id.len() > 256
        || !key(&body.node_public_key)
        || body.grant_epoch == 0
        || body.authority_epoch == 0
        || body.not_before_unix_ms >= body.expires_unix_ms
        || body.expires_unix_ms - body.not_before_unix_ms > MAX_GRANT_LIFETIME_MS
    {
        return Err("formal node read grant bounds/version".into());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFullSyncCutV1 {
    pub world: WorldIdentity,
    pub chain_id: String,
    pub position: u64,
    pub record_id: String,
    pub after_state_root: String,
    pub active_authority_id: String,
    pub finalizing_authority_id: String,
    pub authority_epoch: u64,
}
impl FormalFullSyncCutV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.world.validate().map_err(|error| error.to_string())?;
        if self.chain_id.trim().is_empty()
            || self.chain_id.len() > 256
            || self.position == 0
            || !digest(&self.record_id)
            || !digest(&self.after_state_root)
            || !digest(&self.active_authority_id)
            || !digest(&self.finalizing_authority_id)
            || self.authority_epoch == 0
        {
            return Err("formal full-sync cut bounds".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFullSyncManifestRequestV1 {
    pub schema_version: u32,
    pub grant: FormalNodeReadGrantV1,
    pub cut: FormalFullSyncCutV1,
}
impl FormalFullSyncManifestRequestV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.cut.validate()?;
        validate_grant_body(&self.grant.body)?;
        if self.schema_version != 1 {
            return Err("formal full-sync manifest request version".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormalFullSyncArtifactRoleV1 {
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFullSyncArtifactRootV1 {
    pub role: FormalFullSyncArtifactRoleV1,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFullSyncObjectDescriptorV1 {
    pub content_hash: String,
    pub size_bytes: u64,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFullSyncManifestV1 {
    pub schema_version: u32,
    pub cut: FormalFullSyncCutV1,
    pub artifact_roots: Vec<FormalFullSyncArtifactRootV1>,
    pub objects: Vec<FormalFullSyncObjectDescriptorV1>,
    pub retained_history_root_digest: Option<String>,
}
impl FormalFullSyncManifestV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.cut.validate()?;
        if !matches!(self.schema_version, 1 | 2)
            || self.artifact_roots.len() != if self.schema_version == 2 { 10 } else { 9 }
            || self.objects.is_empty()
            || self.objects.len() > MAX_FULL_SYNC_OBJECTS
            || self
                .retained_history_root_digest
                .as_deref()
                .is_some_and(|value| !digest(value))
        {
            return Err("formal full-sync manifest bounds/version".into());
        }
        let mut roots = BTreeMap::new();
        for root in &self.artifact_roots {
            if !digest(&root.content_hash) || roots.insert(root.role, &root.content_hash).is_some()
            {
                return Err("formal full-sync artifact root identity".into());
            }
        }
        if roots.len() != if self.schema_version == 2 { 10 } else { 9 }
            || roots.contains_key(&FormalFullSyncArtifactRoleV1::NativeExecutionEnvironment)
                != (self.schema_version == 2)
            || !roots.contains_key(&FormalFullSyncArtifactRoleV1::Snapshot)
            || !roots.contains_key(&FormalFullSyncArtifactRoleV1::Journal)
        {
            return Err("formal full-sync required artifact roots".into());
        }
        let mut objects = BTreeMap::new();
        let mut previous = None;
        let mut total = 0u64;
        for object in &self.objects {
            if !digest(&object.content_hash)
                || previous.is_some_and(|value: &str| value >= object.content_hash.as_str())
                || object.size_bytes > MAX_FULL_SYNC_RECORD_BYTES as u64
                || object.references.len() > MAX_FULL_SYNC_OBJECTS
                || object.references.iter().any(|reference| !digest(reference))
                || object.references.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err("formal full-sync object descriptor bounds/order".into());
            }
            total = total
                .checked_add(object.size_bytes)
                .ok_or("formal full-sync total byte overflow")?;
            if total > MAX_FULL_SYNC_RECORD_BYTES as u64 {
                return Err("formal full-sync record byte bound".into());
            }
            previous = Some(object.content_hash.as_str());
            objects.insert(object.content_hash.as_str(), object);
        }
        let mut reached = BTreeSet::new();
        let mut pending = roots.values().copied().collect::<Vec<_>>();
        while let Some(hash) = pending.pop() {
            if !reached.insert(hash.as_str()) {
                continue;
            }
            let object = objects
                .get(hash.as_str())
                .ok_or("formal full-sync manifest omits rooted artifact")?;
            pending.extend(object.references.iter());
        }
        if reached.len() != objects.len() {
            return Err("formal full-sync manifest has unrooted objects".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFullSyncObjectRequestV1 {
    pub schema_version: u32,
    pub grant: FormalNodeReadGrantV1,
    pub cut: FormalFullSyncCutV1,
    pub content_hash: String,
    pub offset: u64,
    pub max_bytes: u32,
    pub retained_history_root_digest: Option<String>,
    /// When present, this is a root-to-target path through the retained
    /// business-index tree. It prevents the CAS route becoming a cross-world
    /// hash oracle for unrelated private objects in a shared store.
    pub business_path: Option<Vec<String>>,
}
impl FormalFullSyncObjectRequestV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.cut.validate()?;
        validate_grant_body(&self.grant.body)?;
        if self.schema_version != 1
            || !digest(&self.content_hash)
            || self.max_bytes == 0
            || self.max_bytes as usize > MAX_FULL_SYNC_CHUNK_BYTES
            || self
                .retained_history_root_digest
                .as_deref()
                .is_some_and(|value| !digest(value))
        {
            return Err("formal full-sync object request bounds/version".into());
        }
        if let Some(path) = &self.business_path
            && (path.is_empty()
                || path.len() > MAX_FULL_SYNC_HISTORY_PATH
                || path.iter().any(|hash| !digest(hash))
                || path.last() != Some(&self.content_hash)
                || self.retained_history_root_digest.is_none())
        {
            return Err("formal full-sync retained-tree path bounds".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalFullSyncObjectChunkV1 {
    pub schema_version: u32,
    pub cut: FormalFullSyncCutV1,
    pub content_hash: String,
    pub total_bytes: u64,
    pub offset: u64,
    pub bytes: Vec<u8>,
}
impl FormalFullSyncObjectChunkV1 {
    pub fn validate_for(&self, request: &FormalFullSyncObjectRequestV1) -> Result<(), String> {
        let end = self
            .offset
            .checked_add(self.bytes.len() as u64)
            .ok_or("formal full-sync chunk offset overflow")?;
        if self.schema_version != 1
            || self.cut != request.cut
            || self.content_hash != request.content_hash
            || self.total_bytes > MAX_FULL_SYNC_RECORD_BYTES as u64
            || self.offset != request.offset
            || self.bytes.len() > request.max_bytes as usize
            || end > self.total_bytes
            || (self.bytes.is_empty() && self.offset < self.total_bytes)
        {
            return Err("formal full-sync object chunk does not match request".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> WorldIdentity {
        WorldIdentity {
            world_id: "world-a".into(),
            genesis_digest: "a".repeat(64),
        }
    }

    fn body(node: &SigningKey) -> FormalNodeReadGrantBodyV1 {
        FormalNodeReadGrantBodyV1 {
            schema_version: 1,
            world: world(),
            chain_id: "chain-a".into(),
            node_public_key: hex::encode(node.verifying_key().to_bytes()),
            role: FormalNodeReadRoleV1::StateSync,
            grant_epoch: 4,
            authority_epoch: 4,
            not_before_unix_ms: 10,
            expires_unix_ms: 100,
        }
    }

    #[test]
    fn grant_is_tied_to_external_issuer_node_world_role_epoch_and_time() {
        let issuer = SigningKey::from_bytes(&[11; 32]);
        let node = SigningKey::from_bytes(&[12; 32]);
        let grant = FormalNodeReadGrantV1::sign(body(&node), &hex::encode([11; 32])).unwrap();
        let issuer_public_key = hex::encode(issuer.verifying_key().to_bytes());
        let node_public_key = hex::encode(node.verifying_key().to_bytes());
        let verify = |issuer_key: &str,
                      world: &WorldIdentity,
                      node_key: &str,
                      role: &FormalNodeReadRoleV1,
                      epoch: u64,
                      now: u64| {
            grant.verify(issuer_key, world, "chain-a", node_key, role, epoch, now)
        };
        assert!(
            verify(
                &issuer_public_key,
                &world(),
                &node_public_key,
                &FormalNodeReadRoleV1::StateSync,
                4,
                50,
            )
            .is_ok()
        );
        assert!(
            verify(
                &hex::encode(SigningKey::from_bytes(&[13; 32]).verifying_key().to_bytes()),
                &world(),
                &node_public_key,
                &FormalNodeReadRoleV1::StateSync,
                4,
                50,
            )
            .is_err()
        );
        assert!(
            verify(
                &issuer_public_key,
                &world(),
                &node_public_key,
                &FormalNodeReadRoleV1::FullReadOnly,
                4,
                50,
            )
            .is_err()
        );
        assert!(
            verify(
                &issuer_public_key,
                &world(),
                &node_public_key,
                &FormalNodeReadRoleV1::StateSync,
                5,
                50,
            )
            .is_err()
        );
        assert!(
            verify(
                &issuer_public_key,
                &world(),
                &node_public_key,
                &FormalNodeReadRoleV1::StateSync,
                4,
                100,
            )
            .is_err()
        );
    }

    #[test]
    fn object_request_limits_chunks_and_requires_rooted_tree_paths() {
        let issuer = SigningKey::from_bytes(&[21; 32]);
        let node = SigningKey::from_bytes(&[22; 32]);
        let grant = FormalNodeReadGrantV1::sign(body(&node), &hex::encode([21; 32])).unwrap();
        let cut = FormalFullSyncCutV1 {
            world: world(),
            chain_id: "chain-a".into(),
            position: 12,
            record_id: "b".repeat(64),
            after_state_root: "c".repeat(64),
            active_authority_id: "d".repeat(64),
            finalizing_authority_id: "e".repeat(64),
            authority_epoch: 4,
        };
        let mut request = FormalFullSyncObjectRequestV1 {
            schema_version: 1,
            grant,
            cut,
            content_hash: "f".repeat(64),
            offset: 0,
            max_bytes: MAX_FULL_SYNC_CHUNK_BYTES as u32,
            retained_history_root_digest: None,
            business_path: None,
        };
        assert!(request.validate().is_ok());
        request.max_bytes = MAX_FULL_SYNC_CHUNK_BYTES as u32 + 1;
        assert!(request.validate().is_err());
        request.max_bytes = 1024;
        request.business_path = Some(vec![request.content_hash.clone()]);
        assert!(request.validate().is_err());
        request.retained_history_root_digest = Some("a".repeat(64));
        assert!(request.validate().is_ok());
        let _ = issuer;
    }
}
