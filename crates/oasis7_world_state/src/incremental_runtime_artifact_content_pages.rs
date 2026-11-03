//! Complete logical state-page set plus separately typed raw-original edges.
//! Copying these state pages does not copy the external originals or finish a
//! buffered target. No target-durability/World-GC capability is exposed.
use super::*;
use crate::{
    incremental_family_page_tracking::{NoSink, State, Tracking, TrackingLabels, Values},
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_material::OriginalObjectReader,
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext},
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
const LABELS: TrackingLabels = TrackingLabels {
    allowance: "artifact-content page metadata",
    hash: "artifact-content transfer hash",
    page_limit: "artifact-content pages",
    state_bound: "artifact-content state page bound",
    state_digest: "artifact-content state digest",
    state_tracking: "artifact-content state tracking",
    value_admission: "artifact-content value admission",
};
pub struct RuntimeArtifactContentPagesV1 {
    pages: ReservedValue<RefCell<BTreeSet<[u8; 32]>>>,
    originals: ReservedValue<RefCell<BTreeMap<[u8; 32], u64>>>,
    qualification: ReservedValue<RuntimeArtifactContentQualificationV1>,
}
impl RuntimeArtifactContentPagesV1 {
    pub fn page_count(&self) -> usize {
        self.pages.borrow().len()
    }
    pub fn original_count(&self) -> usize {
        self.originals.borrow().len()
    }
    pub fn qualification(&self) -> &ReservedValue<RuntimeArtifactContentQualificationV1> {
        &self.qualification
    }
    pub fn visit_pages<E>(
        &self,
        mut visitor: impl FnMut(&[u8; 32]) -> Result<(), E>,
    ) -> Result<(), E> {
        for id in self.pages.borrow().iter() {
            visitor(id)?;
        }
        Ok(())
    }
    pub fn visit_originals<E>(
        &self,
        mut visitor: impl FnMut(OriginalArtifactRefV1) -> Result<(), E>,
    ) -> Result<(), E> {
        for (id, byte_len) in self.originals.borrow().iter() {
            visitor(OriginalArtifactRefV1 {
                id: *id,
                byte_len: *byte_len,
            })?;
        }
        Ok(())
    }
}
pub struct RuntimeArtifactContentPagesWorkV1<'a> {
    pub outer_scan: &'a mut StateScanBudget,
    pub raw: &'a mut RuntimeArtifactOriginalWorkV1,
    pub max_pages: usize,
    pub max_originals: usize,
}
enum ObservedError<E> {
    Source(E),
    Collection(ValueTreeError),
}
struct Observed<'a, O> {
    source: &'a mut O,
    seen: &'a RefCell<BTreeMap<[u8; 32], u64>>,
    max: usize,
}
impl<O: OriginalObjectStreamReader> OriginalObjectReader for Observed<'_, O> {
    type Error = ObservedError<O::Error>;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.source.allocation_budget()
    }
    fn allocation_class(&self) -> MemoryClass {
        self.source.allocation_class()
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        self.source
            .read_original(id, max, memory)
            .map_err(ObservedError::Source)
    }
}
impl<O: OriginalObjectStreamReader> OriginalObjectStreamReader for Observed<'_, O> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        memory: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        if !self.seen.borrow().contains_key(id) && self.seen.borrow().len() >= self.max {
            return Err(ObservedError::Collection(ValueTreeError::ResourceLimited(
                "artifact-content original set",
            )));
        }
        let length = self
            .source
            .stream_original(id, max, memory, output)
            .map_err(ObservedError::Source)?;
        if self.seen.borrow().get(id).is_some_and(|old| *old != length) {
            return Err(ObservedError::Collection(ValueTreeError::Invalid(
                "artifact-content original length alias",
            )));
        }
        self.seen.borrow_mut().insert(*id, length);
        Ok(length)
    }
}
fn unwrap_error<E>(
    error: RuntimeArtifactOriginalErrorV1<ObservedError<E>>,
) -> RuntimeArtifactOriginalErrorV1<E> {
    match error {
        RuntimeArtifactOriginalErrorV1::Execution(e) => {
            RuntimeArtifactOriginalErrorV1::Execution(e)
        }
        RuntimeArtifactOriginalErrorV1::Value(e) => RuntimeArtifactOriginalErrorV1::Value(e),
        RuntimeArtifactOriginalErrorV1::Source(ObservedError::Source(e)) => {
            RuntimeArtifactOriginalErrorV1::Source(e)
        }
        RuntimeArtifactOriginalErrorV1::Source(ObservedError::Collection(e)) => {
            RuntimeArtifactOriginalErrorV1::Value(e)
        }
    }
}
fn collect<M, R, E, C, O, W>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    originals: &mut O,
    work: &mut RuntimeArtifactContentPagesWorkV1<'_>,
    sink: Option<&mut W>,
) -> Result<RuntimeArtifactContentPagesV1, RuntimeArtifactOriginalErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
    W: ValueNodeWriter,
{
    ctx.check()?;
    let bytes = work
        .max_originals
        .checked_mul(256)
        .and_then(|n| n.checked_add(4096))
        .ok_or(ValueTreeError::ResourceLimited(
            "artifact-content original metadata",
        ))?;
    let seen = ctx
        .values
        .memory
        .reserve(bytes, ctx.values.class)
        .map_err(ValueTreeError::Memory)?
        .retain(RefCell::new(BTreeMap::new()));
    let tracking = RefCell::new(Tracking::new(
        ctx.values.memory,
        ctx.values.class,
        work.max_pages,
        sink,
        LABELS,
    )?);
    let state = State {
        source: ctx.state,
        tracking: &tracking,
        memory: ctx.values.memory,
    };
    let values = Values {
        source: ctx.values.source,
        tracking: &tracking,
    };
    let qualification = {
        let mut observed = Observed {
            source: originals,
            seen: &seen,
            max: work.max_originals,
        };
        qualify_runtime_artifact_content_v1(
            roots,
            &mut WorldFieldReadContextV1 {
                root: ctx.root,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: || {
                    tracking
                        .borrow_mut()
                        .error
                        .take()
                        .or_else(&mut ctx.take_state_error)
                },
                cancelled: &mut ctx.cancelled,
            },
            work.outer_scan,
            &mut observed,
            work.raw,
        )
        .map_err(unwrap_error)?
    };
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if let Some(error) = tracking.borrow_mut().error.take() {
        return Err(error.into());
    }
    Ok(RuntimeArtifactContentPagesV1 {
        pages: tracking.into_inner().pages,
        originals: seen,
        qualification,
    })
}
pub fn mark_runtime_artifact_content_pages_v1<M, R, E, C, O>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    originals: &mut O,
    work: &mut RuntimeArtifactContentPagesWorkV1<'_>,
) -> Result<RuntimeArtifactContentPagesV1, RuntimeArtifactOriginalErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
{
    collect::<M, R, E, C, O, NoSink>(roots, ctx, originals, work, None)
}
/// Transfer state/value pages only. Caller must transfer visit_originals()
/// separately and finish every target before adopting a complete mirror.
pub fn transfer_runtime_artifact_content_state_pages_v1<M, R, E, C, O, W>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    originals: &mut O,
    work: &mut RuntimeArtifactContentPagesWorkV1<'_>,
    sink: &mut W,
) -> Result<RuntimeArtifactContentPagesV1, RuntimeArtifactOriginalErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    O: OriginalObjectStreamReader,
    W: ValueNodeWriter,
{
    collect(roots, ctx, originals, work, Some(sink))
}
