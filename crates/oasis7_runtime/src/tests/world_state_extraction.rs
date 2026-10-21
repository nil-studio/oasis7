use crate::{Action, World};
#[test]
fn pre_extraction_snapshot_and_journal_restore_and_admit_next_action() {
    let snapshot = oasis7_world_state::codec::snapshot_from_json(include_bytes!(
        "../../../oasis7_world_state/fixtures/snapshot.json"
    ))
    .unwrap();
    let journal = oasis7_world_state::codec::journal_from_json(include_bytes!(
        "../../../oasis7_world_state/fixtures/journal.json"
    ))
    .unwrap();
    assert!(!journal.events.is_empty());
    let mut world = World::from_snapshot(snapshot, journal).expect("restore old world fixture");
    world.submit_action(Action::RegisterAgent {
        agent_id: "after-extraction".into(),
        pos: super::pos(9, 11),
    });
    world.step().unwrap();
    assert!(world.state().agents.contains_key("after-extraction"));
}
