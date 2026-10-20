//! Local, preserving data-directory migration for an exact stopped observer.
//! The source is never deleted. An immutable source-side intent makes an
//! interrupted attempt query-only if its destination is missing.
use super::{actions, local_io as io, model::NodeConfig, types::LocalInstance};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

const MIGRATIONS_DIR: &str = "management-migrations";
const MAX_ENTRIES: usize = 1_000_000;
const MAX_COPY_BYTES: u64 = 1 << 40;
const COPY_SPACE_RESERVE_BYTES: u64 = 64 * 1024 * 1024;
const COPY_METADATA_RESERVE_PER_ENTRY: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MigrationCapacityOutcomeV1 {
    Ready,
    CapacityInsufficient,
    SourceSizeLimitExceeded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DataDirectoryMigrationCapacityPlanV1 {
    pub api_version: String,
    pub source_data_dir: PathBuf,
    pub destination_data_dir: PathBuf,
    pub aggregate_source_bytes: u64,
    pub entry_count: u64,
    pub available_destination_bytes: u64,
    pub required_destination_bytes: u64,
    pub outcome: MigrationCapacityOutcomeV1,
}

impl DataDirectoryMigrationCapacityPlanV1 {
    pub(crate) fn blocker(&self) -> Option<&'static str> {
        match self.outcome {
            MigrationCapacityOutcomeV1::Ready => None,
            MigrationCapacityOutcomeV1::CapacityInsufficient => Some("CAPACITY_INSUFFICIENT"),
            MigrationCapacityOutcomeV1::SourceSizeLimitExceeded => Some("MIGRATION_SIZE_LIMIT"),
        }
    }

    pub(crate) fn validate(&self, source: &Path, destination: &Path) -> Result<(), String> {
        if self.api_version != "oasis7.local-data-migration-capacity/v1"
            || self.source_data_dir != source
            || self.destination_data_dir != destination
            || self.entry_count > MAX_ENTRIES as u64
            || self.required_destination_bytes
                != required_capacity_bytes(self.aggregate_source_bytes, self.entry_count)
            || self.outcome
                != capacity_outcome(
                    self.aggregate_source_bytes,
                    self.entry_count,
                    self.available_destination_bytes,
                )
        {
            return Err(
                "migration capacity preview differs from its source, destination or budget".into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DataDirectoryMigrationReceiptV1 {
    pub api_version: String,
    pub operation_id: String,
    pub request_digest: String,
    pub source_data_dir: PathBuf,
    pub destination_data_dir: PathBuf,
    pub world_id: String,
    pub chain_id: String,
    pub node_id: String,
    pub root_public_key: String,
    pub peer_id: String,
}

/// Estimate the complete source tree and destination capacity without making
/// an intent or staging directory. The returned enum is suitable for an
/// operation preview; execution repeats the estimate under the source fence.
pub(crate) fn plan_data_directory_migration(
    source_config: &Path,
    destination: &Path,
) -> Result<DataDirectoryMigrationCapacityPlanV1, String> {
    if !destination.is_absolute() {
        return Err("migration destination requires an absolute path".into());
    }
    io::reject_symlinks(destination)?;
    if destination.exists() {
        return Err("migration destination already exists".into());
    }
    let parent = destination
        .parent()
        .ok_or("migration destination parent missing")?;
    super::super::controlled_live_config::require_private_owned(parent, true)?;
    let config = NodeConfig::load(source_config)?;
    config.verify_identity()?;
    let source = fs::canonicalize(&config.identity.data_dir).map_err(|error| error.to_string())?;
    if source == destination || destination.starts_with(&source) || source.starts_with(destination)
    {
        return Err("data-directory migration source/destination overlap is unsupported".into());
    }
    let (aggregate_source_bytes, entry_count) = estimate_tree(&source)?;
    let available_destination_bytes = available_bytes(parent)?;
    let required_destination_bytes = required_capacity_bytes(aggregate_source_bytes, entry_count);
    let outcome = capacity_outcome(
        aggregate_source_bytes,
        entry_count,
        available_destination_bytes,
    );
    Ok(DataDirectoryMigrationCapacityPlanV1 {
        api_version: "oasis7.local-data-migration-capacity/v1".into(),
        source_data_dir: source,
        destination_data_dir: destination.to_path_buf(),
        aggregate_source_bytes,
        entry_count,
        available_destination_bytes,
        required_destination_bytes,
        outcome,
    })
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrationIntentV1 {
    api_version: String,
    operation_id: String,
    request_digest: String,
    source_data_dir: PathBuf,
    destination_data_dir: PathBuf,
}

pub(crate) fn migrate_data_directory(
    source_config: &Path,
    destination: &Path,
    operation_id: &str,
    request_digest: &str,
    expected: &LocalInstance,
) -> Result<DataDirectoryMigrationReceiptV1, String> {
    if !destination.is_absolute()
        || !is_digest(request_digest)
        || operation_id.trim().is_empty()
        || operation_id.len() > 128
    {
        return Err("invalid data-directory migration identity".into());
    }
    let config = NodeConfig::load(source_config)?;
    config.verify_identity()?;
    let source = fs::canonicalize(&config.identity.data_dir).map_err(|error| error.to_string())?;
    io::reject_symlinks(destination)?;
    if source == destination || destination.starts_with(&source) || source.starts_with(destination)
    {
        return Err("data-directory migration source/destination overlap is unsupported".into());
    }
    let parent = destination
        .parent()
        .ok_or("migration destination parent missing")?;
    super::super::controlled_live_config::require_private_owned(parent, true)?;
    let binding = actions::binding(source_config)?;
    if binding.data_dir != source
        || binding.current.as_ref().is_none_or(|actual| {
            actual.incarnation != expected.incarnation
                || actual.generation != expected.generation
                || actual.config_digest != expected.config_digest
                || actual.action_id != expected.action_id
                || !matches!(actual.state.as_str(), "stopped" | "failed")
        })
    {
        return Err("data-directory migration requires the exact stopped source instance".into());
    }
    let source_fence = super::process::admission_lock(&source)?;
    let action_dir = source.join(MIGRATIONS_DIR);
    io::private_dir(&action_dir)?;
    let intent_path = action_dir.join(format!("{}.json", io::digest(&operation_id)?));
    let intended = MigrationIntentV1 {
        api_version: "oasis7.local-data-migration-intent/v1".into(),
        operation_id: operation_id.into(),
        request_digest: request_digest.into(),
        source_data_dir: source.clone(),
        destination_data_dir: destination.to_path_buf(),
    };
    if intent_path.exists() {
        let saved: MigrationIntentV1 = io::read(&intent_path)?;
        if saved.api_version != intended.api_version
            || saved.operation_id != intended.operation_id
            || saved.request_digest != intended.request_digest
            || saved.source_data_dir != intended.source_data_dir
            || saved.destination_data_dir != intended.destination_data_dir
        {
            return Err("data migration operation identity was reused with another request".into());
        }
        if !destination.exists() {
            return Err("ORIGINAL_OUTCOME_UNKNOWN: migration intent exists but destination is absent; query only".into());
        }
        let receipt = read_destination_receipt(destination)?;
        validate_destination(&receipt, &intended, &config)?;
        return Ok(receipt);
    }
    if destination.exists() {
        return Err(
            "migration destination already exists without this operation's source intent".into(),
        );
    }
    let capacity = plan_data_directory_migration(source_config, destination)?;
    if let Some(blocker) = capacity.blocker() {
        return Err(format!(
            "{blocker}: migration requires {} bytes across {} entries, destination has {} bytes available",
            capacity.required_destination_bytes,
            capacity.entry_count,
            capacity.available_destination_bytes
        ));
    }
    io::write(&intent_path, &intended)?;
    // Recheck while the source action-admission fence is held. This excludes a
    // local start/stop admission during the copy while preserving the source.
    let locked = actions::binding(source_config)?;
    if locked.current.as_ref().is_none_or(|actual| {
        actual.incarnation != expected.incarnation
            || actual.generation != expected.generation
            || actual.config_digest != expected.config_digest
            || !matches!(actual.state.as_str(), "stopped" | "failed")
    }) {
        return Err("source instance changed after migration intent; query only".into());
    }
    let stage = parent.join(format!(".oasis7-migrate-{}", io::random_id()?));
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&stage).map_err(|error| error.to_string())?;
    let staged = (|| {
        copy_tree(
            &source,
            &stage,
            capacity.aggregate_source_bytes,
            &intent_path,
        )?;
        let mut migrated: NodeConfig = io::read(&stage.join("node.json"))?;
        if migrated.identity != config.identity
            || migrated.role != config.role
            || migrated.rpc_bind != config.rpc_bind
            || migrated.additional_peers != config.additional_peers
        {
            return Err("copied node identity/config differs from migration source".into());
        }
        migrated.identity.data_dir = destination.to_path_buf();
        io::write(&stage.join("node-binding.json"), &migrated.identity)?;
        io::write(&stage.join("node.json"), &migrated)?;
        let receipt = DataDirectoryMigrationReceiptV1 {
            api_version: "oasis7.local-data-migration-receipt/v1".into(),
            operation_id: operation_id.into(),
            request_digest: request_digest.into(),
            source_data_dir: source.clone(),
            destination_data_dir: destination.to_path_buf(),
            world_id: config.identity.chain.world_id().into(),
            chain_id: config.identity.chain.chain_id().into(),
            node_id: config.identity.node_id.clone(),
            root_public_key: config.identity.root_public_key.clone(),
            peer_id: config.identity.peer_id.clone(),
        };
        io::write(&stage.join("migration-receipt.json"), &receipt)?;
        File::open(&stage)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
        rename_directory_noreplace(&stage, destination)?;
        io::sync_dir(parent)?;
        let migrated = NodeConfig::load(&destination.join("node.json"))?;
        migrated.verify_identity()?;
        if migrated.identity.chain.world_id() != config.identity.chain.world_id()
            || migrated.identity.chain.chain_id() != config.identity.chain.chain_id()
            || migrated.identity.node_id != config.identity.node_id
            || migrated.identity.peer_id != config.identity.peer_id
            || migrated.identity.root_public_key != config.identity.root_public_key
        {
            return Err("published migration changed node/world identity".into());
        }
        Ok(receipt)
    })();
    if staged.is_err() && stage.exists() {
        let _ = fs::remove_dir_all(&stage);
    }
    drop(source_fence);
    staged
}

fn rename_directory_noreplace(source: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::ffi::OsStrExt;
        let source = std::ffi::CString::new(source.as_os_str().as_bytes())
            .map_err(|_| "migration staging path contains NUL")?;
        let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())
            .map_err(|_| "migration destination path contains NUL")?;
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                destination.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::ffi::OsStrExt;
        let source = std::ffi::CString::new(source.as_os_str().as_bytes())
            .map_err(|_| "migration staging path contains NUL")?;
        let destination = std::ffi::CString::new(destination.as_os_str().as_bytes())
            .map_err(|_| "migration destination path contains NUL")?;
        let result =
            unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
        if result != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (source, destination);
        Err("atomic no-replace directory publication is unsupported on this platform".into())
    }
}

/// Read an exact source-side intent and its published destination. This never
/// creates a new intent or destination; an absent intent after a prior local
/// dispatch is an unresolved outcome, not permission to repeat the copy.
pub(crate) fn query_data_directory_migration(
    source_config: &Path,
    destination: &Path,
    operation_id: &str,
    request_digest: &str,
    expected: &LocalInstance,
) -> Result<Option<DataDirectoryMigrationReceiptV1>, String> {
    if !destination.is_absolute()
        || !is_digest(request_digest)
        || operation_id.trim().is_empty()
        || operation_id.len() > 128
    {
        return Err("invalid data-directory migration identity".into());
    }
    let config = NodeConfig::load(source_config)?;
    let source = fs::canonicalize(&config.identity.data_dir).map_err(|error| error.to_string())?;
    io::reject_symlinks(destination)?;
    if source == destination || destination.starts_with(&source) || source.starts_with(destination)
    {
        return Err("data-directory migration source/destination overlap is unsupported".into());
    }
    let _source_fence = super::process::admission_lock(&source)?;
    let binding = actions::binding(source_config)?;
    if binding.data_dir != source
        || binding.current.as_ref().is_none_or(|actual| {
            actual.incarnation != expected.incarnation
                || actual.generation != expected.generation
                || actual.config_digest != expected.config_digest
                || actual.action_id != expected.action_id
                || !matches!(actual.state.as_str(), "stopped" | "failed")
        })
    {
        return Err("data-directory migration query exact stopped source changed".into());
    }
    let action_dir = source.join(MIGRATIONS_DIR);
    io::reject_symlinks(&action_dir)?;
    let intent_path = action_dir.join(format!("{}.json", io::digest(&operation_id)?));
    io::reject_symlinks(&intent_path)?;
    if !intent_path.exists() {
        if destination.exists() {
            return Err(
                "migration destination exists without this operation's immutable source intent"
                    .into(),
            );
        }
        return Ok(None);
    }
    super::super::controlled_live_config::require_private_owned(&action_dir, true)?;
    let saved: MigrationIntentV1 = io::read(&intent_path)?;
    let intended = MigrationIntentV1 {
        api_version: "oasis7.local-data-migration-intent/v1".into(),
        operation_id: operation_id.into(),
        request_digest: request_digest.into(),
        source_data_dir: source.clone(),
        destination_data_dir: destination.to_path_buf(),
    };
    if saved.api_version != intended.api_version
        || saved.operation_id != intended.operation_id
        || saved.request_digest != intended.request_digest
        || saved.source_data_dir != intended.source_data_dir
        || saved.destination_data_dir != intended.destination_data_dir
    {
        return Err("data migration operation identity was reused with another request".into());
    }
    if !destination.exists() {
        return Err("ORIGINAL_OUTCOME_UNKNOWN: migration intent exists but destination is absent; query only".into());
    }
    let receipt = read_destination_receipt(destination)?;
    validate_destination(&receipt, &intended, &config)?;
    Ok(Some(receipt))
}

fn read_destination_receipt(path: &Path) -> Result<DataDirectoryMigrationReceiptV1, String> {
    let directory = fs::canonicalize(path).map_err(|error| error.to_string())?;
    super::super::controlled_live_config::require_private_owned(&directory, true)?;
    io::read(&directory.join("migration-receipt.json"))
}

fn validate_destination(
    receipt: &DataDirectoryMigrationReceiptV1,
    intent: &MigrationIntentV1,
    source: &NodeConfig,
) -> Result<(), String> {
    let config = NodeConfig::load(&intent.destination_data_dir.join("node.json"))?;
    config.verify_identity()?;
    if receipt.api_version != "oasis7.local-data-migration-receipt/v1"
        || receipt.operation_id != intent.operation_id
        || receipt.request_digest != intent.request_digest
        || receipt.source_data_dir != intent.source_data_dir
        || receipt.destination_data_dir != intent.destination_data_dir
        || receipt.world_id != source.identity.chain.world_id()
        || receipt.chain_id != source.identity.chain.chain_id()
        || receipt.node_id != source.identity.node_id
        || receipt.root_public_key != source.identity.root_public_key
        || receipt.peer_id != source.identity.peer_id
        || config.identity.node_id != source.identity.node_id
        || config.identity.root_public_key != source.identity.root_public_key
        || config.identity.peer_id != source.identity.peer_id
        || config.identity.chain != source.identity.chain
    {
        return Err("migration receipt does not match exact source identity".into());
    }
    Ok(())
}

fn copy_tree(
    source: &Path,
    destination: &Path,
    planned_bytes: u64,
    excluded_intent: &Path,
) -> Result<(), String> {
    let source_root = fs::canonicalize(source).map_err(|error| error.to_string())?;
    let mut stack = vec![(source_root.clone(), destination.to_path_buf())];
    let mut entries = 0usize;
    let mut copied_bytes = 0u64;
    while let Some((src, dst)) = stack.pop() {
        for entry in fs::read_dir(&src).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry.path() == excluded_intent {
                continue;
            }
            entries = entries
                .checked_add(1)
                .ok_or("migration entry count overflow")?;
            if entries > MAX_ENTRIES {
                return Err("data-directory migration entry bound".into());
            }
            let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err("data-directory migration refuses symlinks".into());
            }
            let child = dst.join(entry.file_name());
            if metadata.is_dir() {
                fs::create_dir(&child).map_err(|error| error.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&child, fs::Permissions::from_mode(0o700))
                        .map_err(|error| error.to_string())?;
                }
                stack.push((entry.path(), child));
            } else if metadata.is_file() {
                if metadata.len() > planned_bytes.saturating_sub(copied_bytes) {
                    return Err("MIGRATION_SOURCE_CHANGED: source grew beyond the planned aggregate byte budget".into());
                }
                io::reject_symlinks(&entry.path())?;
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if metadata.dev()
                        != fs::metadata(&source_root).map_err(|e| e.to_string())?.dev()
                    {
                        return Err(
                            "data-directory migration refuses nested filesystem mounts".into()
                        );
                    }
                    options.mode(metadata.mode() & 0o700);
                }
                let mut output = options.open(&child).map_err(|error| error.to_string())?;
                let mut source_options = OpenOptions::new();
                source_options.read(true);
                #[cfg(unix)]
                source_options.custom_flags(libc::O_NOFOLLOW);
                let mut input = source_options
                    .open(entry.path())
                    .map_err(|error| error.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    let opened = input.metadata().map_err(|error| error.to_string())?;
                    if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
                        return Err(
                            "data-directory migration source changed during no-follow open".into(),
                        );
                    }
                }
                let copied =
                    std::io::copy(&mut input, &mut output).map_err(|error| error.to_string())?;
                copied_bytes = copied_bytes
                    .checked_add(copied)
                    .ok_or("migration copied byte count overflow")?;
                if copied != metadata.len() || copied_bytes > planned_bytes {
                    return Err("MIGRATION_SOURCE_CHANGED: source changed or exceeded the planned aggregate byte budget".into());
                }
                output.sync_all().map_err(|error| error.to_string())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::{MetadataExt, PermissionsExt};
                    fs::set_permissions(
                        &child,
                        fs::Permissions::from_mode(metadata.mode() & 0o700),
                    )
                    .map_err(|error| error.to_string())?;
                }
            } else {
                return Err("data-directory migration refuses special files".into());
            }
        }
        File::open(&dst)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
    }
    if copied_bytes != planned_bytes {
        return Err(
            "MIGRATION_SOURCE_CHANGED: source aggregate bytes changed after capacity planning"
                .into(),
        );
    }
    Ok(())
}

