//! Immutable local cache of signed healthy durable-set activation evidence.
//!
//! These marker files are hints, never authority. Every startup and history
//! response replays their contents from the configured external origin and
//! verifies the exact qualified boundary, dual seals, and dual installs.

use oasis7::world_service::formal_read::FormalNormalDurableSetReadStepV1;
use oasis7_distfs::controlled_authority::formal::VerifiedDurableSetActivationV1;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MARKER_DIRECTORY: &str = "durable-set-activations";
const MAX_MARKER_COUNT: usize = 65_536;
const MAX_MARKER_BYTES: usize = oasis7_client_api::world_service::MAX_FORMAL_PROOF_BYTES;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurableSetActivationMarkerV1 {
    schema_version: u32,
    boundary_position: u64,
    step: FormalNormalDurableSetReadStepV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DurableSetActivationMarkerRecordV1 {
    pub(super) boundary_position: u64,
    pub(super) step: FormalNormalDurableSetReadStepV1,
}

pub(super) struct DurableSetActivationMarkerStoreV1 {
    writer_directory: PathBuf,
}

impl DurableSetActivationMarkerStoreV1 {
    pub(super) fn new(writer_directory: PathBuf) -> Self {
        Self { writer_directory }
    }

    pub(super) fn positions(&self) -> Result<Vec<u64>, String> {
        crate::controlled_live_config::require_private_owned(&self.writer_directory, true)?;
        let directory = self.directory();
        match fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("inspect activation marker directory: {error}")),
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("formal durable-set activation marker path is not a directory".into());
            }
            Ok(_) => {}
        }
        crate::controlled_live_config::require_private_owned(&directory, true)?;

        let mut positions = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|error| {
            format!("read formal durable-set activation marker directory: {error}")
        })? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(
                    "formal durable-set activation marker entry is not a regular file".into(),
                );
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "formal durable-set activation marker filename encoding")?;
            if is_abandoned_temp_name(&name) {
                continue;
            }
            let position = parse_marker_name(&name)?;
            if positions.len() == MAX_MARKER_COUNT {
                return Err("formal durable-set activation marker count bound".into());
            }
            positions.push(position);
        }
        positions.sort_unstable();
        if positions.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("duplicate formal durable-set activation boundary marker".into());
        }
        Ok(positions)
    }

    pub(super) fn read(
        &self,
        position: u64,
    ) -> Result<Option<DurableSetActivationMarkerRecordV1>, String> {
        if position == 0 {
            return Err("formal durable-set activation boundary must be nonzero".into());
        }
        let path = self.marker_path(position);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("inspect activation marker: {error}")),
            Ok(_) => crate::controlled_live_config::require_private_owned(&path, false)?,
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let file = options
            .open(&path)
            .map_err(|error| format!("read activation marker {}: {error}", path.display()))?;
        let length = file.metadata().map_err(|error| error.to_string())?.len();
        if length == 0 || length > MAX_MARKER_BYTES as u64 {
            return Err("formal durable-set activation marker byte bound".into());
        }
        let mut bytes = Vec::with_capacity(length as usize);
        file.take(MAX_MARKER_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 != length {
            return Err("formal durable-set activation marker changed while reading".into());
        }
        let marker: DurableSetActivationMarkerV1 =
            serde_cbor::from_slice(&bytes).map_err(|error| error.to_string())?;
        if marker.schema_version != 1
            || marker.boundary_position != position
            || marker.step.transition.body.boundary_position != position
            || serde_cbor::to_vec(&marker).map_err(|error| error.to_string())? != bytes
        {
            return Err(
                "formal durable-set activation marker schema/identity/canonical bytes".into(),
            );
        }
        Ok(Some(DurableSetActivationMarkerRecordV1 {
            boundary_position: position,
            step: marker.step,
        }))
    }

    /// Save only evidence paired with the opaque activation returned by the
    /// native dual-seal/dual-install verifier. Startup still re-verifies it.
    pub(super) fn persist_activated(
        &self,
        step: FormalNormalDurableSetReadStepV1,
        activation: &VerifiedDurableSetActivationV1,
    ) -> Result<(), String> {
        validate_step_activation_binding(&step, activation)?;
        let position = activation.boundary().record().position;
        let marker = DurableSetActivationMarkerV1 {
            schema_version: 1,
            boundary_position: position,
            step,
        };
        let bytes = serde_cbor::to_vec(&marker).map_err(|error| error.to_string())?;
        if bytes.is_empty() || bytes.len() > MAX_MARKER_BYTES {
            return Err("formal durable-set activation marker byte bound".into());
        }
        let directory = self.ensure_directory()?;
        let path = self.marker_path(position);
        match publish_immutable(&directory, &path, &bytes)? {
            true => Ok(()),
            false => {
                let existing = self
                    .read(position)?
                    .ok_or("activation marker disappeared after immutable publish race")?;
                if existing.step == marker.step {
                    Ok(())
                } else {
                    Err(
                        "Conflict: another activation marker already exists at this boundary"
                            .into(),
                    )
                }
            }
        }
    }

    fn directory(&self) -> PathBuf {
        self.writer_directory.join(MARKER_DIRECTORY)
    }

    fn marker_path(&self, position: u64) -> PathBuf {
        self.directory().join(format!("{position:020}.cbor"))
    }

    fn ensure_directory(&self) -> Result<PathBuf, String> {
        crate::controlled_live_config::require_private_owned(&self.writer_directory, true)?;
        let directory = self.directory();
        match fs::create_dir(&directory) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                        .map_err(|error| error.to_string())?;
                }
                sync_directory(&self.writer_directory)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(format!("create activation marker directory: {error}"));
            }
        }
        crate::controlled_live_config::require_private_owned(&directory, true)?;
        Ok(directory)
    }
}

