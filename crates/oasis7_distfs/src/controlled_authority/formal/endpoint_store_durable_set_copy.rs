//! Full authenticated closure walk for healthy candidate staging. The operation
//! budget limits one attempted copy; no total historical object/record cap.
//! This child of Store can read the exact native index/history encodings.
use super::*;
use crate::controlled_authority::formal::business_index::{
    BUSINESS_INDEX_DEPTH, BusinessIndexNodeKindV1, BusinessIndexNodeV1, BusinessIndexRootV1,
    empty_subtree_hashes,
};
#[path = "endpoint_store_durable_set_copy_memo.rs"]
mod copy_memo;
use copy_memo::CopyReadMemo;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DurableSetClosureInventoryV1 {
    pub source_checkpoint_digest: String,
    pub inventory_digest: String,
    pub logical_objects: u64,
    pub logical_bytes: u64,
}
#[derive(Clone)]
enum Visit {
    Index(String, Option<u16>),
    History(String, FormalHeadRefV1),
    Entry(String, Option<FormalHeadRefV1>),
    Business(String, usize),
    Raw(String, Vec<u8>),
    Native(String, u32),
}
impl Visit {
    fn id(&self) -> &str {
        match self {
            Self::Index(id, _)
            | Self::History(id, _)
            | Self::Entry(id, _)
            | Self::Business(id, _)
            | Self::Raw(id, _)
            | Self::Native(id, _) => id,
        }
    }
    fn edge_digest(&self) -> Result<String, FormalError> {
        let context = match self {
            Self::Index(_, previous) => serde_cbor::to_vec(previous),
            Self::History(_, head) => serde_cbor::to_vec(head),
            Self::Entry(_, expected) => serde_cbor::to_vec(expected),
            Self::Business(_, depth) => serde_cbor::to_vec(depth),
            Self::Raw(..) => serde_cbor::to_vec(&()),
            Self::Native(_, size) => serde_cbor::to_vec(size),
        }
        .map_err(|error| invalid(error.to_string()))?;
        hash(
            "oasis7.formal.durable_set_copy_edge.v1",
            &(self.tag(), self.id(), context),
        )
    }
    fn tag(&self) -> u8 {
        match self {
            Self::Index(..) => 0,
            Self::History(..) => 1,
            Self::Entry(..) => 2,
            Self::Business(..) => 3,
            Self::Raw(..) => 4,
            Self::Native(..) => 5,
        }
    }
}
// Streaming publication preserves logical ids while batching bounded native
// packs. No total-history object cache: each flushed batch releases its bytes.
const COPY_PACK_BYTES: usize = 4 * 1024 * 1024;
const COPY_PACK_OBJECTS: usize = 256;
#[derive(Default)]
struct CopyPackBatch {
    objects: std::collections::BTreeMap<String, Vec<u8>>,
    bytes: usize,
}
impl CopyPackBatch {
    fn flush(&mut self, directory: &Path) -> Result<(), FormalError> {
        if self.objects.is_empty() {
            return Ok(());
        }
        let writes = std::mem::take(&mut self.objects)
            .into_iter()
            .map(|(digest, bytes)| {
                crate::controlled_authority::formal::business_index::BusinessIndexNodeWriteV1 {
                    digest,
                    bytes,
                }
            })
            .collect::<Vec<_>>();
        crate::controlled_authority::formal::business_pack::persist(directory, &writes)?;
        self.bytes = 0;
        Ok(())
    }
    fn push(&mut self, directory: &Path, id: &str, bytes: &[u8]) -> Result<(), FormalError> {
        if bytes.is_empty()
            || bytes.len() as u64
                > crate::controlled_authority::formal::business_pack::MAX_PACK_NODE
        {
            self.flush(directory)?;
            publish_raw_copy_object(directory, id, bytes)?;
            return Ok(());
        }
        if let Some(existing) = self.objects.get(id) {
            if existing != bytes {
                return Err(invalid("healthy copy immutable batch object changed"));
            }
            return Ok(());
        }
        if !self.objects.is_empty()
            && (self.objects.len() >= COPY_PACK_OBJECTS
                || self.bytes + bytes.len() > COPY_PACK_BYTES)
        {
            self.flush(directory)?;
        }
        self.bytes += bytes.len();
        self.objects.insert(id.into(), bytes.to_vec());
        if self.bytes >= COPY_PACK_BYTES {
            self.flush(directory)?;
        }
        Ok(())
    }
}
// Native index/history/entry codecs still consume raw files. Only Business
// nodes use the existing physical-pack contract; do not broaden native codecs.
fn publish_raw_copy_object(directory: &Path, id: &str, bytes: &[u8]) -> Result<(), FormalError> {
    let path = directory.join(id);
    if let Some(existing) =
        crate::controlled_authority::formal::business_pack::read_object(&path, id)?
    {
        if existing != bytes {
            return Err(invalid("healthy copy immutable local object changed"));
        }
        crate::controlled_authority::formal::business_pack::read_object_durable(&path, id)?
            .ok_or_else(|| invalid("healthy copy existing logical locator missing"))?;
    } else {
        Store::atomic(directory, &path, bytes)?;
    }
    Ok(())
}
fn is_empty_business_edge(
    table: &[String; BUSINESS_INDEX_DEPTH + 1],
    id: &str,
    depth: usize,
) -> Result<bool, FormalError> {
    let empty = table
        .get(depth)
        .ok_or_else(|| invalid("business index subtree depth out of bounds"))?;
    Ok(id == empty)
}
fn check_index_depth(bit: u16, previous: Option<u16>) -> Result<(), FormalError> {
    if bit >= 256 || previous.is_some_and(|p| bit <= p) {
        return Err(invalid("healthy copy index depth"));
    }
    Ok(())
}
fn check_history_head(node: &HistoryNode, expected: &FormalHeadRefV1) -> Result<(), FormalError> {
    if &node.head != expected || node.ancestors.len() > 64 {
        return Err(invalid("healthy copy history identity/skip width"));
    }
    Ok(())
}
// Only the current private run creates these compact projections. They cache
// completed intrinsic validation, never authority or contextual edge admission.
fn check_entry_head(
    head: &FormalHeadRefV1,
    expected: &Option<FormalHeadRefV1>,
) -> Result<(), FormalError> {
    if expected.as_ref().is_some_and(|expected| expected != head) {
        return Err(invalid("healthy copy qualified material/history binding"));
    }
    Ok(())
}
fn read_entry_projection(path: &Path) -> Result<Option<FormalHeadRefV1>, FormalError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(error)),
    };
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_file() || metadata.len() > 256 {
        return Err(invalid("healthy copy private entry projection bound"));
    }
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut bytes).map_err(io)?;
    if bytes.len() > 256 {
        return Err(invalid("healthy copy private entry projection bound"));
    }
    Ok(Some(decode(&bytes)?))
}
fn write_private_marker(path: &Path, bytes: &[u8]) -> Result<(), FormalError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    std::io::Write::write_all(&mut options.open(path).map_err(io)?, bytes).map_err(io)
}
#[cfg(test)]
thread_local! {
    static ENTRY_VALIDATIONS: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}
