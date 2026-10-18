//! Successful detached restores only. This is never a record, CAS or authority cache.
use oasis7_runtime::WorldError;
use oasis7_runtime::{Journal, Snapshot, World};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

const MAX_ENTRIES: usize = 2;
const MAX_ENTRY_INPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_RETAINED_INPUT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
struct Key {
    snapshot_digest: [u8; 32],
    journal_digest: [u8; 32],
    snapshot_len: usize,
    journal_len: usize,
}
impl Key {
    fn from_bytes(snapshot: &[u8], journal: &[u8]) -> Self {
        Self {
            snapshot_digest: blake3::derive_key("oasis7.pinned-world.snapshot.v1", snapshot),
            journal_digest: blake3::derive_key("oasis7.pinned-world.journal.v1", journal),
            snapshot_len: snapshot.len(),
            journal_len: journal.len(),
        }
    }
    fn input_bytes(&self) -> Option<usize> {
        self.snapshot_len.checked_add(self.journal_len)
    }
}
struct Entry {
    key: Key,
    world: Arc<Mutex<World>>,
}
#[derive(Default)]
pub(super) struct RestoreCache {
    entries: VecDeque<Entry>,
    // Serialized source lengths are a retention proxy, not the World heap size.
    retained_input_bytes: usize,
    flights: Vec<(Key, Arc<Flight>)>,
    active_input_bytes: usize,
}
static CACHE: OnceLock<Mutex<RestoreCache>> = OnceLock::new();
pub(super) fn shared() -> &'static Mutex<RestoreCache> {
    CACHE.get_or_init(|| Mutex::new(RestoreCache::default()))
}

pub(super) fn restore(
    cache: &Mutex<RestoreCache>,
    snapshot_bytes: &[u8],
    journal_bytes: &[u8],
    snapshot: Snapshot,
    journal: Journal,
) -> Result<World, oasis7_runtime::WorldError> {
    restore_with(
        cache,
        Key::from_bytes(snapshot_bytes, journal_bytes),
        || oasis7::runtime_host::world::from_snapshot(snapshot, journal),
    )
}

type Restored = Arc<Mutex<World>>;
enum FlightState {
    Running,
    Finished(Result<Restored, WorldError>),
    Abandoned,
}
struct Flight {
    state: Mutex<FlightState>,
    ready: Condvar,
    #[cfg(test)]
    joined: std::sync::atomic::AtomicUsize,
}
impl Flight {
    fn new() -> Self {
        Self {
            state: Mutex::new(FlightState::Running),
            ready: Condvar::new(),
            #[cfg(test)]
            joined: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}
struct Leader<'a> {
    cache: &'a Mutex<RestoreCache>,
    key: Key,
    flight: Arc<Flight>,
}
impl Drop for Leader<'_> {
    fn drop(&mut self) {
        // Even a panicking restore must release all predicate waiters.
        let mut state = self
            .flight
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if matches!(*state, FlightState::Running) {
            *state = FlightState::Abandoned;
        }
        drop(state);
        let mut cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(index) = cache
            .flights
            .iter()
            .position(|(_, flight)| Arc::ptr_eq(flight, &self.flight))
        {
            cache.flights.swap_remove(index);
            cache.active_input_bytes -= self.key.input_bytes().unwrap_or(0);
        }
        drop(cache);
        self.flight.ready.notify_all();
    }
}
enum Selection<'a> {
    Hit(Restored),
    Join(Arc<Flight>),
    Lead(Leader<'a>),
    Bypass,
}
fn select<'a>(cache: &'a Mutex<RestoreCache>, key: &Key) -> Selection<'a> {
    let Some(bytes) = key
        .input_bytes()
        .filter(|bytes| *bytes <= MAX_ENTRY_INPUT_BYTES)
    else {
        return Selection::Bypass;
    };
    let Ok(mut state) = cache.lock() else {
        return Selection::Bypass;
    };
    if let Some(entry) = state.entries.iter().find(|entry| entry.key == *key) {
        return Selection::Hit(Arc::clone(&entry.world));
    }
    // Joining an admitted key still works when all admission slots are occupied.
    if let Some((_, flight)) = state.flights.iter().find(|(active, _)| active == key) {
        return Selection::Join(Arc::clone(flight));
    }
    if state.flights.len() >= MAX_ENTRIES
        || state.active_input_bytes.saturating_add(bytes) > MAX_RETAINED_INPUT_BYTES
    {
        return Selection::Bypass;
    }
    let flight = Arc::new(Flight::new());
    state.active_input_bytes += bytes;
    state.flights.push((key.clone(), Arc::clone(&flight)));
    Selection::Lead(Leader {
        cache,
        key: key.clone(),
        flight,
    })
}
fn retain(cache: &Mutex<RestoreCache>, key: Key, world: &Restored) {
    let Some(bytes) = key
        .input_bytes()
        .filter(|bytes| *bytes <= MAX_ENTRY_INPUT_BYTES)
    else {
        return;
    };
    let Ok(mut cache) = cache.lock() else { return };
    if cache.entries.iter().any(|entry| entry.key == key) {
        return;
    }
    while cache.entries.len() >= MAX_ENTRIES
        || cache.retained_input_bytes.saturating_add(bytes) > MAX_RETAINED_INPUT_BYTES
    {
        let Some(evicted) = cache.entries.pop_front() else {
            break;
        };
        cache.retained_input_bytes -= evicted.key.input_bytes().unwrap_or(0);
    }
    cache.retained_input_bytes += bytes;
    cache.entries.push_back(Entry {
        key,
        world: Arc::clone(world),
    });
}

fn restore_with(
    cache: &Mutex<RestoreCache>,
    key: Key,
    restore: impl FnOnce() -> Result<World, WorldError>,
) -> Result<World, WorldError> {
    let mut restore = Some(restore);
    loop {
        let completed = match select(cache, &key) {
            Selection::Hit(world) => Some(Ok(world)),
            Selection::Join(flight) => {
                let Ok(mut state) = flight.state.lock() else {
                    return restore.take().unwrap()();
                };
                #[cfg(test)]
                {
                    flight
                        .joined
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    flight.ready.notify_all();
                }
                while matches!(*state, FlightState::Running) {
                    state = match flight.ready.wait(state) {
                        Ok(state) => state,
                        Err(_) => return restore.take().unwrap()(),
                    };
                }
                match &*state {
                    FlightState::Finished(result) => Some(result.clone()),
                    FlightState::Abandoned => None,
                    FlightState::Running => unreachable!("predicate wait completed"),
                }
            }
            Selection::Lead(leader) => {
                // No global or per-flight mutex is held during expensive restore.
                let result = restore.take().unwrap()();
                let shared = result
                    .as_ref()
                    .map(|world| Arc::new(Mutex::new(world.clone())))
                    .map_err(Clone::clone);
                if let Ok(world) = &shared {
                    retain(cache, key.clone(), world);
                }
                *leader
                    .flight
                    .state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = FlightState::Finished(shared);
                drop(leader); // Publishes the terminal state, notifies and removes this marker.
                return result;
            }
            Selection::Bypass => return restore.take().unwrap()(),
        };
        if let Some(result) = completed {
            let world = result?;
            // The retained original remains immutable and no global lock covers cloning.
            return match world.lock() {
                Ok(world) => Ok(world.clone()),
                Err(_) => restore.take().unwrap()(),
            };
        }
        // A leader unwound. Retry admission without recursive stack growth.
    }
}

#[cfg(test)]
#[path = "tests/world_service_restore_cache.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/world_service_restore_flights.rs"]
mod flight_tests;
