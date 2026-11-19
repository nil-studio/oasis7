//! Accepted cold conjunction. This is deliberately distinct from a candidate
//! storage source: its selected history/router occurrences remain authenticated
//! accepted occurrences and are never relabelled as checked-candidate facts.
use crate::{
    incremental_cognition_history::CompleteCognitionHistorySourceV3,
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    incremental_world_current_complete::WorldCurrentStateQualificationV1,
    incremental_world_result_router_accepted::AcceptedWorldResultRoutingV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError, VerifiedFormalCommitV1, bounded_context::FormalBoundedContextV1,
    },
    incremental_material::CheckedCommitCore,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub struct AuthenticatedColdWorldStorageSourceV1 {
    current: ReservedValue<WorldCurrentStateQualificationV1>,
    results: ReservedValue<AcceptedWorldResultRoutingV1>,
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
}
#[derive(Debug)]
pub enum ColdWorldStorageSourceErrorV1 {
    Formal(FormalError),
    Value(ValueTreeError),
}
impl From<FormalError> for ColdWorldStorageSourceErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Formal(e)
    }
}
impl From<ValueTreeError> for ColdWorldStorageSourceErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
fn bad() -> ColdWorldStorageSourceErrorV1 {
    ValueTreeError::Invalid("authenticated cold full storage source").into()
}
impl AuthenticatedColdWorldStorageSourceV1 {
    pub fn current(&self) -> &ReservedValue<WorldCurrentStateQualificationV1> {
        &self.current
    }
    pub fn results(&self) -> &ReservedValue<AcceptedWorldResultRoutingV1> {
        &self.results
    }
    pub fn record_id(&self) -> [u8; 32] {
        self.record
    }
    pub fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub fn roots_id(&self) -> [u8; 32] {
        self.roots
    }
    pub fn validate_sources_v1(
        &self,
        head: &VerifiedFormalCommitV1,
        core: &ReservedValue<CheckedCommitCore>,
        runtime: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
        cognition: &ReservedValue<CompleteCognitionHistorySourceV3>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), ColdWorldStorageSourceErrorV1> {
        common.charge(32, 8192)?;
        if !core.uses_budget(common.memory())
            || !self.current.uses_budget(common.memory())
            || !self.results.uses_budget(common.memory())
            || !runtime.uses_budget(common.memory())
            || !cognition.uses_budget(common.memory())
            || self.current.allocation_class() != MemoryClass::Ordinary
            || self.results.allocation_class() != MemoryClass::Ordinary
            || runtime.allocation_class() != MemoryClass::Ordinary
            || cognition.selected().occurrence().allocation_class() != MemoryClass::Ordinary
            || self.record != head.verified_record_id()
            || self.record != core.candidate_record_id()
            || self.body != core.catalog().entry.commit
            || self.roots != core.body().roots
            || self.current.roots_id() != self.roots
        {
            return Err(bad());
        }
        let raw = self.current.raw().mailbox_anchor();
        let r = runtime.occurrence();
        let c = cognition.selected().occurrence();
        if raw.record_id() != self.record
            || raw.body_id() != self.body
            || raw.roots_id() != self.roots
            || r.head_record_id() != self.record
            || r.head_body_id() != self.body
            || r.fact_body_id() != self.body
            || r.roots_id() != self.roots
            || runtime.manifest().roots != self.roots
            || c.head_record_id() != self.record
            || c.head_body_id() != self.body
            || c.fact_body_id() != self.body
            || c.roots_id() != self.roots
            || cognition.selected().manifest().roots != self.roots
            || cognition.current().head_record_id() != self.record
            || cognition.current().roots_id() != self.roots
            || self.results.record_id() != self.record
            || self.results.body_id() != self.body
            || self.results.roots_id() != self.roots
            || self.results.runtime_history().occurrence().location() != r.location()
            || self.results.cognition().occurrence().location() != c.location()
        {
            return Err(bad());
        }
        self.current.check_pool(common.memory())?;
        Ok(())
    }
}
/// All inputs are genuine private producer outputs. The cache which owns the
/// returned source must retain the same accepted Runtime/Cognition siblings;
/// every Host borrow rechecks their exact immutable occurrence identities.
pub fn qualify_authenticated_cold_world_storage_source_v1(
    head: &VerifiedFormalCommitV1,
    core: &ReservedValue<CheckedCommitCore>,
    current: ReservedValue<WorldCurrentStateQualificationV1>,
    results: ReservedValue<AcceptedWorldResultRoutingV1>,
    runtime: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    cognition: &ReservedValue<CompleteCognitionHistorySourceV3>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<AuthenticatedColdWorldStorageSourceV1>, ColdWorldStorageSourceErrorV1> {
    common.consume_steps(1)?;
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<AuthenticatedColdWorldStorageSourceV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let source = AuthenticatedColdWorldStorageSourceV1 {
        current,
        results,
        record: head.verified_record_id(),
        body: core.catalog().entry.commit,
        roots: core.body().roots,
    };
    source.validate_sources_v1(head, core, runtime, cognition, common)?;
    Ok(fee.retain(source))
}

use crate::incremental_world_result_router::{CheckedWorldResultRoutingV1, RoutedWorldResultV1};
use crate::incremental_world_result_router_accepted::AcceptedRoutedWorldResultV1;
use crate::incremental_world_storage_source::CheckedWorldStorageSourceV1;
use oasis7_formal_protocol::incremental_material::{
    CheckedCandidateFactOccurrenceV1, FactLocationV1, VerifiedFactOccurrenceV1,
};
use oasis7_proto::memory_budget::MemoryBudget;
mod sealed {
    pub trait Sealed {}
}
/// Only a borrowed view over original immutable proof owners. No public
/// constructors, fact refinements or receipt permissions are introduced here.
#[derive(Clone, Copy)]
pub enum WorldParentResultRoutingRefV1<'a> {
    Checked(&'a ReservedValue<CheckedWorldResultRoutingV1>),
    Accepted(&'a ReservedValue<AcceptedWorldResultRoutingV1>),
}
#[derive(Clone, Copy)]
pub enum WorldParentRoutedResultRefV1<'a> {
    Checked(&'a RoutedWorldResultV1),
    Accepted(&'a AcceptedRoutedWorldResultV1),
}
#[derive(Clone, Copy)]
pub enum WorldParentFactOccurrenceRefV1<'a> {
    Checked(&'a ReservedValue<CheckedCandidateFactOccurrenceV1>),
    Accepted(&'a ReservedValue<VerifiedFactOccurrenceV1>),
}
impl<'a> WorldParentResultRoutingRefV1<'a> {
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        match self {
            Self::Checked(q) => q.uses_budget(memory),
            Self::Accepted(q) => q.uses_budget(memory),
        }
    }
    pub fn cognition(self) -> WorldParentRoutedResultRefV1<'a> {
        match self {
            Self::Checked(q) => WorldParentRoutedResultRefV1::Checked(q.cognition()),
            Self::Accepted(q) => WorldParentRoutedResultRefV1::Accepted(q.cognition()),
        }
    }
    pub fn proof_ledger(self) -> Option<WorldParentRoutedResultRefV1<'a>> {
        match self {
            Self::Checked(q) => q.proof_ledger().map(WorldParentRoutedResultRefV1::Checked),
            Self::Accepted(q) => q.proof_ledger().map(WorldParentRoutedResultRefV1::Accepted),
        }
    }
}
impl<'a> WorldParentRoutedResultRefV1<'a> {
    pub fn occurrence(self) -> WorldParentFactOccurrenceRefV1<'a> {
        match self {
            Self::Checked(q) => WorldParentFactOccurrenceRefV1::Checked(q.occurrence()),
            Self::Accepted(q) => WorldParentFactOccurrenceRefV1::Accepted(q.occurrence()),
        }
    }
}
impl WorldParentFactOccurrenceRefV1<'_> {
    /// Same immutable parent head identity, not a candidate qualification.
    pub fn candidate_record_id(&self) -> [u8; 32] {
        match self {
            Self::Checked(q) => q.candidate_record_id(),
            Self::Accepted(q) => q.head_record_id(),
        }
    }
    pub fn body_id(&self) -> [u8; 32] {
        match self {
            Self::Checked(q) => q.body_id(),
            Self::Accepted(q) => q.head_body_id(),
        }
    }
    pub fn roots_id(&self) -> [u8; 32] {
        match self {
            Self::Checked(q) => q.roots_id(),
            Self::Accepted(q) => q.roots_id(),
        }
    }
    pub fn location(&self) -> FactLocationV1 {
        match self {
            Self::Checked(q) => q.location(),
            Self::Accepted(q) => q.location(),
        }
    }
}
/// Sealed to the two genuine whole-source owner classes below. Callers cannot
/// implement a reported root, boolean, callback or partial collection as a source.
pub trait AuthenticatedWorldStorageParentSourceV1: sealed::Sealed {
    fn current(&self) -> &ReservedValue<WorldCurrentStateQualificationV1>;
    fn results(&self) -> WorldParentResultRoutingRefV1<'_>;
    fn uses_budget(&self, memory: &MemoryBudget) -> bool;
    fn validate_parent_sources_v1(
        &self,
        head: &VerifiedFormalCommitV1,
        core: &ReservedValue<CheckedCommitCore>,
        runtime: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
        cognition: &ReservedValue<CompleteCognitionHistorySourceV3>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), ColdWorldStorageSourceErrorV1>;
}
impl sealed::Sealed for ReservedValue<AuthenticatedColdWorldStorageSourceV1> {}
impl AuthenticatedWorldStorageParentSourceV1
    for ReservedValue<AuthenticatedColdWorldStorageSourceV1>
{
    fn current(&self) -> &ReservedValue<WorldCurrentStateQualificationV1> {
        (**self).current()
    }
    fn results(&self) -> WorldParentResultRoutingRefV1<'_> {
        WorldParentResultRoutingRefV1::Accepted((**self).results())
    }
    fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        ReservedValue::uses_budget(self, memory)
    }
    fn validate_parent_sources_v1(
        &self,
        head: &VerifiedFormalCommitV1,
        core: &ReservedValue<CheckedCommitCore>,
        runtime: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
        cognition: &ReservedValue<CompleteCognitionHistorySourceV3>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), ColdWorldStorageSourceErrorV1> {
        (**self).validate_sources_v1(head, core, runtime, cognition, common)
    }
}
impl sealed::Sealed for ReservedValue<CheckedWorldStorageSourceV1> {}
impl AuthenticatedWorldStorageParentSourceV1 for ReservedValue<CheckedWorldStorageSourceV1> {
    fn current(&self) -> &ReservedValue<WorldCurrentStateQualificationV1> {
        (**self).current()
    }
    fn results(&self) -> WorldParentResultRoutingRefV1<'_> {
        WorldParentResultRoutingRefV1::Checked((**self).results())
    }
    fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        ReservedValue::uses_budget(self, memory)
    }
    fn validate_parent_sources_v1(
        &self,
        head: &VerifiedFormalCommitV1,
        core: &ReservedValue<CheckedCommitCore>,
        runtime: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
        cognition: &ReservedValue<CompleteCognitionHistorySourceV3>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), ColdWorldStorageSourceErrorV1> {
        common.charge(16, 8192)?;
        let checked = (**self).runtime().occurrence();
        let accepted = runtime.occurrence();
        let c = (**self).cognition();
        if !self.uses_budget(common.memory())
            || !core.uses_budget(common.memory())
            || !runtime.uses_budget(common.memory())
            || !cognition.uses_budget(common.memory())
            || core.candidate_record_id() != head.verified_record_id()
            || checked.candidate_record_id() != head.verified_record_id()
            || accepted.head_record_id() != head.verified_record_id()
            || checked.body_id() != accepted.head_body_id()
            || checked.roots_id() != core.body().roots
            || c.candidate_record_id() != head.verified_record_id()
            || c.body_id() != accepted.head_body_id()
            || c.roots_id() != core.body().roots
            || cognition.current().head_record_id() != head.verified_record_id()
            || cognition.current().roots_id() != core.body().roots
            || cognition.selected().occurrence().head_body_id() != accepted.head_body_id()
        {
            return Err(bad());
        }
        Ok(())
    }
}
