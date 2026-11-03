//! Typed reachability for the complete Runtime Artifact family.
use super::*;
use crate::incremental_family_page_tracking::{NoSink, State, Tracking, TrackingLabels, Values};
use crate::incremental_runtime_artifact_resources::RuntimeArtifactResourcesReadContextV1;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{cell::RefCell, collections::BTreeSet};

const LABELS: TrackingLabels = TrackingLabels {
    allowance: "runtime artifact mark allowance",
    hash: "runtime artifact transfer content hash",
    page_limit: "runtime artifact marked pages",
    state_bound: "runtime artifact state source bound",
    state_digest: "runtime artifact state source digest",
    state_tracking: "runtime artifact state page tracking",
    value_admission: "runtime artifact value source admission/bound",
};

/// A completed mark set for the three Artifact fields, their typed child roots,
/// and the complete six-root scan evidence. Other owners remain independent.
pub struct RuntimeArtifactResourcesPagesV1 {
    pages: ReservedValue<RefCell<BTreeSet<[u8; 32]>>>,
    qualification: ReservedValue<RuntimeArtifactResourcesQualificationV1>,
}

impl RuntimeArtifactResourcesPagesV1 {
    pub fn visit_pages<E>(
        &self,
        mut visitor: impl FnMut(&[u8; 32]) -> Result<(), E>,
    ) -> Result<(), E> {
        for page in self.pages.borrow().iter() {
            visitor(page)?;
        }
        Ok(())
    }

    pub fn contains(&self, page: &[u8; 32]) -> bool {
        self.pages.borrow().contains(page)
    }

    pub fn page_count(&self) -> usize {
        self.pages.borrow().len()
    }

    pub fn qualification(&self) -> &ReservedValue<RuntimeArtifactResourcesQualificationV1> {
        &self.qualification
    }
}

/// Caller-owned scan and cell quotas for one complete Artifact mark/transfer.
pub struct RuntimeArtifactResourcesPagesWorkV1<'a> {
    pub work: &'a mut RuntimeArtifactResourcesWorkV1,
    pub max_pages: usize,
}

fn collect<M, R, E, C, W>(
    roots: &RootSetV1,
    context: &mut RuntimeArtifactResourcesReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeArtifactResourcesPagesWorkV1<'_>,
    sink: Option<&mut W>,
) -> Result<RuntimeArtifactResourcesPagesV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
{
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let memory = context.values.memory;
    let class = context.values.class;
    if context.state.allocation_class() != Some(class)
        || !context
            .state
            .allocation_budget()
            .is_some_and(|budget| budget.same_pool(memory))
    {
        return Err(ValueTreeError::Invalid("runtime artifact page source pool"));
    }
    let tracking = RefCell::new(Tracking::new(memory, class, work.max_pages, sink, LABELS)?);
    let state = State {
        source: context.state,
        tracking: &tracking,
        memory,
    };
    let values = Values {
        source: context.values.source,
        tracking: &tracking,
    };
    let qualification = qualify_runtime_artifact_resources_v1(
        roots,
        &mut RuntimeArtifactResourcesReadContextV1 {
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            max_payload_bytes: context.record_bytes,
            scans: &mut *context.scans,
            take_state_error: || {
                tracking
                    .borrow_mut()
                    .error
                    .take()
                    .or_else(&mut context.take_state_error)
            },
            cancelled: &mut context.cancelled,
        },
        work.work,
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = tracking.borrow_mut().error.take() {
        return Err(error);
    }
    Ok(RuntimeArtifactResourcesPagesV1 {
        pages: tracking.into_inner().pages,
        qualification,
    })
}

/// No complete set escapes if any top-level or typed child page is missing,
/// malformed, over budget, or cancelled.
pub fn mark_runtime_artifact_resources_pages_v1<M, R, E, C>(
    roots: &RootSetV1,
    context: &mut RuntimeArtifactResourcesReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeArtifactResourcesPagesWorkV1<'_>,
) -> Result<RuntimeArtifactResourcesPagesV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    collect::<M, R, E, C, NoSink>(roots, context, work, None)
}

/// Transfers every reached top-level state page and typed-child page to a
/// provisional sink. The caller publishes only after the complete owner set
/// and all other World owners accept the same root.
pub fn transfer_runtime_artifact_resources_pages_v1<M, R, E, C, W>(
    roots: &RootSetV1,
    context: &mut RuntimeArtifactResourcesReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeArtifactResourcesPagesWorkV1<'_>,
    sink: &mut W,
) -> Result<RuntimeArtifactResourcesPagesV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
{
    collect(roots, context, work, Some(sink))
}
