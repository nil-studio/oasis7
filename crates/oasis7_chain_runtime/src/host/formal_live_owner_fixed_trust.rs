//! Immutable worker admission binding for a referenced external read trust.
use crate::node_lifecycle::local_io as io;
use serde::{Deserialize, Serialize};
use std::{fs::OpenOptions, io::Write, path::Path};
pub(super) const FILE: &str = "fixed-read-trust.json";
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Binding {
    api_version: String,
    request_digest: String,
    config_digest: String,
    read_trust_digest: String,
}
fn binding(request: &str, config: &str, trust: &str) -> Binding {
    Binding {
        api_version: "oasis7.formal-owner-fixed-trust/v1".into(),
        request_digest: request.into(),
        config_digest: config.into(),
        read_trust_digest: trust.into(),
    }
}
pub(super) fn publish(root: &Path, request: &str, config: &str, trust: &str) -> Result<(), String> {
    if trust.len() != 64
        || !trust
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid expected formal read trust digest".into());
    }
    let path = root.join(FILE);
    let bytes = serde_json::to_vec(&binding(request, config, trust)).map_err(|e| e.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(&path)
        .map_err(|e| format!("original fixed trust admission unavailable: {e}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    io::sync_dir(root)
}
pub(super) fn check(root: &Path, request: &str, config: &str, trust: &str) -> Result<(), String> {
    let path = root.join(FILE);
    crate::controlled_live_config::require_private_owned(&path, false)?;
    let saved: Binding = io::read(&path)?;
    if saved != binding(request, config, trust) {
        return Err("formal owner fixed trust admission differs from original operation".into());
    }
    Ok(())
}
