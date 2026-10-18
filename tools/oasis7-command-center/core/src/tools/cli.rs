//! Optional CLI installation from a fixed, bundled release catalog only.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use ts_rs::TS;
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CliCatalog {
    pub schema_version: u32,
    pub tool_id: String,
    pub source_head_oid: String,
    pub target: String,
    pub release_id: String,
    pub file: String,
    pub sha256: String,
    pub size_bytes: u64,
}
#[derive(Debug, Clone, Deserialize, Serialize, TS)]
pub struct CliInstallation {
    pub tool_id: String,
    pub release_id: String,
    pub path: String,
    pub sha256: String,
    pub terminal_path_configured: bool,
}
const INSTALL_LOCK: &str = ".command-center-install.lock";
/// Installation and absence reconciliation share this cross-process lock.
fn installation_lock(user_root: &Path) -> Result<fs::File> {
    if !user_root.is_absolute() {
        return Err(Error::Invalid(
            "Absolute per-user tool root required".into(),
        ));
    }
    fs::create_dir_all(user_root)?;
    if fs::canonicalize(user_root)? != user_root {
        return Err(Error::Invalid("Tool root aliases are forbidden".into()));
    }
    #[cfg(unix)]
    {
        use std::os::{
            fd::AsRawFd,
            unix::fs::{MetadataExt, OpenOptionsExt},
        };
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(user_root.join(INSTALL_LOCK))?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(Error::Invalid("Unsafe CLI installation lock".into()));
        }
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(Error::Invalid(
                "Original CLI installer may still be active; retain unknown state and check again"
                    .into(),
            ));
        }
        Ok(file)
    }
    #[cfg(not(unix))]
    {
        Err(Error::Invalid(
            "CLI installation locking unsupported".into(),
        ))
    }
}

fn validate_catalog(c: &CliCatalog) -> Result<()> {
    if c.schema_version != 1
        || c.tool_id != "oasis7_context"
        || c.file != "oasis7_context"
        || c.source_head_oid.len() != 40
        || !c
            .source_head_oid
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || c.target != current_target()
        || c.release_id.is_empty()
        || !c
            .release_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Error::Invalid("Invalid bundled CLI catalog".into()));
    }
    Ok(())
}

pub fn install_bundled(resource_root: &Path, user_root: &Path) -> Result<CliInstallation> {
    let c: CliCatalog = serde_json::from_slice(&fs::read(resource_root.join("catalog.json"))?)?;
    if c.schema_version != 1
        || c.tool_id != "oasis7_context"
        || c.file != "oasis7_context"
        || c.source_head_oid.len() != 40
        || !c
            .source_head_oid
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || c.target != current_target()
        || c.release_id.is_empty()
        || !c
            .release_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Error::Invalid("Invalid bundled CLI catalog".into()));
    }
    let binary = resource_root.join(&c.file);
    let metadata = fs::symlink_metadata(&binary)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Error::Invalid("Bundled CLI is not a regular file".into()));
    }
    let bytes = fs::read(&binary)?;
    if bytes.len() as u64 != c.size_bytes
        || oasis7_signer_protocol::identity::sha256_hex(&bytes) != c.sha256
    {
        return Err(Error::Invalid("Bundled CLI catalog digest mismatch".into()));
    }
    if !user_root.is_absolute() {
        return Err(Error::Invalid(
            "Absolute per-user tool root required".into(),
        ));
    }
    fs::create_dir_all(user_root)?;
    let root = fs::canonicalize(user_root)?;
    if root != user_root {
        return Err(Error::Invalid("Tool root aliases are forbidden".into()));
    }
    let _install_lock = installation_lock(&root)?;
    let version = root.join(&c.release_id);
    fs::create_dir_all(&version)?;
    if fs::canonicalize(&version)? != version {
        return Err(Error::Invalid(
            "CLI release directory aliases are forbidden".into(),
        ));
    }
    let path = version.join(&c.file);
    if path.exists() {
        if fs::symlink_metadata(&path)?.file_type().is_symlink() || fs::read(&path)? != bytes {
            return Err(Error::Invalid(
                "Existing CLI is user maintained or differs; refusing overwrite".into(),
            ));
        }
    } else {
        let staging = version.join(format!(".installing-{}", oasis7_context::fs::random_id()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o500);
        }
        let mut file = options.open(&staging)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::hard_link(&staging, &path)?;
        fs::remove_file(staging)?;
    }
    observe_installed(resource_root, user_root)?.ok_or_else(|| {
        Error::Invalid("Installed CLI could not be observed after publication".into())
    })
}
pub fn default_user_root(home: &Path) -> PathBuf {
    home.join("Library/Application Support/com.oasis7.command-center/tools")
}

