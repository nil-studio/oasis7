use super::*;

// Keep the long scenario's two successive execution drivers on the heap.
// Construct them in separate frames so Box::new's by-value temporary is not
// retained in the scenario frame for every subsequent restore/HTTP call.
pub(super) fn boxed_context_driver_fixture(
    native: bool,
    other_provider: bool,
) -> (
    std::path::PathBuf,
    Box<NodeRuntimeExecutionDriver>,
    WorldIdentity,
    String,
    String,
) {
    let (dir, driver, world, owner, public) = if other_provider {
        canonical_agent_chat::driver_fixture_without_provider_with_unrelated_provider()
    } else if native {
        canonical_agent_chat::driver_fixture_without_provider()
    } else {
        canonical_agent_chat::driver_fixture_with_verified_authority()
    };
    (dir, Box::new(driver), world, owner, public)
}
pub(super) fn boxed_context_driver_restart(
    dir: &std::path::Path,
    bootstrap: oasis7_node::NodeExecutionBootstrap,
) -> Box<NodeRuntimeExecutionDriver> {
    Box::new(
        NodeRuntimeExecutionDriver::new_with_local_bootstrap(
            dir.join("state.json"),
            dir.join("world"),
            dir.join("records"),
            dir.join("store"),
            &oasis7_proto::storage_profile::StorageProfileConfig::default(),
            bootstrap,
        )
        .unwrap(),
    )
}

#[inline(never)]
pub(super) fn boxed_context_value<T>(make: impl FnOnce() -> T) -> Box<T> {
    Box::new(make())
}

#[inline(never)]
pub(super) fn run_context_stage(run: impl FnOnce()) {
    run();
}

pub(super) fn assert_fixture_authorities(
    driver: &NodeRuntimeExecutionDriver,
    native: bool,
    other_provider: bool,
) {
    if native {
        assert!(
            driver
                .execution_world
                .capability_invocation_contexts()
                .values()
                .all(|context| !matches!(&context.subject, oasis7_wasm_abi::CapabilitySubject::Agent { agent_id, .. } if agent_id == "agent-a"))
        );
        assert!(
            driver
                .execution_world
                .canonical_agent_service_context("agent-a")
                .is_err()
        );
    }
    if other_provider {
        assert!(
            driver
                .execution_world
                .canonical_agent_service_context("other-agent")
                .is_ok()
        );
    }
}

#[test]
fn private_task_context_native_complete_prepare_capture_read_action_and_restart() {
    super::complete_context_scenario(None, false);
}
#[test]
fn private_task_context_explicit_native_authority_complete_without_provider_selection() {
    super::complete_context_scenario(
        Some(oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1),
        false,
    );
}
#[test]
fn private_task_context_native_complete_with_unrelated_provider() {
    super::complete_context_scenario(
        Some(oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1),
        true,
    );
}

#[test]
fn private_task_context_legacy_prefix_capture_read_and_restart_remain_verifiable() {
    super::complete_context_scenario_with_revision(
        Some(oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1),
        false,
        0,
    );
}
