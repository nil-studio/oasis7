//! Explicit optional Resource archive producer configuration/lifecycle only.
use super::PublishedReadState;
impl PublishedReadState {
    #[cfg(test)]
    pub(crate) fn with_archive_for_test(
        queue: oasis7::world_service::formal_read_service::GoalActivationArchiveQueueV1,
    ) -> Self {
        Self {
            resource_archive: Some(queue),
            ..Self::default()
        }
    }
    pub(crate) fn publication_capture_mode(
        config: &crate::formal_live_config::FormalLiveConfigV1,
    ) -> crate::execution_bridge::formal_writer::PublicationMaterialCaptureV1 {
        crate::execution_bridge::formal_writer::PublicationMaterialCaptureV1::for_storage_capture(
            config.capture_qualified_mutation_material || std::env::var_os("OASIS7_RESOURCE_ACTIVATION_ARCHIVE_DIR").is_some())
    }
    /// Explicit local producer configuration. Missing archive never enables a
    /// Resource route or creates a trusted floor. Ordinary opens create no dir.
    pub(crate) fn from_resource_archive_environment() -> Result<Self, String> {
        let Some(directory) = std::env::var_os("OASIS7_RESOURCE_ACTIVATION_ARCHIVE_DIR") else {
            return Ok(Self::default());
        };
        #[cfg(not(unix))]
        {
            let _ = directory;
            Err("resource activation archive requires private Unix storage".into())
        }
        #[cfg(unix)]
        {
            let maximum = std::env::var("OASIS7_RESOURCE_ACTIVATION_ARCHIVE_MAX_SNAPSHOT_BYTES")
                .map_err(|_| "archive capture requires an explicit local byte bound")?
                .parse::<usize>()
                .map_err(|_| "invalid archive capture byte bound")?;
            let total = std::env::var("OASIS7_RESOURCE_ACTIVATION_ARCHIVE_MAX_TOTAL_BYTES")
                .map_err(|_| "archive requires total byte bound")?
                .parse::<u64>()
                .map_err(|_| "invalid archive total byte bound")?;
            let entries = std::env::var("OASIS7_RESOURCE_ACTIVATION_ARCHIVE_MAX_ENTRIES")
                .map_err(|_| "archive requires entry bound")?
                .parse::<usize>()
                .map_err(|_| "invalid archive entry bound")?;
            let queue = oasis7::world_service::formal_read_service::GoalActivationArchiveQueueV1::open_existing(
                std::path::PathBuf::from(directory), maximum, total, entries).map_err(|e| e.to_string())?;
            Ok(Self {
                resource_archive: Some(queue),
                ..Self::default()
            })
        }
    }
    pub(crate) fn request_resource_archive_stop(&self) -> Result<(), String> {
        if let Some(queue) = &self.resource_archive {
            queue.request_stop().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub(crate) fn finish_resource_archive_stop(&self) -> Result<(), String> {
        if let Some(queue) = &self.resource_archive {
            queue.finish_shutdown().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
