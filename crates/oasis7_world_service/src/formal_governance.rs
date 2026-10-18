//! Derive the body from one Runtime World, then bind it in the Service projection.
pub(super) fn committed_body(
    world: &oasis7_runtime::World,
) -> Result<oasis7_client_api::world_service::FormalGovernanceMembershipV1, String> {
    world.governance_membership_projection()
}
