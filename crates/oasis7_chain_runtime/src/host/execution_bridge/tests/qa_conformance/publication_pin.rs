//! Durable pin/cache independence and genuine concurrent publication reads.
use super::*;

#[test]
fn real_tcp_disposable_world_cache_eviction_preserves_exact_result_and_view() {
    let fixture = Fixture::with_controlled_commits(true);
    let original = fixture.delegation();
    fixture.client.submit(original.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        0,
        Some(original.clone()),
    );
    let commit = fixture.committed(&original);
    let old = fixture
        .client
        .read_view(fixture.view(Some(commit.clone())))
        .unwrap();
    let cache = fixture.root.join("world/snapshot.json");
    assert!(cache.is_file(), "fixture disposable snapshot missing");
    fs::rename(&cache, fixture.root.join("evicted-world-snapshot.json")).unwrap();
    let outcome = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key,
            },
            original.signed_payload,
        )
        .unwrap();
    assert!(
        matches!(outcome.outcome,IntentOutcome::Committed { commit:found,.. } if *found==commit)
    );
    let recovered = fixture
        .client
        .read_view(fixture.view(Some(commit)))
        .unwrap();
    assert_eq!(old.version(), recovered.version());
    assert_eq!(old.continuation(), recovered.continuation());
    assert_eq!(old.logical_tick(), recovered.logical_tick());
    println!(
        "PRE2_DISPOSABLE_CACHE_EVICTION_CANONICAL_RESULT_PASSED authoritative_cas_preserved=true"
    );
}

#[test]
fn real_tcp_fixed_projection_remains_one_generation_during_publication() {
    let fixture = Fixture::with_controlled_commits(true);
    let baseline = fixture.client.read_view(fixture.view(None)).unwrap();
    let mut fixed = fixture.view(None);
    fixed.fixed_commit = Some(baseline.version().commit.clone());
    let reader = RemoteWorldServiceClient::new(fixture.client.config().clone()).unwrap();
    let baseline_version = baseline.version().clone();
    let baseline_cursor = baseline.continuation().clone();
    let baseline_tick = baseline.logical_tick();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let ready = barrier.clone();
    let worker = thread::spawn(move || {
        ready.wait();
        for _ in 0..16 {
            let observed = reader.read_view(fixed.clone()).unwrap();
            assert_eq!(observed.version(), &baseline_version);
            assert_eq!(observed.continuation(), &baseline_cursor);
            assert_eq!(observed.logical_tick(), baseline_tick);
            assert_eq!(observed.projection().state.time, baseline_tick);
        }
    });
    barrier.wait();
    for _ in 0..4 {
        commit_request(&mut fixture.driver.lock().unwrap(), 0, None);
    }
    worker.join().unwrap();
    let current = fixture.client.read_view(fixture.view(None)).unwrap();
    assert!(current.version().commit.position > baseline.version().commit.position);
    assert_eq!(&current.version().commit, &current.continuation().commit);
    assert_eq!(current.logical_tick(), current.projection().state.time);
    println!(
        "PRE2_FIXED_PROJECTION_PUBLICATION_PIN_PASSED signed_reads=16 real_successor_commits=4"
    );
}