#[cfg(test)]
pub(crate) fn entry_validation_counts() -> (usize, usize) {
    ENTRY_VALIDATIONS.with(std::cell::Cell::get)
}
#[derive(Default)]
struct WalkTrace {
    enabled: bool,
    started: Option<std::time::Instant>,
    entry_decode_us: u128,
    material_us: u128,
    enqueue_us: u128,
    lookup_us: u128,
    directory_sync_us: u128,
    empty_hash_us: u128,
    empty_hash_calls: u64,
    empty_hash_builds: u64,
    marker_us: u128,
    read_us: u128,
    setup_us: u128,
    entries: u64,
    entry_bytes: u64,
    reused: u64,
    lookups: u64,
}
impl WalkTrace {
    fn new() -> Self {
        let enabled = std::env::var_os("OASIS7_HEALTHY_NATIVE_TRACE").is_some_and(|v| v == "1");
        crate::controlled_authority::formal::business_pack::begin_readback_trace(enabled);
        Self {
            enabled,
            started: enabled.then(std::time::Instant::now),
            ..Self::default()
        }
    }
    fn start(&self) -> Option<std::time::Instant> {
        self.enabled.then(std::time::Instant::now)
    }
}
fn elapsed_us(started: Option<std::time::Instant>) -> u128 {
    started
        .map(|started| started.elapsed().as_micros())
        .unwrap_or(0)
}
impl Drop for WalkTrace {
    fn drop(&mut self) {
        if self.enabled {
            eprintln!(
                "healthy-native-walk total_us={} entries={} entry_bytes={} reused={} entry_decode_us={} material_us={} enqueue_us={} lookups={} lookup_us={} directory_sync_us={} empty_hash_us={} empty_hash_calls={} empty_hash_builds={} marker_us={} read_us={} setup_us={}",
                elapsed_us(self.started),
                self.entries,
                self.entry_bytes,
                self.reused,
                self.entry_decode_us,
                self.material_us,
                self.enqueue_us,
                self.lookups,
                self.lookup_us,
                self.directory_sync_us,
                self.empty_hash_us,
                self.empty_hash_calls,
                self.empty_hash_builds,
                self.marker_us,
                self.read_us,
                self.setup_us
            );
            crate::controlled_authority::formal::business_pack::finish_readback_trace();
        }
    }
}
/// Caller must own the exclusive endpoint lock. A fresh private visitation
/// directory is used for THIS verification run; stale marker files never prove
/// completeness. The trusted installation provider independently repeats this
/// walk from signed source roots before it attests an inventory.
pub(crate) fn copy_authenticated_closure(
    target: &Path,
    source_checkpoint_bytes: &[u8],
    source_key: &str,
    source: &dyn FormalArchiveSourceV1,
    profile_version: u32,
    max_operation_bytes: u64,
    copy: bool,
) -> Result<DurableSetClosureInventoryV1, FormalError> {
    let mut trace = WalkTrace::new();
    #[cfg(test)]
    ENTRY_VALIDATIONS.with(|count| count.set((0, 0)));
    let body = Store::verify_public(source_checkpoint_bytes, source_key)?;
    let seal = body
        .seal
        .as_ref()
        .ok_or_else(|| invalid("healthy copy requires sealed native roots"))?;
    if seal.tail != fencing::SealedTailV1::empty()
        || body.pending.is_some()
        || body.decision_head.is_some()
        || body.head.position != seal.highest_complete.position
        || body.head.record_id != seal.highest_complete.decision_hash
    {
        return Err(invalid("healthy copy sealed exact H and empty slot"));
    }
    let objects = target.join("formal-objects-v1");
    directory(&objects)?;
    FileFormalArchiveSourceV1::new(target.to_path_buf())?;
    let staging = target.join(".formal-durable-set-staging");
    directory(&staging)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(&staging).map_err(io)?;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(invalid("healthy copy staging private owner/mode"));
        }
    }
    // Caller holds the exclusive endpoint lock. Only dedicated verification
    // runs are removed; digest CAS links and signed metadata are never deleted.
    for entry in fs::read_dir(&staging).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| invalid("healthy copy staging name"))?;
        let Some(id) = name.strip_prefix("run-") else {
            return Err(invalid("healthy copy foreign staging entry"));
        };
        hash_hex(id)?;
        if !fs::symlink_metadata(entry.path()).map_err(io)?.is_dir() {
            return Err(invalid("healthy copy staging symlink/non-directory"));
        }
        fs::remove_dir_all(entry.path()).map_err(io)?;
    }
    sync(&staging)?;
    let visits = staging.join(format!(
        "run-{}",
        fencing::fresh_challenge().map_err(fencing_error)?
    ));
    // No reuse on name collision, including a stale directory or symlink.
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(&visits).map_err(io)?;
    struct Cleanup(PathBuf, bool);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let started = self.1.then(std::time::Instant::now);
            let _ = fs::remove_dir_all(&self.0);
            if self.1 {
                eprintln!("healthy-native-cleanup cleanup_us={}", elapsed_us(started));
            }
        }
    }
    let _cleanup = Cleanup(visits.clone(), trace.enabled);
    let mut work = vec![
        Visit::Index(
            body.index
                .clone()
                .ok_or_else(|| invalid("healthy copy missing index"))?,
            None,
        ),
        Visit::History(
            body.head_node
                .clone()
                .or(body.imported_head_node.clone())
                .ok_or_else(|| invalid("healthy copy missing history"))?,
            body.head.clone(),
        ),
    ];
    let mut inventory = DurableSetClosureInventoryV1 {
        source_checkpoint_digest: blake3::hash(source_checkpoint_bytes).to_hex().to_string(),
        inventory_digest: hash("oasis7.formal.durable_set_inventory_start.v1", &body.head)?,
        logical_objects: 0,
        logical_bytes: 0,
    };
    let mut copy_pack = CopyPackBatch::default();
    // The lifetime is exactly this signed-root walk. The independent provider
    // must still read its registered local storage, never these copied bytes.
    let mut source_memo = copy.then(|| CopyReadMemo::new(source, trace.enabled));
    let mut empty_hash_table: Option<[String; BUSINESS_INDEX_DEPTH + 1]> = None;
    trace.setup_us = elapsed_us(trace.started);
    while let Some(visit) = work.pop() {
        hash_hex(visit.id())?;
        if let Visit::Business(id, depth) = &visit {
            let started = trace.start();
            if *depth > BUSINESS_INDEX_DEPTH {
                return Err(invalid("business index subtree depth out of bounds"));
            }
            if empty_hash_table.is_none() {
                empty_hash_table = Some(empty_subtree_hashes().map_err(invalid)?);
                trace.empty_hash_builds += 1;
            }
            let table = empty_hash_table
                .as_ref()
                .ok_or_else(|| invalid("healthy copy missing empty hash table"))?;
            let is_empty = is_empty_business_edge(table, id, *depth)?;
            trace.empty_hash_us += elapsed_us(started);
            trace.empty_hash_calls += 1;
            if is_empty {
                continue;
            }
        }
        let marker_started = trace.start();
        // Contextual validation cannot be skipped merely because the same
        // logical bytes appeared under a different ancestry/depth edge.
        let marker = visits.join(format!("edge-{}", visit.edge_digest()?));
        let object_marker = visits.join(format!("object-{}", visit.id()));
        let first_object = !object_marker.try_exists().map_err(io)?;
        if fs::symlink_metadata(&marker).is_ok() {
            continue;
        }
        let projection_path = visits.join(format!("entry-{}", visit.id()));
        if let Visit::Entry(_, expected) = &visit
            && let Some(head) = read_entry_projection(&projection_path)?
        {
            #[cfg(test)]
            ENTRY_VALIDATIONS.with(|count| {
                let (validated, reused) = count.get();
                count.set((validated, reused + 1));
            });
            trace.reused += 1;
            // A second edge may bind the same entry to a different head. Always
            // validate that context even though its intrinsic bytes are cached.
            check_entry_head(&head, expected)?;
            write_private_marker(&marker, &[])?;
            continue;
        }
        trace.marker_us += elapsed_us(marker_started);
        let read_started = trace.start();
        let bytes = if copy {
            match &visit {
                Visit::Raw(_, bytes) => bytes.clone(),
                _ => source_memo
                    .as_mut()
                    .ok_or_else(|| invalid("healthy copy source memo absent"))?
                    .read_object(visit.id())?
                    .ok_or_else(|| invalid("healthy copy referenced object missing"))?,
            }
        } else {
            // Independent provider reads its registered target directly, never
            // a caller's source adapter or inline material as storage evidence.
            let bytes =
                crate::controlled_authority::formal::business_pack::read_object_durable_once(
                    &objects.join(visit.id()),
                    visit.id(),
                    &visits,
                )?
                .ok_or_else(|| invalid("healthy independent local readback missing object"))?;
            if let Visit::Raw(_, expected) = &visit
                && expected != &bytes
            {
                return Err(invalid(
                    "healthy independent material differs from local object",
                ));
            }
            bytes
        };
        trace.read_us += elapsed_us(read_started);
        // Registered-local reads already verified the exact logical hash on the
        // no-follow descriptor; remote/inline copy bytes still need this check.
        if bytes.len() > LIMIT || (copy && blake3::hash(&bytes).to_hex().as_str() != visit.id()) {
            return Err(invalid("healthy independent copy object checksum/bound"));
        }
        if first_object {
            inventory.logical_bytes = inventory
                .logical_bytes
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| invalid("healthy copy byte overflow"))?;
            if inventory.logical_bytes > max_operation_bytes {
                return Err(invalid(
                    "healthy copy explicit operation byte budget exhausted",
                ));
            }
            inventory.logical_objects = inventory
                .logical_objects
                .checked_add(1)
                .ok_or_else(|| invalid("healthy copy count overflow"))?;
        }
        match &visit {
            Visit::Index(_, previous) => match decode::<Index>(&bytes)? {
                Index::Fork { bit, left, right } => {
                    check_index_depth(bit, *previous)?;
                    work.push(Visit::Index(left, Some(bit)));
                    work.push(Visit::Index(right, Some(bit)));
                }
                Index::Leaf { request, entry } => {
                    hash_hex(&request)?;
                    work.push(Visit::Entry(entry, None));
                }
            },
            Visit::History(_, expected) => {
                let node: HistoryNode = decode(&bytes)?;
                check_history_head(&node, expected)?;
                let mut load = |id: &str| {
                    let bytes = match &mut source_memo {
                        Some(memo) => memo.read_object(id)?,
                        None => source.read_object(id)?,
                    };
                    bytes.ok_or_else(|| {
                        invalid("healthy copy history/index referenced object missing")
                    })
                };
                let started = trace.start();
                let indexed =
                    Store::lookup_from_root(body.index.as_deref(), &node.request, &mut load)?;
                trace.lookup_us += elapsed_us(started);
                trace.lookups += 1;
                if indexed != Some(node.entry.clone()) {
                    return Err(invalid(
                        "healthy copy permanent index omitted or changed a canonical history request",
                    ));
                }
                work.push(Visit::Entry(node.entry, Some(node.head.clone())));
                for (index, link) in node.ancestors.into_iter().enumerate() {
                    let distance = 1_u64
                        .checked_shl(index as u32)
                        .ok_or_else(|| invalid("healthy copy skip distance"))?;
                    if link.head.position.checked_add(distance) != Some(node.head.position) {
                        return Err(invalid("healthy copy history ancestry position"));
                    }
                    work.push(Visit::History(link.node, link.head));
                }
            }
            Visit::Entry(_, expected) => {
                let started = trace.start();
                let FormalRequestStateV1::Qualified { candidate, proof } =
                    decode::<FormalRequestStateV1>(&bytes)?
                else {
                    return Err(invalid(
                        "healthy copy permanent index contains unresolved slot",
                    ));
                };
                trace.entry_decode_us += elapsed_us(started);
                trace.entries += 1;
                trace.entry_bytes += bytes.len() as u64;
                let record = &proof.decision.record;
                let head = FormalHeadRefV1 {
                    position: record.position,
                    record_id: record.record_id()?,
                };
                check_entry_head(&head, expected)?;
                if candidate.candidate.record != *record {
                    return Err(invalid("healthy copy qualified material/history binding"));
                }
                let started = trace.start();
                verify_record_material(record, &candidate.material)?;
                for material in
                    std::iter::once(&candidate.material).chain(candidate.frozen_material.iter())
                {
                    if let Some(descriptor) = material
                        .native_environment()
                        .map_err(|e| invalid(e.to_string()))?
                    {
                        let marker = visits.join(format!(
                            "native-environment-{}",
                            descriptor.digest().map_err(invalid)?
                        ));
                        if !marker.try_exists().map_err(io)? {
                            crate::controlled_authority::formal::native_environment::verify_material_chunks(material, |id| {
                                if copy {
                                    source_memo.as_mut().ok_or_else(|| invalid("native copy source absent"))?
                                        .read_object(id)?.ok_or_else(|| invalid("native copy chunk absent"))
                                } else {
                                    crate::controlled_authority::formal::business_pack::read_object_durable(
                                        &objects.join(id), id)?.ok_or_else(|| invalid("native independent chunk absent"))
                                }
                            }, |_, _| Ok(()))?;
                            write_private_marker(&marker, &[])?;
                        }
                    }
                }

                trace.material_us += elapsed_us(started);
                let started = trace.start();
                enqueue_material(
                    &candidate.material,
                    profile_version,
                    true,
                    &mut work,
                    source_memo.as_mut(),
                )?;
                trace.enqueue_us += elapsed_us(started);
                if let Some(material) = &candidate.frozen_material {
                    let started = trace.start();
                    verify_frozen_material(
                        &candidate
                            .frozen
                            .as_ref()
                            .ok_or_else(|| {
                                invalid("healthy copy frozen material lacks certificate")
                            })?
                            .reservation
                            .body,
                        material,
                    )?;
                    trace.material_us += elapsed_us(started);
                    let started = trace.start();
                    enqueue_material(
                        material,
                        profile_version,
                        false,
                        &mut work,
                        source_memo.as_mut(),
                    )?;
                    trace.enqueue_us += elapsed_us(started);
                }
                // Publish only after all intrinsic checks and closure enqueue
                // succeeded. Fresh private run state is discarded on any error.
                write_private_marker(
                    &projection_path,
                    &serde_cbor::to_vec(&head).map_err(|e| invalid(e.to_string()))?,
                )?;
                #[cfg(test)]
                ENTRY_VALIDATIONS.with(|count| {
                    let (validated, reused) = count.get();
                    count.set((validated + 1, reused));
                });
            }
            Visit::Business(_, expected_depth) => {
                match BusinessIndexNodeV1::decode(&bytes).map_err(invalid)?.kind {
                    BusinessIndexNodeKindV1::Branch {
                        depth,
                        left_hash,
                        right_hash,
                    } => {
                        if depth as usize != *expected_depth {
                            return Err(invalid("healthy copy sparse branch depth"));
                        }
                        work.push(Visit::Business(left_hash, *expected_depth + 1));
                        work.push(Visit::Business(right_hash, *expected_depth + 1));
                    }
                    BusinessIndexNodeKindV1::Leaf(_) if *expected_depth == 256 => {}
                    _ => return Err(invalid("healthy copy sparse leaf depth")),
                }
            }
            Visit::Raw(..) => {}
            Visit::Native(_, size) => {
                if bytes.len() != *size as usize {
                    return Err(invalid("native copied chunk length"));
                }
            }
        }
        // Logical raw hash stays unchanged even when the source is a physical pack.
        if copy && first_object {
            if matches!(&visit, Visit::Business(..)) {
                copy_pack.push(&objects, visit.id(), &bytes)?;
            } else {
                publish_raw_copy_object(&objects, visit.id(), &bytes)?;
            }
        }
        if first_object {
            inventory.inventory_digest = hash(
                "oasis7.formal.durable_set_inventory_step.v1",
                &(
                    &inventory.inventory_digest,
                    visit.tag(),
                    visit.id(),
                    bytes.len() as u64,
                ),
            )?;
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            options.open(object_marker).map_err(io)?;
        }
        // This private marker is process-local deduplication, not an authorization.
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        options.open(marker).map_err(io)?;
    }
    if copy {
        copy_pack.flush(&objects)?;
    }
    let started = trace.start();
    sync(&objects)?;
    trace.directory_sync_us += elapsed_us(started);
    Ok(inventory)
}
fn enqueue_material(
    material: &crate::controlled_authority::replicated_protocol::ClosedRecord,
    profile_version: u32,
    candidate: bool,
    work: &mut Vec<Visit>,
    mut memo: Option<&mut CopyReadMemo<'_>>,
) -> Result<(), FormalError> {
    use crate::controlled_authority::replicated_protocol::ArtifactRole;
    for object in &material.objects {
        work.push(Visit::Raw(
            object.content_hash.clone(),
            object.bytes.clone(),
        ));
    }
    if let Some(descriptor) = material
        .native_environment()
        .map_err(|e| invalid(e.to_string()))?
    {
        for component in descriptor.components {
            for chunk in component.chunks {
                work.push(Visit::Native(chunk.digest, chunk.byte_len));
            }
        }
    }
    if profile_version != 2 {
        return Ok(());
    }
    let result = material
        .objects
        .iter()
        .find(|o| material.roots.get(&ArtifactRole::Result) == Some(&o.content_hash))
        .ok_or_else(|| invalid("healthy copy Result closure missing"))?;
    // Frozen material's Result need not be a business result envelope. Its
    // authenticated Snapshot still binds the retained history root below.
    if candidate {
        let envelope = FormalBusinessResultEnvelopeV1::decode(&result.bytes)?;
        enqueue_business_delta(&envelope.delta, work, memo.as_deref_mut())?;
        if let Some(delta) = envelope.retained_history_delta {
            enqueue_business_delta(&delta, work, memo)?;
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
    struct Root {
        index_root: BusinessIndexRootV1,
    }
    let snapshot_bytes = material
        .snapshot_bytes()
        .map_err(|e| invalid(e.to_string()))?;
    let decoded: Snapshot = decode(&snapshot_bytes)?;
    if let Some(root) = decoded.state.retained_history_root {
        root.index_root.validate().map_err(invalid)?;
        work.push(Visit::Business(root.index_root.node_root, 0));
    }
    Ok(())
}

fn enqueue_business_delta(
    delta: &crate::controlled_authority::formal::business_index::BusinessIndexDeltaV1,
    work: &mut Vec<Visit>,
    mut memo: Option<&mut CopyReadMemo<'_>>,
) -> Result<(), FormalError> {
    // The signed transition commits every synthesized write, including branch
    // nodes superseded by later updates in this same delta. The final root walk
    // cannot reach those intermediate nodes, but activation re-verifies the
    // complete write set from CAS. Queue writes before the root so the LIFO
    // walker structurally validates and packs reachable nodes first; remaining
    // signed writes are then copied as raw, hash-checked closure objects.
    for write in &delta.node_writes {
        // The Result material was checked against the authenticated entry.
        // Recheck each content hash before memo admission; the subsequent root
        // walk still validates every node and its exact structural edge.
        if let Some(memo) = memo.as_deref_mut() {
            memo.seed(&write.digest, &write.bytes)?;
        }
        work.push(Visit::Raw(write.digest.clone(), write.bytes.clone()));
    }
    work.push(Visit::Business(delta.new.node_root.clone(), 0));
    Ok(())
}

#[cfg(test)]
#[path = "endpoint_store_durable_set_copy_tests.rs"]
mod tests;