fn estimate_tree(source: &Path) -> Result<(u64, u64), String> {
    let source_root = fs::canonicalize(source).map_err(|error| error.to_string())?;
    let mut stack = vec![source_root.clone()];
    let mut entries = 0u64;
    let mut bytes = 0u64;
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            entries = entries
                .checked_add(1)
                .ok_or("migration entry count overflow")?;
            if entries > MAX_ENTRIES as u64 {
                return Err("MIGRATION_SIZE_LIMIT: data-directory migration entry bound".into());
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err("data-directory migration refuses symlinks".into());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.dev()
                    != fs::metadata(&source_root)
                        .map_err(|error| error.to_string())?
                        .dev()
                {
                    return Err("data-directory migration refuses nested filesystem mounts".into());
                }
            }
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file() {
                bytes = bytes
                    .checked_add(metadata.len())
                    .ok_or("migration aggregate byte estimate overflow")?;
            } else {
                return Err("data-directory migration refuses special files".into());
            }
        }
    }
    Ok((bytes, entries))
}

fn required_capacity_bytes(source_bytes: u64, entry_count: u64) -> u64 {
    source_bytes
        .saturating_add(COPY_SPACE_RESERVE_BYTES)
        .saturating_add(entry_count.saturating_mul(COPY_METADATA_RESERVE_PER_ENTRY))
}

