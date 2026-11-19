//! Chain host configuration and native runtime implementation.
//!
//! `execution_config` remains available without the network host feature.
//! The native host is compiled into this library so entry-point-only changes can
//! reuse its compiled implementation.

#[cfg(not(target_arch = "wasm32"))]
pub mod execution_config;

#[cfg(all(not(target_arch = "wasm32"), feature = "node-libp2p"))]
include!("host.rs");

#[cfg(not(target_arch = "wasm32"))]
pub mod incremental_cognition_history_store;
pub mod incremental_emergency_observation_store;

#[cfg(all(unix, not(target_arch = "wasm32")))]
pub mod iws_genesis;

#[cfg(all(unix, not(target_arch = "wasm32")))]
pub mod iws_host_runtime;

#[cfg(all(unix, not(target_arch = "wasm32")))]
mod iws_normal_input;
#[cfg(all(unix, not(target_arch = "wasm32")))]
mod iws_normal_input_origin;

mod committed_input_metadata;

/// The fixture image delegates directly to the shipping configuration and
/// JSONL consumer; it does not substitute a test Cold constructor or owner.
#[cfg(all(unix, feature = "iws-host-native-test-support"))]
pub fn native_worker_fixture_iws_formal_live_cli_v1(path: &std::path::Path) -> Result<(), String> {
    if iws_live::try_run(path)? {
        Ok(())
    } else {
        Err("native clean fixture requires the IWS production CLI schema".into())
    }
}

/// Executes the real native fixture within its own captured worker ELF.
#[cfg(all(unix, feature = "iws-host-native-test-support"))]
pub fn native_worker_fixture_iws_live_node_v1(
    root: &std::path::Path,
    cold_recovery: bool,
) -> Result<Vec<u8>, String> {
    iws_host_runtime::native_closure_test_support::live_node::run(root, cold_recovery)
}

/// Executes the real native fixture within its own captured worker ELF.
#[cfg(all(unix, feature = "iws-host-native-test-support"))]
pub fn native_worker_fixture_iws_nonempty_normal_cold_v1(
    root: &std::path::Path,
) -> Result<Vec<u8>, String> {
    iws_host_runtime::native_closure_test_support::run(root)
}

#[cfg(all(unix, feature = "iws-host-native-test-support"))]
pub fn native_worker_fixture_iws_normal_cold_failure_v1(
    root: &std::path::Path,
    before_publication: bool,
) -> Result<Vec<u8>, String> {
    iws_host_runtime::native_closure_test_support::run_with_fault(root, before_publication)
}

#[cfg(all(unix, feature = "iws-host-native-test-support"))]
pub fn native_worker_fixture_iws_checkpoint_suffix_v1(
    root: &std::path::Path,
) -> Result<Vec<u8>, String> {
    iws_host_runtime::native_closure_test_support::run_checkpoint_suffix(root)
}

#[cfg(all(unix, feature = "iws-host-native-test-support"))]
pub fn native_worker_fixture_iws_source_work_return_v1(
    root: &std::path::Path,
    partial_source: bool,
) -> Result<Vec<u8>, String> {
    iws_host_runtime::native_closure_test_support::run_source_work_return_fault(
        root,
        partial_source,
    )
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
mod iws_normal_input_decode;

#[cfg(all(unix, not(target_arch = "wasm32")))]
mod iws_normal_input_plan;
