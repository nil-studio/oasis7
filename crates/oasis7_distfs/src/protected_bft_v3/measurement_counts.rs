//! Direct old-history validations in this thread, excluding fixture construction.
use std::cell::Cell;
thread_local! {
    static BINDINGS: Cell<u64> = const { Cell::new(0) };
    static HISTORY_QCS: Cell<u64> = const { Cell::new(0) };
}
pub(super) fn history_qcs() -> u64 {
    HISTORY_QCS.with(Cell::get)
}
pub(super) fn record_history_qc() {
    HISTORY_QCS.with(|count| count.set(count.get() + 1));
}

pub(super) fn bindings() -> u64 {
    BINDINGS.with(Cell::get)
}
pub(super) fn record_binding() {
    BINDINGS.with(|count| count.set(count.get() + 1));
}