fn capacity_outcome(
    source_bytes: u64,
    entry_count: u64,
    available_bytes: u64,
) -> MigrationCapacityOutcomeV1 {
    if source_bytes > MAX_COPY_BYTES {
        MigrationCapacityOutcomeV1::SourceSizeLimitExceeded
    } else if available_bytes < required_capacity_bytes(source_bytes, entry_count) {
        MigrationCapacityOutcomeV1::CapacityInsufficient
    } else {
        MigrationCapacityOutcomeV1::Ready
    }
}

#[cfg(unix)]
#[allow(
    clippy::unnecessary_cast,
    reason = "statvfs integer widths differ across supported Unix ABIs"
)]
fn available_bytes(path: &Path) -> Result<u64, String> {
    use std::os::unix::ffi::OsStrExt;
    let canonical = fs::canonicalize(path).map_err(|error| error.to_string())?;
    let path = std::ffi::CString::new(canonical.as_os_str().as_bytes())
        .map_err(|_| "migration destination path contains NUL")?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return Err(format!(
            "cannot inspect migration destination capacity: {}",
            std::io::Error::last_os_error()
        ));
    }
    let stats = unsafe { stats.assume_init() };
    let blocks = stats.f_bavail as u64;
    let block_size = stats.f_frsize as u64;
    blocks
        .checked_mul(block_size)
        .ok_or_else(|| "migration destination free-space estimate overflow".into())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
