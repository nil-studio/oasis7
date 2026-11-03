//! Real protected Commit exercises same-backend verification without a self-read.
//! This case stops before the separately implemented Active adoption boundary.
use super::*;
use std::cell::Cell;
thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }
pub(super) fn requested() -> bool {
    ENABLED.with(Cell::get)
}
#[test]
fn wire3_protected_source_commit_borrows_originals_without_network_reentry() {
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
