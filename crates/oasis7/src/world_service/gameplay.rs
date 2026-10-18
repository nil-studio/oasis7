//! Native application action assembly around the authorized Service use case.
use crate::runtime::{Action, GameplaySubmissionOrigin, World};
pub use oasis7_world_service::gameplay::{
    CollectDataQualificationError, consumed_collect_data_nonce, resolve_collect_data_collector,
};
pub fn authenticated_action(world: &World, bytes: &[u8]) -> Result<Action, String> {
    oasis7_world_service::gameplay::authenticated_action(
        world,
        bytes,
        crate::viewer::build_runtime_action_from_gameplay_request,
    )
}
pub fn authenticated_action_with_origin(
    world: &World,
    bytes: &[u8],
) -> Result<(Action, Option<GameplaySubmissionOrigin>), String> {
    oasis7_world_service::gameplay::authenticated_action_with_origin(
        world,
        bytes,
        crate::viewer::build_runtime_action_from_gameplay_request,
    )
}
