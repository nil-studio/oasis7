//! Shared immutable CAS transport and bounded content cache. Authority is only
//! rebound during construction from a completely verified startup activation.
use super::*;

/// Read-only CAS adapter for the profile-2 business tree. Every cache entry
/// was fetched from both signed replicas and checked against its content hash;
/// absence is deliberately not cached because a later qualified commit may
/// introduce that immutable object.
pub(crate) struct RemoteBusinessIndexReader {
    primary: Mutex<RemoteFormalEndpoint>,
    replica: Mutex<RemoteFormalEndpoint>,
    cache: Mutex<BoundedBusinessNodeCache>,
}

impl RemoteBusinessIndexReader {
    /// Constructor-only rebinding after complete external-origin activation
    /// replay. Both the resolver and writer retain this same content-reader Arc.
    pub(super) fn bind_verified_startup(
        &self,
        validated: &ValidatedFormalLiveConfigV1,
        startup: &super::super::formal_live_history::FormalLiveStartupAnchorV1,
    ) -> Result<(), String> {
        let Some(activation) = startup.activation.as_ref() else {
            return Ok(());
        };
        let authority = startup.anchor.authority();
        let context = authority
            .context_digest()
            .map_err(|error| error.to_string())?;
        if context
            != activation
                .successor()
                .context_digest()
                .map_err(|error| error.to_string())?
            || startup.anchor.head().record() != activation.boundary().record()
        {
            return Err("formal business reader startup differs from verified activation".into());
        }
        if context
            == validated
                .active_authority()
                .context_digest()
                .map_err(|error| error.to_string())?
        {
            // No authority transition: preserve the original clients and their
            // monotonically advanced minimum rather than creating fresh ones.
            return Ok(());
        }
        let (primary, replica) =
            business_endpoints_for(validated, authority, startup.anchor.head().head())?;
        // Acquire both locks before changing either endpoint; a poisoned second
        // lock must not leave a half-rebound reader. Construction has no workers.
        let mut current_primary = self
            .primary
            .lock()
            .map_err(|_| "formal primary business reader poisoned")?;
        let mut current_replica = self
            .replica
            .lock()
            .map_err(|_| "formal replica business reader poisoned")?;
        *current_primary = primary;
        *current_replica = replica;
        Ok(())
    }

    pub(crate) fn new(
        primary: RemoteFormalEndpoint,
        replica: RemoteFormalEndpoint,
        max_bytes: usize,
        max_entries: usize,
    ) -> Self {
        Self {
            primary: Mutex::new(primary),
            replica: Mutex::new(replica),
            cache: Mutex::new(BoundedBusinessNodeCache::new(max_bytes, max_entries)),
        }
    }

    pub(crate) fn seed_verified_writes(
        &self,
        writes: Vec<BusinessIndexNodeWriteV1>,
    ) -> Result<(), String> {
        // Validate the complete batch before mutating the shared cache. Startup
        // history prewarming stages an all-or-nothing verified suffix, so a
        // malformed later write must not leave earlier bytes installed.
        for node in &writes {
            validate_business_node_digest(&node.digest)?;
            if hex::encode(blake3::hash(&node.bytes).as_bytes()) != node.digest {
                return Err(format!(
                    "verified formal business node {} hash mismatch",
                    node.digest
                ));
            }
        }
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "formal business node cache poisoned")?;
        for node in writes {
            cache.insert(node.digest, node.bytes);
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn seed_verified_business_writes(
    cache: &mut BoundedBusinessNodeCache,
    writes: Vec<BusinessIndexNodeWriteV1>,
) -> Result<(), String> {
    for node in writes {
        validate_business_node_digest(&node.digest)?;
        if hex::encode(blake3::hash(&node.bytes).as_bytes()) != node.digest {
            return Err(format!(
                "verified formal business node {} hash mismatch",
                node.digest
            ));
        }
        cache.insert(node.digest, node.bytes);
    }
    Ok(())
}

// A cache probe must never invoke remote prefetch recursively. Missing bytes are
// an incomplete local path, not a proof of absence.
struct CachedBusinessPath<'a>(&'a RemoteBusinessIndexReader);

impl BusinessIndexNodeReader for CachedBusinessPath<'_> {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        self.0.read_cached_node(digest)
    }
}

impl BusinessIndexNodeReader for RemoteBusinessIndexReader {
    fn prefetch_path(&self, root: &BusinessIndexRootV1, key: &str) -> Result<(), String> {
        if root.get(key, &CachedBusinessPath(self)).is_ok() {
            return Ok(());
        }
        let primary = self
            .primary
            .lock()
            .map_err(|_| "formal primary business reader poisoned")?
            .business_index_path(root, key)
            .map_err(|error| format!("formal primary business path: {error}"))?;
        let replica = self
            .replica
            .lock()
            .map_err(|_| "formal replica business reader poisoned")?
            .business_index_path(root, key)
            .map_err(|error| format!("formal replica business path: {error}"))?;
        // Both authenticated responses must independently prove this exact
        // root/key. Stage everything before mutating the shared content cache.
        self.seed_verified_writes(verify_dual_business_path(root, key, primary, replica)?)
    }

