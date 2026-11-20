//! Execution-before-candidate schema-4 Frozen content. Never an acceptance,
//! endpoint durability, parent-cache, native lease or Runtime business permit.
use super::{
    FormalError, VerifiedFormalCommitV1, VerifiedFormalReservationV1,
    frozen_material_digest_bounded_v1,
};
use crate::controlled_authority::replicated_protocol::package::{MAX_OBJECTS, MAX_RECORD_BYTES};
use crate::controlled_authority::replicated_protocol::{ArtifactRole, ClosedRecord};
use crate::incremental_artifacts::{
    ExecutionManifestModeV1, IwsExecutionManifestV1, OriginalArtifactRefV1,
};
use crate::incremental_execution::ExecutionError;
use crate::incremental_history::{GenesisConfigV1, HISTORY_MAX_HEADER_BYTES};
use crate::incremental_material::{NATIVE_MATERIAL_WORKSPACE_BYTES, OriginalObjectStreamReader};
use crate::native_environment::{
    MAX_NATIVE_DESCRIPTOR_BYTES, NativeEnvironmentRole, NativeExecutionEnvironmentV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryLimitExceeded, ReservedValue};
use std::{
    io::{self, Write},
    sync::Arc,
};
const PARENT_ROLES: [ArtifactRole; 7] = [
    ArtifactRole::IncrementalGenesisConfig,
    ArtifactRole::IncrementalRootSet,
    ArtifactRole::IncrementalCommitBody,
    ArtifactRole::IncrementalHistoryPrefix,
    ArtifactRole::Wasm,
    ArtifactRole::Rules,
    ArtifactRole::NativeExecutionEnvironment,
];
/// All counters are finite, caller-owned and consumed in place, including on
/// errors. Structural metadata, comparisons, hashes, wire and source bytes are
/// distinct work; repeated original occurrences are charged again.
pub struct IncrementalFrozenMaterialWorkV1 {
    pub metadata_entries_remaining: u64,
    pub comparison_bytes_remaining: u64,
    pub hash_bytes_remaining: u64,
    pub frozen_wire_bytes_remaining: u64,
    pub original_calls_remaining: u64,
    pub original_bytes_remaining: u64,
    pub max_original_bytes: u64,
}
#[derive(Debug)]
pub enum IncrementalFrozenMaterialErrorV1<E> {
    Source(E),
    Formal(FormalError),
    Execution(ExecutionError),
    Memory(MemoryLimitExceeded),
    Invalid(&'static str),
    ResourceLimited(&'static str),
    Cancelled,
}
impl<E: std::fmt::Display> std::fmt::Display for IncrementalFrozenMaterialErrorV1<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(e) => write!(f, "Frozen original source: {e}"),
            Self::Formal(e) => e.fmt(f),
            Self::Execution(e) => e.fmt(f),
            Self::Memory(e) => e.fmt(f),
            Self::Invalid(e) => write!(f, "invalid Frozen content: {e}"),
            Self::ResourceLimited(e) => write!(f, "Frozen content resource limited: {e}"),
            Self::Cancelled => f.write_str("Frozen content cancelled"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for IncrementalFrozenMaterialErrorV1<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(e) => Some(e),
            Self::Formal(e) => Some(e),
            Self::Execution(e) => Some(e),
            Self::Memory(e) => Some(e),
            _ => None,
        }
    }
}
type Error<E> = IncrementalFrozenMaterialErrorV1<E>;
fn poll<E>(cancelled: &mut impl FnMut() -> bool) -> Result<(), Error<E>> {
    if cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
fn take<E>(left: &mut u64, n: u64, label: &'static str) -> Result<(), Error<E>> {
    *left = left.checked_sub(n).ok_or(Error::ResourceLimited(label))?;
    Ok(())
}
fn step<E>(
    work: &mut IncrementalFrozenMaterialWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), Error<E>> {
    poll(cancelled)?;
    take(&mut work.metadata_entries_remaining, 1, "metadata entries")
}
fn compare<E>(
    work: &mut IncrementalFrozenMaterialWorkV1,
    n: usize,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), Error<E>> {
    step(work, cancelled)?;
    take(
        &mut work.comparison_bytes_remaining,
        u64::try_from(n).map_err(|_| Error::ResourceLimited("comparison length"))?,
        "comparison bytes",
    )
}
fn hash_id<E>(
    text: &str,
    work: &mut IncrementalFrozenMaterialWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<[u8; 32], Error<E>> {
    compare(work, text.len(), cancelled)?;
    if text.len() != 64
        || !text
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(Error::Invalid("lowercase digest"));
    }
    let mut id = [0; 32];
    hex::decode_to_slice(text, &mut id).map_err(|_| Error::Invalid("digest bytes"))?;
    Ok(id)
}
fn find<E>(
    ids: &[[u8; 32]],
    id: &[u8; 32],
    work: &mut IncrementalFrozenMaterialWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<usize, Error<E>> {
    let (mut left, mut right) = (0, ids.len());
    while left < right {
        compare(work, 32, cancelled)?;
        let mid = left + (right - left) / 2;
        match ids[mid].cmp(id) {
            std::cmp::Ordering::Equal => return Ok(mid),
            std::cmp::Ordering::Less => left = mid + 1,
            std::cmp::Ordering::Greater => right = mid,
        }
    }
    Err(Error::Invalid("declared object missing"))
}
fn hash_bytes<E>(
    bytes: &[u8],
    work: &mut IncrementalFrozenMaterialWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<[u8; 32], Error<E>> {
    let mut hash = blake3::Hasher::new();
    for chunk in bytes.chunks(64 << 10) {
        poll(cancelled)?;
        take(
            &mut work.hash_bytes_remaining,
            chunk.len() as u64,
            "hash bytes",
        )?;
        hash.update(chunk);
    }
    poll(cancelled)?;
    Ok(*hash.finalize().as_bytes())
}
/// The borrowing marker prevents the caller from mutating the checked material
/// while this content proof exists. Equal DTOs never supply parent acceptance:
/// the original verified reservation/commit are retained as immutable borrows.
pub struct CheckedIncrementalFrozenMaterialV1<'a> {
    reservation: &'a VerifiedFormalReservationV1,
    parent: &'a VerifiedFormalCommitV1,
    material: &'a ClosedRecord,
    reservation_id: [u8; 32],
    parent_id: [u8; 32],
    material_id: [u8; 32],
    input: OriginalArtifactRefV1,
    manifest: [u8; 32],
    policy: OriginalArtifactRefV1,
    continuation: OriginalArtifactRefV1,
}
impl CheckedIncrementalFrozenMaterialV1<'_> {
    pub fn reservation(&self) -> &VerifiedFormalReservationV1 {
        self.reservation
    }
    pub fn parent(&self) -> &VerifiedFormalCommitV1 {
        self.parent
    }
    pub fn material(&self) -> &ClosedRecord {
        self.material
    }
    pub fn reservation_id(&self) -> [u8; 32] {
        self.reservation_id
    }
    pub fn parent_record_id(&self) -> [u8; 32] {
        self.parent_id
    }
    pub fn material_id(&self) -> [u8; 32] {
        self.material_id
    }
    pub fn input(&self) -> OriginalArtifactRefV1 {
        self.input
    }
    pub fn manifest_id(&self) -> [u8; 32] {
        self.manifest
    }
    pub fn policy(&self) -> OriginalArtifactRefV1 {
        self.policy
    }
    pub fn continuation(&self) -> OriginalArtifactRefV1 {
        self.continuation
    }
}
fn load<O: OriginalObjectStreamReader>(
    objects: &mut O,
    id: &[u8; 32],
    max: usize,
    memory: &MemoryBudget,
    work: &mut IncrementalFrozenMaterialWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Arc<ReservedValue<Vec<u8>>>, Error<O::Error>> {
    poll(cancelled)?;
    take(&mut work.original_calls_remaining, 1, "original calls")?;
    let bound = (max as u64)
        .min(work.max_original_bytes)
        .min(work.original_bytes_remaining);
    let bound = usize::try_from(bound).map_err(|_| Error::ResourceLimited("original bound"))?;
    let value = objects
        .read_original(id, bound, memory)
        .map_err(Error::Source)?;
    if !value.uses_budget(memory)
        || value.reserved_bytes() < value.capacity()
        || value.len() > bound
    {
        return Err(Error::Invalid("original reader admission/bound"));
    }
    take(
        &mut work.original_bytes_remaining,
        value.len() as u64,
        "original bytes",
    )?;
    if hash_bytes(&value, work, cancelled)? != *id {
        return Err(Error::Invalid("original hash"));
    }
    Ok(value)
}
struct Stream<'a, C> {
    hash: blake3::Hasher,
    written: u64,
    expected: u64,
    bytes: &'a mut u64,
    hash_bytes: &'a mut u64,
    cancelled: C,
    stop: Option<StreamStop>,
}
#[derive(Clone, Copy)]
enum StreamStop {
    Cancelled,
    Limit,
    Long,
}
impl<C: FnMut() -> bool> Write for Stream<'_, C> {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if self.stop.is_some() {
            return Err(io::Error::other("Frozen stream stopped"));
        }
        if (self.cancelled)() {
            self.stop = Some(StreamStop::Cancelled);
            return Err(io::Error::other("Frozen stream cancelled"));
        }
        let input = &input[..input.len().min(64 << 10)];
        let n = input.len() as u64;
        let Some(next) = self.written.checked_add(n).filter(|n| *n <= self.expected) else {
            self.stop = Some(StreamStop::Long);
            return Err(io::Error::other("Frozen original length"));
        };
        let Some(bytes) = self.bytes.checked_sub(n) else {
            self.stop = Some(StreamStop::Limit);
            return Err(io::Error::other("Frozen stream bytes"));
        };
        let Some(hash_bytes) = self.hash_bytes.checked_sub(n) else {
            self.stop = Some(StreamStop::Limit);
            return Err(io::Error::other("Frozen stream hash work"));
        };
        *self.bytes = bytes;
        *self.hash_bytes = hash_bytes;
        self.hash.update(input);
        self.written = next;
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.stop.is_some() {
            Err(io::Error::other("Frozen stream stopped"))
        } else {
            Ok(())
        }
    }
}
fn original<O: OriginalObjectStreamReader>(
    objects: &mut O,
    reference: OriginalArtifactRefV1,
    memory: &MemoryBudget,
    work: &mut IncrementalFrozenMaterialWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), Error<O::Error>> {
    poll(cancelled)?;
    if reference.byte_len > work.max_original_bytes
        || reference.byte_len > work.original_bytes_remaining
    {
        return Err(Error::ResourceLimited("original length"));
    }
    take(&mut work.original_calls_remaining, 1, "original calls")?;
    let _phase = memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(Error::Memory)?;
    let mut sink = Stream {
        hash: blake3::Hasher::new(),
        written: 0,
        expected: reference.byte_len,
        bytes: &mut work.original_bytes_remaining,
        hash_bytes: &mut work.hash_bytes_remaining,
        cancelled: &mut *cancelled,
        stop: None,
    };
    let result = objects.stream_original(&reference.id, reference.byte_len, memory, &mut sink);
    // A sink failure happened before the source's resulting Write error. An
    // independent source error is preserved as its exact original typed cause.
    if let Some(stop) = sink.stop {
        return Err(match stop {
            StreamStop::Cancelled => Error::Cancelled,
            StreamStop::Limit => Error::ResourceLimited("original stream work"),
            StreamStop::Long => Error::Invalid("original output too long"),
        });
    }
    let reported = result.map_err(Error::Source)?;
    if reported != reference.byte_len
        || sink.written != reference.byte_len
        || sink.hash.finalize().as_bytes() != &reference.id
    {
        return Err(Error::Invalid("original stream digest/length"));
    }
    poll(&mut sink.cancelled)?;
    Ok(())
}
/// Exact self-contained schema-4 content. Endpoint accepted-parent/store/native
/// lease/live checks remain mandatory before any Frozen receipt is signed.
#[allow(clippy::too_many_arguments)]
pub fn verify_incremental_frozen_material_v1<'a, O: OriginalObjectStreamReader>(
    reservation: &'a VerifiedFormalReservationV1,
    parent: &'a VerifiedFormalCommitV1,
    material: &'a ClosedRecord,
    objects: &mut O,
    memory: &MemoryBudget,
    work: &mut IncrementalFrozenMaterialWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedIncrementalFrozenMaterialV1<'a>>, Error<O::Error>> {
    poll(&mut cancelled)?;
    if objects.allocation_class() != MemoryClass::Ordinary
        || !objects.allocation_budget().same_pool(memory)
    {
        return Err(Error::Invalid("original source pool/class"));
    }
    // Fixed arrays plus the bounded reachability queue, canonical config/
    // manifest metadata and formal identity CBOR all fit this prepaid workspace.
    let _phase = memory
        .reserve(64 << 10, MemoryClass::Ordinary)
        .map_err(Error::Memory)?;
    let body = reservation.body();
    let committed = parent.record();
    if body.schema_version != super::incremental::IWS_FROZEN_SCHEMA
        || !super::incremental::is_iws_record(committed.schema_version)
        || body.parent.position != committed.position
        || body.parent.record_id.len() != 64
        || body.next_position
            != committed
                .position
                .checked_add(1)
                .ok_or(Error::Invalid("position overflow"))?
    {
        return Err(Error::Invalid("Frozen/parent format/position"));
    }
    let parent_id = parent.verified_record_id();
    if hash_id(&body.parent.record_id, work, &mut cancelled)? != parent_id {
        return Err(Error::Invalid("Frozen parent record"));
    }
    compare(
        work,
        body.world.world_id.len() + body.world.chain_id.len() + 64,
        &mut cancelled,
    )?;
    if body.world != committed.world
        || body.pre_state_root != committed.after_state_root
        || body.governing_manifest_digest != committed.governing_manifest_digest
        || body.execution_policy_digest != committed.execution_policy_digest
    {
        return Err(Error::Invalid("Frozen committed scope/state/policy"));
    }
    let reservation_text = reservation.reservation_id().map_err(Error::Formal)?;
    let reservation_id = hash_id(&reservation_text, work, &mut cancelled)?;
    if material.roots.len() != 9
        || material.objects.is_empty()
        || material.objects.len() > MAX_OBJECTS
    {
        return Err(Error::Invalid("Frozen exact roles/object count"));
    }
    let input_id = hash_id(&body.payload_digest, work, &mut cancelled)?;
    let manifest_id = hash_id(&body.runtime_manifest_digest, work, &mut cancelled)?;
    for (a, b) in [
        (&material.payload_digest, &body.payload_digest),
        (&material.before_state_root, &body.pre_state_root),
        (&material.after_state_root, &body.pre_state_root),
    ] {
        compare(work, a.len().max(b.len()), &mut cancelled)?;
        if a != b {
            return Err(Error::Invalid("Frozen payload/prestate"));
        }
    }
    for role in PARENT_ROLES {
        step(work, &mut cancelled)?;
        let expected = committed
            .artifact_roots
            .get(&role)
            .ok_or(Error::Invalid("parent role missing"))?;
        let actual = material
            .roots
            .get(&role)
            .ok_or(Error::Invalid("Frozen parent role missing"))?;
        compare(work, expected.len().max(actual.len()), &mut cancelled)?;
        if expected != actual {
            return Err(Error::Invalid("Frozen parent role differs"));
        }
    }
    for (role, id) in [
        (ArtifactRole::Input, input_id),
        (ArtifactRole::ExecutionManifest, manifest_id),
    ] {
        let actual = material
            .roots
            .get(&role)
            .ok_or(Error::Invalid("Frozen request role missing"))?;
        if hash_id(actual, work, &mut cancelled)? != id {
            return Err(Error::Invalid("Frozen request role differs"));
        }
    }
    let mut ids = [[0u8; 32]; MAX_OBJECTS];
    let mut total = 0usize;
    for (index, object) in material.objects.iter().enumerate() {
        step(work, &mut cancelled)?;
        if object.references.len() > MAX_OBJECTS {
            return Err(Error::Invalid("reference count"));
        }
        total = total
            .checked_add(object.bytes.len())
            .filter(|n| *n <= MAX_RECORD_BYTES)
            .ok_or(Error::Invalid("Frozen 8 MiB originals"))?;
        let id = hash_id(&object.content_hash, work, &mut cancelled)?;
        if index > 0 {
            compare(work, 32, &mut cancelled)?;
            if ids[index - 1] >= id {
                return Err(Error::Invalid("strict object order"));
            }
        }
        if hash_bytes(&object.bytes, work, &mut cancelled)? != id {
            return Err(Error::Invalid("material original hash"));
        }
        ids[index] = id;
        let mut previous = None;
        for reference in &object.references {
            let id = hash_id(reference, work, &mut cancelled)?;
            if let Some(prev) = previous {
                compare(work, 32, &mut cancelled)?;
                if prev >= id {
                    return Err(Error::Invalid("strict reference order"));
                }
            }
            previous = Some(id);
        }
    }
    let ids = &ids[..material.objects.len()];
    let mut reached = [false; MAX_OBJECTS];
    let mut queue = Vec::with_capacity(material.objects.len());
    if queue.capacity() > MAX_OBJECTS {
        return Err(Error::ResourceLimited("closure queue capacity"));
    }
    for value in material.roots.values() {
        let id = hash_id(value, work, &mut cancelled)?;
        let index = find(ids, &id, work, &mut cancelled)?;
        if !reached[index] {
            reached[index] = true;
            queue.push(index);
        }
    }
    let mut cursor = 0;
    while cursor < queue.len() {
        step(work, &mut cancelled)?;
        let index = queue[cursor];
        cursor += 1;
        for reference in &material.objects[index].references {
            let id = hash_id(reference, work, &mut cancelled)?;
            let index = find(ids, &id, work, &mut cancelled)?;
            if !reached[index] {
                reached[index] = true;
                queue.push(index);
            }
        }
    }
    if queue.len() != material.objects.len() {
        return Err(Error::Invalid("unreachable material object"));
    }
    let expected_material = hash_id(&body.frozen_material_digest, work, &mut cancelled)?;
    let mut stopped = false;
    let result = frozen_material_digest_bounded_v1(
        material,
        memory,
        &mut work.frozen_wire_bytes_remaining,
        || {
            if cancelled() {
                stopped = true;
                true
            } else {
                false
            }
        },
    );
    if stopped {
        return Err(Error::Cancelled);
    }
    let material_id = result.map_err(Error::Formal)?;
    if material_id != expected_material {
        return Err(Error::Invalid("signed Frozen material digest"));
    }
    let config_id = hash_id(
        &material.roots[&ArtifactRole::IncrementalGenesisConfig],
        work,
        &mut cancelled,
    )?;
    if config_id != hash_id(&body.world.genesis_digest, work, &mut cancelled)? {
        return Err(Error::Invalid("Frozen genesis role"));
    }
    let config_bytes = load(
        objects,
        &config_id,
        HISTORY_MAX_HEADER_BYTES,
        memory,
        work,
        &mut cancelled,
    )?;
    let config = GenesisConfigV1::from_canonical_bytes(&config_bytes).map_err(Error::Execution)?;
    let manifest_bytes = load(objects, &manifest_id, 1024, memory, work, &mut cancelled)?;
    let manifest =
        IwsExecutionManifestV1::from_canonical_bytes(&manifest_bytes).map_err(Error::Execution)?;
    let native_id = hash_id(
        &material.roots[&ArtifactRole::NativeExecutionEnvironment],
        work,
        &mut cancelled,
    )?;
    compare(
        work,
        config.world_id.len().max(body.world.world_id.len())
            + manifest.scope.world_id.len().max(config.world_id.len())
            + 32 * 7,
        &mut cancelled,
    )?;
    if config.world_id != body.world.world_id
        || manifest.scope.world_id != config.world_id
        || manifest.scope.genesis_config != config_id
        || manifest.format_profile != config.format_profile
        || manifest.native != native_id
        || config.execution_environment != native_id
        || manifest.wasm != hash_id(&material.roots[&ArtifactRole::Wasm], work, &mut cancelled)?
        || manifest.rules != hash_id(&material.roots[&ArtifactRole::Rules], work, &mut cancelled)?
        || manifest.policy.id != hash_id(&body.execution_policy_digest, work, &mut cancelled)?
    {
        return Err(Error::Invalid("Frozen config/profile/environment/policy"));
    }
    // Reuse the existing bounded native descriptor decoder/workspace contract;
    // no native chunk availability or executable lease is asserted here.
    let native_index = find(ids, &native_id, work, &mut cancelled)?;
    if !material.objects[native_index].references.is_empty() {
        return Err(Error::Invalid("native chunks are external dependencies"));
    }
    let _native_phase = memory
        .reserve(NATIVE_MATERIAL_WORKSPACE_BYTES, MemoryClass::Ordinary)
        .map_err(Error::Memory)?;
    let native_bytes = load(
        objects,
        &native_id,
        MAX_NATIVE_DESCRIPTOR_BYTES,
        memory,
        work,
        &mut cancelled,
    )?;
    take(
        &mut work.metadata_entries_remaining,
        (native_bytes.len() as u64)
            .checked_mul(4)
            .ok_or(Error::ResourceLimited("native metadata work"))?,
        "native metadata work",
    )?;
    take(
        &mut work.comparison_bytes_remaining,
        (native_bytes.len() as u64)
            .checked_mul(257)
            .ok_or(Error::ResourceLimited("native comparison work"))?,
        "native comparison work",
    )?;
    poll(&mut cancelled)?;
    let native = NativeExecutionEnvironmentV1::decode(&native_bytes)
        .map_err(|_| Error::Invalid("native canonical descriptor"))?;
    poll(&mut cancelled)?;
    let executable = native
        .components
        .iter()
        .find(|c| c.role == NativeEnvironmentRole::Executable)
        .ok_or(Error::Invalid("native executable component"))?;
    if hash_id(&executable.content_digest, work, &mut cancelled)? != manifest.executable {
        return Err(Error::Invalid("Frozen executable differs"));
    }
    let parent_body = hash_id(
        &material.roots[&ArtifactRole::IncrementalCommitBody],
        work,
        &mut cancelled,
    )?;
    let (continuation, input) = match manifest.mode {
        ExecutionManifestModeV1::Execution {
            parent_commit,
            parent_roots,
            operation,
            input,
            continuation,
        } if parent_commit == parent_body
            && parent_roots == hash_id(&body.pre_state_root, work, &mut cancelled)?
            && operation == hash_id(&body.request_digest, work, &mut cancelled)?
            && input == input_id =>
        {
            (continuation, input)
        }
        _ => {
            return Err(Error::Invalid(
                "Frozen execution manifest mode/parent/request",
            ));
        }
    };
    let input_index = find(ids, &input, work, &mut cancelled)?;
    let input = OriginalArtifactRefV1 {
        id: input,
        byte_len: material.objects[input_index].bytes.len() as u64,
    };
    for reference in [input, manifest.policy, continuation] {
        let index = find(ids, &reference.id, work, &mut cancelled)?;
        if material.objects[index].bytes.len() as u64 != reference.byte_len {
            return Err(Error::Invalid("declared original length differs"));
        }
        original(objects, reference, memory, work, &mut cancelled)?;
    }
    poll(&mut cancelled)?;
    let fee = memory
        .reserve(
            std::mem::size_of::<CheckedIncrementalFrozenMaterialV1<'a>>(),
            MemoryClass::Ordinary,
        )
        .map_err(Error::Memory)?;
    Ok(fee.retain(CheckedIncrementalFrozenMaterialV1 {
        reservation,
        parent,
        material,
        reservation_id,
        parent_id,
        material_id,
        input,
        manifest: manifest_id,
        policy: manifest.policy,
        continuation,
    }))
}
#[cfg(test)]
#[path = "incremental_frozen_material_tests.rs"]
mod tests;
