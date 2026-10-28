//! Read-back verification of one IWS commit's structural core. This is not
//! complete execution/value/artifact closure, durability or head qualification.
use crate::{
    controlled_authority::{
        formal::{
            self, VerifiedAuthorityV1, VerifiedCandidateContextV1, VerifiedFormalCommitV1,
            incremental::{self, BindingError, CommitMaterialRefs},
        },
        replicated_protocol::ArtifactRole,
    },
    incremental_execution::{
        EXECUTION_MAX_RECORD_BYTES, ExecutionError, ExecutionFactKindV1, ExecutionParent,
        RootSetV1, TransitionRecordV1, TransitionVerifier,
    },
    incremental_history::*,
    incremental_state::{MerkleNodeReader, STATE_MAX_PAGE_BYTES},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[path = "incremental_fact_material.rs"]
mod fact_material;
pub use fact_material::*;
#[path = "incremental_written_values.rs"]
mod written_values;
pub use written_values::*;
#[path = "incremental_native_material.rs"]
mod native_material;
pub use native_material::*;
#[path = "incremental_execution_artifact_material.rs"]
mod execution_artifacts;
pub use execution_artifacts::*;

use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

/// Reader retains admission until the last Arc. Errors are not absence; full
/// original bytes and their flat digest must be checked, not locator identity.
pub trait OriginalObjectReader {
    type Error;
    fn allocation_budget(&self) -> &MemoryBudget;
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error>;
}
#[derive(Debug)]
pub enum CoreError<E> {
    Object(E),
    Artifact(&'static str),
    Execution(ExecutionError),
    Binding(BindingError),
    Invalid(&'static str),
    ResourceLimited(&'static str),
    Cancelled,
}
impl<E> From<ExecutionError> for CoreError<E> {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}
impl<E> From<BindingError> for CoreError<E> {
    fn from(error: BindingError) -> Self {
        Self::Binding(error)
    }
}
impl<E: std::fmt::Display> std::fmt::Display for CoreError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Object(e) => e.fmt(f),
            Self::Artifact(e) => write!(f, "invalid IWS execution artifact: {e}"),
            Self::Execution(e) => e.fmt(f),
            Self::Binding(e) => e.fmt(f),
            Self::Invalid(s) => write!(f, "invalid IWS core: {s}"),
            Self::ResourceLimited(s) => write!(f, "IWS core resource limited: {s}"),
            Self::Cancelled => f.write_str("IWS core check cancelled"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for CoreError<E> {}

/// Failed reads spend the admitted allowance; success refunds unused bound.
/// Physical IO/decompression quotas remain with the actual source sessions.
pub struct CoreBudget {
    pub objects_remaining: u64,
    pub original_bytes_remaining: u64,
    pub new_pages_remaining: u64,
    pub merkle_reads_remaining: u64,
}
pub struct CoreRequest<'a> {
    pub candidate: &'a VerifiedCandidateContextV1,
    pub original_authority: &'a VerifiedAuthorityV1,
    pub parent: Option<&'a VerifiedFormalCommitV1>,
    pub max_pending_node_bytes: usize,
}

/// Exact original record role references. This is no closure/acceptance proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionArtifactRefs {
    pub manifest: [u8; 32],
    pub wasm: [u8; 32],
    pub rules: [u8; 32],
    pub native: [u8; 32],
}
/// Only these structural objects/edges have been checked. No receipt builder
/// consumes this type. Referenced facts, state values, initial state, executable
/// closure and business legality remain mandatory full-verifier obligations.
#[derive(Debug)]
pub struct CheckedCommitCore {
    execution_artifacts: ExecutionArtifactRefs,
    original_input: [u8; 32],
    original_execution_policy: [u8; 32],
    config: GenesisConfigV1,
    roots: RootSetV1,
    body: CommitBodyV1,
    history: HistoryPrefixV1,
    segment: HistorySegmentV1,
    checkpoint: Option<CheckpointDescriptorV1>,
    facts: ReservedValue<FactSetV1>,
    transition: Option<ReservedValue<TransitionRecordV1>>,
    catalog: ReservedValue<CatalogDeltaV1>,
}

impl CheckedCommitCore {
    pub fn original_execution_policy_id(&self) -> [u8; 32] {
        self.original_execution_policy
    }
    pub fn original_input_id(&self) -> [u8; 32] {
        self.original_input
    }
    pub fn execution_artifacts(&self) -> ExecutionArtifactRefs {
        self.execution_artifacts
    }
    pub fn config(&self) -> &GenesisConfigV1 {
        &self.config
    }
    pub fn roots(&self) -> &RootSetV1 {
        &self.roots
    }
    pub fn body(&self) -> &CommitBodyV1 {
        &self.body
    }
    pub fn history(&self) -> &HistoryPrefixV1 {
        &self.history
    }
    pub fn segment(&self) -> &HistorySegmentV1 {
        &self.segment
    }
    pub fn checkpoint(&self) -> Option<&CheckpointDescriptorV1> {
        self.checkpoint.as_ref()
    }
    pub fn facts(&self) -> &FactSetV1 {
        &self.facts
    }
    pub fn transition(&self) -> Option<&TransitionRecordV1> {
        self.transition.as_deref()
    }
    pub fn catalog(&self) -> &CatalogDeltaV1 {
        &self.catalog
    }
}

fn role<E>(
    record: &formal::CanonicalRecordV1,
    role: ArtifactRole,
) -> Result<[u8; 32], CoreError<E>> {
    let value = record
        .artifact_roots
        .get(&role)
        .ok_or(CoreError::Invalid("required original root role"))?;
    hex::decode(value)
        .map_err(|_| CoreError::Invalid("original root ID"))?
        .try_into()
        .map_err(|_| CoreError::Invalid("original root width"))
}
fn load<O: OriginalObjectReader>(
    objects: &mut O,
    id: &[u8; 32],
    bound: usize,
    memory: &MemoryBudget,
    budget: &mut CoreBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Arc<ReservedValue<Vec<u8>>>, CoreError<O::Error>> {
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    if budget.objects_remaining == 0 || budget.original_bytes_remaining == 0 {
        return Err(CoreError::ResourceLimited("original object allowance"));
    }
    let admitted = bound.min(budget.original_bytes_remaining.min(usize::MAX as u64) as usize);
    budget.objects_remaining -= 1;
    budget.original_bytes_remaining -= admitted as u64;
    let bytes = objects
        .read_original(id, admitted, memory)
        .map_err(CoreError::Object)?;
    if !bytes.uses_budget(memory)
        || bytes.capacity() > bytes.reserved_bytes()
        || bytes.len() > admitted
        || blake3::hash(&bytes).as_bytes() != id
    {
        return Err(CoreError::Invalid("original object admission/identity"));
    }
    budget.original_bytes_remaining += (admitted - bytes.len()) as u64;
    Ok(bytes)
}
#[derive(Clone, Copy)]
enum Stop {
    Cancelled,
    Limited,
}
struct GuardedPages<'a, R, C> {
    source: &'a R,
    remaining: &'a Cell<u64>,
    cancelled: RefCell<&'a mut C>,
    stop: Cell<Option<Stop>>,
}
impl<R: MerkleNodeReader, C: FnMut() -> bool> MerkleNodeReader for GuardedPages<'_, R, C> {
    type Node = R::Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if (self.cancelled.borrow_mut())() {
            self.stop.set(Some(Stop::Cancelled));
            return Err("core page walk cancelled".into());
        }
        if self.remaining.get() == 0 {
            self.stop.set(Some(Stop::Limited));
            return Err("core page walk allowance".into());
        }
        self.remaining.set(self.remaining.get() - 1);
        self.source.read_node(id, max)
    }
}
fn with_pages<E, R: MerkleNodeReader, C: FnMut() -> bool, T>(
    source: &R,
    budget: &mut CoreBudget,
    cancelled: &mut C,
    phase: impl FnOnce(&GuardedPages<'_, R, C>, &mut u64) -> Result<T, CoreError<E>>,
) -> Result<T, CoreError<E>> {
    let CoreBudget {
        merkle_reads_remaining,
        new_pages_remaining,
        ..
    } = budget;
    // The cell updates the caller's actual counter, even if a source unwinds.
    let guard = GuardedPages {
        source,
        remaining: Cell::from_mut(merkle_reads_remaining),
        cancelled: RefCell::new(cancelled),
        stop: Cell::new(None),
    };
    let result = phase(&guard, new_pages_remaining);
    match guard.stop.get() {
        Some(Stop::Cancelled) => Err(CoreError::Cancelled),
        Some(Stop::Limited) => Err(CoreError::ResourceLimited("Merkle read allowance")),
        None => result,
    }
}
fn check_pages<'a, E, R: MerkleNodeReader>(
    pages: impl Iterator<Item = (&'a String, &'a Vec<u8>)>,
    source: &R,
    bound: usize,
    remaining: &mut u64,
) -> Result<(), CoreError<E>> {
    for (id, expected) in pages {
        if *remaining == 0 {
            return Err(CoreError::ResourceLimited("new page allowance"));
        }
        *remaining -= 1;
        let bytes = source
            .read_node(id, bound)
            .map_err(ExecutionError::Tree)?
            .ok_or(CoreError::Invalid("candidate new page missing"))?;
        if bytes.as_ref() != expected {
            return Err(CoreError::Invalid("candidate new page differs"));
        }
    }
    Ok(())
}

/// Live candidate checking uses ordinary budget; recovery admission is a
/// separate path until all reused producer/decoder phases support that class.
pub fn verify_commit_core<O: OriginalObjectReader, R: MerkleNodeReader>(
    request: &CoreRequest<'_>,
    objects: &mut O,
    pages: &R,
    memory: &MemoryBudget,
    budget: &mut CoreBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedCommitCore>, CoreError<O::Error>> {
    let charge = memory
        .reserve(128 * 1024, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    if !objects.allocation_budget().same_pool(memory)
        || pages
            .allocation_budget()
            .is_none_or(|pool| !pool.same_pool(memory))
        || pages.allocation_class() != Some(MemoryClass::Ordinary)
    {
        return Err(CoreError::Invalid("core source memory pools"));
    }
    let record = request.candidate.record();
    record.validate().map_err(BindingError::from)?;
    if !incremental::is_iws_record(record.schema_version) {
        return Err(CoreError::Invalid("IWS original record required"));
    }
    let config = GenesisConfigV1::from_canonical_bytes(&load(
        objects,
        &role(record, ArtifactRole::IncrementalGenesisConfig)?,
        HISTORY_MAX_HEADER_BYTES,
        memory,
        budget,
        &mut cancelled,
    )?)?;
    let roots = RootSetV1::from_canonical_bytes(&load(
        objects,
        &role(record, ArtifactRole::IncrementalRootSet)?,
        1024,
        memory,
        budget,
        &mut cancelled,
    )?)?;
    let body = CommitBodyV1::from_canonical_bytes(&load(
        objects,
        &role(record, ArtifactRole::IncrementalCommitBody)?,
        HISTORY_MAX_HEADER_BYTES,
        memory,
        budget,
        &mut cancelled,
    )?)?;
    let history = HistoryPrefixV1::from_canonical_bytes(&load(
        objects,
        &role(record, ArtifactRole::IncrementalHistoryPrefix)?,
        HISTORY_MAX_HEADER_BYTES,
        memory,
        budget,
        &mut cancelled,
    )?)?;
    incremental::verify_commit_binding(
        record,
        request.original_authority,
        request.parent,
        CommitMaterialRefs {
            config: &config,
            roots: &roots,
            body: &body,
            history: &history,
        },
        memory,
    )?;
    let facts = FactSetV1::from_canonical_bytes(
        &load(
            objects,
            &body.facts,
            256 * 1024,
            memory,
            budget,
            &mut cancelled,
        )?,
        memory,
    )?;
    let input_id = role(record, ArtifactRole::Input)?;
    if !facts
        .facts
        .iter()
        .any(|fact| fact.kind == ExecutionFactKindV1::Input && fact.id == input_id)
    {
        return Err(CoreError::Invalid("original input absent from facts"));
    }
    let segment = HistorySegmentV1::from_canonical_bytes(&load(
        objects,
        &history.segment,
        HISTORY_MAX_HEADER_BYTES,
        memory,
        budget,
        &mut cancelled,
    )?)?;
    let checkpoint = body
        .checkpoint
        .map(|id| -> Result<_, CoreError<O::Error>> {
            Ok(CheckpointDescriptorV1::from_canonical_bytes(&load(
                objects,
                &id,
                HISTORY_MAX_HEADER_BYTES,
                memory,
                budget,
                &mut cancelled,
            )?)?)
        })
        .transpose()?;
    let transition = body
        .transition
        .map(|id| -> Result<_, CoreError<O::Error>> {
            Ok(TransitionRecordV1::from_canonical_bytes(
                &load(
                    objects,
                    &id,
                    EXECUTION_MAX_RECORD_BYTES,
                    memory,
                    budget,
                    &mut cancelled,
                )?,
                memory,
                MemoryClass::Ordinary,
            )?)
        })
        .transpose()?;
    let parent = if let Some(parent) = request.parent {
        let parent_record = parent.record();
        let parent_body = CommitBodyV1::from_canonical_bytes(&load(
            objects,
            &role(parent_record, ArtifactRole::IncrementalCommitBody)?,
            HISTORY_MAX_HEADER_BYTES,
            memory,
            budget,
            &mut cancelled,
        )?)?;
        let parent_history = HistoryPrefixV1::from_canonical_bytes(&load(
            objects,
            &role(parent_record, ArtifactRole::IncrementalHistoryPrefix)?,
            HISTORY_MAX_HEADER_BYTES,
            memory,
            budget,
            &mut cancelled,
        )?)?;
        let parent_roots = RootSetV1::from_canonical_bytes(&load(
            objects,
            &role(parent_record, ArtifactRole::IncrementalRootSet)?,
            1024,
            memory,
            budget,
            &mut cancelled,
        )?)?;
        if parent_body.roots != parent_roots.digest()?
            || hex::encode(parent_body.roots) != parent_record.after_state_root
            || parent_history.digest()? != parent_body.history
        {
            return Err(CoreError::Invalid("parent original core binding"));
        }
        let extension = with_pages(pages, budget, &mut cancelled, |source, _| {
            Ok(parent_body.catalog_before.extend(
                &parent_body,
                source,
                memory,
                request.max_pending_node_bytes,
            )?)
        })?;
        if extension.after != body.catalog_before {
            return Err(CoreError::Invalid("parent certified catalog extension"));
        }
        Some((parent_body, parent_history, parent_roots))
    } else {
        None
    };
    let expected = HeaderPreparation {
        config: &config,
        catalog: &body.catalog_before,
        roots: &roots,
        parent: parent.as_ref().map(|p| &p.0),
        parent_history: parent.as_ref().map(|p| &p.1),
        transition: transition.as_deref(),
        genesis_facts: if transition.is_none() {
            &facts.facts
        } else {
            &[]
        },
        operation: body.operation,
        authority: body.authority,
        execution_environment: body.execution_environment,
        checkpoint: body.checkpoint.is_some(),
    }
    .prepare(memory)?;
    if expected.body != body
        || expected.facts != *facts
        || expected.segment != segment
        || expected.prefix != history
        || expected.checkpoint != checkpoint
    {
        return Err(CoreError::Invalid(
            "candidate core does not reproduce producer",
        ));
    }
    drop(expected);
    if let Some(transition) = &transition {
        let parent = parent
            .as_ref()
            .ok_or(CoreError::Invalid("transition parent core missing"))?;
        let fixed = ExecutionParent {
            commit: parent.0.digest()?,
            roots: parent.2.clone(),
            authority: body.authority,
        };
        with_pages(pages, budget, &mut cancelled, |source, remaining| {
            let rebuilt = TransitionVerifier {
                parent: &fixed,
                operation: body.operation,
                source,
                memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: request.max_pending_node_bytes,
            }
            .verify(transition)?;
            check_pages(
                rebuilt
                    .new_nodes
                    .iter()
                    .map(|node| (&node.digest, &node.bytes)),
                source,
                STATE_MAX_PAGE_BYTES,
                remaining,
            )
        })?;
    }
    let catalog = with_pages(pages, budget, &mut cancelled, |source, remaining| {
        let catalog =
            body.catalog_before
                .extend(&body, source, memory, request.max_pending_node_bytes)?;
        check_pages(
            catalog
                .new_nodes
                .iter()
                .map(|node| (&node.digest, &node.bytes)),
            source,
            CATALOG_MAX_NODE_BYTES,
            remaining,
        )?;
        Ok(catalog)
    })?;
    let descriptor = catalog.after.canonical_bytes()?;
    let id = *blake3::hash(&descriptor).as_bytes();
    if load(
        objects,
        &id,
        HISTORY_MAX_HEADER_BYTES,
        memory,
        budget,
        &mut cancelled,
    )?
    .as_slice()
        != descriptor
    {
        return Err(CoreError::Invalid("candidate catalog descriptor"));
    }
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    let execution_artifacts = ExecutionArtifactRefs {
        manifest: role(record, ArtifactRole::ExecutionManifest)?,
        wasm: role(record, ArtifactRole::Wasm)?,
        rules: role(record, ArtifactRole::Rules)?,
        native: role(record, ArtifactRole::NativeExecutionEnvironment)?,
    };
    Ok(charge.retain(CheckedCommitCore {
        execution_artifacts,
        original_input: input_id,
        original_execution_policy: crate::incremental_history::wire::parse_hash(
            &record.execution_policy_digest,
        )?,
        config,
        roots,
        body,
        history,
        segment,
        checkpoint,
        facts,
        transition,
        catalog,
    }))
}

#[cfg(test)]
#[path = "incremental_material_tests.rs"]
mod tests;
