//! Physical incremental-state contracts shared with the complete integration suite.
//!
//! Keep the original Raw/Zstd reopen, continuation, ordering, and bounded-read cases.
//! These exercise candidate persistence and queries, not canonical acceptance.
#![cfg(all(unix, not(target_arch = "wasm32")))]

// The shared fixture imports AgentState through the oasis7 facade. Bind that name
// to the same type's owning library without adding the application dependency.
extern crate oasis7_world_state as oasis7;

#[path = "../../oasis7/tests/incremental_due_module_batch.rs"]
mod due_module_batch;
// The unchanged standalone cases each own this fixture module. Keep their
// original scopes when composing the cases into one fast integration target.
#[allow(clippy::duplicate_mod)]
#[path = "../../oasis7/tests/incremental_runtime_allocators.rs"]
mod runtime_allocators;
#[path = "../../oasis7/tests/incremental_runtime_commit.rs"]
mod runtime_commit;
