//! Fixed Current finalization stages. No complete-tick or acceptance capability
//! is issued here: the finalizer must finish every original stage and retain
//! the genuine Node/worker sources before any outer execution result can mint.
#[path="incremental_normal_tick_finalize_threat_fold.rs"]
pub(in crate::world) mod threat_fold;

#[path = "incremental_normal_tick_finalize_threat.rs"]
mod threat;

#[path = "incremental_normal_tick_finalize_commitment.rs"]
pub(in crate::world) mod commitment;

#[path = "incremental_normal_tick_finalize_journal.rs"]
pub(super) mod journal;

#[path = "incremental_normal_tick_finalize_consensus.rs"]
pub(in crate::world) mod consensus;
