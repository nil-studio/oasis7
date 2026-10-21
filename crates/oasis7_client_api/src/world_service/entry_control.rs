//! Authenticated owner-control transport, separate from executor game requests.
//! Private goal bodies are never canonical public payloads or diagnostics.
use super::{
    SignedReadRequest, SubmitIntentRequest, participant_discovery::ParticipantOwnerViewRequestV1,
    private_goal::SignedGoalPrepareV1, private_goal_read::CurrentGoalRequest,
};
use crate::world_service_wire::{AuthenticatedLookup, WorldServicePayloadV1};
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryOwnerControlRequestV1 {
    pub access: ParticipantOwnerViewRequestV1,
    pub operation: EntryOwnerOperationV1,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EntryOwnerOperationV1 {
    Submit(SubmitIntentRequest<WorldServicePayloadV1>),
    Lookup(AuthenticatedLookup),
    /// Current owner may reconcile an executor's original after stop/revoke.
    /// This variant never submits or opens the executor's private context.
    LookupExecutor(AuthenticatedLookup),
    PrepareGoal(SignedGoalPrepareV1),
    ReadGoal(SignedReadRequest<CurrentGoalRequest>),
}
