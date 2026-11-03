//! Actual typed-family reachability and provisional physical transfer.
use super::*;
use crate::incremental_family_page_tracking::{NoSink, State, Tracking, TrackingLabels, Values};
const PAGE_LABELS: TrackingLabels = TrackingLabels {
    allowance: "logistics mark allowance",
    hash: "logistics transfer content hash",
    page_limit: "logistics marked pages",
    state_bound: "logistics state source bound",
    state_digest: "logistics state source digest",
    state_tracking: "logistics state page tracking",
    value_admission: "logistics value source admission/bound",
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext},
};
use oasis7_proto::memory_budget::MemoryClass;
use std::{cell::RefCell, collections::BTreeSet};

/// A completed mark set for these two families, including all six top-level
/// state proof trees and their typed Logistics descendants. Other owners' value
/// objects must be unioned by the World owner registry before global GC/transfer.
pub struct LogisticsFamilyPagesV1 {
    pages: ReservedValue<RefCell<BTreeSet<[u8; 32]>>>,
    qualification: ReservedValue<LogisticsFamilyQualificationV1>,
}
impl LogisticsFamilyPagesV1 {
    pub fn visit_pages<E>(
        &self,
        mut visitor: impl FnMut(&[u8; 32]) -> Result<(), E>,
    ) -> Result<(), E> {
        for page in self.pages.borrow().iter() {
            visitor(page)?;
        }
        Ok(())
    }
    pub fn page_count(&self) -> usize {
        self.pages.borrow().len()
    }
    pub fn contains(&self, id: &[u8; 32]) -> bool {
        self.pages.borrow().contains(id)
    }
    pub fn qualification(&self) -> &ReservedValue<LogisticsFamilyQualificationV1> {
        &self.qualification
    }
}
fn collect<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
>(
    roots: &RootSetV1,
    context: &mut LogisticsFamilyReadContextV1<'_, M, R, E, C>,
    work: &mut LogisticsWorkBudgetV1,
    max_pages: usize,
    sink: Option<&mut W>,
) -> Result<LogisticsFamilyPagesV1, ValueTreeError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let memory = context.values.memory;
    // BTreeSet worst-case per-entry overhead is admitted up front. The caller's
    // explicit finite page cap limits both retention and deduplication work.
    let tracking = RefCell::new(Tracking::new(
        memory,
        MemoryClass::Ordinary,
        max_pages,
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
    let qualification = qualify_logistics_family_v1(
        roots,
        &mut LogisticsFamilyReadContextV1 {
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
            max_payload_bytes: context.max_payload_bytes,
            outer_scan: &mut *context.outer_scan,
            take_state_error: || {
                tracking
                    .borrow_mut()
                    .error
                    .take()
                    .or_else(&mut context.take_state_error)
            },
            cancelled: &mut context.cancelled,
        },
        work,
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(LogisticsFamilyPagesV1 {
        pages: tracking.into_inner().pages,
        qualification,
    })
}
/// A successful set can be unioned into the global GC live set. No set or token
/// escapes if any state/typed-child/value page, quota or cancellation fails.
pub fn mark_logistics_family_pages_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    context: &mut LogisticsFamilyReadContextV1<'_, M, R, E, C>,
    work: &mut LogisticsWorkBudgetV1,
    max_pages: usize,
) -> Result<LogisticsFamilyPagesV1, ValueTreeError> {
    collect::<M, R, E, C, NoSink>(roots, context, work, max_pages, None)
}
/// Copies each reached physical page to the caller's provisional destination,
/// including nested StateRoots; partial objects never imply a publishable head.
/// Success returns the exact mark set and same-root schema qualification.
pub fn transfer_logistics_family_pages_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
>(
    roots: &RootSetV1,
    context: &mut LogisticsFamilyReadContextV1<'_, M, R, E, C>,
    work: &mut LogisticsWorkBudgetV1,
    max_pages: usize,
    sink: &mut W,
) -> Result<LogisticsFamilyPagesV1, ValueTreeError> {
    collect(roots, context, work, max_pages, Some(sink))
}
