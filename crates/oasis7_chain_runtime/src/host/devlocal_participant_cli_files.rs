use oasis7_game_client::private_storage;
use serde::{Serialize, de::DeserializeOwned};
use std::path::Path;

pub(super) fn canonical(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .canonicalize()
            .map_err(|_| "offline input unavailable")?
            != path
    {
        return Err("offline paths must be canonical and absolute, without symlinks".into());
    }
    Ok(())
}
pub(super) fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    canonical(path)?;
    let dir = private_storage::open_root(path.parent().ok_or("input parent unavailable")?)?;
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("invalid input name")?;
    let bytes = private_storage::read_private(&dir, name, 16 * 1024 * 1024)?;
    oasis7_agent_api::game::parse_bounded_json(&bytes, 16 * 1024 * 1024)
        .map_err(|_| "invalid bounded offline material".into())
}
pub(super) fn publish(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let parent = path.parent().ok_or("output parent unavailable")?;
    canonical(parent)?;
    if !path.is_absolute()
        || path.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err("output must be a new absolute private path".into());
    }
    let dir = private_storage::open_root(parent)?;
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("invalid output name")?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "offline material encoding failed")?;
    // The live startup authorization reader accepts at most 1 MiB. Never
    // publish material that cannot be installed through that same boundary.
    if bytes.len() > 1024 * 1024 {
        return Err("offline output exceeds the live authorization size limit".into());
    }
    if !private_storage::publish_immutable(&dir, name, &bytes)? {
        return Err("output exists; original material retained".into());
    }
    Ok(())
}
