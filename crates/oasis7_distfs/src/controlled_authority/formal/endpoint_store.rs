//! Immutable authenticated objects and a signed, bounded hot checkpoint.
//! An index is authority only when its root is committed by that checkpoint.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
const LIMIT: usize = 64 * 1024 * 1024;
const HOT: &str = "formal-checkpoint-v1.cbor";
const DOMAIN: &str = "oasis7.formal.endpoint_checkpoint.sign.v1";
static TEMP: AtomicU64 = AtomicU64::new(0);
#[path = "endpoint_store_durable_set.rs"]
pub(super) mod durable_set;
#[path = "endpoint_store_native.rs"]
mod native;
#[path = "endpoint_store_published_closure.rs"]
pub(super) mod published_closure;
#[path = "endpoint_store_root_verification.rs"]
mod root_verification;

pub(super) use oasis7_formal_protocol::controlled_authority::formal::proof::{
    NativeCheckpointParentV1 as CheckpointParent, NativeHistoryLinkV1 as HistoryLink,
    NativeHistoryNodeV1 as HistoryNode, NativeHotBodyV1 as HotBody,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Hot {
    body: HotBody,
    signature: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
enum Index {
    Leaf {
        request: String,
        entry: String,
    },
    Fork {
        bit: u16,
        left: String,
        right: String,
    },
}
pub(super) struct Store {
    pub path: PathBuf,
    pub body: HotBody,
    pub checkpoint: fencing::EndpointCheckpointBindingV1,
    _lock: File,
    sources: Vec<Arc<dyn super::archive_transfer::FormalArchiveSourceV1>>,
    #[cfg(test)]
    pub fault: Option<u8>,
}
fn io(error: impl std::fmt::Display) -> FormalError {
    FormalError::Io(error.to_string())
}
fn regular(path: &Path) -> Result<(), FormalError> {
    let metadata = fs::symlink_metadata(path).map_err(io)?;
    if !metadata.is_file() || metadata.len() > LIMIT as u64 {
        return Err(invalid("formal storage regular file/size"));
    }
    Ok(())
}
fn directory(path: &Path) -> Result<(), FormalError> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    match builder.create(path) {
        Ok(()) => {
            sync(path)?;
            if let Some(parent) = path.parent() {
                sync(parent)?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if !fs::symlink_metadata(path).map_err(io)?.is_dir() {
                return Err(invalid("formal storage directory/symlink"));
            }
        }
        Err(error) => return Err(io(error)),
    }
    Ok(())
}
fn sync(path: &Path) -> Result<(), FormalError> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(io)
}
fn read(path: &Path) -> Result<Vec<u8>, FormalError> {
    regular(path)?;
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io)?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > LIMIT {
        return Err(invalid("formal storage byte limit"));
    }
    Ok(bytes)
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, FormalError> {
    serde_cbor::from_slice(bytes).map_err(|error| invalid(error.to_string()))
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, FormalError> {
    let bytes = serde_cbor::to_vec(value).map_err(|error| invalid(error.to_string()))?;
    if bytes.len() > LIMIT {
        return Err(invalid("formal object byte limit"));
    }
    Ok(bytes)
}
impl Store {
    pub fn checkpoint_signing_bytes(body: &HotBody) -> Result<Vec<u8>, FormalError> {
        crypto::bytes(DOMAIN, body)
    }
    pub fn sources(&self) -> &[Arc<dyn super::archive_transfer::FormalArchiveSourceV1>] {
        &self.sources
    }
    pub fn open(
        path: &Path,
        initial: HotBody,
        key: &SigningKey,
        sources: Vec<Arc<dyn super::archive_transfer::FormalArchiveSourceV1>>,
    ) -> Result<Self, FormalError> {
        let bytes = Self::signed(&initial, key)?;
        Self::open_signed(
            path,
            initial,
            &hex::encode(key.verifying_key().to_bytes()),
            bytes,
            sources,
        )
    }
    pub fn open_signed(
        path: &Path,
        initial: HotBody,
        public_key: &str,
        initial_bytes: Vec<u8>,
        sources: Vec<Arc<dyn super::archive_transfer::FormalArchiveSourceV1>>,
    ) -> Result<Self, FormalError> {
        if !cfg!(unix) {
            return Err(invalid("formal durable endpoint needs directory fsync"));
        }
        match fs::symlink_metadata(path.join(super::super::storage_shadow::SHADOW_MARKER)) {
            Ok(_) => return Err(invalid("formal shadow directory cannot open an endpoint")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(io(error)),
        }
        directory(path)?;
        let lock_path = path.join(".formal-endpoint.lock");
        if fs::symlink_metadata(&lock_path).is_ok() {
            regular(&lock_path)?;
        }
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let lock = options.open(lock_path).map_err(io)?;
        lock.try_lock()
            .map_err(|_| FormalError::Conflict("formal endpoint locked".into()))?;
        directory(&path.join("formal-objects-v1"))?;
        super::super::business_pack::cleanup_staging_locked(&path.join("formal-objects-v1"))?;
        let file = path.join(HOT);
        if !file.try_exists().map_err(io)?
            || super::super::physical::enabled(&path.join("formal-objects-v1"))?
        {
            super::super::physical::initialize(&path.join("formal-objects-v1"))?;
        }
        let bytes = if file.try_exists().map_err(io)? {
            read(&file)?
        } else {
            initial_bytes
        };
        let body = Self::verify_public(&bytes, public_key)?;
        if body.version != 1
            || body.instance_digest != initial.instance_digest
            || body.authority_id != initial.authority_id
            || body.epoch != initial.epoch
            || body.activation_ref != initial.activation_ref
            || body.import_digest != initial.import_digest
        {
            return Err(invalid("formal checkpoint instance/authority/activation"));
        }
        if !file.try_exists().map_err(io)? {
            Self::atomic(path, &file, &bytes)?;
        }
        let checkpoint = Self::binding(&body, &bytes);
        Ok(Self {
            path: path.to_path_buf(),
            body,
            checkpoint,
            _lock: lock,
            sources,
            #[cfg(test)]
            fault: None,
        })
    }
    fn signed(body: &HotBody, key: &SigningKey) -> Result<Vec<u8>, FormalError> {
        encode(&Hot {
            body: body.clone(),
            signature: hex::encode(key.sign(&crypto::bytes(DOMAIN, body)?).to_bytes()),
        })
    }
    pub fn verify_public(bytes: &[u8], key: &str) -> Result<HotBody, FormalError> {
        let hot: Hot = decode(bytes)?;
        crypto::verify(DOMAIN, &hot.body, &hot.signature, key)?;
        if let Some(binding) = &hot.body.durable_set {
            binding.validate().map_err(fencing_error)?;
        }
        Ok(hot.body)
    }
    pub fn checkpoint_bytes(&self) -> Result<Vec<u8>, FormalError> {
        read(&self.path.join(HOT))
    }
    pub fn raw_object(&self, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
        hash_hex(id)?;
        let path = self.path.join("formal-objects-v1").join(id);
        if let Some(bytes) = super::super::business_pack::read_object(&path, id)? {
            return Ok(Some(bytes));
        }
        for source in &self.sources {
            if let Some(bytes) = source.read_object(id)? {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }
    pub fn binding(body: &HotBody, bytes: &[u8]) -> fencing::EndpointCheckpointBindingV1 {
        let head = body.decision_head.as_ref().unwrap_or(&body.head);
        fencing::EndpointCheckpointBindingV1 {
            revision: body.revision,
            commitment: blake3::hash(bytes).to_hex().to_string(),
            head: crate::controlled_authority::replicated_protocol::HeadAnchor {
                position: head.position,
                decision_hash: head.record_id.clone(),
                qualified: body.decision_head.is_none(),
            },
        }
    }
    fn atomic(dir: &Path, target: &Path, bytes: &[u8]) -> Result<(), FormalError> {
        let path = dir.join(format!(
            ".formal-temp-{}-{}",
            std::process::id(),
            TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&path).map_err(io)?;
        file.write_all(bytes).map_err(io)?;
        file.sync_all().map_err(io)?;
        fs::rename(&path, target).map_err(io)?;
        sync(dir)
    }
    pub fn get<T: DeserializeOwned>(&self, id: &str) -> Result<T, FormalError> {
        let bytes = self
            .raw_object(id)?
            .ok_or_else(|| invalid("referenced formal archive object missing"))?;
        if bytes.len() > LIMIT {
            return Err(invalid("formal archive source object limit"));
        }
        if blake3::hash(&bytes).to_hex().as_str() != id {
            return Err(invalid("formal object checksum"));
        }
        decode(&bytes)
    }
    pub fn put<T: Serialize>(&self, value: &T) -> Result<String, FormalError> {
        let bytes = encode(value)?;
        let id = blake3::hash(&bytes).to_hex().to_string();
        let dir = self.path.join("formal-objects-v1");
        let target = dir.join(&id);
        let writer = super::super::physical::publication_guard(&dir)?;
        if let Some(existing) = super::super::business_pack::read_object(&target, &id)? {
            if existing != bytes {
                return Err(invalid("formal immutable object changed"));
            }
            super::super::business_pack::read_object_durable(&target, &id)?;
            sync(&dir)?;
        } else {
            if !super::super::physical::persist_guarded(
                &dir,
                &[(id.clone(), bytes.clone())],
                &writer,
            )? {
                Self::atomic(&dir, &target, &bytes)?;
            }
        }
        if super::super::physical::enabled(&dir)? {
            super::super::physical::roots::retain(&dir, std::slice::from_ref(&id))?;
        }
        Ok(id)
    }
    pub fn refresh_public(&mut self, public_key: &str) -> Result<(), FormalError> {
        let bytes = read(&self.path.join(HOT))?;
        let body = Self::verify_public(&bytes, public_key)?;
        if Self::binding(&body, &bytes) != self.checkpoint || body != self.body {
            return Err(FormalError::Rollback(
                "formal checkpoint externally changed".into(),
            ));
        }
        Ok(())
    }
    pub fn reconcile_public(
        &mut self,
        guard: &mut dyn fencing::MonotonicEndpointGuard,
        public_key: &str,
    ) -> Result<(), FormalError> {
        if let Some(intent) = guard.pending().cloned() {
            if self.checkpoint != intent.previous && self.checkpoint != intent.next {
                return Err(FormalError::Rollback(
                    "formal checkpoint outside monotonic intent".into(),
                ));
            }
            let body = Self::verify_public(&intent.checkpoint_bytes, public_key)?;
            if Self::binding(&body, &intent.checkpoint_bytes) != intent.next
                || body.instance_digest != self.body.instance_digest
                || body.authority_id != self.body.authority_id
                || Some(body.revision) != intent.previous.revision.checked_add(1)
                || body.seal != intent.seal.or(self.body.seal.clone())
            {
                return Err(invalid("formal external checkpoint intent binding"));
            }
            self.validate_refs(&body)?;
            if self.checkpoint == intent.previous {
                Self::atomic(&self.path, &self.path.join(HOT), &intent.checkpoint_bytes)?;
            }
            sync(&self.path.join(HOT))?;
            sync(&self.path)?;
            guard.finish().map_err(fencing_error)?;
            self.body = body;
            self.checkpoint = intent.next;
        }
        if guard.checkpoint() != &self.checkpoint || guard.seal() != self.body.seal.as_ref() {
            return Err(FormalError::Rollback(
                "formal monotonic checkpoint/seal mismatch".into(),
            ));
        }
        self.validate_refs(&self.body)
    }
    fn validate_refs(&self, body: &HotBody) -> Result<(), FormalError> {
        if body.head.position > 0
            && (body.index.is_none()
                || (body.head_node.is_none() && body.imported_head_node.is_none()))
        {
            return Err(invalid(
                "formal nonzero head requires permanent request/history roots",
            ));
        }

        if let Some(id) = &body.imported_head_node {
            let node: HistoryNode = self.get(id)?;
            let parent = body
                .base_parent
                .as_ref()
                .ok_or_else(|| invalid("formal imported archive missing verified cut"))?;
            if node.head.position != parent.record.position
                || node.head.record_id != parent.record.record_id()?
            {
                return Err(invalid("formal imported archive canonical cut"));
            }
        }
        if let Some(root) = &body.index {
            let _: Index = self.get(root)?;
        }
        if let Some(id) = &body.pending {
            let _: FormalRequestStateV1 = self.get(id)?;
        }
        if let Some(id) = &body.head_node {
            let node: HistoryNode = self.get(id)?;
            if node.head != body.head || node.ancestors.len() > 64 {
                return Err(invalid("formal head/node"));
            }
            let entry: FormalRequestStateV1 = self.get(&node.entry)?;
            if !matches!(entry, FormalRequestStateV1::Qualified { .. })
                || entry.position() != node.head.position
                || entry.request_digest() != node.request
            {
                return Err(invalid("formal qualified head entry"));
            }
        }
        Ok(())
    }
    pub fn publish_with(
        &mut self,
        mut next: HotBody,
        guard: &mut dyn fencing::MonotonicEndpointGuard,
        public_key: &str,
        sign: impl FnOnce(
            &HotBody,
            &mut dyn fencing::MonotonicEndpointGuard,
        ) -> Result<String, FormalError>,
    ) -> Result<(), FormalError> {
        self.refresh_public(public_key)?;
        if guard.seal().is_some() || self.body.seal.is_some() {
            return Err(FormalError::Conflict("formal epoch sealed".into()));
        }
        next.revision = self
            .body
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("formal revision overflow"))?;
        self.validate_refs(&next)?;
        let bytes = encode(&Hot {
            body: next.clone(),
            signature: sign(&next, guard)?,
        })?;
        Self::verify_public(&bytes, public_key)?;
        let binding = Self::binding(&next, &bytes);
        guard
            .begin(fencing::CheckpointIntentV1 {
                previous: self.checkpoint.clone(),
                next: binding.clone(),
                checkpoint_bytes: bytes.clone(),
                seal: next.seal.clone(),
            })
            .map_err(fencing_error)?;
        #[cfg(test)]
        let fault = self.fault.take();
        #[cfg(test)]
        if fault == Some(1) {
            return Err(FormalError::Io("injected after provider intent".into()));
        }
        Self::atomic(&self.path, &self.path.join(HOT), &bytes)?;
        #[cfg(test)]
        if fault == Some(2) {
            return Err(FormalError::Io("injected after native checkpoint".into()));
        }
        guard.finish().map_err(fencing_error)?;
        self.body = next;
        self.checkpoint = binding;
        Ok(())
    }
    pub fn lookup(&self, request: &str) -> Result<Option<String>, FormalError> {
        hash_hex(request)?;
        let mut id = self.body.index.clone();
        let mut previous = None;
        while let Some(reference) = id {
            match self.get::<Index>(&reference)? {
                Index::Leaf {
                    request: key,
                    entry,
                } => {
                    hash_hex(&key)?;
                    return Ok((key == request).then_some(entry));
                }
                Index::Fork { bit, left, right } => {
                    if bit >= 256 || previous.is_some_and(|old| bit <= old) {
                        return Err(invalid("formal index path order"));
                    }
                    previous = Some(bit);
                    id = Some(if Self::bit(request, bit)? {
                        right
                    } else {
                        left
                    });
                }
            }
        }
        Ok(None)
    }
    fn bit(key: &str, bit: u16) -> Result<bool, FormalError> {
        let bytes = hex::decode(key).map_err(|_| invalid("formal index key"))?;
        Ok(bytes[bit as usize / 8] & (128 >> (bit % 8)) != 0)
    }
    pub fn insert(
        &self,
        root: Option<&str>,
        request: &str,
        entry: &str,
    ) -> Result<String, FormalError> {
        hash_hex(request)?;
        hash_hex(entry)?;
        let leaf = Index::Leaf {
            request: request.into(),
            entry: entry.into(),
        };
        let Some(root) = root else {
            return self.put(&leaf);
        };
        let mut cursor = root.to_string();
        let existing;
        let mut previous = None;
        loop {
            match self.get::<Index>(&cursor)? {
                Index::Leaf { request: key, .. } => {
                    existing = key;
                    break;
                }
                Index::Fork { bit, left, right } => {
                    if bit >= 256 || previous.is_some_and(|old| bit <= old) {
                        return Err(invalid("formal index bit order"));
                    }
                    previous = Some(bit);
                    cursor = if Self::bit(request, bit)? {
                        right
                    } else {
                        left
                    };
                }
            }
        }
        let differing =
            (0..256).find(|bit| Self::bit(request, *bit).ok() != Self::bit(&existing, *bit).ok());
        self.insert_at(root, request, &leaf, differing, 0)
    }
    fn insert_at(
        &self,
        root: &str,
        request: &str,
        leaf: &Index,
        differing: Option<u16>,
        depth: u16,
    ) -> Result<String, FormalError> {
        if depth > 256 {
            return Err(invalid("formal index depth"));
        }
        let node: Index = self.get(root)?;
        if let Some(bit) = differing
            && match &node {
                Index::Leaf { .. } => true,
                Index::Fork { bit: next, .. } => bit < *next,
            }
        {
            let id = self.put(leaf)?;
            let (left, right) = if Self::bit(request, bit)? {
                (root.into(), id)
            } else {
                (id, root.into())
            };
            return self.put(&Index::Fork { bit, left, right });
        }
        match node {
            Index::Leaf { request: key, .. } if key == request => self.put(leaf),
            Index::Leaf { .. } => Err(invalid("formal index insertion")),
            Index::Fork {
                bit,
                mut left,
                mut right,
            } => {
                if Self::bit(request, bit)? {
                    right = self.insert_at(&right, request, leaf, differing, depth + 1)?;
                } else {
                    left = self.insert_at(&left, request, leaf, differing, depth + 1)?;
                }
                self.put(&Index::Fork { bit, left, right })
            }
        }
    }
    pub fn head_entry(&self) -> Result<Option<FormalRequestStateV1>, FormalError> {
        self.body
            .head_node
            .as_ref()
            .map(|id| {
                self.get::<HistoryNode>(id)
                    .and_then(|node| self.get(&node.entry))
            })
            .transpose()
    }
    fn walk_minimum(
        body: &HotBody,
        minimum: &FormalHeadRefV1,
        mut load: impl FnMut(&str) -> Result<Vec<u8>, FormalError>,
    ) -> Result<Vec<Vec<u8>>, FormalError> {
        hash_hex(&minimum.record_id)?;
        if minimum.position > body.head.position {
            return Err(FormalError::Rollback("formal below minimum".into()));
        }
        if minimum.position == 0 {
            return Ok(vec![]);
        }
        let mut head = body.head.clone();
        let mut cursor = body.head_node.clone().or(body.imported_head_node.clone());
        let mut witness = Vec::new();
        while head.position > minimum.position {
            if witness.len() >= 64 {
                return Err(invalid("formal ancestry witness bound"));
            }
            let id = cursor.ok_or_else(|| invalid("formal minimum history unavailable"))?;
            hash_hex(&id)?;
            let bytes = load(&id)?;
            if bytes.len() > 128 * 1024 || blake3::hash(&bytes).to_hex().as_str() != id {
                return Err(invalid("formal ancestry node checksum/bound"));
            }
            let node: HistoryNode = decode(&bytes)?;
            if node.head != head || node.ancestors.is_empty() || node.ancestors.len() > 64 {
                return Err(invalid("formal minimum node identity"));
            }
            let level = (63 - (head.position - minimum.position).leading_zeros()) as usize;
            let used = level.min(node.ancestors.len() - 1);
            let link = &node.ancestors[used];
            if head.position.checked_sub(1u64 << used) != Some(link.head.position) {
                return Err(invalid("formal minimum skip position"));
            }
            head = link.head.clone();
            cursor = Some(link.node.clone());
            witness.push(bytes);
        }
        if &head != minimum {
            return Err(FormalError::Rollback(
                "formal minimum canonical fork".into(),
            ));
        }
        Ok(witness)
    }
    pub fn minimum_witness(&self, minimum: &FormalHeadRefV1) -> Result<Vec<Vec<u8>>, FormalError> {
        Self::walk_minimum(&self.body, minimum, |id| {
            self.raw_object(id)?
                .ok_or_else(|| invalid("formal ancestry object missing"))
        })
    }
    pub fn verify_minimum(&self, minimum: &FormalHeadRefV1) -> Result<(), FormalError> {
        self.minimum_witness(minimum).map(|_| ())
    }
    pub fn verify_remote_minimum(
        bytes: &[u8],
        key: &str,
        minimum: &FormalHeadRefV1,
        witness: &[Vec<u8>],
    ) -> Result<HotBody, FormalError> {
        if bytes.len() > 64 * 1024 || witness.len() > 64 {
            return Err(invalid("formal RPC checkpoint/ancestry bound"));
        }
        let body = Self::verify_public(bytes, key)?;
        if body.version != 1 {
            return Err(invalid("formal RPC checkpoint version"));
        }
        let mut nodes = witness.iter();
        Self::walk_minimum(&body, minimum, |_| {
            nodes
                .next()
                .cloned()
                .ok_or_else(|| invalid("formal RPC missing ancestry bridge"))
        })?;
        if nodes.next().is_some() {
            return Err(invalid("formal RPC extra ancestry nodes"));
        }
        Ok(body)
    }
    pub fn append_node(
        &self,
        head: FormalHeadRefV1,
        request: String,
        entry: String,
    ) -> Result<String, FormalError> {
        if Some(head.position) != self.body.head.position.checked_add(1) {
            return Err(invalid("formal history position"));
        }
        let mut ancestors = Vec::new();
        if let Some(id) = self
            .body
            .head_node
            .as_ref()
            .or(self.body.imported_head_node.as_ref())
        {
            ancestors.push(HistoryLink {
                head: self.body.head.clone(),
                node: id.clone(),
            });
            for level in 1..64 {
                let node: HistoryNode = self.get(&ancestors[level - 1].node)?;
                let Some(link) = node.ancestors.get(level - 1) else {
                    break;
                };
                if head.position.checked_sub(1u64 << level) != Some(link.head.position) {
                    return Err(invalid("formal history ancestor position"));
                }
                ancestors.push(link.clone());
            }
        }
        self.put(&HistoryNode {
            head,
            request,
            entry,
            ancestors,
        })
    }
}
