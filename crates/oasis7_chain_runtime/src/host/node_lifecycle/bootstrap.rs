//! Install an already bound trusted runtime world; never call World::new.
use super::{local_io as io, model::NodeConfig};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

fn files(
    root: &Path,
    dir: &Path,
    entries: &mut Vec<PathBuf>,
    depth: usize,
    walked: &mut u64,
) -> Result<(), String> {
    if depth > 64 {
        return Err("bootstrap tree depth bound".into());
    }
    io::reject_symlinks(dir)?;
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        *walked = walked
            .checked_add(1)
            .ok_or("bootstrap entry count overflow")?;
        if *walked > 100_000 {
            return Err("bootstrap entry count bound".into());
        }
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let path = entry.path();
        io::reject_symlinks(&path)?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(".node-bootstrap-")
        {
            return Err("reserved bootstrap temporary filename".into());
        }
        if kind.is_dir() {
            files(root, &path, entries, depth + 1, walked)?
        } else if kind.is_file() {
            entries.push(
                path.strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_path_buf(),
            );
        } else {
            return Err("bootstrap non-regular entry".into());
        }
        if entries.len() > 100_000 {
            return Err("bootstrap file count bound".into());
        }
    }
    Ok(())
}
fn bounded_sha(path: &Path, size: u64) -> Result<String, String> {
    let file = File::open(io::regular(path)?).map_err(|e| e.to_string())?;
    let mut input = file.take(size.checked_add(1).ok_or("bootstrap hash size overflow")?);
    let mut hash = Sha256::new();
    let mut actual = 0u64;
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        actual += count as u64;
        if actual > size {
            return Err("bootstrap source changed during bounded hash".into());
        }
        hash.update(&buffer[..count]);
    }
    if actual != size {
        return Err("bootstrap source truncated during hash".into());
    }
    Ok(hex::encode(hash.finalize()))
}
pub(super) fn tree(root: &Path) -> Result<(String, u64, u64), String> {
    let mut entries = vec![];
    files(root, root, &mut entries, 0, &mut 0)?;
    entries.sort();
    let mut combined = Sha256::new();
    let mut total = 0u64;
    for relative in &entries {
        let path = root.join(relative);
        let size = fs::metadata(&path).map_err(|e| e.to_string())?.len();
        total = total.checked_add(size).ok_or("bootstrap size overflow")?;
        if total > 64 * 1024 * 1024 * 1024 {
            return Err("bootstrap byte bound".into());
        }
        combined.update(
            relative
                .to_str()
                .ok_or("bootstrap UTF8 path required")?
                .replace('\\', "/"),
        );
        combined.update(b"\0");
        combined.update(bounded_sha(&path, size)?);
        combined.update(b"\0");
        combined.update(size.to_string());
        combined.update(b"\n");
    }
    Ok((
        hex::encode(combined.finalize()),
        entries.len() as u64,
        total,
    ))
}
fn validate_world(config: &NodeConfig, path: &Path) -> Result<(), String> {
    let chain = config.identity.chain.compatibility()?;
    for name in ["snapshot.json", "journal.json"] {
        io::regular(&path.join(name))?;
    }
    let world = super::super::runtime_authority::load_authority_world(path, &chain.world_id)?;
    let manifest = world.chain_resource_manifest();
    if manifest.chain_id != chain.chain_id
        || !manifest.genesis_ref.as_deref().is_some_and(|g| {
            g == chain.genesis.sha256 || g == format!("sha256:{}", chain.genesis.sha256)
        })
    {
        return Err("existing world checkpoint chain/genesis binding mismatch".into());
    }
    Ok(())
}
fn copy(
    source: &Path,
    destination: &Path,
    depth: usize,
    remaining_entries: &mut u64,
    remaining_bytes: &mut u64,
) -> Result<(), String> {
    if depth > 64 {
        return Err("bootstrap copy depth bound".into());
    }
    io::private_dir(destination)?;
    for entry in fs::read_dir(destination).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(".node-bootstrap-")
        {
            io::regular(&entry.path())?;
            super::super::controlled_live_config::require_private_owned(&entry.path(), false)?;
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
        }
    }
    io::sync_dir(destination)?;
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        *remaining_entries = remaining_entries
            .checked_sub(1)
            .ok_or("bootstrap copy entry bound")?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        io::reject_symlinks(&from)?;
        io::reject_symlinks(&to)?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            copy(&from, &to, depth + 1, remaining_entries, remaining_bytes)?;
        } else if kind.is_file() {
            let size = fs::metadata(&from).map_err(|e| e.to_string())?.len();
            if size > *remaining_bytes {
                return Err("bootstrap copy byte bound".into());
            }
            if to.exists() {
                *remaining_bytes -= size;
                if bounded_sha(&from, size)? != bounded_sha(&to, size)? {
                    return Err("existing bootstrap stage file mismatch".into());
                }
                continue;
            }
            let temporary = destination.join(format!(".node-bootstrap-{}", io::random_id()?));
            let result = (|| {
                let mut input = File::open(&from).map_err(|e| e.to_string())?;
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&temporary)
                    .map_err(|e| e.to_string())?;
                let copied = std::io::copy(
                    &mut Read::by_ref(&mut input).take(remaining_bytes.saturating_add(1)),
                    &mut output,
                )
                .map_err(|e| e.to_string())?;
                if copied != size || copied > *remaining_bytes {
                    return Err("bootstrap source changed during bounded copy".into());
                }
                *remaining_bytes -= copied;
                output.flush().map_err(|e| e.to_string())?;
                output.sync_all().map_err(|e| e.to_string())?;
                fs::rename(&temporary, &to).map_err(|e| e.to_string())?;
                io::sync_dir(destination)
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            result?;
        } else {
            return Err("bootstrap non-regular entry".into());
        }
    }
    io::sync_dir(destination)
}
pub(super) fn prepare(config: &NodeConfig, incarnation: &str) -> Result<(), String> {
    let chain = config.identity.chain.compatibility()?;
    let target = config.identity.data_dir.join("state/world");
    io::reject_symlinks(&target)?;
    if target.exists() {
        return validate_world(config, &target);
    }
    let bundle: serde_json::Value = io::read(&chain.release_bundle.path)?;
    let artifact = &bundle["generated_world_sidecar"];
    if artifact["kind"] != "directory" {
        return Err("existing runtime world artifact directory required".into());
    }
    let source = PathBuf::from(
        artifact["resolved_path"]
            .as_str()
            .ok_or("existing world artifact path missing")?,
    );
    if !source.is_absolute() {
        return Err("existing world artifact requires absolute path".into());
    }
    let expected = (
        artifact["sha256_tree"]
            .as_str()
            .ok_or("existing world artifact hash missing")?
            .to_owned(),
        artifact["file_count"]
            .as_u64()
            .ok_or("existing world file count missing")?,
        artifact["total_bytes"]
            .as_u64()
            .ok_or("existing world size missing")?,
    );
    if tree(&source)? != expected {
        return Err("existing world artifact tree mismatch".into());
    }
    validate_world(config, &source)?;
    let state = config.identity.data_dir.join("state");
    io::private_dir(&state)?;
    let stage = state.join(format!(".world-bootstrap-{incarnation}"));
    // Reusable stage name is bound to the already durable receiver intent.
    let mut remaining_entries = 100_000;
    let mut remaining_bytes = expected.2;
    copy(
        &source,
        &stage,
        0,
        &mut remaining_entries,
        &mut remaining_bytes,
    )?;
    if tree(&stage)? != expected {
        return Err("copied existing world artifact tree mismatch".into());
    }
    validate_world(config, &stage)?;
    config.identity.chain.verify()?;
    fs::rename(&stage, &target).map_err(|e| e.to_string())?;
    io::sync_dir(&state)
}
