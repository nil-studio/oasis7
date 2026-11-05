//! Fixed private parent certificate for unchanged native/artifact inventories.
//! It never grants executable equivalence, ABI loading, or business permission.
use super::*;
use crate::incremental_page_store::{IncrementalPageStore, PageCompression};
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_artifacts::{
        ExecutionManifestModeV1, IwsExecutionManifestV1, OriginalArtifactRefV1,
    },
    incremental_material::{ExecutionArtifactRefs, OriginalObjectReader},
};
use std::sync::Arc;
#[path = "endpoint_incremental_closure_native_reader.rs"]
mod native_reader;
pub(crate) use native_reader::NativeChunkReadWorkV1;
#[path = "endpoint_incremental_closure_executable.rs"]
mod executable;
#[path = "endpoint_incremental_closure_frozen.rs"]
mod frozen;
#[path = "endpoint_incremental_closure_stream.rs"]
mod stream;

/// One static baseline, shared directly rather than retaining an O(H) parent
/// chain. Both real namespace leases outlive every copied Ordinary carrier.
pub(crate) struct StaticExecutionMaterialV1 {
    proof: ReservedValue<DeclaredExecutionMaterial>,
    config: [u8; 32],
    refs: ExecutionArtifactRefs,
    policy: OriginalArtifactRefV1,
    pages: IncrementalPageStore,
    page_identity: (u64, u64),
    native_identity: (u64, u64),
    executable: Option<ReservedValue<executable::VerifiedNativeExecutableV1>>,
}
pub(crate) struct CandidateExecutionMaterialV1 {
    baseline: Arc<ReservedValue<StaticExecutionMaterialV1>>,
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
    manifest: [u8; 32],
}
impl CandidateExecutionMaterialV1 {
    pub(crate) fn candidate_record_id(&self) -> [u8; 32] {
        self.record
    }
    pub(crate) fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub(crate) fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub(crate) fn manifest_id(&self) -> [u8; 32] {
        self.manifest
    }
    pub(crate) fn page_namespace_identity(&self) -> (u64, u64) {
        self.baseline.page_identity
    }
    pub(crate) fn native_namespace_identity(&self) -> (u64, u64) {
        self.baseline.native_identity
    }
    pub(crate) fn sealed_executable(
        &self,
    ) -> Option<&ReservedValue<executable::VerifiedNativeExecutableV1>> {
        self.baseline.executable.as_ref()
    }
    pub(crate) fn baseline(&self) -> &Arc<ReservedValue<StaticExecutionMaterialV1>> {
        &self.baseline
    }
}
fn file_identity(file: &std::fs::File) -> Result<(u64, u64), FormalError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file
            .metadata()
            .map_err(|_| invalid("execution lease metadata"))?;
        Ok((m.dev(), m.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Err(invalid("execution material requires Unix lease durability"))
    }
}
fn page_identity(pages: &IncrementalPageStore) -> Result<(u64, u64), FormalError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::symlink_metadata(pages.root().join("lease"))
            .map_err(|_| invalid("execution PageStore lease"))?;
        if !m.file_type().is_file() || m.nlink() != 1 || m.len() != 0 {
            return Err(invalid("execution PageStore fixed lease"));
        }
        Ok((m.dev(), m.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = pages;
        Err(invalid("execution material requires Unix PageStore"))
    }
}
fn manifest<O: OriginalObjectReader>(
    core: &CheckedCommitCore,
    objects: &mut O,
    memory: &MemoryBudget,
    work: &mut CoreBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<IwsExecutionManifestV1, FormalError> {
    if cancelled() {
        return Err(invalid("execution manifest cancelled"));
    }
    work.objects_remaining = work
        .objects_remaining
        .checked_sub(1)
        .ok_or_else(|| invalid("execution manifest objects"))?;
    let max = usize::try_from(work.original_bytes_remaining.min(1024))
        .map_err(|_| invalid("execution manifest allowance"))?;
    if max == 0 {
        return Err(invalid("execution manifest bytes"));
    }
    let id = core.execution_artifacts().manifest;
    let bytes = objects
        .read_original(&id, max, memory)
        .map_err(|_| invalid("execution manifest actual original"))?;
    if !bytes.uses_budget(memory)
        || bytes.reserved_bytes() < bytes.len()
        || bytes.len() > max
        || blake3::hash(&bytes).as_bytes() != &id
    {
        return Err(invalid("execution manifest bytes/charge/hash"));
    }
    work.original_bytes_remaining = work
        .original_bytes_remaining
        .checked_sub(bytes.len() as u64)
        .ok_or_else(|| invalid("execution manifest bytes"))?;
    let value = IwsExecutionManifestV1::from_canonical_bytes(&bytes)
        .map_err(|_| invalid("execution manifest wire"))?;
    if cancelled() {
        return Err(invalid("execution manifest final cancellation"));
    }
    Ok(value)
}
fn bind_manifest(
    core: &CheckedCommitCore,
    value: &IwsExecutionManifestV1,
    executable: [u8; 32],
) -> Result<(), FormalError> {
    let refs = core.execution_artifacts();
    if value.scope != core.body().scope
        || value.format_profile != core.config().format_profile
        || value.native != refs.native
        || value.executable != executable
        || value.wasm != refs.wasm
        || value.rules != refs.rules
        || value.policy.id != core.original_execution_policy_id()
    {
        return Err(invalid("execution manifest candidate binding"));
    }
    match (&value.mode, core.transition()) {
        (ExecutionManifestModeV1::Genesis, None)
            if core.parent_record_id().is_none() && core.roots().height == 0 =>
        {
            Ok(())
        }
        (
            ExecutionManifestModeV1::Execution {
                parent_commit,
                parent_roots,
                operation,
                input,
                ..
            },
            Some(_),
        ) if Some(*parent_commit) == core.body().parent_commit
            && core.parent_body().map(|b| b.roots) == Some(*parent_roots)
            && *operation == core.body().operation
            && *input == core.original_input_id() =>
        {
            Ok(())
        }
        _ => Err(invalid("execution manifest parent/mode/input")),
    }
}
/// Parent may only be the actual endpoint accepted cache plus genuine Formal
/// commit. A cold cache without the static baseline must perform full reads.
#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_candidate_execution_material_v1<
    O: OriginalObjectStreamReader,
    R: MerkleNodeReader,
>(
    endpoint: &FileFormalEndpoint,
    candidate: &FormalCandidateMaterialV1,
    context: &oasis7_formal_protocol::controlled_authority::formal::VerifiedCandidateContextV1,
    core: &ReservedValue<CheckedCommitCore>,
    objects: &mut O,
    pages: &IncrementalPageStore,
    reader: &R,
    parent: Option<(
        &VerifiedFormalCommitV1,
        &ReservedValue<CandidateExecutionMaterialV1>,
    )>,
    native_work: &mut CoreBudget,
    artifact_work: &mut ExecutionArtifactBudget,
    bridge_work: &mut NativeChunkReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<CandidateExecutionMaterialV1>, FormalError> {
    let fee = endpoint
        .store
        .path
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add(1024)
        .ok_or_else(|| invalid("execution native path bound"))?;
    let _path_phase = pages
        .memory_budget()
        .reserve(fee, MemoryClass::Ordinary)
        .map_err(|_| invalid("execution native path admission"))?;
    let directory = endpoint.store.path.join("formal-objects-v1");
    verify_in_directory(
        &directory,
        candidate,
        context,
        core,
        objects,
        pages,
        reader,
        parent,
        native_work,
        artifact_work,
        bridge_work,
        cancelled,
    )
}

#[allow(clippy::too_many_arguments)]
fn verify_in_directory<O: OriginalObjectStreamReader, R: MerkleNodeReader>(
    directory: &std::path::Path,
    candidate: &FormalCandidateMaterialV1,
    context: &oasis7_formal_protocol::controlled_authority::formal::VerifiedCandidateContextV1,
    core: &ReservedValue<CheckedCommitCore>,
    objects: &mut O,
    pages: &IncrementalPageStore,
    reader: &R,
    parent: Option<(
        &VerifiedFormalCommitV1,
        &ReservedValue<CandidateExecutionMaterialV1>,
    )>,
    native_work: &mut CoreBudget,
    artifact_work: &mut ExecutionArtifactBudget,
    bridge_work: &mut NativeChunkReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<CandidateExecutionMaterialV1>, FormalError> {
    if cancelled() {
        return Err(invalid("execution material candidate cancelled"));
    }
    let memory = pages.memory_budget();
    let charge = memory
        .reserve(
            std::mem::size_of::<CandidateExecutionMaterialV1>() + 512,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("execution candidate metadata admission"))?;
    let _phase = memory
        // This path hashes only the already verified fixed CanonicalRecord:
        // 9 IWS artifact hashes, fixed hash32 fields, two <=256B world IDs and
        // an optional <=256B handoff name. Its CBOR is <8KiB; serde Vec growth
        // and the domain-framed second Vec together stay <32KiB. Config has
        // one <=256B world ID; execution manifest wire is <=1024B. The 128KiB
        // protocol phase covers those bounded encodings, decoded manifest and
        // fixed path/continuation hash metadata. Native DTOs are not decoded
        // on the inherited path. See CanonicalRecordV1::validate and the
        // existing 128KiB candidate_promotion_binding admission.
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(|_| invalid("execution material fixed phase admission"))?;
    if !core.uses_budget(memory)
        || core.reserved_bytes() < std::mem::size_of::<CheckedCommitCore>()
        || !objects.allocation_budget().same_pool(memory)
        || objects.allocation_class() != MemoryClass::Ordinary
        || reader.allocation_class() != Some(MemoryClass::Ordinary)
        || !reader
            .allocation_budget()
            .is_some_and(|p| p.same_pool(memory))
    {
        return Err(invalid("execution material actual pool/class"));
    }
    if context
        .record()
        .record_id()
        .map_err(|_| invalid("execution material verified record"))?
        != hex::encode(core.candidate_record_id())
        || context.branch() != candidate.branch
        || candidate.candidate.record != *context.record()
        || candidate.material.roots != context.record().artifact_roots
        || candidate.material.payload_digest != context.record().payload_digest
        || candidate.material.before_state_root != context.record().before_state_root
        || candidate.material.after_state_root != context.record().after_state_root
    {
        return Err(invalid("execution material exact candidate"));
    }
    if let Some((accepted, parent)) = parent {
        if !parent.uses_budget(memory)
            || parent.reserved_bytes() < std::mem::size_of::<CandidateExecutionMaterialV1>() + 512
            || !parent.baseline.uses_budget(memory)
            || parent.baseline.reserved_bytes()
                < std::mem::size_of::<StaticExecutionMaterialV1>() + 512
            || accepted.verified_record_id() != parent.record
            || core.parent_record_id() != Some(parent.record)
            || core.body().parent_commit != Some(parent.body)
            || core.parent_body().map(|b| b.roots) != Some(parent.roots)
        {
            return Err(invalid("execution material exact accepted parent"));
        }
    } else if core.parent_record_id().is_none() && core.roots().height != 0 {
        return Err(invalid("execution material GEN height"));
    }
    frozen::verify(
        candidate,
        context,
        core,
        memory,
        &mut bridge_work.frozen_wire_bytes_remaining,
        cancelled,
    )?;
    let value = manifest(
        core,
        objects,
        memory,
        &mut artifact_work.structural,
        cancelled,
    )?;
    let config = core
        .config()
        .digest()
        .map_err(|_| invalid("execution config digest"))?;
    let refs = core.execution_artifacts();
    let native_pin = crate::controlled_authority::formal::physical::pin(directory, false)?
        .ok_or_else(|| invalid("execution material native live pin"))?;
    let native_identity = file_identity(&native_pin)?;
    let page_identity = page_identity(pages)?;
    let inherited = parent.filter(|(_, parent)| {
        let b = &parent.baseline;
        b.config == config
            && b.refs.native == refs.native
            && b.refs.wasm == refs.wasm
            && b.refs.rules == refs.rules
            && b.policy == value.policy
            && b.page_identity == page_identity
            && b.native_identity == native_identity
            && b.pages.root() == pages.root()
    });
    let baseline = if let Some((_, parent)) = inherited {
        bind_manifest(core, &value, parent.baseline.proof.native.executable_id())?;
        if let ExecutionManifestModeV1::Execution { continuation, .. } = &value.mode {
            stream::verify_original(
                *continuation,
                objects,
                memory,
                &mut artifact_work.originals,
                cancelled,
            )?;
        }
        parent.baseline.clone()
    } else {
        let _native_phase = memory
            .reserve(
                oasis7_formal_protocol::incremental_material::NATIVE_MATERIAL_WORKSPACE_BYTES,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("execution native descriptor phase admission"))?;
        let baseline_charge = memory
            .reserve(
                std::mem::size_of::<StaticExecutionMaterialV1>() + 512,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("execution static baseline admission"))?;
        let proof_charge = memory
            .reserve(
                std::mem::size_of::<DeclaredExecutionMaterial>() + 1024,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("execution actual closure admission"))?;
        let encoded = hex::encode(refs.native);
        let supplied = candidate
            .material
            .objects
            .iter()
            .chain(
                candidate
                    .frozen_material
                    .iter()
                    .flat_map(|m| m.objects.iter()),
            )
            .find(|o| o.content_hash == encoded)
            .map(|o| o.bytes.as_slice());
        native_work.objects_remaining = native_work
            .objects_remaining
            .checked_sub(1)
            .ok_or_else(|| invalid("native descriptor occurrence allowance"))?;
        let loaded = if supplied.is_none() {
            let max = usize::try_from(native_work.original_bytes_remaining.min(
                oasis7_formal_protocol::native_environment::MAX_NATIVE_DESCRIPTOR_BYTES as u64,
            ))
            .map_err(|_| invalid("native descriptor byte bound"))?;
            Some(
                objects
                    .read_original(&refs.native, max, memory)
                    .map_err(|_| invalid("native descriptor actual reader"))?,
            )
        } else {
            None
        };
        let descriptor_bytes = supplied
            .or_else(|| loaded.as_ref().map(|b| b.as_slice()))
            .ok_or_else(|| invalid("native descriptor bytes missing"))?;
        native_work.original_bytes_remaining = native_work
            .original_bytes_remaining
            .checked_sub(descriptor_bytes.len() as u64)
            .ok_or_else(|| invalid("native descriptor byte allowance"))?;
        if descriptor_bytes.len()
            > oasis7_formal_protocol::native_environment::MAX_NATIVE_DESCRIPTOR_BYTES
            || blake3::hash(descriptor_bytes).as_bytes() != &refs.native
            || loaded
                .as_ref()
                .is_some_and(|b| !b.uses_budget(memory) || b.reserved_bytes() < b.len())
        {
            return Err(invalid("native descriptor actual original binding"));
        }
        let descriptor =
            oasis7_formal_protocol::native_environment::NativeExecutionEnvironmentV1::decode(
                descriptor_bytes,
            )
            .map_err(|_| invalid("native descriptor canonical bytes"))?;
        if descriptor
            .digest()
            .map_err(|_| invalid("execution native digest"))?
            != hex::encode(refs.native)
        {
            return Err(invalid("execution native actual descriptor binding"));
        }
        if candidate.frozen.is_some()&&candidate.frozen_material.as_ref().is_none_or(|m|
            m.roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::NativeExecutionEnvironment)
                !=candidate.material.roots.get(&crate::controlled_authority::replicated_protocol::ArtifactRole::NativeExecutionEnvironment)){
            return Err(invalid("execution Frozen native reference"));
        }
        let stopped = std::cell::RefCell::new(&mut *cancelled);
        let mut bridge = native_reader::NativeChunkBridgeV1 {
            originals: objects,
            descriptor: &descriptor,
            descriptor_id: refs.native,
            descriptor_bytes,
            directory,
            memory,
            work: bridge_work,
            cancelled: || (*stopped.borrow_mut())(),
        };
        let native = verify_native_environment(core, &mut bridge, memory, native_work, || {
            (*stopped.borrow_mut())()
        })
        .map_err(|_| invalid("execution native actual chunks"))?;
        bind_manifest(core, &value, native.executable_id())?;
        let artifacts = verify_execution_artifacts(
            core,
            &native,
            ExecutionArtifactReadContext {
                objects: &mut bridge,
                pages: reader,
                memory,
            },
            artifact_work,
            || (*stopped.borrow_mut())(),
        )
        .map_err(|_| invalid("execution actual artifact inventories"))?;
        if native.body_id() != core.catalog().entry.commit
            || artifacts.body_id() != core.catalog().entry.commit
            || artifacts.manifest_id() != refs.manifest
        {
            return Err(invalid("execution full closure body/manifest"));
        }
        let executable = executable::prepare(&native, &descriptor, &mut bridge, memory)?;
        let proof = proof_charge.retain(DeclaredExecutionMaterial {
            native,
            artifacts,
            _native_pin: native_pin,
        });
        let store = IncrementalPageStore::open(
            pages.root(),
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Ordinary,
        )
        .map_err(page_store_formal_error)?;
        Arc::new(baseline_charge.retain(StaticExecutionMaterialV1 {
            proof,
            config,
            refs,
            policy: value.policy,
            pages: store,
            page_identity,
            native_identity,
            executable,
        }))
    };
    if cancelled() {
        return Err(invalid("execution material final cancellation"));
    }
    Ok(charge.retain(CandidateExecutionMaterialV1 {
        baseline,
        record: core.candidate_record_id(),
        body: core.catalog().entry.commit,
        roots: core.body().roots,
        manifest: refs.manifest,
    }))
}

#[cfg(all(test, unix))]
#[path = "endpoint_incremental_closure_material_tests.rs"]
mod tests;
