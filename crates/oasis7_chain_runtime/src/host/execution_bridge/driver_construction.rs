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
            crate::execution_config::ExecutionDriverConfig::from(storage_profile),
            None,
        )
    }
}

impl NodeRuntimeExecutionDriver {
    pub(crate) fn new_with_participant_control(
        state_path: std::path::PathBuf,
        world_dir: std::path::PathBuf,
        records_dir: std::path::PathBuf,
        storage_root: std::path::PathBuf,
        storage_profile: &StorageProfileConfig,
        participant: crate::execution_config::DevLocalParticipantConfig,
    ) -> Result<Self, String> {
        if participant.authority.is_some()
            && storage_profile.profile != oasis7_proto::storage_profile::StorageProfile::DevLocal
        {
            return Err(
                "local participant constructor requires actual DevLocal storage profile".into(),
            );
        }
        let mut config = crate::execution_config::ExecutionDriverConfig::from(storage_profile);
        config.devlocal_participant = participant;
        Self::new_with_config_and_local_bootstrap(
            state_path,
            world_dir,
            records_dir,
            storage_root,
            config,
            None,
        )
    }
}

impl NodeRuntimeExecutionDriver {
    #[cfg(not(test))]
    pub(crate) fn new_with_participant_paths(
        paths: &crate::RuntimePaths,
        profile: &StorageProfileConfig,
        participant: crate::execution_config::DevLocalParticipantConfig,
    ) -> Result<Self, String> {
        Self::new_with_participant_control(
            paths.execution_bridge_state_path.clone(),
            paths.execution_world_dir.clone(),
            paths.execution_records_dir.clone(),
            paths.storage_root.clone(),
            profile,
            participant,
        )
    }
}
