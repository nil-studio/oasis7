//! Compatibility adapters for the single lower observation DATA formatter.
//! These moves/borrows add no validation, qualification, allocation or formatter policy.
use crate::{WorldServiceFeedbackHistory, WorldServiceProjection};
use oasis7_agent_api::RuntimeBindingV1;
use oasis7_client_api::world_service::FormalProjectionCommitHeaderV1;
use oasis7_proto::viewer::CanonicalAgentOwnerFenceV1;
use oasis7_world_state::{WorldEvent, observation_projection::original_formatter as data};

pub use data::owner_key_matches_data_v1;

pub fn base_data_v1<S>(
    state: S,
    world_rule_status: Option<serde_json::Value>,
    canonical_agent_owner: Option<CanonicalAgentOwnerFenceV1>,
    feedback_history: Option<WorldServiceFeedbackHistory>,
    runtime_binding: Option<RuntimeBindingV1>,
) -> WorldServiceProjection<S> {
    data::base_data_v1(
        state,
        world_rule_status,
        canonical_agent_owner,
        feedback_history,
        runtime_binding,
    )
    .into()
}

pub fn attach_gameplay_query_data_v1<S>(
    projection: &mut WorldServiceProjection<S>,
    query: serde_json::Value,
) {
    data::attach_gameplay_query_data_v1(projection.data_mut(), query);
}

pub fn attach_formal_header_data_v1<S>(
    projection: &mut WorldServiceProjection<S>,
    header: FormalProjectionCommitHeaderV1,
    current_era_events: Vec<WorldEvent>,
) {
    data::attach_formal_header_data_v1(projection.data_mut(), header, current_era_events);
}
