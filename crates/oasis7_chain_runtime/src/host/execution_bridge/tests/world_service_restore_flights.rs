use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(5);

fn actual_restore() -> Result<World, WorldError> {
    let world = oasis7::runtime_host::world::new();
    oasis7::runtime_host::world::from_snapshot(world.snapshot(), world.journal().clone())
}

fn wait_for_join(cache: &Mutex<RestoreCache>, key: &Key) {
    let flight = {
        let cache = cache.lock().unwrap();
        Arc::clone(
            &cache
                .flights
                .iter()
                .find(|(active, _)| active == key)
                .unwrap()
                .1,
        )
    };
    let deadline = Instant::now() + DEADLINE;
    let mut state = flight.state.lock().unwrap();
    while flight.joined.load(Ordering::SeqCst) == 0 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "actual restore follower did not join");
        state = flight.ready.wait_timeout(state, remaining).unwrap().0;
    }
    assert!(matches!(*state, FlightState::Running));
}

#[test]
fn simultaneous_same_content_restores_once_and_returns_isolated_worlds() {
    let cache = Arc::new(Mutex::new(RestoreCache::default()));
    let calls = Arc::new(AtomicUsize::new(0));
    let key = Key::from_bytes(b"same snapshot", b"complete journal");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let leader = {
        let cache = Arc::clone(&cache);
        let calls = Arc::clone(&calls);
        let key = key.clone();
        thread::spawn(move || {
            restore_with(&cache, key, || {
                calls.fetch_add(1, Ordering::SeqCst);
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(DEADLINE).unwrap();
                actual_restore()
            })
        })
    };
    entered_rx.recv_timeout(DEADLINE).unwrap();
    let follower = {
        let cache = Arc::clone(&cache);
        let calls = Arc::clone(&calls);
        let key = key.clone();
        thread::spawn(move || {
            restore_with(&cache, key, || {
                calls.fetch_add(1, Ordering::SeqCst);
                actual_restore()
            })
        })
    };
    wait_for_join(&cache, &key);
    release_tx.send(()).unwrap();
    let baseline = leader.join().unwrap().unwrap();
    let mut returned = follower.join().unwrap().unwrap();
    returned.submit_action(oasis7_runtime::Action::RegisterAgent {
        agent_id: "independent-follower".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(1, 2, 3),
    });
    returned.step().unwrap();
    let next = restore_with(&cache, key, || panic!("warm original must remain retained")).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(next.snapshot() == baseline.snapshot());
    assert!(!next.state().agents.contains_key("independent-follower"));
    let cache = cache.lock().unwrap();
    assert!(cache.flights.is_empty());
    assert_eq!(cache.active_input_bytes, 0);
}

#[test]
fn distinct_admitted_keys_restore_concurrently_and_full_capacity_still_joins() {
    let cache = Arc::new(Mutex::new(RestoreCache::default()));
    let (entered_tx, entered_rx) = mpsc::channel();
    let mut releases = Vec::new();
    let mut leaders = Vec::new();
    let keys = [
        Key::from_bytes(b"a", b"journal"),
        Key::from_bytes(b"b", b"journal"),
    ];
    for key in &keys {
        let (release_tx, release_rx) = mpsc::channel();
        releases.push(release_tx);
        let cache = Arc::clone(&cache);
        let key = key.clone();
        let entered_tx = entered_tx.clone();
        leaders.push(thread::spawn(move || {
            restore_with(&cache, key, || {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(DEADLINE).unwrap();
                actual_restore()
            })
        }));
    }
    // Both closures must enter before either is released: no global restore lock.
    for _ in 0..2 {
        entered_rx.recv_timeout(DEADLINE).unwrap();
    }
    assert_eq!(cache.lock().unwrap().flights.len(), MAX_ENTRIES);
    let follower = {
        let cache = Arc::clone(&cache);
        let key = keys[0].clone();
        thread::spawn(move || {
            restore_with(&cache, key, || {
                panic!("admitted key must join even at capacity")
            })
        })
    };
    wait_for_join(&cache, &keys[0]);
    let overflow_calls = AtomicUsize::new(0);
    restore_with(&cache, Key::from_bytes(b"overflow", b"journal"), || {
        overflow_calls.fetch_add(1, Ordering::SeqCst);
        actual_restore()
    })
    .unwrap();
    assert_eq!(overflow_calls.load(Ordering::SeqCst), 1);
    assert_eq!(cache.lock().unwrap().flights.len(), MAX_ENTRIES);
    for release in releases {
        release.send(()).unwrap();
    }
    for leader in leaders {
        leader.join().unwrap().unwrap();
    }
    follower.join().unwrap().unwrap();
    let cache = cache.lock().unwrap();
    assert!(cache.flights.is_empty());
    assert_eq!(cache.active_input_bytes, 0);
    assert!(cache.entries.len() <= MAX_ENTRIES);
    assert!(cache.retained_input_bytes <= MAX_RETAINED_INPUT_BYTES);
}

