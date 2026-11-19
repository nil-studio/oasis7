//! Fixed Current Cognition submission. All intermediate carriers are paid
//! DATA; only the fixed full-step consumer may later create a terminal marker.
use super::incremental_body::BodyMutationError;
use oasis7_agent_api::world_cognition::{
    RuntimeCognitionCommitRequestV1, RuntimeCognitionResponseArtifactV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, mem::size_of};
fn bad(note: &'static str) -> BodyMutationError {
    BodyMutationError::Invalid(note)
}
fn original(error: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(error))
}
#[derive(Debug)]
pub(in crate::world) struct PaidCurrentCognitionCommitRefusalV1 {
    error: crate::WorldError,
    _fee: MemoryReservation,
}
impl PaidCurrentCognitionCommitRefusalV1 {
    pub(in crate::world) fn error(&self) -> &crate::WorldError {
        &self.error
    }
}
#[derive(Debug)]
pub(in crate::world) enum CurrentCognitionCommitErrorV1 {
    Body(BodyMutationError),
    Business(PaidCurrentCognitionCommitRefusalV1),
}
impl From<BodyMutationError> for CurrentCognitionCommitErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<ValueTreeError> for CurrentCognitionCommitErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Body(e.into())
    }
}
#[path = "incremental_normal_cognition_commit_admission.rs"]
mod admission;
#[path = "incremental_normal_cognition_commit_work.rs"]
mod work;
pub(in crate::world) use admission::{
    CurrentCognitionCommitInputV1, HistoricalCurrentCognitionInputV1,
};

// One portable original kernel shared with the legacy World entry.

#[path = "incremental_normal_cognition_commit_facts.rs"]
mod facts;

#[path = "incremental_normal_cognition_commit_authority.rs"]
mod authority;
#[path = "incremental_normal_cognition_commit_response_index.rs"]
mod response_index;

#[path = "incremental_normal_cognition_commit_response_read.rs"]
mod response_read;
#[path = "incremental_normal_cognition_commit_responses.rs"]
mod responses;
