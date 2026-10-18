use super::{NodeRuntimeExecutionDriver, StorageProfileConfig};

impl NodeRuntimeExecutionDriver {
    pub(crate) fn set_controlled_capture_enabled(&mut self, enabled: bool) -> Result<(), String> {
        if enabled && !cfg!(unix) {
            return Err("durable history capture is unsupported on this platform".into());
        }
        if enabled && !cfg!(feature = "wasmtime") {
            return Err("historical capture requires the real wasmtime executor".into());
        }
        self.controlled_capture_enabled = enabled;
        Ok(())
    }
    pub(crate) fn new(
        state_path: std::path::PathBuf,
        world_dir: std::path::PathBuf,
        records_dir: std::path::PathBuf,
        storage_root: std::path::PathBuf,
    ) -> Result<Self, String> {
        Self::new_with_storage_profile(
            state_path,
            world_dir,
            records_dir,
            storage_root,
            &StorageProfileConfig::default(),
        )
    }

    pub(crate) fn new_with_storage_profile(
        state_path: std::path::PathBuf,
        world_dir: std::path::PathBuf,
        records_dir: std::path::PathBuf,
        storage_root: std::path::PathBuf,
        storage_profile: &StorageProfileConfig,
    ) -> Result<Self, String> {
        Self::new_with_config_and_local_bootstrap(
            state_path,
            world_dir,
            records_dir,
            storage_root,
            oasis7_chain_runtime::execution_config::ExecutionDriverConfig::from(storage_profile),
            None,
        )
    }
}
