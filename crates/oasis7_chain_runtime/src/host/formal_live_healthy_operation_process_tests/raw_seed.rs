//! Operator preparation reconstructs immutable logical CAS bytes, never authority or heads.
use crate::node_lifecycle::local_io as io;
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FileFormalArchiveSourceV1, FormalArchiveSourceV1,
};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path};
pub(super) fn copy_objects(source: &Path, target: &Path) -> Result<(), String> {
    crate::controlled_live_config::require_private_owned(source, true)?;
    crate::controlled_live_config::require_private_owned(target, true)?;
    let source_reader =
        FileFormalArchiveSourceV1::new(source.to_path_buf()).map_err(|e| e.to_string())?;
    let target_root = target.to_path_buf();
    let target = target.join("formal-objects-v1");
    io::private_dir(&target)?;
    let ids = source_reader.logical_ids().map_err(|e| e.to_string())?;
    if ids.len() > 65_536 {
        return Err("fixture CAS seed exceeds 65,536 logical object budget".into());
    }
    let mut logical_bytes = 0u64;
    for id in ids {
        let logical = source_reader
            .read_object(&id)
            .map_err(|e| format!("validate source logical CAS {id}: {e}"))?
            .ok_or_else(|| format!("source CAS object missing: {id}"))?;
        logical_bytes = logical_bytes
            .checked_add(logical.len() as u64)
            .ok_or("fixture CAS seed byte count overflow")?;
        if logical_bytes > 512 * 1024 * 1024 {
            return Err("fixture CAS seed exceeds 512MiB logical byte budget".into());
        }
        // Write exact logical bytes, independent of pack/recipe/generation
        // representation. The output's physical size equals its logical size.
        let destination = target.join(&id);
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&destination)
            .map_err(|e| e.to_string())?;
        output.write_all(&logical).map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        let copied = FileFormalArchiveSourceV1::new(target_root.clone())
            .map_err(|e| e.to_string())?
            .read_object(&id)
            .map_err(|e| format!("validate copied logical CAS {id}: {e}"))?
            .ok_or_else(|| format!("copied CAS object missing: {id}"))?;
        if copied != logical {
            return Err(format!("copied logical CAS bytes differ for {id}"));
        }
    }
    io::sync_dir(&target)
}

pub(super) fn replace_private_fixture_bytes(
    root: &Path,
    path: &Path,
    bytes: &[u8],
) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    crate::controlled_live_config::require_private_owned(root, true)?;
    let resolved = io::regular(path)?;
    if path.parent() != Some(root) || resolved != path {
        return Err("trust overwrite must name a direct private fixture file".into());
    }
    let original = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| error.to_string())?;
    if !original
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("trust overwrite target is not a regular private fixture file".into());
    }
    crate::controlled_live_config::require_private_owned(path, false)?;
    let temporary = root.join(format!(".fixture-trust-{}", io::random_id()?));
    let result = (|| {
        super::oracle_support::write_private(&temporary, bytes)?;
        io::reject_symlinks(path)?;
        std::fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        io::sync_dir(root)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub(super) fn reject_exited_retry(record: &serde_json::Value) -> Result<(), String> {
    if let Some(reason) = record["blocker"].as_str()
        && (reason.contains("owner worker exited before a durable owner receipt appeared")
            || reason.contains("owner worker exited while owner state could not be queried"))
    {
        return Err(format!(
            "retry worker exited before its exact receipt: {reason}"
        ));
    }
    Ok(())
}