    fn cache_content_node(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        self.seed_verified_writes(vec![BusinessIndexNodeWriteV1 {
            digest: digest.to_owned(),
            bytes: bytes.to_vec(),
        }])
    }
    fn read_cached_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        validate_business_node_digest(digest)?;
        Ok(self
            .cache
            .lock()
            .map_err(|_| "formal business node cache poisoned")?
            .get(digest))
    }
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        validate_business_node_digest(digest)?;
        if let Some(bytes) = self
            .cache
            .lock()
            .map_err(|_| "formal business node cache poisoned")?
            .get(digest)
        {
            return Ok(Some(bytes));
        }

        let primary = self
            .primary
            .lock()
            .map_err(|_| "formal primary business reader poisoned")?
            .archive_object(digest)
            .map_err(|error| format!("formal primary business node {digest}: {error}"))?;
        let replica = self
            .replica
            .lock()
            .map_err(|_| "formal replica business reader poisoned")?
            .archive_object(digest)
            .map_err(|error| format!("formal replica business node {digest}: {error}"))?;
        let Some(bytes) = verify_dual_business_node(digest, primary, replica)? else {
            return Ok(None);
        };
        self.cache
            .lock()
            .map_err(|_| "formal business node cache poisoned")?
            .insert(digest.to_owned(), bytes.clone());
        Ok(Some(bytes))
    }
}

pub(super) fn verify_dual_business_path(
    root: &BusinessIndexRootV1,
    key: &str,
    primary: BusinessIndexPathV1,
    replica: BusinessIndexPathV1,
) -> Result<Vec<BusinessIndexNodeWriteV1>, String> {
    primary.verify(root, key)?;
    replica.verify(root, key)?;
    if primary.nodes != replica.nodes {
        return Err("formal business path differs between signed replicas".into());
    }
    Ok(primary.nodes)
}

pub(super) struct BoundedBusinessNodeCache {
    pub(super) entries: HashMap<String, Vec<u8>>,
    pub(super) insertion_order: VecDeque<String>,
    pub(super) bytes: usize,
    pub(super) max_bytes: usize,
    pub(super) max_entries: usize,
}

impl BoundedBusinessNodeCache {
    pub(super) fn new(max_bytes: usize, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            insertion_order: VecDeque::new(),
            bytes: 0,
            max_bytes,
            max_entries,
        }
    }

    pub(super) fn get(&self, digest: &str) -> Option<Vec<u8>> {
        self.entries.get(digest).cloned()
    }

    pub(super) fn insert(&mut self, digest: String, bytes: Vec<u8>) {
        if self.max_entries == 0
            || bytes.len() > self.max_bytes
            || self.entries.contains_key(&digest)
        {
            return;
        }
        while self.bytes.saturating_add(bytes.len()) > self.max_bytes
            || self.entries.len() >= self.max_entries
        {
            let Some(oldest) = self.insertion_order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(evicted.len());
            }
        }
        if bytes.len() <= self.max_bytes.saturating_sub(self.bytes) {
            self.bytes += bytes.len();
            self.insertion_order.push_back(digest.clone());
            self.entries.insert(digest, bytes);
        }
    }
}

pub(super) fn validate_business_node_digest(digest: &str) -> Result<(), String> {
    let decoded = hex::decode(digest).map_err(|_| "formal business node digest is not hex")?;
    if decoded.len() != 32 || hex::encode(decoded) != digest {
        return Err("formal business node digest is not canonical BLAKE3 hex".into());
    }
    Ok(())
}

pub(super) fn verify_dual_business_node(
    digest: &str,
    primary: Option<Vec<u8>>,
    replica: Option<Vec<u8>>,
) -> Result<Option<Vec<u8>>, String> {
    match (primary, replica) {
        (None, None) => Ok(None),
        (Some(bytes), Some(replica_bytes)) => {
            if bytes != replica_bytes {
                return Err(format!(
                    "formal business node {digest} differs between signed replicas"
                ));
            }
            if hex::encode(blake3::hash(&bytes).as_bytes()) != digest {
                return Err(format!(
                    "formal business node {digest} content hash mismatch"
                ));
            }
            Ok(Some(bytes))
        }
        _ => Err(format!(
            "formal business node {digest} is missing from only one signed replica"
        )),
    }
}

pub(crate) fn business_reader_for(
    validated: &ValidatedFormalLiveConfigV1,
) -> Result<Arc<RemoteBusinessIndexReader>, String> {
    let (primary, replica) = business_endpoints_for(
        validated,
        validated.active_authority(),
        validated.anchor.head().head(),
    )?;
    Ok(Arc::new(RemoteBusinessIndexReader::new(
        primary,
        replica,
        BUSINESS_NODE_CACHE_BYTES,
        BUSINESS_NODE_CACHE_ENTRIES,
    )))
}

fn business_endpoints_for(
    validated: &ValidatedFormalLiveConfigV1,
    authority: &VerifiedAuthorityV1,
    minimum: oasis7_distfs::controlled_authority::formal::FormalHeadRefV1,
) -> Result<(RemoteFormalEndpoint, RemoteFormalEndpoint), String> {
    let primary = RemoteFormalEndpoint::new(
        validated.config.primary,
        validated.timeout,
        authority.clone(),
        EndpointRole::Primary,
        validated.writer_key.clone(),
        minimum.clone(),
    )
    .map_err(|error| format!("formal primary business reader: {error}"))?;
    let replica = RemoteFormalEndpoint::new(
        validated.config.replica,
        validated.timeout,
        authority.clone(),
        EndpointRole::Replica,
        validated.writer_key.clone(),
        minimum,
    )
    .map_err(|error| format!("formal replica business reader: {error}"))?;
    Ok((primary, replica))
}
