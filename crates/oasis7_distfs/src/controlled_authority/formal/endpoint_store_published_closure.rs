//! Bounded fresh readback of a qualified candidate's complete typed closure.
use super::super::business_index::{
    BusinessIndexNodeKindV1, BusinessIndexNodeV1, BusinessIndexRootV1,
};
use super::super::business_material::FormalBusinessResultEnvelopeV1;
use super::super::coordinator::FormalPublishedClosureReceiptV1;
use super::*;
use crate::controlled_authority::replicated_protocol::{ArtifactRole, ClosedRecord};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
const MAX_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_NODES: usize = 262_144;

/// In-process progress is reusable only while this exact representation is
/// pinned. Reopening drops both this cursor and its pin and starts fresh.
pub(crate) struct ClosureJob {
    _pin: File,
    proof_digest: String,
    report: FormalPublishedClosureReceiptV1,
    seen: Seen,
    native: Vec<crate::controlled_authority::formal::native_environment::VerificationJob>,
    native_index: usize,
    roots: Vec<BusinessIndexRootV1>,
    work: Vec<(String, usize, [u8; 32])>,
    leaves: u64,
    root: usize,
    complete: bool,
    retiring_scratch: bool,
    started: bool,
}

// Diagnostic scratch is never a durable authority. The append-only hashes
// use hash identities; only the currently pinned job may consult them.
struct Seen {
    root: PathBuf,
    _lock: File,
    cleanup: Option<fs::ReadDir>,
    garbage: Option<fs::ReadDir>,
}
impl Seen {
    fn new(store: &Store) -> Result<Self, FormalError> {
        let parent = store.path.join("closure-scratch-v1");
        fs::create_dir_all(&parent).map_err(io)?;
        let meta = fs::symlink_metadata(&parent).map_err(io)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(invalid("published closure scratch directory type"));
        }
        #[cfg(unix)]
        {
            // SAFETY: geteuid has no caller preconditions.
            if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o022 != 0 {
                return Err(invalid("published closure scratch directory owner/mode"));
            }
        }
        Self::scavenge(&parent)?;
        let root = parent.join(super::super::fencing::fresh_challenge().map_err(io)?);
        fs::create_dir(&root).map_err(io)?;
        #[cfg(unix)]
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(io)?;
        let lock = Self::lock(&root, true)?.ok_or_else(|| invalid("new closure scratch locked"))?;
        fs::create_dir(root.join("fsync-v1")).map_err(io)?;
        Ok(Self {
            root,
            _lock: lock,
            cleanup: None,
            garbage: None,
        })
    }
    fn lock(root: &Path, create: bool) -> Result<Option<File>, FormalError> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(create);
        #[cfg(unix)]
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        let file = options.open(root.join("lock")).map_err(io)?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // SAFETY: file owns a live descriptor; flock does not retain pointers.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::WouldBlock {
                    return Ok(None);
                }
                return Err(io(error));
            }
        }
        Ok(Some(file))
    }
    fn scavenge(parent: &Path) -> Result<(), FormalError> {
        for item in fs::read_dir(parent).map_err(io)?.take(8) {
            Self::scavenge_one(item.map_err(io)?)?;
        }
        Ok(())
    }
    fn scavenge_page(&mut self) -> Result<(), FormalError> {
        if self.garbage.is_none() {
            self.garbage = Some(
                fs::read_dir(
                    self.root
                        .parent()
                        .ok_or_else(|| invalid("closure scratch parent"))?,
                )
                .map_err(io)?,
            );
        }
        for _ in 0..8 {
            let Some(item) = self.garbage.as_mut().and_then(Iterator::next) else {
                self.garbage = None;
                break;
            };
            Self::scavenge_one(item.map_err(io)?)?;
        }
        Ok(())
    }
    fn scavenge_one(item: fs::DirEntry) -> Result<(), FormalError> {
        let path = item.path();
        hash_hex(
            item.file_name()
                .to_str()
                .ok_or_else(|| invalid("closure scratch name"))?,
        )?;
        let meta = match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            value => value.map_err(io)?,
        };
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(invalid("closure scratch type"));
        }
        // An interruption before creating the lock can leave only an empty
        // private directory, which never contains reusable readback authority.
        if !path.join("lock").try_exists().map_err(io)? {
            if fs::read_dir(&path).map_err(io)?.next().is_none() {
                fs::remove_dir(&path).map_err(io)?;
                return Ok(());
            }
            return Err(invalid("closure scratch missing lock with markers"));
        }
        let Some(lock) = Self::lock(&path, false)? else {
            return Ok(());
        };
        let mut old = Self {
            root: path,
            _lock: lock,
            cleanup: None,
            garbage: None,
        };
        old.clean_page()?;
        Ok(())
    }
    fn clean_page(&mut self) -> Result<bool, FormalError> {
        if self.cleanup.is_none() {
            self.cleanup = Some(fs::read_dir(&self.root).map_err(io)?);
        }
        let entries = self
            .cleanup
            .as_mut()
            .ok_or_else(|| invalid("closure cleanup cursor"))?;
        let mut remaining = 256usize;
        while remaining > 0 {
            remaining -= 1;
            let Some(entry) = entries.next() else {
                self.cleanup = None;
                fs::remove_file(self.root.join("lock")).map_err(io)?;
                fs::remove_dir(&self.root).map_err(io)?;
                return Ok(true);
            };
            let entry = entry.map_err(io)?;
            let name = entry.file_name();
            if name == "lock" {
                continue;
            }
            if name == "fsync-v1" {
                if !Self::clean_sync_page(&entry.path(), &mut remaining)? {
                    self.cleanup = None;
                    return Ok(false);
                }
                continue;
            }
            hash_hex(
                name.to_str()
                    .ok_or_else(|| invalid("closure seen filename"))?,
            )?;
            let meta = fs::symlink_metadata(entry.path()).map_err(io)?;
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != 0 {
                return Err(invalid("closure seen marker type/length"));
            }
            fs::remove_file(entry.path()).map_err(io)?;
        }
        Ok(false)
    }
    fn clean_sync_page(dir: &Path, remaining: &mut usize) -> Result<bool, FormalError> {
        let meta = fs::symlink_metadata(dir).map_err(io)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(invalid("closure fsync scratch type"));
        }
        let mut entries = fs::read_dir(dir).map_err(io)?;
        while *remaining > 0 {
            let Some(entry) = entries.next() else {
                fs::remove_dir(dir).map_err(io)?;
                return Ok(true);
            };
            *remaining -= 1;
            let entry = entry.map_err(io)?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| invalid("closure sync filename"))?;
            if name.len() > 220
                || !name.starts_with("physical-")
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err(invalid("closure fsync marker identity"));
            }
            let meta = fs::symlink_metadata(entry.path()).map_err(io)?;
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != 0 {
                return Err(invalid("closure fsync marker type/length"));
            }
            fs::remove_file(entry.path()).map_err(io)?;
        }
        Ok(false)
    }
    fn insert(&mut self, id: &str) -> Result<bool, FormalError> {
        hash_hex(id)?;
        let path = self.root.join(id);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        match options.open(path) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let meta = fs::symlink_metadata(self.root.join(id)).map_err(io)?;
                if !meta.is_file() || meta.file_type().is_symlink() || meta.len() != 0 {
                    return Err(invalid("closure seen marker type/length"));
                }
                Ok(false)
            }
            Err(e) => Err(io(e)),
        }
    }
}
fn account_seen(
    id: &str,
    bytes: &[u8],
    seen: &mut Seen,
    report: &mut FormalPublishedClosureReceiptV1,
) -> Result<(), FormalError> {
    if !seen.insert(id)? {
        return Ok(());
    }
    report.logical_bytes = report
        .logical_bytes
        .checked_add(bytes.len() as u64)
        .ok_or_else(|| invalid("published closure byte overflow"))?;
    report.logical_objects = report
        .logical_objects
        .checked_add(1)
        .ok_or_else(|| invalid("published closure object overflow"))?;
    report.inventory_digest = hash(
        "oasis7.formal.published_closure.v1",
        &(&report.inventory_digest, id, bytes.len() as u64),
    )?;
    Ok(())
}

