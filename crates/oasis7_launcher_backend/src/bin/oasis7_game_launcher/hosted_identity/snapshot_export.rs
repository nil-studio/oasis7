//! Offline operator command; no HTTP route and no secret material in output.
use super::{
    model::{IdentityNamespace, RpConfig},
    store::IdentityStore,
};
use std::{io::Read, path::Path};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    identity_namespace: IdentityNamespace,
    rp_config: RpConfig,
}
#[cfg(unix)]
fn private_file(path: &Path) -> Result<std::fs::File, String> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if !path.is_absolute() || path.canonicalize().map_err(|_| "input unavailable")? != path {
        return Err("input path must be canonical and absolute".into());
    }
    verify_parent(path)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| "input unavailable")?;
    let m = file.metadata().map_err(|_| "input metadata unavailable")?;
    // SAFETY: geteuid has no preconditions and reads the calling process identity.
    if !m.is_file()
        || m.nlink() != 1
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o777 != 0o600
    {
        return Err("input must be operator owned, single linked, regular mode 0600".into());
    }
    Ok(file)
}
#[cfg(unix)]
fn verify_parent(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let parent = path.parent().ok_or("parent required")?;
    let m = std::fs::metadata(parent).map_err(|_| "parent metadata unavailable")?;
    // SAFETY: geteuid has no preconditions.
    let uid = unsafe { libc::geteuid() };
    if !m.is_dir() || m.mode() & 0o022 != 0 || (m.uid() != uid && m.uid() != 0) {
        return Err(
            "parent directory must be operator/root owned and not group/world writable".into(),
        );
    }
    Ok(())
}
#[cfg(not(unix))]
fn private_file(_: &Path) -> Result<std::fs::File, String> {
    Err("export requires Unix private storage".into())
}
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    if args.len() != 3 {
        return Err("usage: --identity-snapshot-export <absolute SQLite DB> <private namespace-RP request.json> <new output.json>; Tablestore unsupported without a consistent snapshot exporter".into());
    }
    let db = Path::new(&args[0]);
    let request = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let mut database_guard = private_file(db)?;
    let mut magic = [0; 16];
    database_guard
        .read_exact(&mut magic)
        .map_err(|_| "SQLite identity database header unavailable")?;
    if &magic != b"SQLite format 3\0" {
        return Err("unsupported identity backend: consistent SQLite snapshot required; Tablestore export is unavailable".into());
    }
    let mut bytes = Vec::new();
    private_file(request)?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "request read failed")?;
    if bytes.len() > 65536 {
        return Err("request exceeds byte bound".into());
    }
    let request: Request =
        serde_json::from_slice(&bytes).map_err(|_| "invalid namespace-RP request")?;
    if !output.is_absolute()
        || output
            .parent()
            .ok_or("output parent required")?
            .canonicalize()
            .map_err(|_| "output parent unavailable")?
            != output.parent().ok_or("output parent required")?
    {
        return Err("output parent must be canonical and absolute".into());
    }
    #[cfg(unix)]
    verify_parent(output)?;
    // The verified inode remains open and is rechecked after SQLite's transaction.
    // Offline operators must fence writers and protect the parent directory;
    // opening the canonical path preserves SQLite's real WAL filename.
    let export_path = db.to_path_buf();
    let snapshot = IdentityStore::export_sqlite_snapshot(
        &export_path,
        request.identity_namespace,
        request.rp_config,
    )
    .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let before = database_guard
            .metadata()
            .map_err(|_| "database metadata unavailable")?;
        let after = std::fs::symlink_metadata(db).map_err(|_| "database path changed")?;
        if before.dev() != after.dev()
            || before.ino() != after.ino()
            || after.file_type().is_symlink()
        {
            return Err("database path changed during export".into());
        }
    }
    let bytes =
        serde_json::to_vec_pretty(&snapshot).map_err(|_| "snapshot serialization failed")?;
    let directory = oasis7_game_client::private_storage::open_root(
        output.parent().ok_or("output parent unavailable")?,
    )?;
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid output name")?;
    if !oasis7_game_client::private_storage::publish_immutable(&directory, name, &bytes)? {
        return Err("output must be new; existing evidence retained".into());
    }
    println!(
        "Complete identity snapshot exported: {} accounts, {} bindings; digest {}",
        snapshot.account_count, snapshot.binding_count, snapshot.source_snapshot_digest
    );
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn command_rejects_namespace_tamper_links_and_overwrite_without_partial_output() {
        let dir = std::env::temp_dir().join(format!("oasis7-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let dir = dir.canonicalize().unwrap();
        let db = dir.join("identity.sqlite");
        let request = dir.join("request.json");
        let out = dir.join("snapshot.json");
        let ns = IdentityNamespace {
            identity_service_id: "test".into(),
            environment_id: "isolated".into(),
        };
        let rp = RpConfig {
            rp_id: "localhost".into(),
            allowed_origins: vec!["http://localhost".into()],
            version: 1,
            identity_instance_epoch: 1,
        };
        let _store = IdentityStore::open(&db, ns.clone(), rp.clone()).unwrap();
        let write_request = |namespace: &IdentityNamespace| {
            std::fs::write(
                &request,
                serde_json::to_vec(
                    &serde_json::json!({"identity_namespace":namespace,"rp_config":rp}),
                )
                .unwrap(),
            )
            .unwrap();
            std::fs::set_permissions(&request, std::fs::Permissions::from_mode(0o600)).unwrap();
        };
        let args = vec![
            db.to_str().unwrap().into(),
            request.to_str().unwrap().into(),
            out.to_str().unwrap().into(),
        ];
        let mut wrong = ns.clone();
        wrong.environment_id = "other".into();
        write_request(&wrong);
        assert!(run(&args).is_err());
        assert!(!out.exists());
        write_request(&ns);
        std::fs::hard_link(&request, dir.join("linked.json")).unwrap();
        assert!(run(&args).is_err());
        assert!(!out.exists());
        std::fs::remove_file(dir.join("linked.json")).unwrap();
        std::fs::set_permissions(&request, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(run(&args).is_err());
        assert!(!out.exists());
        std::fs::set_permissions(&request, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&db, dir.join("link.sqlite")).unwrap();
        let mut invalid = args.clone();
        invalid[0] = dir.join("link.sqlite").to_str().unwrap().into();
        assert!(run(&invalid).is_err());
        assert!(!out.exists());
        let unsupported = dir.join("remote.json");
        std::fs::write(&unsupported, b"not sqlite backend document").unwrap();
        std::fs::set_permissions(&unsupported, std::fs::Permissions::from_mode(0o600)).unwrap();
        invalid[0] = unsupported.to_str().unwrap().into();
        assert!(
            run(&invalid)
                .unwrap_err()
                .contains("unsupported identity backend")
        );
        assert!(!out.exists());
        run(&args).unwrap();
        let original = std::fs::read(&out).unwrap();
        assert!(run(&args).is_err());
        assert_eq!(std::fs::read(&out).unwrap(), original);
        assert_eq!(
            std::fs::metadata(&out).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(_store);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