fn current_target() -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        "unsupported"
    };
    format!("{arch}-apple-darwin")
}

pub fn bundle_identity(resource_root: &Path, user_root: &Path) -> Result<(String, String)> {
    let catalog = fs::read(resource_root.join("catalog.json"))?;
    let parsed: CliCatalog = serde_json::from_slice(&catalog)?;
    validate_catalog(&parsed)?;
    let binary = fs::read(resource_root.join("oasis7_context"))?;
    if parsed.file != "oasis7_context"
        || oasis7_signer_protocol::identity::sha256_hex(&binary) != parsed.sha256
    {
        return Err(Error::Invalid("CLI catalog file digest mismatch".into()));
    }
    let fingerprint = oasis7_signer_protocol::identity::sha256_hex(
        &oasis7_signer_protocol::identity::canonical_json(&(
            user_root,
            parsed.release_id,
            &catalog,
            oasis7_signer_protocol::identity::sha256_hex(&binary),
        ))
        .map_err(|e| Error::Invalid(e.to_string()))?,
    );
    Ok((format!("cli:{}", user_root.display()), fingerprint))
}

/// Observe the exact release carried by this App. Other local versions remain
/// explicitly unknown rather than being guessed compatible or absent.
pub fn observe_installed(
    resource_root: &Path,
    user_root: &Path,
) -> Result<Option<CliInstallation>> {
    let parsed: CliCatalog =
        serde_json::from_slice(&fs::read(resource_root.join("catalog.json"))?)?;
    if parsed.schema_version != 1
        || parsed.tool_id != "oasis7_context"
        || parsed.file != "oasis7_context"
        || parsed.release_id.is_empty()
        || !parsed
            .release_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Error::Invalid("Invalid packaged catalog".into()));
    }
    let path = user_root.join(&parsed.release_id).join("oasis7_context");
    if !path.exists() {
        if user_root.exists()
            && fs::read_dir(user_root)?.any(|entry| {
                entry
                    .map(|entry| {
                        entry.file_name() != std::ffi::OsStr::new(&parsed.release_id)
                            && entry.file_name() != std::ffi::OsStr::new(INSTALL_LOCK)
                    })
                    .unwrap_or(true)
            })
        {
            return Err(Error::Invalid(
                "Other local versions exist; their compatibility is unknown to this App release"
                    .into(),
            ));
        }
        return Ok(None);
    }
    if fs::canonicalize(&path)? != path || !fs::symlink_metadata(&path)?.is_file() {
        return Err(Error::Invalid("Installed CLI path identity changed".into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::PermissionsExt;
        let name = std::ffi::CString::new(path.as_os_str().as_bytes())
            .map_err(|_| Error::Invalid("Invalid installed CLI path".into()))?;
        if fs::metadata(&path)?.permissions().mode() & 0o111 == 0
            || unsafe { libc::access(name.as_ptr(), libc::X_OK) } != 0
        {
            return Err(Error::Invalid(
                "Installed CLI is not executable by the current user".into(),
            ));
        }
    }
    let (digest, size) = super::selected_file_digest(&path, 128 * 1024 * 1024)?;
    let (packaged_digest, packaged_size) =
        super::selected_file_digest(&resource_root.join("oasis7_context"), 128 * 1024 * 1024)?;
    if digest != parsed.sha256
        || size != parsed.size_bytes
        || digest != packaged_digest
        || size != packaged_size
    {
        return Err(Error::Invalid(
            "Installed CLI does not match the App's packaged release".into(),
        ));
    }
    Ok(Some(CliInstallation {
        tool_id: parsed.tool_id,
        release_id: parsed.release_id,
        path: path.display().to_string(),
        sha256: digest,
        terminal_path_configured: false,
    }))
}

pub fn reconcile_cli_install(
    jobs: &super::jobs::ToolJobs,
    operation_id: &str,
    resource_root: &Path,
    user_root: &Path,
) -> Result<Option<CliInstallation>> {
    let (resource, fingerprint) = bundle_identity(resource_root, user_root)?;
    let job = jobs
        .list()?
        .into_iter()
        .find(|job| job.id == operation_id)
        .ok_or_else(|| Error::Invalid("Unknown original CLI operation".into()))?;
    if job.kind != "cli_install" || job.resource != resource || job.fingerprint != fingerprint {
        return Err(Error::Invalid(
            "Original CLI release identity differs; retain its unknown state".into(),
        ));
    }
    if !matches!(job.state.as_str(), "unknown" | "succeeded" | "failed") {
        return Err(Error::Invalid(
            "Original CLI operation is still running; retain reservation".into(),
        ));
    }
    let _install_lock = installation_lock(user_root)?;
    // Absence is about this exact catalog's final publication, not another local
    // release or the installer's residual staging. Never delete those files.
    let catalog: CliCatalog =
        serde_json::from_slice(&fs::read(resource_root.join("catalog.json"))?)?;
    let version = user_root.join(&catalog.release_id);
    for directory in [user_root, version.as_path()] {
        match fs::symlink_metadata(directory) {
            Ok(metadata)
                if metadata.is_dir()
                    && !metadata.file_type().is_symlink()
                    && fs::canonicalize(directory)? == directory => {}
            Ok(_) => {
                return Err(Error::Invalid(
                    "Original CLI directory identity changed; retain unknown".into(),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    let final_path = version.join(&catalog.file);
    match fs::symlink_metadata(&final_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if job.state == "unknown" {
                jobs.finish(operation_id, "failed", Some("Original CLI release has no published binary; residual staging retained. Explicit new installation is allowed."))?;
            }
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
        Ok(_) => (),
    }
    let installation = observe_installed(resource_root, user_root)?;
    if installation.is_some() && job.state == "unknown" {
        jobs.finish(
            operation_id,
            "succeeded",
            Some("Original bundled CLI release installed; digest verified"),
        )?;
    }
    Ok(installation)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_unpublished_release_reconciles_without_deleting_staging_or_other_versions() {
        for write_staging in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let base = fs::canonicalize(temp.path()).unwrap();
            let resource = base.join("resource");
            fs::create_dir(&resource).unwrap();
            let bytes = b"synthetic CLI";
            fs::write(resource.join("oasis7_context"), bytes).unwrap();
            let catalog = CliCatalog {
                schema_version: 1,
                tool_id: "oasis7_context".into(),
                source_head_oid: "a".repeat(40),
                target: current_target(),
                release_id: "release-one".into(),
                file: "oasis7_context".into(),
                sha256: oasis7_signer_protocol::identity::sha256_hex(bytes),
                size_bytes: bytes.len() as u64,
            };
            fs::write(
                resource.join("catalog.json"),
                serde_json::to_vec(&catalog).unwrap(),
            )
            .unwrap();
            let root = base.join("tools");
            let version = root.join("release-one");
            fs::create_dir_all(&version).unwrap(); // crash after mkdir
            assert!(observe_installed(&resource, &root).unwrap().is_none());
            if write_staging {
                fs::write(version.join(".installing"), b"partial retained staging").unwrap();
            }
            let other = root.join("another-release");
            fs::create_dir(&other).unwrap();
            fs::write(other.join("user-file"), b"user maintained").unwrap();
            let jobs = super::super::jobs::ToolJobs::open(":memory:").unwrap();
            let (reservation, fingerprint) = bundle_identity(&resource, &root).unwrap();
            jobs.admit("interrupted", &fingerprint, &reservation, "cli_install")
                .unwrap();
            jobs.finish("interrupted", "unknown", None).unwrap();
            let active_installer = installation_lock(&root).unwrap();
            assert!(reconcile_cli_install(&jobs, "interrupted", &resource, &root).is_err());
            assert_eq!(jobs.list().unwrap()[0].state, "unknown");
            drop(active_installer);

            assert!(
                reconcile_cli_install(&jobs, "interrupted", &resource, &root)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(jobs.list().unwrap()[0].state, "failed");
            jobs.admit("explicit-new", &fingerprint, &reservation, "cli_install")
                .unwrap();
            let installed = install_bundled(&resource, &root).unwrap();
            assert_eq!(fs::read(&installed.path).unwrap(), bytes);
            assert_eq!(
                fs::read(other.join("user-file")).unwrap(),
                b"user maintained"
            );
            if write_staging {
                assert_eq!(
                    fs::read(version.join(".installing")).unwrap(),
                    b"partial retained staging"
                );
            }
        }
    }
    #[test]
    fn verified_bundle_installs_once_and_never_replaces_user_file() {
        let temp = tempfile::tempdir().unwrap();
        let resource = temp.path().join("resource");
        fs::create_dir(&resource).unwrap();
        let bytes = b"synthetic CLI";
        fs::write(resource.join("oasis7_context"), bytes).unwrap();
        let c = CliCatalog {
            schema_version: 1,
            tool_id: "oasis7_context".into(),
            source_head_oid: "a".repeat(40),
            target: current_target(),
            release_id: "release-one".into(),
            file: "oasis7_context".into(),
            sha256: oasis7_signer_protocol::identity::sha256_hex(bytes),
            size_bytes: bytes.len() as u64,
        };
        fs::write(
            resource.join("catalog.json"),
            serde_json::to_vec(&c).unwrap(),
        )
        .unwrap();
        let root = fs::canonicalize(temp.path()).unwrap().join("tools");
        let first = install_bundled(&resource, &root).unwrap();
        let jobs = super::super::jobs::ToolJobs::open(":memory:").unwrap();
        let (reservation, fingerprint) = bundle_identity(&resource, &root).unwrap();
        jobs.admit("cli-op", &fingerprint, &reservation, "cli_install")
            .unwrap();
        jobs.finish("cli-op", "unknown", None).unwrap();
        let active_installer = installation_lock(&root).unwrap();
        assert!(reconcile_cli_install(&jobs, "cli-op", &resource, &root).is_err());
        assert_eq!(jobs.list().unwrap()[0].state, "unknown");
        drop(active_installer);

        assert!(
            reconcile_cli_install(&jobs, "cli-op", &resource, &root)
                .unwrap()
                .is_some()
        );
        assert_eq!(jobs.list().unwrap()[0].state, "succeeded");
        assert!(!first.terminal_path_configured);
        assert_eq!(
            observe_installed(&resource, &root).unwrap().unwrap().path,
            first.path
        );
        assert_eq!(install_bundled(&resource, &root).unwrap().path, first.path);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&first.path, fs::Permissions::from_mode(0o400)).unwrap();
            assert!(observe_installed(&resource, &root).is_err());
            assert!(install_bundled(&resource, &root).is_err());
            fs::set_permissions(&first.path, fs::Permissions::from_mode(0o500)).unwrap();
            assert!(observe_installed(&resource, &root).unwrap().is_some());
            fs::set_permissions(&first.path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::write(&first.path, b"user binary").unwrap();
        assert!(install_bundled(&resource, &root).is_err());
        assert!(observe_installed(&resource, &root).is_err());
        assert_eq!(fs::read(first.path).unwrap(), b"user binary");
    }
}
