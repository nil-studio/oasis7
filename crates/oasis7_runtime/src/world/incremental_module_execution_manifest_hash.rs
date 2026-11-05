//! Actual World Manifest body -> original SHA256(JSON(Manifest)) data cache.
//! The cache grants no execution/acceptance authority. Every warm use first
//! opens the actual current owner and compares its complete immutable body root.
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::{ExecutionError, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    Manifest,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::JsonReadWorkV1,
    incremental_runtime_artifact_config::*,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    sync::Arc,
};
#[derive(Debug)]
pub enum RootWorldManifestHashErrorV1 {
    Formal(oasis7_formal_protocol::incremental_material::CoreError<std::convert::Infallible>),
    Value(ValueTreeError),
    Overlay(WorldFieldOverlayErrorV1),
    Execution(ExecutionError),
}
impl From<ValueTreeError> for RootWorldManifestHashErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl From<WorldFieldOverlayErrorV1> for RootWorldManifestHashErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Overlay(e)
    }
}
impl From<ExecutionError> for RootWorldManifestHashErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("root World Manifest hash cache")
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Scope {
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
}
struct Baseline {
    body: ReservedValue<ArtifactConfigBodyQualificationV1>,
    hash: String,
}
pub struct AcceptedWorldManifestHashV1 {
    baseline: Arc<ReservedValue<Baseline>>,
    scope: Scope,
    class: MemoryClass,
}
pub struct CurrentWorldManifestHashV1 {
    baseline: Arc<ReservedValue<Baseline>>,
    parent: Scope,
    guard: PreparedGuard,
}
impl AcceptedWorldManifestHashV1 {
    pub fn hash(&self) -> &str {
        &self.baseline.hash
    }
    pub fn body_root(&self) -> &StateRootV1 {
        self.baseline.body.root()
    }
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
}
impl CurrentWorldManifestHashV1 {
    pub fn hash(&self) -> &str {
        &self.baseline.hash
    }
    pub fn body_root(&self) -> &StateRootV1 {
        self.baseline.body.root()
    }
}
pub struct RootWorldManifestHashWorkV1 {
    pub body_scan: StateScanBudget,
    pub fields: ArtifactConfigReadWorkV1,
    pub json: JsonReadWorkV1,
    /// Includes an attempt debit for every writer call, even an empty write.
    pub hash_bytes_remaining: u64,
    pub max_json_heap_bytes: usize,
}
fn address(column: &'static str) -> ArtifactConfigAddressV1<'static> {
    ArtifactConfigAddressV1 {
        section: "manifest",
        ordinal: None,
        inner_ordinal: None,
        member: None,
        column,
    }
}
fn scope(
    parent: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<Scope, RootWorldManifestHashErrorV1> {
    let anchor = parent.occurrence();
    let roots_id = roots.digest()?;
    if !parent.uses_budget(memory)
        || parent.allocation_class() != class
        || anchor.head_record_id() != head.verified_record_id()
        || anchor.head_body_id() != anchor.fact_body_id()
        || anchor.roots_id() != roots_id
        || anchor.location().height != roots.height
    {
        return Err(invalid().into());
    }
    Ok(Scope {
        record: anchor.head_record_id(),
        body: anchor.head_body_id(),
        roots: roots_id,
    })
}
struct Hash<'a, C> {
    hash: Sha256,
    left: &'a mut u64,
    cancelled: &'a mut C,
    error: Option<ValueTreeError>,
}
impl<C: FnMut() -> bool> Write for Hash<'_, C> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.error.is_some() {
            return Err(io::Error::other("Manifest hash stopped"));
        }
        if (self.cancelled)() {
            self.error = Some(ValueTreeError::Cancelled);
            return Err(io::Error::other("Manifest hash cancelled"));
        }
        let b = &b[..b.len().min(64 << 10)];
        let cost = (b.len() as u64)
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Manifest hash work"))?;
        let Some(left) = self.left.checked_sub(cost) else {
            self.error = Some(ValueTreeError::ResourceLimited("Manifest JSON hash bytes"));
            return Err(io::Error::other("Manifest hash work"));
        };
        *self.left = left;
        self.hash.update(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.error.is_some() {
            Err(io::Error::other("Manifest hash stopped"))
        } else {
            Ok(())
        }
    }
}
/// Exact existing Manifest serializer, streaming into SHA instead of making an
/// encoded JSON Vec. One selected JSON DTO is admitted by its actual reader;
/// its temporary clone into the ORIGINAL Manifest type is independently paid.
fn hash_body<M, R, E, C>(
    body: &ReservedValue<ArtifactConfigBodyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RootWorldManifestHashWorkV1,
) -> Result<String, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    if body.kind() != ArtifactConfigBodyKindV1::Manifest
        || body.root() != ctx.root
        || !body.uses_budget(ctx.values.memory)
        || body.allocation_class() != ctx.values.class
    {
        return Err(invalid());
    }
    oasis7_world_state::incremental_world_event_decode::verify_module_output_layout_inventory_v1(
        ctx.values.memory,
        ctx.values.class,
    )?;
    let version = inspect_artifact_config_cell_v1(
        ArtifactConfigBodyKindV1::Manifest,
        address("version"),
        8,
        ctx,
        &mut io::sink(),
    )?;
    let ArtifactConfigValueV1::Unsigned(version) = *version else {
        return Err(invalid());
    };
    let content = open_artifact_config_json_v1(body, address("content"), ctx, &mut work.json)?;
    let json = materialize_artifact_config_json_v1(
        &content,
        ctx,
        &mut work.json,
        work.max_json_heap_bytes,
    )?;
    let fee = json
        .reserved_bytes()
        .checked_mul(2)
        .and_then(|n| n.checked_add(std::mem::size_of::<Manifest>() + 8192))
        .ok_or(ValueTreeError::ResourceLimited("Manifest clone heap"))?;
    let _clone = ctx.values.memory.reserve(fee, ctx.values.class)?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let actual = Manifest {
        version,
        content: (*json).clone(),
    };
    let mut sink = Hash {
        hash: Sha256::new(),
        left: &mut work.hash_bytes_remaining,
        cancelled: &mut ctx.cancelled,
        error: None,
    };
    let encoded = serde_json::to_writer(&mut sink, &actual);
    if let Some(error) = sink.error.take() {
        return Err(error);
    }
    encoded.map_err(|_| invalid())?;
    let hash = sink.hash.finalize();
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(hex::encode(hash))
}
fn baseline<M, R, E, C>(
    body: ReservedValue<ArtifactConfigBodyQualificationV1>,
    state: &M,
    values: &mut ValueReadContext<'_, R>,
    keys: &mut oasis7_world_state::incremental_semantic_key::SemanticKeyBudget,
    semantic_bytes: u64,
    record_bytes: u64,
    error: E,
    cancelled: C,
    work: &mut RootWorldManifestHashWorkV1,
) -> Result<Arc<ReservedValue<Baseline>>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let charge = values.memory.reserve(1024, values.class)?;
    let mut ctx = WorldFieldReadContextV1 {
        root: body.root(),
        state,
        values: ValueReadContext {
            source: values.source,
            memory: values.memory,
            class: values.class,
            io: &mut *values.io,
        },
        key_work: keys,
        semantic_bytes,
        record_bytes,
        take_state_error: error,
        cancelled,
    };
    let hash = hash_body(&body, &mut ctx, work)?;
    drop(ctx);
    Ok(Arc::new(charge.retain(Baseline { body, hash })))
}
/// GEN/cold point recovery, bound to a genuine accepted Runtime selected Fact.
/// Only this selected Manifest is read; no World or History is reconstructed.
#[expect(
    clippy::too_many_arguments,
    reason = "Actual accepted scope, typed owner and bounded readers are separate obligations."
)]
pub fn recover_world_manifest_hash_v1<M, R, E, C>(
    parent: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    family: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RootWorldManifestHashWorkV1,
) -> Result<ReservedValue<AcceptedWorldManifestHashV1>, RootWorldManifestHashErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let class = ctx.values.class;
    let charge = ctx
        .values
        .memory
        .reserve(8192, class)
        .map_err(ValueTreeError::Memory)?;
    let parent_scope = scope(parent, head, roots, ctx.values.memory, class)?;
    let body = open_qualified_artifact_config_body_v1(
        ArtifactConfigFieldV1::Manifest,
        ArtifactConfigRecordKeyV1::Global,
        roots,
        family,
        ctx,
    )?
    .ok_or_else(invalid)?;
    let value = baseline(
        body,
        ctx.state,
        &mut ctx.values,
        ctx.key_work,
        ctx.semantic_bytes,
        ctx.record_bytes,
        &mut ctx.take_state_error,
        &mut ctx.cancelled,
        work,
    )?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(charge.retain(AcceptedWorldManifestHashV1 {
        baseline: value,
        scope: parent_scope,
        class,
    }))
}
/// Actual same-TX current owner open. A warm unchanged body uses only fixed
/// owner metadata; every changed root is re-read and re-hashed under budget.
#[expect(
    clippy::too_many_arguments,
    reason = "The actual transaction, immutable body reader and accepted parent remain explicit."
)]
pub fn prepare_current_world_manifest_hash_v1<M, R, E, C>(
    parent: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    cache: &ReservedValue<AcceptedWorldManifestHashV1>,
    current: Option<&ReservedValue<CurrentWorldManifestHashV1>>,
    family: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut RootWorldManifestHashWorkV1,
) -> Result<ReservedValue<CurrentWorldManifestHashV1>, RootWorldManifestHashErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let memory = ctx.values.memory;
    let charge = memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let parent_scope = scope(parent, head, roots, memory, MemoryClass::Ordinary)?;
    if !cache.uses_budget(memory)
        || cache.class != MemoryClass::Ordinary
        || cache.scope != parent_scope
        || ctx.values.class != MemoryClass::Ordinary
        || ctx.transaction.parent_fence().parent_roots != parent_scope.roots
        || ctx.transaction.parent_fence().parent_commit != parent_scope.body
    {
        return Err(invalid().into());
    }
    let previous = if let Some(current) = current {
        if !current.uses_budget(memory) || current.parent != parent_scope {
            return Err(invalid().into());
        }
        ctx.transaction
            .validate_transaction_identity(&current.guard)?;
        &current.baseline
    } else {
        &cache.baseline
    };
    let body = open_artifact_config_body_overlay_v1(
        ArtifactConfigSelectedBodyRequestV1 {
            field: ArtifactConfigFieldV1::Manifest,
            key: ArtifactConfigRecordKeyV1::Global,
            family,
            parent_roots: roots,
            current: Some(&previous.body),
        },
        state,
        ctx,
        &mut work.body_scan,
        &mut work.fields,
    )?
    .ok_or_else(invalid)?;
    let value = if body.root() == previous.body.root() {
        previous.clone()
    } else {
        baseline(
            body,
            state,
            &mut ctx.values,
            ctx.key_work,
            ctx.semantic_bytes,
            ctx.record_bytes,
            &mut ctx.take_state_error,
            &mut ctx.cancelled,
            work,
        )?
    };
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(charge.retain(CurrentWorldManifestHashV1 {
        baseline: value,
        parent: parent_scope,
        guard: ctx.transaction.guard(),
    }))
}
/// Recovery A allocates fresh Ordinary body qualification and string/carriers;
/// no Recovery Arc is retained, and the true accepted parent is checked again.
#[expect(
    clippy::too_many_arguments,
    reason = "A retains actual accepted scope rather than caller-reported parent IDs."
)]
pub fn readmit_world_manifest_hash_ordinary_v1(
    source: &ReservedValue<AcceptedWorldManifestHashV1>,
    parent: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AcceptedWorldManifestHashV1>, RootWorldManifestHashErrorV1> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let charge = memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let parent_scope = scope(parent, head, roots, memory, MemoryClass::Ordinary)?;
    if !source.uses_budget(memory)
        || source.class != MemoryClass::Recovery
        || source.scope != parent_scope
        || !source.baseline.uses_budget(memory)
    {
        return Err(invalid().into());
    }
    let base_charge = memory
        .reserve(1024, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let body = readmit_artifact_config_body_ordinary_v1(
        &source.baseline.body,
        ArtifactConfigBodyKindV1::Manifest,
        source.baseline.body.root(),
        memory,
        &mut cancelled,
    )?;
    let value = Arc::new(base_charge.retain(Baseline {
        body,
        hash: source.baseline.hash.clone(),
    }));
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(charge.retain(AcceptedWorldManifestHashV1 {
        baseline: value,
        scope: parent_scope,
        class: MemoryClass::Ordinary,
    }))
}

#[path = "incremental_module_execution_manifest_hash_promotion.rs"]
mod promotion;
pub use promotion::*;
