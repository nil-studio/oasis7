use super::*;

// Keep the long scenario's two successive execution drivers on the heap.
// Construct them in separate frames so Box::new's by-value temporary is not
// retained in the scenario frame for every subsequent restore/HTTP call.
pub(super) fn boxed_context_driver_fixture() -> (
    std::path::PathBuf,
    Box<NodeRuntimeExecutionDriver>,
    WorldIdentity,
    String,
    String,
) {
    let (dir, driver, world, owner, public) =
        canonical_agent_chat::driver_fixture_with_verified_authority();
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
