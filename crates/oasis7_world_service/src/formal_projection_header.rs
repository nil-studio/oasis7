//! Global journal bounds are committed even when an audience sees no events.
use oasis7_client_api::world_service::FormalProjectionCommitHeaderV1;
use oasis7_runtime::{World, WorldEvent, WorldEventBody};
pub(super) fn committed_header(
    world: &World,
) -> Result<(FormalProjectionCommitHeaderV1, usize), String> {
    let snapshot = world.snapshot();
    let journal = &world.journal().events;
    // Event ids wrap only at an era boundary. Keep the latest era's suffix in
    // a formal stream so an old retained id cannot masquerade as a new event.
    let start = journal
        .windows(2)
        .rposition(|pair| pair[0].id >= pair[1].id)
        .map_or(0, |i| i + 1);
    let high = snapshot.last_event_id;
    let floor = journal
        .get(start)
        .map_or_else(|| high.saturating_add(1), |event| event.id);
    let era = snapshot
        .event_id_era
        .saturating_sub(u64::from(high == u64::MAX));
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
        .filter(|event| match &event.body {
            WorldEventBody::Domain(domain) => {
                agent.is_some_and(|agent| domain.agent_id() == Some(agent))
            }
            _ => false,
        })
        .cloned()
        .collect()
}
