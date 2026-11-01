//! Agent state reachability and external original IDs, not a complete World or
//! accepted-history live set. Original objects require their separate proof/copy.
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
    allowance: "agent marked page allowance",
    hash: "agent transfer page hash",
    page_limit: "agent marked pages",
    state_bound: "agent state page bound",
    state_digest: "agent state page digest",
    state_tracking: "agent state tracking",
    value_admission: "agent value page admission",
};
pub struct AgentSubtreePagesV1 {
    pages: ReservedValue<RefCell<BTreeSet<[u8; 32]>>>,
    originals: ReservedValue<RefCell<BTreeSet<[u8; 32]>>>,
    structure: ReservedValue<AgentSubtreeStructureQualificationV1>,
}
impl AgentSubtreePagesV1 {
    pub fn visit_pages<E>(
        &self,
        mut visit: impl FnMut(&[u8; 32]) -> Result<(), E>,
    ) -> Result<(), E> {
        for id in self.pages.borrow().iter() {
            visit(id)?;
        }
        Ok(())
    }
    /// IDs only. They do not assert original availability, Domain semantics,
    /// accepted provenance, or that an external-object transfer has completed.
    pub fn visit_external_originals<E>(
        &self,
        mut visit: impl FnMut(&[u8; 32]) -> Result<(), E>,
    ) -> Result<(), E> {
        for id in self.originals.borrow().iter() {
            visit(id)?;
        }
        Ok(())
    }
    pub fn page_count(&self) -> usize {
        self.pages.borrow().len()
    }
    pub fn original_count(&self) -> usize {
        self.originals.borrow().len()
    }
    pub fn contains(&self, id: &[u8; 32]) -> bool {
        self.pages.borrow().contains(id)
    }
    pub fn structure(&self) -> &ReservedValue<AgentSubtreeStructureQualificationV1> {
        &self.structure
    }
}
pub struct AgentSubtreePagesWorkV1<'a> {
    pub scan: &'a mut StateScanBudget,
    pub structure: &'a mut AgentSubtreeScanWorkV1,
    pub max_pages: usize,
    pub max_originals: usize,
}
fn collect<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
>(
    roots: &RootSetV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut AgentSubtreePagesWorkV1<'_>,
    sink: Option<&mut W>,
) -> Result<AgentSubtreePagesV1, ValueTreeError> {
    context.check()?;
    let memory = context.values.memory;
    let class = context.values.class;
    let bytes = work
        .max_originals
        .checked_mul(128)
        .and_then(|n| n.checked_add(4096))
        .ok_or(ValueTreeError::ResourceLimited(
            "agent external original allowance",
        ))?;
    let originals = memory
        .reserve(bytes, class)?
        .retain(RefCell::new(BTreeSet::new()));
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
    let structure = qualify_agent_subtree_structure_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root: context.root,
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
            take_state_error: || {
                tracking
                    .borrow_mut()
                    .error
                    .take()
                    .or_else(&mut context.take_state_error)
            },
            cancelled: &mut context.cancelled,
        },
        work.scan,
        work.structure,
        &mut |reference| {
            let mut ids = originals.borrow_mut();
            if !ids.contains(&reference.original) && ids.len() >= work.max_originals {
                return Err(ValueTreeError::ResourceLimited(
                    "agent external original limit",
                ));
            }
            ids.insert(reference.original);
            Ok(())
        },
    )?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = tracking.borrow_mut().error.take() {
        return Err(error);
    }
    Ok(AgentSubtreePagesV1 {
        pages: tracking.into_inner().pages,
        originals,
        structure,
    })
}
/// Validates the full requested state closure and returns no set on failure.
/// Other owners and referenced original/history objects remain separate scopes.
pub fn mark_agent_subtree_pages_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut AgentSubtreePagesWorkV1<'_>,
) -> Result<AgentSubtreePagesV1, ValueTreeError> {
    collect::<M, R, E, C, NoSink>(roots, context, work, None)
}
/// Pages remain provisional until this call AND all buffered sink finishes
/// succeed. This never signs accepted history or a complete World transfer.
pub fn transfer_agent_subtree_pages_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
>(
    roots: &RootSetV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut AgentSubtreePagesWorkV1<'_>,
    sink: &mut W,
) -> Result<AgentSubtreePagesV1, ValueTreeError> {
    collect(roots, context, work, Some(sink))
}
