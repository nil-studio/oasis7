//! Fixed root-owned OS routing policy. Not model lifecycle or Skill attestation.
use super::{ipc::PublicBinding, state::denied};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, MetadataExt, OpenOptionsExt, PermissionsExt};
use serde::Deserialize;
use std::{
    io::Read,
    path::{Component, PathBuf},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeBinding {
    api_version: u8,
    transport_profile: String,
    identity_digest: String,
    descriptor_digest: String,
    session_digest: String,
    resource_id: String,
    restricted_runtime_identity: String,
    isolation: super::state::Isolation,
}
fn root_directory(dir: &Dir) -> Result<(), String> {
    let m = dir.dir_metadata().map_err(|_| denied())?;
    if m.uid() != 0 || m.permissions().mode() & 0o022 != 0 {
        return Err(denied());
    }
    Ok(())
}
fn fixed_path(model: u32) -> PathBuf {
    #[cfg(target_os = "macos")]
    let root = "/private/var/lib/oasis7-custody/model-bindings";
    #[cfg(not(target_os = "macos"))]
    let root = "/var/lib/oasis7-custody/model-bindings";
    PathBuf::from(root).join(format!("{model}.json"))
}
pub(super) fn check(expected: &PublicBinding) -> Result<(), String> {
    let path = fixed_path(expected.isolation.model_uid);
    let mut dir = Dir::open_ambient_dir("/", cap_std::ambient_authority()).map_err(|_| denied())?;
    root_directory(&dir)?;
    for component in path.parent().ok_or_else(denied)?.components().skip(1) {
        if !matches!(component, Component::Normal(_)) {
            return Err(denied());
        }
        dir = dir
            .open_dir_nofollow(component.as_os_str())
            .map_err(|_| denied())?;
        root_directory(&dir)?;
    }
    let mut options = cap_std::fs::OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .follow(FollowSymlinks::No);
    let f = dir
        .open_with(path.file_name().ok_or_else(denied)?, &options)
        .map_err(|_| denied())?;
    let m = f.metadata().map_err(|_| denied())?;
    if !m.is_file()
        || m.uid() != 0
        || m.nlink() != 1
        || m.permissions().mode() & 0o022 != 0
        || m.len() > 32768
    {
        return Err(denied());
    }
    let mut bytes = Vec::new();
    f.take(32769)
        .read_to_end(&mut bytes)
        .map_err(|_| denied())?;
    if bytes.len() > 32768 {
        return Err(denied());
    }
    let actual: RuntimeBinding = serde_json::from_slice(&bytes).map_err(|_| denied())?;
    matches(&actual, expected)
}
fn matches(actual: &RuntimeBinding, expected: &PublicBinding) -> Result<(), String> {
    if actual.api_version != 1
        || actual.transport_profile != expected.transport_profile
        || actual.identity_digest != expected.identity_digest
        || actual.descriptor_digest != expected.descriptor_digest
        || actual.session_digest != expected.session_digest
        || actual.resource_id != expected.resource_id
        || actual.restricted_runtime_identity != expected.restricted_runtime_identity
        || actual.isolation != expected.isolation
    {
        return Err(denied());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    // Pure matching tests do not claim root ownership, a different-UID positive,
    // process retirement or actual Skill loading was exercised.
    #[test]
    fn os_record_matching_cannot_change_session_or_scope() {
        let isolation = super::super::state::Isolation {
            host_uid: 501,
            model_uid: 502,
            native: super::super::state::NativePlacement::Local { uid: 503 },
        };
        let expected = PublicBinding {
            api_version: 1,
            transport_profile: "native-broker-custody-v1".into(),
            socket_path: "/socket".into(),
            identity_digest: "11".repeat(32),
            descriptor_digest: "12".repeat(32),
            session_digest: "13".repeat(32),
            resource_id: "one".into(),
            restricted_runtime_identity: "restricted-instance".into(),
            isolation: isolation.clone(),
        };
        let mut actual = RuntimeBinding {
            api_version: 1,
            transport_profile: expected.transport_profile.clone(),
            identity_digest: expected.identity_digest.clone(),
            descriptor_digest: expected.descriptor_digest.clone(),
            session_digest: expected.session_digest.clone(),
            resource_id: expected.resource_id.clone(),
            restricted_runtime_identity: expected.restricted_runtime_identity.clone(),
            isolation,
        };
        assert!(matches(&actual, &expected).is_ok());
        actual.session_digest = "14".repeat(32);
        assert!(matches(&actual, &expected).is_err());
        actual.session_digest = expected.session_digest.clone();
        actual.isolation.model_uid = 504;
        assert!(matches(&actual, &expected).is_err());
    }
}
