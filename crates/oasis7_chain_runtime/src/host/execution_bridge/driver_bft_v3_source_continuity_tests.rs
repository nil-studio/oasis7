//! Continuity remains bound to the exact actual replay and complete original carry.
use super::*;
use std::cell::Cell;
thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }
pub(super) fn requested() -> bool {
    ENABLED.with(Cell::get)
}
#[test]
fn wire3_protected_continuity_rejects_changed_cut_source_and_missing_carry() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            ENABLED.with(|enabled| enabled.set(true));
            install_case_with_retention(
                false,
                false,
                66,
                CandidateRetentionProfile::PublicationReserved,
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