#[test]
fn attached_followers_share_failure_but_later_requests_retry() {
    let cache = Arc::new(Mutex::new(RestoreCache::default()));
    let key = Key::from_bytes(b"invalid snapshot", b"journal");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let leader = {
        let cache = Arc::clone(&cache);
        let key = key.clone();
        thread::spawn(move || {
            restore_with(&cache, key, || {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(DEADLINE).unwrap();
                Err(WorldError::JournalMismatch)
            })
        })
    };
    entered_rx.recv_timeout(DEADLINE).unwrap();
    let follower = {
        let cache = Arc::clone(&cache);
        let key = key.clone();
        thread::spawn(move || {
            restore_with(&cache, key, || {
                panic!("attached follower shares original failure")
            })
        })
    };
    wait_for_join(&cache, &key);
    release_tx.send(()).unwrap();
    assert!(matches!(
        leader.join().unwrap(),
        Err(WorldError::JournalMismatch)
    ));
    assert!(matches!(
        follower.join().unwrap(),
        Err(WorldError::JournalMismatch)
    ));
    assert!(cache.lock().unwrap().entries.is_empty());
    assert!(cache.lock().unwrap().flights.is_empty());
    let calls = AtomicUsize::new(0);
    restore_with(&cache, key, || {
        calls.fetch_add(1, Ordering::SeqCst);
        actual_restore()
    })
    .unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "failure must not persist in cache"
    );
}

#[test]
fn unwinding_leader_wakes_attached_follower_for_nonrecursive_retry() {
    let cache = Arc::new(Mutex::new(RestoreCache::default()));
    let key = Key::from_bytes(b"panic snapshot", b"journal");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let leader = {
        let cache = Arc::clone(&cache);
        let key = key.clone();
        thread::spawn(move || {
            restore_with(&cache, key, || {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(DEADLINE).unwrap();
                panic!("controlled restore unwind");
            })
        })
    };
    entered_rx.recv_timeout(DEADLINE).unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let follower = {
        let cache = Arc::clone(&cache);
        let key = key.clone();
        thread::spawn(move || {
            let result = restore_with(&cache, key, actual_restore);
            done_tx.send(result.is_ok()).unwrap();
        })
    };
    wait_for_join(&cache, &key);
    release_tx.send(()).unwrap();
    assert!(leader.join().is_err());
    assert!(
        done_rx.recv_timeout(DEADLINE).unwrap(),
        "unwound flight must notify its follower"
    );
    follower.join().unwrap();
    restore_with(&cache, key, || {
        panic!("follower replacement should be retained")
    })
    .unwrap();
    let cache = cache.lock().unwrap();
    assert!(cache.flights.is_empty());
    assert_eq!(cache.active_input_bytes, 0);
}

#[test]
fn poisoned_shared_original_and_flight_fall_back_without_deadlock() {
    let cache = Mutex::new(RestoreCache::default());
    let key = Key::from_bytes(b"original", b"journal");
    restore_with(&cache, key.clone(), actual_restore).unwrap();
    let original = Arc::clone(&cache.lock().unwrap().entries[0].world);
    let _ = std::panic::catch_unwind(|| {
        let _guard = original.lock().unwrap();
        panic!("controlled retained original poison");
    });
    let calls = AtomicUsize::new(0);
    restore_with(&cache, key, || {
        calls.fetch_add(1, Ordering::SeqCst);
        actual_restore()
    })
    .unwrap();
    let key = Key::from_bytes(b"flight", b"journal");
    let Selection::Lead(leader) = select(&cache, &key) else {
        panic!("leader required")
    };
    let flight = Arc::clone(&leader.flight);
    let _ = std::panic::catch_unwind(|| {
        let _guard = flight.state.lock().unwrap();
        panic!("controlled flight poison");
    });
    restore_with(&cache, key, || {
        calls.fetch_add(1, Ordering::SeqCst);
        actual_restore()
    })
    .unwrap();
    drop(leader);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(cache.lock().unwrap().flights.is_empty());
    assert_eq!(cache.lock().unwrap().active_input_bytes, 0);
}

#[test]
fn old_cleanup_cannot_remove_same_key_replacement_marker() {
    let cache = Mutex::new(RestoreCache::default());
    let key = Key::from_bytes(b"replacement", b"journal");
    let Selection::Lead(old) = select(&cache, &key) else {
        panic!("leader required")
    };
    // Exercise the exact marker-identity boundary if a new leader is registered
    // before stale cleanup. The production cleanup never removes by key alone.
    {
        let mut cache = cache.lock().unwrap();
        cache.flights.clear();
        cache.active_input_bytes = 0;
    }
    let Selection::Lead(new) = select(&cache, &key) else {
        panic!("replacement required")
    };
    drop(old);
    assert!(Arc::ptr_eq(
        &cache.lock().unwrap().flights[0].1,
        &new.flight
    ));
    assert_eq!(
        cache.lock().unwrap().active_input_bytes,
        key.input_bytes().unwrap()
    );
    drop(new);
    assert!(cache.lock().unwrap().flights.is_empty());
    assert_eq!(cache.lock().unwrap().active_input_bytes, 0);
}