impl ClosureJob {
    pub(crate) fn new(
        store: &Store,
        candidate: &FormalCandidateMaterialV1,
        profile: u32,
        digest: &str,
        proof_digest: &str,
    ) -> Result<Self, FormalError> {
        let pin = super::super::physical::pin(&store.path.join("formal-objects-v1"), false)?
            .ok_or_else(|| invalid("published closure resumable representation pin unavailable"))?;
        let seen = Seen::new(store)?;
        let sync_markers = seen.root.join("fsync-v1");
        if !matches!(profile, 1 | 2) {
            return Err(invalid("published closure unsupported profile"));
        }
        verify_record_material(&candidate.candidate.record, &candidate.material)?;
        if profile == 2 {
            super::super::business_material::verify_committed_business_material(
                &candidate.candidate.record,
                &candidate.material,
                candidate.frozen_material.as_ref(),
                &RegisteredReader(store, Some(&sync_markers)),
            )?;
        }
        let mut raw = BTreeMap::new();
        let mut roots = Vec::new();
        collect(&candidate.material, profile, true, &mut raw, &mut roots)?;
        if let Some(frozen) = &candidate.frozen_material {
            verify_frozen_material(
                &candidate
                    .frozen
                    .as_ref()
                    .ok_or_else(|| invalid("published closure missing Frozen certificate"))?
                    .reservation
                    .body,
                frozen,
            )?;
            collect(frozen, profile, false, &mut raw, &mut roots)?;
        }
        let mut job = Self {
            _pin: pin,
            proof_digest: proof_digest.into(),
            report: FormalPublishedClosureReceiptV1 {
                candidate_digest: digest.into(),
                qualified_proof_digest: proof_digest.into(),
                inventory_digest: digest.into(),
                logical_objects: 0,
                logical_bytes: 0,
            },
            seen,
            native: std::iter::once(&candidate.material)
                .chain(candidate.frozen_material.iter())
                .map(|material| {
                    material
                        .native_environment()
                        .map_err(|e| invalid(e.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .map(crate::controlled_authority::formal::native_environment::VerificationJob::new)
                .collect(),
            native_index: 0,
            roots,
            work: Vec::new(),
            leaves: 0,
            root: 0,
            complete: false,
            retiring_scratch: false,
            started: false,
        };
        for (id, expected) in raw {
            let bytes = store.published_local_read_once(&id, &sync_markers)?;
            if bytes != expected {
                return Err(invalid("published closure exact artifact bytes differ"));
            }
            job.account(&id, &bytes)?;
        }
        Ok(job)
    }
    pub(crate) fn advance(
        &mut self,
        store: &Store,
        proof_digest: &str,
    ) -> Result<Option<FormalPublishedClosureReceiptV1>, FormalError> {
        let page_started = std::time::Instant::now();
        if self.proof_digest != proof_digest {
            return Err(invalid("published closure job proof changed"));
        }
        self.seen.scavenge_page()?;
        if self.complete {
            return Ok(Some(self.report.clone()));
        }
        if self.native_index < self.native.len() {
            let sync_markers = self.seen.root.join("fsync-v1");
            let job = &mut self.native[self.native_index];
            let seen = &mut self.seen;
            let report = &mut self.report;
            job.advance_with_visit(
                |id| store.published_local_read_once(id, &sync_markers),
                |id, bytes| account_seen(id, bytes, seen, report),
            )?;
            if job.complete() {
                self.native_index += 1;
            }
            return Ok(None);
        }
        let empty = super::super::business_index::empty_subtree_hashes().map_err(invalid)?;
        let mut nodes = 0usize;
        let mut read_bytes = 0usize;
        loop {
            if self.root == self.roots.len() {
                self.retiring_scratch = true;
                // Scratch reclamation is also paged. Dropping an interrupted
                // job releases its lock; later jobs scavenge bounded pages.
                if !self.seen.clean_page()? {
                    return Ok(None);
                }
                sync(&store.path)?;
                self.complete = true;
                return Ok(Some(self.report.clone()));
            }
            if !self.started {
                self.started = true;
                self.roots[self.root].validate().map_err(invalid)?;
                self.work
                    .push((self.roots[self.root].node_root.clone(), 0, [0; 32]));
            }
            let Some((id, depth, prefix)) = self.work.pop() else {
                if self.leaves != self.roots[self.root].leaf_count {
                    return Err(invalid("published closure leaf count"));
                }
                self.root += 1;
                self.leaves = 0;
                self.started = false;
                continue;
            };
            if depth > 256 {
                return Err(invalid("published closure index depth"));
            }
            if id == empty[depth] {
                continue;
            }
            // A nonempty shared subtree at distinct paths cannot validate:
            // every leaf is bound to all 256 path bits below. Depth bounds
            // guarantee termination without an unbounded in-memory edge set.
            let bytes = store.published_local_read_once(&id, &self.seen.root.join("fsync-v1"))?;
            read_bytes += bytes.len();
            nodes += 1;
            self.account(&id, &bytes)?;
            match BusinessIndexNodeV1::decode(&bytes).map_err(invalid)?.kind {
                BusinessIndexNodeKindV1::Branch {
                    depth: actual,
                    left_hash,
                    right_hash,
                } if actual as usize == depth && depth < 256 => {
                    let mut right = prefix;
                    right[depth / 8] |= 1 << (7 - depth % 8);
                    self.work.push((right_hash, depth + 1, right));
                    self.work.push((left_hash, depth + 1, prefix));
                }
                BusinessIndexNodeKindV1::Leaf(leaf) if depth == 256 => {
                    if leaf.key_digest != hex::encode(prefix) {
                        return Err(invalid("published closure leaf key/path binding"));
                    }
                    self.leaves += 1;
                }
                _ => return Err(invalid("published closure sparse node type/depth")),
            }
            // Preserve the design caps, and yield after a complete bounded
            // object when this page has used its local work time slice.
            if nodes >= 256
                || read_bytes >= 8 * 1024 * 1024
                || page_started.elapsed() >= std::time::Duration::from_secs(1)
            {
                #[cfg(test)]
                if std::env::var_os("OASIS7_FORMAL_CLOSURE_DIAGNOSTICS").is_some() {
                    eprintln!(
                        "published_closure_page nodes={nodes} bytes={read_bytes} elapsed_us={}",
                        page_started.elapsed().as_micros()
                    );
                }
                return Ok(None);
            }
        }
    }
    fn account(&mut self, id: &str, bytes: &[u8]) -> Result<(), FormalError> {
        account_seen(id, bytes, &mut self.seen, &mut self.report)
    }
    pub(crate) fn sync_markers(&self) -> Option<PathBuf> {
        // Cleanup may already have removed these markers. The physical pin
        // remains live, but completed/cleaning jobs use ordinary durable reads.
        (!self.retiring_scratch).then(|| self.seen.root.join("fsync-v1"))
    }
    #[cfg(test)]
    pub(crate) fn append_test_root(&mut self, root: BusinessIndexRootV1) {
        self.roots.push(root);
    }
    pub(crate) fn complete(&self) -> bool {
        self.complete
    }
    pub(crate) fn persist_diagnostic(&self, store: &Store) -> Result<(), FormalError> {
        // This checkpoint deliberately contains no authority to skip a read.
        // Kernel pin continuity cannot survive process death; reopening always
        // discards this cursor and rereads from the original exact candidate.
        let checkpoint = serde_cbor::to_vec(&(
            "oasis7.formal.published-closure-progress.v1",
            &self.report.candidate_digest,
            &self.proof_digest,
            self.root as u64,
            self.report.logical_objects,
            self.report.logical_bytes,
            self.complete,
            "restart-requires-fresh-readback",
        ))
        .map_err(|error| invalid(error.to_string()))?;
        Store::atomic(
            &store.path,
            &store.path.join("published-closure-progress-v1.cbor"),
            &checkpoint,
        )
    }
}

impl Store {
    #[cfg(test)]
    pub(crate) fn verify_published_raw_aliases(
        &self,
        candidate: &FormalCandidateMaterialV1,
    ) -> Result<(), FormalError> {
        self.verify_published_raw_aliases_with_sync(candidate, None)
    }
    pub(crate) fn verify_published_raw_aliases_with_sync(
        &self,
        candidate: &FormalCandidateMaterialV1,
        sync_markers: Option<&Path>,
    ) -> Result<(), FormalError> {
        for material in std::iter::once(&candidate.material).chain(candidate.frozen_material.iter())
        {
            for object in &material.objects {
                let bytes = match sync_markers {
                    Some(markers) => {
                        self.published_local_read_once(&object.content_hash, markers)?
                    }
                    None => self.published_local_read(&object.content_hash)?,
                };
                if bytes != object.bytes {
                    return Err(invalid("published closure exact artifact bytes differ"));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn read_registered_local_request_with_sync(
        &self,
        request: &str,
        sync_markers: Option<&Path>,
    ) -> Result<FormalRequestStateV1, FormalError> {
        hash_hex(request)?;
        let mut load = |id: &str| match sync_markers {
            Some(markers) => self.published_local_read_once(id, markers),
            None => self.published_local_read(id),
        };
        let id = Self::lookup_from_root(self.body.index.as_deref(), request, &mut load)?
            .ok_or_else(|| invalid("published local request index unavailable"))?;
        let state: FormalRequestStateV1 = decode(&load(&id)?)?;
        if state.request_digest() != request {
            return Err(invalid("published local request identity"));
        }
        Ok(state)
    }
    pub(super) fn verified_candidate_objects(
        &self,
        candidate: &FormalCandidateMaterialV1,
        profile: u32,
    ) -> Result<BTreeSet<String>, FormalError> {
        self.verify_candidate_closure(candidate, profile, &candidate.material.payload_digest)
            .map(|(_, objects)| objects)
    }
    fn verify_candidate_closure(
        &self,
        candidate: &FormalCandidateMaterialV1,
        profile: u32,
        candidate_digest: &str,
    ) -> Result<(FormalPublishedClosureReceiptV1, BTreeSet<String>), FormalError> {
        if !matches!(profile, 1 | 2) {
            return Err(invalid("published closure unsupported profile"));
        }
        verify_record_material(&candidate.candidate.record, &candidate.material)?;
        if profile == 2 {
            super::super::business_material::verify_committed_business_material(
                &candidate.candidate.record,
                &candidate.material,
                candidate.frozen_material.as_ref(),
                &RegisteredReader(self, None),
            )?;
        }

        let mut raw = BTreeMap::new();
        let mut roots = Vec::new();
        collect(&candidate.material, profile, true, &mut raw, &mut roots)?;
        if let Some(frozen) = &candidate.frozen_material {
            verify_frozen_material(
                &candidate
                    .frozen
                    .as_ref()
                    .ok_or_else(|| invalid("published closure missing Frozen certificate"))?
                    .reservation
                    .body,
                frozen,
            )?;
            collect(frozen, profile, false, &mut raw, &mut roots)?;
        }
        for material in std::iter::once(&candidate.material).chain(candidate.frozen_material.iter())
        {
            crate::controlled_authority::formal::native_environment::verify_material_chunks(
                material,
                |id| self.published_local_read(id),
                |id, bytes| {
                    raw.insert(id.into(), bytes.to_vec());
                    Ok(())
                },
            )?;
        }
        self.verify_collected_closure(raw, roots, candidate_digest, true)
    }
    fn verify_collected_closure(
        &self,
        raw: BTreeMap<String, Vec<u8>>,
        roots: Vec<BusinessIndexRootV1>,
        candidate_digest: &str,
        require_raw_aliases: bool,
    ) -> Result<(FormalPublishedClosureReceiptV1, BTreeSet<String>), FormalError> {
        let mut report = FormalPublishedClosureReceiptV1 {
            candidate_digest: candidate_digest.into(),
            qualified_proof_digest: String::new(),
            inventory_digest: candidate_digest.into(),
            logical_objects: 0,
            logical_bytes: 0,
        };
        let mut objects = BTreeSet::new();
        for (id, expected) in raw {
            // Frozen's signed state object itself durably embeds all raw
            // artifacts; freeze does not promise separate raw aliases yet.
            let bytes = if require_raw_aliases {
                self.published_local_read(&id)?
            } else {
                expected.clone()
            };
            if bytes != expected {
                return Err(invalid("published closure exact artifact bytes differ"));
            }
            account(&id, &bytes, &mut objects, &mut report)?;
        }
        let empty = super::super::business_index::empty_subtree_hashes().map_err(invalid)?;
        for root in roots {
            root.validate().map_err(invalid)?;
            let mut edges = BTreeSet::new();
            let mut work = vec![(root.node_root.clone(), 0usize, [0u8; 32])];
            let mut leaves = 0u64;
            while let Some((id, depth, prefix)) = work.pop() {
                if depth > 256 {
                    return Err(invalid("published closure index depth"));
                }
                if id == empty[depth] {
                    continue;
                }
                if !edges.insert((id.clone(), depth)) {
                    return Err(invalid("published closure repeated nonempty sparse edge"));
                }
                if edges.len() > MAX_NODES {
                    return Err(invalid("published closure node budget"));
                }
                let bytes = self.published_local_read(&id)?;
                account(&id, &bytes, &mut objects, &mut report)?;
                match BusinessIndexNodeV1::decode(&bytes).map_err(invalid)?.kind {
                    BusinessIndexNodeKindV1::Branch {
                        depth: actual,
                        left_hash,
                        right_hash,
                    } if actual as usize == depth => {
                        if depth >= 256 {
                            return Err(invalid("published closure branch beyond key"));
                        }
                        let mut right_prefix = prefix;
                        right_prefix[depth / 8] |= 1 << (7 - depth % 8);
                        work.push((right_hash, depth + 1, right_prefix));
                        work.push((left_hash, depth + 1, prefix));
                    }
                    BusinessIndexNodeKindV1::Leaf(leaf) if depth == 256 => {
                        // Values are inline; the complete key must equal this
                        // edge's exact 256 branch decisions.
                        if leaf.key_digest != hex::encode(prefix) {
                            return Err(invalid("published closure leaf key/path binding"));
                        }
                        leaves += 1;
                    }
                    _ => return Err(invalid("published closure sparse node type/depth")),
                }
            }
            if leaves != root.leaf_count {
                return Err(invalid("published closure leaf count"));
            }
        }
        // All puts/checkpoints are already fsynced; finish by syncing the active
        // locator directory while holding this endpoint's exclusive lease.
        sync(&self.path)?;
        Ok((report, objects))
    }
    pub(super) fn verify_pinned_frozen_closure(
        &self,
        material: &ClosedRecord,
        profile: u32,
    ) -> Result<BTreeSet<String>, FormalError> {
        let mut raw = BTreeMap::new();
        let mut roots = Vec::new();
        collect(material, profile, false, &mut raw, &mut roots)?;
        let mut external = BTreeSet::new();
        crate::controlled_authority::formal::native_environment::verify_material_chunks(
            material,
            |id| self.published_local_read(id),
            |id, _| {
                external.insert(id.into());
                Ok(())
            },
        )?;
        self.verify_collected_closure(raw, roots, &material.payload_digest, false)
            .map(|(_, mut objects)| {
                objects.extend(external);
                objects
            })
    }
    fn published_local_read_once(
        &self,
        id: &str,
        sync_markers: &Path,
    ) -> Result<Vec<u8>, FormalError> {
        hash_hex(id)?;
        // These markers are owned by the live kernel-pinned ClosureJob. They
        // amortize fsync only; every access still reads and hashes current CAS
        // bytes and all required frames. Restart never reuses this directory.
        let bytes = crate::controlled_authority::formal::business_pack::read_object_durable_once(
            &self.path.join("formal-objects-v1").join(id),
            id,
            sync_markers,
        )?
        .ok_or_else(|| invalid("published local closure object missing"))?;
        if bytes.len() > LIMIT || blake3::hash(&bytes).to_hex().as_str() != id {
            return Err(invalid("published local closure object checksum/bound"));
        }
        Ok(bytes)
    }
    pub(super) fn published_local_read(&self, id: &str) -> Result<Vec<u8>, FormalError> {
        hash_hex(id)?;
        // raw_object reads registered local storage, never archive sources.
        let bytes = super::super::business_pack::read_object_durable(
            &self.path.join("formal-objects-v1").join(id),
            id,
        )?
        .ok_or_else(|| invalid("published local closure object missing"))?;
        if bytes.len() > LIMIT || blake3::hash(&bytes).to_hex().as_str() != id {
            return Err(invalid("published local closure object checksum/bound"));
        }
        Ok(bytes)
    }
}
struct RegisteredReader<'a>(&'a Store, Option<&'a Path>);
impl super::super::business_index::BusinessIndexNodeReader for RegisteredReader<'_> {
    fn read_node(&self, id: &str) -> Result<Option<Vec<u8>>, String> {
        match self.1 {
            Some(markers) => self.0.published_local_read_once(id, markers),
            None => self.0.published_local_read(id),
        }
        .map(Some)
        .map_err(|error| error.to_string())
    }
}
fn account(
    id: &str,
    bytes: &[u8],
    objects: &mut BTreeSet<String>,
    report: &mut FormalPublishedClosureReceiptV1,
) -> Result<(), FormalError> {
    if !objects.insert(id.into()) {
        return Ok(());
    }
    if objects.len() > MAX_NODES {
        return Err(invalid("published closure object budget"));
    }
    report.logical_bytes = report
        .logical_bytes
        .checked_add(bytes.len() as u64)
        .ok_or_else(|| invalid("published closure byte overflow"))?;
    if report.logical_bytes > MAX_BYTES {
        return Err(invalid("published closure byte budget"));
    }
    report.logical_objects += 1;
    report.inventory_digest = hash(
        "oasis7.formal.published_closure.v1",
        &(&report.inventory_digest, id, bytes.len() as u64),
    )?;
    Ok(())
}
fn collect(
    material: &ClosedRecord,
    profile: u32,
    candidate: bool,
    raw: &mut BTreeMap<String, Vec<u8>>,
    roots: &mut Vec<BusinessIndexRootV1>,
) -> Result<(), FormalError> {
    for object in &material.objects {
        raw.insert(object.content_hash.clone(), object.bytes.clone());
    }
    if profile == 1 {
        return Ok(());
    }
    let artifact = |role| {
        material
            .objects
            .iter()
            .find(|o| material.roots.get(&role) == Some(&o.content_hash))
            .ok_or_else(|| invalid("published closure artifact absent"))
    };
    if candidate {
        let envelope =
            FormalBusinessResultEnvelopeV1::decode(&artifact(ArtifactRole::Result)?.bytes)?;
        for delta in std::iter::once(&envelope.delta).chain(envelope.retained_history_delta.iter())
        {
            roots.push(delta.new.clone());
            for node in &delta.node_writes {
                raw.insert(node.digest.clone(), node.bytes.clone());
            }
        }
    }
    #[derive(Deserialize)]
    struct Snapshot {
        state: State,
    }
    #[derive(Deserialize)]
    struct State {
        retained_history_root: Option<Root>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Root {
        schema_version: u32,
        index_root: BusinessIndexRootV1,
        root_digest: String,
    }
    let snapshot: Snapshot = decode(
        &material
            .snapshot_bytes()
            .map_err(|e| invalid(e.to_string()))?,
    )?;
    if let Some(root) = snapshot.state.retained_history_root {
        let value = serde_cbor::value::to_value((
            "oasis7.formal.retained-history.root.v1",
            root.schema_version,
            &root.index_root,
        ))
        .map_err(io)?;
        let bytes = serde_cbor::to_vec(&value).map_err(io)?;
        if root.schema_version != 1 || blake3::hash(&bytes).to_hex().as_str() != root.root_digest {
            return Err(invalid(
                "published retained-history descriptor schema/digest",
            ));
        }
        root.index_root.validate().map_err(invalid)?;
        roots.push(root.index_root);
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod scratch_tests {
    use super::*;
    #[test]
    fn closure_disk_seen_reclaims_in_pages_and_never_scavenges_live_jobs() {
        let parent = std::env::temp_dir().join(format!(
            "oasis7-closure-seen-{}",
            crate::controlled_authority::fencing::fresh_challenge().unwrap()
        ));
        fs::create_dir(&parent).unwrap();
        let root = parent.join(blake3::hash(b"job").to_hex().as_str());
        fs::create_dir(&root).unwrap();
        let lock = Seen::lock(&root, true).unwrap().unwrap();
        let mut seen = Seen {
            root: root.clone(),
            _lock: lock,
            cleanup: None,
            garbage: None,
        };
        for index in 0u32..300 {
            let id = blake3::hash(&index.to_be_bytes()).to_hex().to_string();
            assert!(seen.insert(&id).unwrap());
            assert!(!seen.insert(&id).unwrap());
        }
        let sync = root.join("fsync-v1");
        fs::create_dir(&sync).unwrap();
        for index in 0..20 {
            fs::write(sync.join(format!("physical-1-{index}")), b"").unwrap();
        }
        Seen::scavenge(&parent).unwrap();
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            302,
            "an active kernel-locked job cannot lose its seen set"
        );
        assert!(!seen.clean_page().unwrap());
        assert!(root.exists());
        assert!(fs::read_dir(&root).unwrap().count() >= 45);
        drop(seen);
        Seen::scavenge(&parent).unwrap();
        assert!(
            !root.exists(),
            "interrupted scratch has no authority and is reclaimed"
        );
        fs::remove_dir(parent).unwrap();
    }
}