pub(super) fn validate_step_activation_binding(
    step: &FormalNormalDurableSetReadStepV1,
    activation: &VerifiedDurableSetActivationV1,
) -> Result<(), String> {
    let boundary = activation.boundary();
    let predecessor_id = activation
        .predecessor()
        .authority_id()
        .map_err(|error| error.to_string())?;
    let successor_id = activation
        .successor()
        .authority_id()
        .map_err(|error| error.to_string())?;
    activation
        .verify_evidence(
            &step.transition,
            [&step.old_seals[0], &step.old_seals[1]],
            [&step.new_installs[0], &step.new_installs[1]],
        )
        .map_err(|error| error.to_string())?;
    let transition_digest = step
        .transition
        .body
        .digest()
        .map_err(|error| error.to_string())?;
    let expected_boundary = boundary.head();
    if step.transition.body.predecessor_authority_id != predecessor_id
        || step
            .transition
            .body
            .successor
            .authority_id()
            .map_err(|e| e.to_string())?
            != successor_id
        || step.transition.body.world != boundary.record().world
        || step.transition.body.boundary_position != boundary.record().position
        || transition_digest != activation.transition_digest()
        || step
            .new_installs
            .iter()
            .any(|install| install.body.boundary != expected_boundary)
    {
        return Err("formal durable-set marker differs from opaque activated boundary".into());
    }
    Ok(())
}

fn parse_marker_name(name: &str) -> Result<u64, String> {
    let position = name
        .strip_suffix(".cbor")
        .ok_or("unexpected file in durable-set activation marker directory")?;
    if position.len() != 20 || !position.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("malformed formal durable-set activation marker filename".into());
    }
    let parsed = position
        .parse::<u64>()
        .map_err(|_| "formal durable-set activation position overflow")?;
    if parsed == 0 || format!("{parsed:020}.cbor") != name {
        return Err("noncanonical formal durable-set activation marker filename".into());
    }
    Ok(parsed)
}

fn is_abandoned_temp_name(name: &str) -> bool {
    name.starts_with(".durable-set-activation-") && name.ends_with(".tmp")
}

fn publish_immutable(directory: &Path, path: &Path, bytes: &[u8]) -> Result<bool, String> {
    let suffix = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = directory.join(format!(
        ".durable-set-activation-{}-{suffix}.tmp",
        std::process::id()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temp)
        .map_err(|error| format!("create activation marker temp: {error}"))?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temp);
        return Err(format!("write activation marker temp: {error}"));
    }
    match fs::hard_link(&temp, path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = fs::remove_file(&temp);
            return Ok(false);
        }
        Err(error) => {
            let _ = fs::remove_file(&temp);
            return Err(format!("publish immutable activation marker: {error}"));
        }
    }
    fs::remove_file(&temp).map_err(|error| error.to_string())?;
    sync_directory(directory)?;
    Ok(true)
}

fn sync_directory(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync activation marker directory: {error}"))
}

#[cfg(test)]
#[path = "formal_live_durable_set_activation_tests.rs"]
mod tests;
