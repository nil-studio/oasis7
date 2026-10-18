use super::*;

#[test]
fn restore_rejects_mismatched_next_event_id() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.step_until_empty();

    let mut snapshot = kernel.kernel_snapshot();
    let journal = kernel.kernel_journal_snapshot();
    snapshot.next_event_id = snapshot.next_event_id.saturating_add(1);

    let err = WorldKernel::from_kernel_snapshot(snapshot, journal).unwrap_err();
    assert!(
        matches!(err, PersistError::ReplayConflict { message } if message.contains("next_event_id mismatch"))
    );
}

#[test]
fn restore_rejects_gapful_journal_event_ids() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.step_until_empty();

    let snapshot = kernel.kernel_snapshot();
    let mut journal = kernel.kernel_journal_snapshot();
    journal.events[0].id = journal.events[0].id.saturating_add(1);

    let err = WorldKernel::from_kernel_snapshot(snapshot, journal).unwrap_err();
    assert!(
        matches!(err, PersistError::ReplayConflict { message } if message.contains("event id mismatch"))
    );
}

#[test]
fn replay_from_snapshot_rejects_gapful_snapshot_prefix_event_ids() {
    let config = WorldConfig {
        move_cost_per_km_electricity: 0,
        ..Default::default()
    };
    let mut kernel = WorldKernel::with_config(config);
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-1".to_string(),
        name: "base".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-2".to_string(),
        name: "outpost".to_string(),
        pos: pos(1, 1),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "loc-1".to_string(),
    });
    kernel.step_until_empty();

    let snapshot = kernel.kernel_snapshot();
    let mut journal = kernel.kernel_journal_snapshot();
    journal.events[0].id = journal.events[0].id.saturating_add(1);

    kernel.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: "loc-2".to_string(),
    });
    journal.events.push(kernel.step().expect("move event"));

    let err = WorldKernel::replay_from_kernel_snapshot(snapshot, journal).unwrap_err();
    assert!(
        matches!(err, PersistError::ReplayConflict { message } if message.contains("event id mismatch"))
    );
}
