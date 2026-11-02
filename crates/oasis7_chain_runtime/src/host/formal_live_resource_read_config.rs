//! Explicit World operator installation; no ordinary open initializes floors.
use super::*;
#[cfg(unix)]
use oasis7::world_service::{
    formal_read_service::ResourceGoalArchiveReaderV2,
    private_goal_store::PrivateGoalStore,
    resource_world_paid_ledger_v1::{
        PaidResourceReadIssuerPinV1, PaidResourceReadStageV1, WorldResourcePaidLedgerV1,
    },
    resource_world_read_service_v1::ResourceWorldReadServiceV1,
};
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceWorldReadConfigV1 {
    api_version: u8,
    issuer: PaidResourceReadIssuerPinV1,
    floor_primary: PathBuf,
    external_guard: PathBuf,
    floor_key_file: PathBuf,
    private_goal_directory: PathBuf,
    private_goal_key_file: PathBuf,
    archive_directory: PathBuf,
    maximum_events: u64,
    maximum_archive_entries: usize,
    maximum_archive_total_bytes: u64,
    maximum_archive_record_bytes: usize,
}
impl FormalLiveService {
    /// Default is absent/unsupported. Presence supplies trusted public operator
    /// configuration, not a request profile or a substitute for actual paid
    /// issuer/Current/Root/Owner checks. Missing state never gets initialized.
    pub(crate) fn with_resource_world_reads_from_environment(self) -> Result<Self, String> {
        let Some(raw) = std::env::var_os("OASIS7_RESOURCE_PAID_WORLD_CONFIG_JSON") else {
            return Ok(self);
        };
        #[cfg(not(unix))]
        {
            let _ = raw;
            Err("private Resource World profile requires Unix storage".into())
        }
        #[cfg(unix)]
        {
            let raw = raw
                .into_string()
                .map_err(|_| "invalid Resource World configuration")?;
            if raw.is_empty() || raw.len() > 32 * 1024 {
                return Err("Resource World configuration bound".into());
            }
            let config: ResourceWorldReadConfigV1 = serde_json::from_str(&raw)
                .map_err(|_| "invalid closed Resource World configuration")?;
            if config.api_version != 1
                || config.issuer.allowed_stage != PaidResourceReadStageV1::OwnerSetupFinal
            {
                return Err("unsupported Resource World profile".into());
            }
            let floor_key = oasis7::world_service::private_key_file::read_private_key_file(
                &config.floor_key_file,
            )?;
            let goal_key = oasis7::world_service::private_key_file::read_private_key_file(
                &config.private_goal_key_file,
            )?;
            let ledger = WorldResourcePaidLedgerV1::open_existing(
                &config.floor_primary,
                &config.external_guard,
                *floor_key,
                config.issuer,
                config.maximum_events,
            )?;
            let archive = ResourceGoalArchiveReaderV2::open_existing_path(
                &config.archive_directory,
                self.trust.initial_trust.clone(),
                config.maximum_archive_entries,
                config.maximum_archive_total_bytes,
                config.maximum_archive_record_bytes,
            )
            .map_err(|e| e.to_string())?;
            let goals = PrivateGoalStore::open(&config.private_goal_directory, *goal_key)?;
            let resource = ResourceWorldReadServiceV1::from_configured_stores(
                self.world.clone(),
                hex::encode(self.signer.verifying_key().to_bytes()),
                ledger,
                archive,
                goals,
            )
            .map_err(|e| e.to_string())?;
            Ok(self.with_resource_world_reads(Arc::new(resource)))
        }
    }
}
