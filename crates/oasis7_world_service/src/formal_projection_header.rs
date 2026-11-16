//! Global journal bounds are committed even when an audience sees no events.
use oasis7_client_api::world_service::FormalProjectionCommitHeaderV1;
use oasis7_runtime::{World, WorldEvent};
use oasis7_world_projection::current_commitment_history::{
    original_journal_bounds_with_work_v1, original_projection_event_visible_v1,
};
pub(super) fn committed_header(
    world: &World,
) -> Result<(FormalProjectionCommitHeaderV1, usize), String> {
    let snapshot = world.snapshot();
    let journal = &world.journal().events;
    let bounds = original_journal_bounds_with_work_v1(
        journal,
        snapshot.last_event_id,
        snapshot.event_id_era,
        &mut |_, _| Ok::<(), std::convert::Infallible>(()),
    )
    .unwrap_or_else(|never| match never {});
    let start = bounds.start;
    let high = bounds.high;
    let floor = bounds.floor;
    let era = bounds.era;
    let revocations = world.capability_revocation_state();
    Ok((
        FormalProjectionCommitHeaderV1 {
            authority_generation: revocations
                .authority_records
                .values()
                .map(|record| record.governance_epoch)
                .max()
                .unwrap_or(0),
            permission_generation: revocations.epoch,
            event_id_era: era,
            journal_retention_floor: floor,
            journal_high_watermark: high,
            governance_membership: Some(super::formal_governance::committed_body(world)?),
        },
        start,
    ))
}
pub(super) fn current_era_events(
    world: &World,
    start: usize,
    agent: Option<&str>,
) -> Vec<WorldEvent> {
    world
        .journal()
        .events
        .iter()
        .skip(start)
        .filter(|event| original_projection_event_visible_v1(event, agent))
        .cloned()
        .collect()
}
