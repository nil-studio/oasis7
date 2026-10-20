//! Task-fenced execution transport; contains no owner-control operation.
use super::{
    task_context::*, task_factory_quote::TaskFactoryQuoteRequestV1,
    task_recipe_quote::TaskRecipeQuoteRequestV1, *,
};
use crate::world_service_wire::{AuthenticatedLookup, WorldServicePayloadV1};
use oasis7_agent_api::game::GameTaskFenceV1;
use serde::{Deserialize, Serialize};
pub const EXECUTOR_LOOKUP_PATH: &str = "/v1/world/executor-lookup";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryExecutorLookupRequestV1 {
    pub access: EntryExecutorAccessV1,
    pub lookup: AuthenticatedLookup,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryExecutorAccessV1 {
    pub read: SignedReadRequest<ReadWorldViewRequest>,
    pub fence: GameTaskFenceV1,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryExecutorControlRequestV1 {
    pub access: EntryExecutorAccessV1,
    pub operation: EntryExecutorOperationV1,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EntryExecutorOperationV1 {
    Describe(SignedReadRequest<DescribeWorldRequest>),
    ReadView(SignedReadRequest<ReadWorldViewRequest>),
    Submit(SubmitIntentRequest<WorldServicePayloadV1>),
    Lookup(AuthenticatedLookup),
    PrepareContext(SignedReadRequest<TaskContextPrepareV1>),
    ReadContext(SignedReadRequest<TaskContextGetV1>),
    GoalSnapshot(SignedReadRequest<TaskGoalSnapshotRequestV1>),
    RecipeQuote(SignedReadRequest<TaskRecipeQuoteRequestV1>),
    FactoryQuote(SignedReadRequest<TaskFactoryQuoteRequestV1>),
}
