//! Qualified RuntimeSchedulerControls reachability; not the complete World live set.
use super::*;
use crate::incremental_family_page_tracking::{NoSink, State, Tracking, TrackingLabels, Values};
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::incremental_state::{MerkleNodeReader, StateScanBudget};

const PAGE_LABELS: TrackingLabels = TrackingLabels {
    allowance: "runtime_scheduler_controls mark allowance",
    hash: "runtime_scheduler_controls transfer content hash",
    page_limit: "runtime_scheduler_controls marked pages",
    state_bound: "runtime_scheduler_controls state source bound",
    state_digest: "runtime_scheduler_controls state source digest",
    state_tracking: "runtime_scheduler_controls state page tracking",
    value_admission: "runtime_scheduler_controls value source admission/bound",
};

use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{cell::RefCell, collections::BTreeSet};

/// All six top state trees and this owner's Scheduler routing child.
/// Other owners must contribute their own closures before global GC/transfer.
pub struct RuntimeSchedulerControlsFamilyPagesV1 {
    pages: ReservedValue<RefCell<BTreeSet<[u8; 32]>>>,
    qualification: ReservedValue<RuntimeSchedulerControlsQualificationV1>,
}
impl RuntimeSchedulerControlsFamilyPagesV1 {
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
    pub fn qualification(&self) -> &ReservedValue<RuntimeSchedulerControlsQualificationV1> {
        &self.qualification
    }
}

/// Continuous caller budgets for the complete requested RuntimeSchedulerControls closure.
pub struct RuntimeSchedulerControlsPagesWorkV1<'a> {
    pub outer_scan: &'a mut StateScanBudget,
    pub inner_scan: &'a mut StateScanBudget,
    pub cells: &'a mut u64,
    pub max_cursor_bytes: u64,
    pub max_pages: usize,
}
fn collect<M, R, E, C, W>(
    roots: &RootSetV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeSchedulerControlsPagesWorkV1<'_>,
    sink: Option<&mut W>,
) -> Result<RuntimeSchedulerControlsFamilyPagesV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
{
    context.check()?;
    let memory = context.values.memory;
    let tracking = RefCell::new(Tracking::new(
        memory,
        context.values.class,
        work.max_pages,
        sink,
        PAGE_LABELS,
    )?);
    let state = State {
        source: context.state,
        tracking: &tracking,
        memory,
    };
    let values = Values {
        source: context.values.source,
        tracking: &tracking,
    };
    let qualification = qualify_runtime_scheduler_controls_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root: context.root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: context.values.class,
                io: &mut *context.values.io,
            },
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            take_state_error: || {
                tracking
                    .borrow_mut()
                    .error
                    .take()
                    .or_else(&mut context.take_state_error)
            },
            cancelled: &mut context.cancelled,
        },
        work.outer_scan,
        work.inner_scan,
        work.cells,
        work.max_cursor_bytes,
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = tracking.borrow_mut().error.take() {
        return Err(error);
    }
    Ok(RuntimeSchedulerControlsFamilyPagesV1 {
        pages: tracking.into_inner().pages,
        qualification,
    })
}
/// No completed set escapes on a missing/invalid child, exhausted quota or cancel.
pub fn mark_runtime_scheduler_controls_family_pages_v1<M, R, E, C>(
    roots: &RootSetV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeSchedulerControlsPagesWorkV1<'_>,
) -> Result<RuntimeSchedulerControlsFamilyPagesV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    collect::<M, R, E, C, NoSink>(roots, context, work, None)
}
/// Sink output stays provisional until this call AND any buffered sink finish
/// succeed. Caller must also union the other owners before accepting a World.
pub fn transfer_runtime_scheduler_controls_family_pages_v1<M, R, E, C, W>(
    roots: &RootSetV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeSchedulerControlsPagesWorkV1<'_>,
    sink: &mut W,
) -> Result<RuntimeSchedulerControlsFamilyPagesV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    collect(roots, context, work, Some(sink))
}
