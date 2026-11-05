//! Actual cold local continuity consumer. A native ref alone cannot construct
//! this token: checked logical sources, a fresh provider guard, fixed original,
//! both actual opened leases and unchanged native generation are required.
use super::super::provisional::{AcceptedCandidateCache, AcceptedCurrentOwner};
use super::*;
use crate::incremental_page_store::{IncrementalPageStore, PageCompression};
use std::fs::File;
pub(crate) struct LocalClosureReadWorkV1 {
    pub(crate) reads_remaining: u64,
    pub(crate) original_bytes_remaining: u64,
}
pub(crate) struct AuthenticatedLocalClosureV1 {
    class: MemoryClass,
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    page_identity: (u64, u64),
    pages: IncrementalPageStore,
    _native: File,
}
impl AuthenticatedLocalClosureV1 {
    pub(crate) fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub(crate) fn record_id(&self) -> [u8; 32] {
        self.record
    }
    pub(crate) fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub(crate) fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub(crate) fn page_identity(&self) -> (u64, u64) {
        self.page_identity
    }
    pub(crate) fn pages(&self) -> &IncrementalPageStore {
        &self.pages
    }
}
fn file_identity(file: &File) -> Result<(u64, u64), FormalError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file
            .metadata()
            .map_err(|e| FormalError::Io(e.to_string()))?;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Err(invalid("local closure requires Unix fixed leases"))
    }
}
fn read_record(
    endpoint: &FileFormalEndpoint,
    memory: &MemoryBudget,
    reference: &str,
    work: &mut LocalClosureReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<LocalDataClosureRecordV1>, FormalError> {
    if cancelled() {
        return Err(invalid("cold local record cancelled"));
    }
    work.reads_remaining = work
        .reads_remaining
        .checked_sub(1)
        .ok_or_else(|| invalid("cold local record read allowance"))?;
    work.original_bytes_remaining = work
        .original_bytes_remaining
        .checked_sub(
            u64::try_from(LOCAL_DATA_CLOSURE_WIRE_BYTES)
                .map_err(|_| invalid("local wire byte bound"))?,
        )
        .ok_or_else(|| invalid("cold local record byte allowance"))?;
    let output = memory
        .reserve(
            std::mem::size_of::<LocalDataClosureRecordV1>() + 128,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("cold local record admission"))?;
    let _phase = memory
        .reserve(128 * 1024 * 1024, MemoryClass::Ordinary)
        .map_err(|_| invalid("cold local uncached metadata admission"))?;
    let directory = endpoint.store.path.join("formal-objects-v1");
    let path = crate::controlled_authority::formal::physical::selected_path_uncached(
        &directory.join(reference),
        reference,
    )?;
    let mut batch = crate::controlled_authority::formal::physical::read_durable_batch_uncached(
        &directory,
        &[(
            path.clone(),
            reference.to_owned(),
            LOCAL_DATA_CLOSURE_WIRE_BYTES,
        )],
    )?;
    let raw = match batch
        .pop()
        .ok_or_else(|| invalid("cold local record batch coverage"))?
    {
        Some(bytes) => bytes,
        None => crate::controlled_authority::formal::business_pack::read_legacy_durable_expected(
            &path,
            reference,
            LOCAL_DATA_CLOSURE_WIRE_BYTES,
        )?
        .ok_or_else(|| invalid("cold local record missing"))?,
    };
    if raw.len() != LOCAL_DATA_CLOSURE_WIRE_BYTES
        || blake3::hash(&raw).to_hex().as_str() != reference
    {
        return Err(invalid("cold local record exact identity/length"));
    }
    let record: LocalDataClosureRecordV1 =
        serde_cbor::from_slice(&raw).map_err(|e| invalid(e.to_string()))?;
    if cancelled() {
        return Err(invalid("cold local record final cancellation"));
    }
    Ok(output.retain(record))
}
/// The caller retains the actual guard returned by FileFormalEndpoint::guard()
/// across its reload, full cold Q/A and this function. Evidence is independently
/// cryptographically checked here, so a trait's claimed checkpoint is not proof.
pub(crate) fn recover_authenticated_local_closure_v1(
    endpoint: &FileFormalEndpoint,
    guard: &dyn fencing::MonotonicEndpointGuard,
    head: &VerifiedFormalCommitV1,
    cache: &AcceptedCandidateCache,
    pages: &IncrementalPageStore,
    work: &mut LocalClosureReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<AuthenticatedLocalClosureV1>, FormalError> {
    if cancelled() {
        return Err(invalid("cold local closure cancelled"));
    }
    let memory = pages.memory_budget();
    if cache.commit_record != head.verified_record_id()
        || !cache.current().uses_budget(memory)
        || cache.current().allocation_class() != MemoryClass::Ordinary
        || !cache.roots.uses_budget(memory)
        || cache.current().reserved_bytes() < std::mem::size_of_val(&**cache.current())
        || !cache.runtime.uses_budget(memory)
        || cache.runtime.reserved_bytes() < std::mem::size_of_val(&*cache.runtime)
        || cache.runtime.allocation_class() != MemoryClass::Ordinary
        || !cache.cognition.uses_budget(memory)
        || cache.cognition.reserved_bytes() < std::mem::size_of_val(&*cache.cognition)
        || cache.cognition.selected().occurrence().allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid("cold local exact Ordinary source admission"));
    }
    let AcceptedCurrentOwner::Cold { results, .. } = &cache.current_owner else {
        return Err(invalid(
            "cold local requires actual recovered closed Results",
        ));
    };
    if !results.uses_budget(memory)
        || results.allocation_class() != MemoryClass::Ordinary
        || results.record_id() != cache.commit_record
        || results.body_id() != cache.runtime.occurrence().head_body_id()
    {
        return Err(invalid("cold local Result admission"));
    }
    let policy = endpoint.authority.policy();
    let fee = policy
        .authorized_fencing_resource_bindings
        .iter()
        .try_fold(8192usize, |n, id| {
            n.checked_add(id.len())
                .and_then(|n| n.checked_add(128))
                .ok_or_else(|| invalid("cold policy accounting"))
        })?;
    let _phase = memory
        .reserve(fee, MemoryClass::Ordinary)
        .map_err(|_| invalid("cold protected scope admission"))?;
    let evidence = guard.evidence().map_err(fencing_error)?;
    if evidence.signature.len() != 128 || guard.checkpoint() != &endpoint.store.checkpoint {
        return Err(invalid("cold actual protected checkpoint"));
    }
    fencing::verify_monotonic_checkpoint_evidence(
        &evidence,
        endpoint.instance(),
        &endpoint.store.checkpoint,
        &fencing::TrustedFencingPolicyV1 {
            provider_public_key: policy.fencing_provider_public_key.clone(),
            authorized_resource_bindings: policy.authorized_fencing_resource_bindings.clone(),
        },
    )
    .map_err(fencing_error)?;
    let reference = endpoint.store.body.data_closure.as_deref().ok_or_else(|| {
        invalid("cold local durability attestation absent; full rebuild required")
    })?;
    let native_directory = endpoint.store.path.join("formal-objects-v1");
    let native = crate::controlled_authority::formal::physical::pin(&native_directory, false)?
        .ok_or_else(|| invalid("cold native actual lease unavailable"))?;
    let record = read_record(endpoint, memory, reference, work, cancelled)?;
    record.validate_scope(&endpoint.store.body)?;
    let expected_manifest = fixed_id(
        head.record()
            .artifact_roots
            .get(&crate::controlled_authority::replicated_protocol::ArtifactRole::ExecutionManifest)
            .ok_or_else(|| invalid("cold local accepted execution manifest missing"))?,
    )?;
    let roots = cache.roots.digest().map_err(|e| invalid(e.to_string()))?;
    if record.record != cache.commit_record
        || record.body != cache.runtime.occurrence().head_body_id()
        || record.roots != roots
        || record.manifest != expected_manifest
        || record.cognition.height != cache.cognition.selected().occurrence().location().height
        || record.cognition.ordinal != cache.cognition.selected().occurrence().location().ordinal
        || record.cognition.original != cache.cognition.selected().occurrence().location().original
        || record.runtime.height != cache.runtime.occurrence().location().height
        || record.runtime.ordinal != cache.runtime.occurrence().location().ordinal
        || record.runtime.original != cache.runtime.occurrence().location().original
    {
        return Err(invalid(
            "cold local exact head/body/roots/History selections",
        ));
    }
    let opened = IncrementalPageStore::open(
        pages.root(),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .map_err(page_store_formal_error)?;
    let page_identity = opened
        .opened_lease_identity()
        .map_err(page_store_formal_error)?;
    let native_identity = file_identity(&native)?;
    if page_identity != record.page_lease || native_identity != record.native_lease {
        return Err(invalid("cold local foreign or replaced namespace"));
    }
    check_maintenance_generation(&record, &native_directory, memory, cancelled)?;
    if cancelled() {
        return Err(invalid("cold local closure final cancellation"));
    }
    let charge = memory
        .reserve(
            std::mem::size_of::<AuthenticatedLocalClosureV1>() + 512,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("cold local namespace token admission"))?;
    Ok(charge.retain(AuthenticatedLocalClosureV1 {
        class: MemoryClass::Ordinary,
        record: record.record,
        body: record.body,
        roots: record.roots,
        page_identity,
        pages: opened,
        _native: native,
    }))
}

fn check_maintenance_generation(
    record: &LocalDataClosureRecordV1,
    root: &std::path::Path,
    memory: &MemoryBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Option<[u8; 32]>, FormalError> {
    let observed =
        crate::controlled_authority::formal::physical::current_generation_digest_uncached(
            root,
            memory,
            MemoryClass::Ordinary,
            cancelled,
        )?;
    if observed != record.native_generation {
        return Err(invalid(
            "cold local maintenance generation changed; full rebuild required",
        ));
    }
    Ok(observed)
}

#[cfg(test)]
#[path = "endpoint_incremental_cold_local_tests.rs"]
mod tests;
