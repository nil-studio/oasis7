//! These tests use cfg(test) parent tokens to exercise exact restore mechanics;
//! they do not attest a production authority or storage fault domain.
use super::*;
use crate::formal_retained_history::{
    FormalRetainedHistoryIndexViewV1, FormalRetainedHistoryRootV1, retained_tick_journal_leaf,
};
use oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexDeltaV1;

fn cold_tick_world(include_locator: bool) -> (World, SharedMemoryNodes) {
    let mut world = World::new();
    world.initialize_empty_formal_history().unwrap();
    let nodes = SharedMemoryNodes::default();
    attach_test_parent(&mut world, Arc::new(nodes.clone()));
    world.submit_action(crate::Action::RegisterAgent {
        agent_id: "cold-agent".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    for i in 0..300 {
        world.submit_action(crate::Action::MoveAgent {
            agent_id: "cold-agent".into(),
            to: oasis7_agent_api::world_action::GeoPos::new(i % 2, 0, 0),
        });
    }
    world.step().unwrap();
    assert!(world.journal.len() > HOT_FORMAL_JOURNAL_EVENTS);
    attach_test_parent(&mut world, Arc::new(nodes.clone()));
    world.submit_action(crate::Action::MoveAgent {
        agent_id: "cold-agent".into(),
        to: oasis7_agent_api::world_action::GeoPos::new(1, 1, 0),
    });
    world.step().unwrap();
    let plan = world.prepare_formal_retention_candidate().unwrap();
    assert!(!plan.journal_events_to_prune.is_empty());
    persist_retention_delta(&nodes, &plan);
    world.apply_formal_retention_plan(&plan).unwrap();
    if !include_locator {
        // Model an authenticated old index that retained events but omitted
        // the time-to-era locator. Missing is never treated as an empty event.
        let delta = plan.delta.as_ref().unwrap();
        let updates = delta
            .updates
            .iter()
            .filter(|leaf| {
                !matches!(
                    serde_cbor::from_slice::<RetainedHistoryLeafV1>(&leaf.canonical_value).unwrap(),
                    RetainedHistoryLeafV1::JournalTickLocator(_)
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        let without = BusinessIndexDeltaV1::build(delta.base.clone(), updates, &nodes).unwrap();
        for write in without.node_writes {
            nodes.0.lock().unwrap().insert(write.digest, write.bytes);
        }
        world.state.retained_history_root =
            Some(FormalRetainedHistoryRootV1::from_index_root(without.new).unwrap());
        attach_test_parent(&mut world, Arc::new(nodes.clone()));
        let record = world.latest_tick_consensus_record().unwrap().clone();
        let events = world
            .journal
            .events
            .iter()
            .filter(|event| event.time == world.state.time)
            .cloned()
            .collect::<Vec<_>>();
        world
            .refresh_candidate_tick_after_retention(&record, &events)
            .unwrap();
    }
    attach_test_parent(&mut world, Arc::new(nodes.clone()));
    (world, nodes)
}
fn exact_test_parent(world: &World) -> VerifiedFormalRetainedHistoryParentV1 {
    VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(
        world.state.retained_history_root.clone().unwrap(),
        &serde_cbor::to_vec(&world.snapshot()).unwrap(),
    )
}
#[derive(Clone, Copy)]
enum PrefetchFault {
    None,
    Corrupt,
    Missing,
}
struct PrefetchedColdNodes {
    source: SharedMemoryNodes,
    cached: std::sync::Mutex<BTreeMap<String, Vec<u8>>>,
    prefetches: std::sync::atomic::AtomicUsize,
    uncached_reads: std::sync::atomic::AtomicUsize,
    fault: PrefetchFault,
}
impl BusinessIndexNodeReader for PrefetchedColdNodes {
    fn prefetch_path(&self, root: &BusinessIndexRootV1, key: &str) -> Result<(), String> {
        self.prefetches
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = root.path_nodes(key, &self.source)?;
        let mut cached = self.cached.lock().unwrap();
        for (index, mut node) in path.nodes.into_iter().enumerate() {
            if index == 0 {
                match self.fault {
                    PrefetchFault::None => {}
                    PrefetchFault::Corrupt => node.bytes = vec![0xff],
                    PrefetchFault::Missing => continue,
                }
            }
            cached.insert(node.digest, node.bytes);
        }
        Ok(())
    }
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        if let Some(bytes) = self.cached.lock().unwrap().get(digest).cloned() {
            return Ok(Some(bytes));
        }
        self.uncached_reads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Err("cold node requires its authenticated path prefetch".into())
    }
}
#[test]
fn formal_typed_snapshot_cold_tick_restore_prefetches_and_verifies_each_path() {
    let (world, nodes) = cold_tick_world(true);
    for fault in [
        PrefetchFault::None,
        PrefetchFault::Corrupt,
        PrefetchFault::Missing,
    ] {
        let reader = Arc::new(PrefetchedColdNodes {
            source: nodes.clone(),
            cached: std::sync::Mutex::new(BTreeMap::new()),
            prefetches: std::sync::atomic::AtomicUsize::new(0),
            uncached_reads: std::sync::atomic::AtomicUsize::new(0),
            fault,
        });
        let result = World::from_verified_formal_snapshot(
            world.snapshot(),
            world.journal().clone(),
            exact_test_parent(&world),
            reader.clone(),
        );
        match fault {
            PrefetchFault::None => {
                let restored = result.expect("cold World restore must prefetch the exact paths");
                assert_eq!(restored.snapshot(), world.snapshot());
                assert_eq!(restored.journal(), world.journal());
                assert_eq!(
                    reader
                        .uncached_reads
                        .load(std::sync::atomic::Ordering::Relaxed),
                    0
                );
            }
            PrefetchFault::Corrupt | PrefetchFault::Missing => assert!(result.is_err()),
        }
        assert!(reader.prefetches.load(std::sync::atomic::Ordering::Relaxed) > 0);
    }
}
#[test]
fn formal_typed_snapshot_restores_cold_tick_before_validation_without_replay() {
    let (world, nodes) = cold_tick_world(true);
    world.verify_tick_consensus_chain().unwrap();
    let snapshot = world.snapshot();
    let journal = world.journal().clone();
    assert!(World::from_snapshot(snapshot.clone(), journal.clone()).is_err());
    let restored = World::from_verified_formal_snapshot(
        snapshot.clone(),
        journal.clone(),
        exact_test_parent(&world),
        Arc::new(nodes),
    )
    .unwrap();
    assert_eq!(restored.snapshot(), snapshot);
    assert_eq!(restored.journal(), &journal);
    restored.verify_tick_consensus_chain().unwrap();
    let exact = world
        .restore_snapshot_with_attached_formal_history(snapshot, journal)
        .unwrap();
    assert_eq!(exact.snapshot(), world.snapshot());
}
#[test]
fn formal_typed_snapshot_rejects_missing_locator_and_corrupt_cold_nodes() {
    let (world, nodes) = cold_tick_world(false);
    let cold_id = world.tick_consensus_records[0].block.ordered_event_ids[0];
    assert!(
        world
            .read_retained_journal_event(0, cold_id)
            .unwrap()
            .is_some()
    );
    let error = World::from_verified_formal_snapshot(
        world.snapshot(),
        world.journal().clone(),
        exact_test_parent(&world),
        Arc::new(nodes),
    )
    .unwrap_err();
    assert!(format!("{error:?}").contains("tick event missing"));
    let (world, _) = cold_tick_world(true);
    assert!(
        World::from_verified_formal_snapshot(
            world.snapshot(),
            world.journal().clone(),
            exact_test_parent(&world),
            Arc::new(CorruptNodes)
        )
        .is_err()
    );
}
#[test]
fn formal_typed_snapshot_rejects_tamper_and_extra_replay_suffix() {
    let (world, nodes) = cold_tick_world(true);
    let mut snapshot = world.snapshot();
    snapshot.state.time += 1;
    assert!(
        World::from_verified_formal_snapshot(
            snapshot,
            world.journal().clone(),
            exact_test_parent(&world),
            Arc::new(nodes.clone())
        )
        .is_err()
    );
    let mut journal = world.journal().clone();
    journal
        .events
        .push(synthetic_journal_event(world.next_event_id));
    assert!(
        World::from_verified_formal_snapshot(
            world.snapshot(),
            journal,
            exact_test_parent(&world),
            Arc::new(nodes)
        )
        .is_err()
    );
}
#[test]
fn formal_cold_tick_locator_selects_exact_time_and_event_era() {
    let first = synthetic_journal_event(1);
    let mut second = first.clone();
    second.time = 2;
    let mut updates = Vec::new();
    for (era, event) in [(0u64, &first), (1u64, &second)] {
        let key = retained_key_digest("world-journal-event", &(era, event.id)).unwrap();
        updates.push(
            retained_leaf(
                key,
                &RetainedHistoryLeafV1::JournalEvent(
                    RetainedJournalEventV1::encode(era, event).unwrap(),
                ),
            )
            .unwrap(),
        );
        let (key, value) = retained_tick_journal_leaf(era, event).unwrap();
        updates.push(retained_leaf(key, &value).unwrap());
    }
    let nodes = SharedMemoryNodes::default();
    updates.sort_by(|a, b| a.key_digest.cmp(&b.key_digest));
    let delta = BusinessIndexDeltaV1::build(BusinessIndexRootV1::empty().unwrap(), updates, &nodes)
        .unwrap();
    for write in delta.node_writes {
        nodes.0.lock().unwrap().insert(write.digest, write.bytes);
    }
    let view = FormalRetainedHistoryIndexViewV1::from_validated_root(
        FormalRetainedHistoryRootV1::from_index_root(delta.new).unwrap(),
    )
    .unwrap();
    assert_eq!(
        view.read_tick_journal_event(1, 1, 1, &nodes).unwrap(),
        Some(first)
    );
    assert_eq!(
        view.read_tick_journal_event(2, 1, 1, &nodes).unwrap(),
        Some(second)
    );
    assert_eq!(view.read_tick_journal_event(3, 1, 1, &nodes).unwrap(), None);
    assert!(
        view.read_tick_journal_event(2, 1, 0, &nodes).is_err(),
        "future era must not be selected"
    );
}
#[test]
fn formal_retention_cannot_rebind_qualified_tick_and_preserves_transaction() {
    let mut world = World::new();
    world.initialize_empty_formal_history().unwrap();
    world.step().unwrap();
    let nodes = SharedMemoryNodes::default();
    let bytes = serde_cbor::to_vec(&world.snapshot()).unwrap();
    let parent = VerifiedFormalRetainedHistoryParentV1::for_test_snapshot_at_tick(
        world.state.retained_history_root.clone().unwrap(),
        &bytes,
        world.state.time,
    );
    world
        .attach_verified_formal_history(parent, Arc::new(nodes.clone()))
        .unwrap();
    let before = serde_cbor::to_vec(&world.snapshot()).unwrap();
    let plan = world.prepare_formal_retention_candidate().unwrap();
    assert!(
        world
            .apply_formal_retention_plan(&plan)
            .unwrap_err()
            .contains("already qualified Tick")
    );
    assert_eq!(serde_cbor::to_vec(&world.snapshot()).unwrap(), before);
}
