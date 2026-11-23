//! Native application action assembly around the authorized Service use case.
use oasis7_runtime::{Action, GameplaySubmissionOrigin, World};
pub use oasis7_world_service::gameplay::{
    CollectDataQualificationError, consumed_collect_data_nonce, resolve_collect_data_collector,
};
pub fn authenticated_action(world: &World, bytes: &[u8]) -> Result<Action, String> {
    oasis7_world_service::gameplay::authenticated_action(
        world,
        bytes,
        crate::gameplay_actions::build_runtime_action_from_gameplay_request,
    )
}
pub fn authenticated_action_with_origin(
    world: &World,
    bytes: &[u8],
) -> Result<(Action, Option<GameplaySubmissionOrigin>), String> {
    oasis7_world_service::gameplay::authenticated_action_with_origin(
        world,
        bytes,
        crate::gameplay_actions::build_runtime_action_from_gameplay_request,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_gameplay_payload_fails_closed_without_world_mutation() {
        let world = crate::world::new();
        let before_time = world.state().time;
        let before_pending = world.pending_actions_len();
        assert!(authenticated_action(&world, b"not-a-signed-gameplay-request").is_err());
        assert!(
            authenticated_action_with_origin(&world, b"not-a-signed-gameplay-request").is_err()
        );
        assert_eq!(world.state().time, before_time);
        assert_eq!(world.pending_actions_len(), before_pending);
    }
}
