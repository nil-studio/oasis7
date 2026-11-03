//! Typed configuration state/value pages. This does not transfer WASM originals
//! or finish a buffered target, and never grants complete World GC authority.
use super::*;
use crate::{
    incremental_family_page_tracking::{NoSink, State, Tracking, TrackingLabels, Values},
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{cell::RefCell, collections::BTreeSet};

const LABELS: TrackingLabels = TrackingLabels {
    allowance: "artifact config page metadata",
    hash: "artifact config page hash",
    page_limit: "artifact config pages",
    state_bound: "artifact config state bound",
    state_digest: "artifact config state digest",
    state_tracking: "artifact config state tracking",
    value_admission: "artifact config value admission",
};
pub struct ArtifactConfigPagesV1 {
    pages: ReservedValue<RefCell<BTreeSet<[u8; 32]>>>,
    qualification: ReservedValue<ArtifactConfigFamilyQualificationV1>,
}
impl ArtifactConfigPagesV1 {
    pub fn page_count(&self) -> usize {
        self.pages.borrow().len()
    }
    pub fn qualification(&self) -> &ReservedValue<ArtifactConfigFamilyQualificationV1> {
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
}
pub struct ArtifactConfigPagesWorkV1<'a> {
    pub outer_scan: &'a mut StateScanBudget,
    pub inner_scan: &'a mut StateScanBudget,
    pub body: &'a mut ArtifactConfigReadWorkV1,
    pub max_pages: usize,
}
fn collect<M, R, E, C, W>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ArtifactConfigPagesWorkV1<'_>,
    sink: Option<&mut W>,
) -> Result<ArtifactConfigPagesV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
{
    ctx.check()?;
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
    let qualification = qualify_artifact_config_family_v1(
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
        work.inner_scan,
        work.body,
    )?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = tracking.borrow_mut().error.take() {
        return Err(error);
    }
    Ok(ArtifactConfigPagesV1 {
        pages: tracking.into_inner().pages,
        qualification,
    })
}
pub fn mark_artifact_config_pages_v1<M, R, E, C>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ArtifactConfigPagesWorkV1<'_>,
) -> Result<ArtifactConfigPagesV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    collect::<M, R, E, C, NoSink>(roots, ctx, work, None)
}
/// Provisional state/value transfer only; the caller must finish every target
/// and establish its complete execution/original closure independently.
pub fn transfer_artifact_config_pages_v1<M, R, E, C, W>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ArtifactConfigPagesWorkV1<'_>,
    sink: &mut W,
) -> Result<ArtifactConfigPagesV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
{
    collect(roots, ctx, work, Some(sink))
}
