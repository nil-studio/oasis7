//! Opt-in test instrumentation; counters are local to the executing thread.
use std::cell::Cell;
thread_local! {
    static REPLAYS: Cell<u64> = const { Cell::new(0) };
}
pub fn replay_count() -> u64 {
    REPLAYS.with(Cell::get)
}
pub(crate) fn record_replay() {
    REPLAYS.with(|count| count.set(count.get() + 1));
}
