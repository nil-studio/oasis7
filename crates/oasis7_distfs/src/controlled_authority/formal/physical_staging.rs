//! Only known, never-selected publication temporaries can be reclaimed here.
//! Both passes run under the stable root's exclusive physical fence.
use super::*;
const STAGING: &str = ".physical-staging-v1";
fn validate(root: &Path, path: &Path) -> Result<u64, FormalError> {
    if path.parent() != Some(root.join(STAGING).as_path()) {
        return Err(invalid("physical staging scope"));
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("physical staging filename"))?;
    hash_hex(name)?;
    paths::parents(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    let file = options.open(path).map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    paths::metadata(&metadata, false)?;
    if metadata.len() > LIMIT as u64 {
        return Err(invalid("physical staging file bound"));
    }
    Ok(metadata.len())
}
/// Preflight the ENTIRE staging namespace before the caller unlinks anything,
/// including frames. Unknown names/types/owners/modes stop the whole sweep.
pub(crate) fn preflight(root: &Path) -> Result<(Vec<PathBuf>, u64), FormalError> {
    let staging = root.join(STAGING);
    match fs::symlink_metadata(&staging) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(e) => return Err(io(e)),
        Ok(_) => paths::directory(&staging)?,
    }
    let mut paths = Vec::new();
    let mut bytes = 0u64;
    for entry in fs::read_dir(staging).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if paths.len() >= MAX_OBJECTS {
            return Err(invalid("physical staging preflight budget"));
        }
        bytes = bytes
            .checked_add(validate(root, &path)?)
            .ok_or_else(|| invalid("physical staging accounting overflow"))?;
        paths.push(path);
    }
    Ok((paths, bytes))
}
pub(crate) fn reclaim(root: &Path, paths: &[PathBuf]) -> Result<(), FormalError> {
    // Retest all paths before the first unlink. Concurrent same-user mutation
    // remains outside the registered endpoint lease/physical fence contract.
    for path in paths {
        validate(root, path)?;
    }
    for path in paths {
        fs::remove_file(path).map_err(io)?;
    }
    if !paths.is_empty() {
        sync(&root.join(STAGING))?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::super::tests::Fixture;
    use super::*;
    #[test]
    fn unknown_staging_blocks_all_unlinks_then_known_partial_private_data_can_reclaim() {
        let fixture = Fixture::new();
        let staging = fixture.0.join(STAGING);
        directory(&staging).unwrap();
        let known = staging.join("ab".repeat(32));
        create(&known, b"partial private frame").unwrap();
        let unknown = staging.join("unknown");
        create(&unknown, b"do not remove").unwrap();
        let _fence = pin(&fixture.0, true).unwrap();
        assert!(preflight(&fixture.0).is_err());
        assert!(known.exists());
        assert!(unknown.exists());
        fs::remove_file(&unknown).unwrap();
        let (files, bytes) = preflight(&fixture.0).unwrap();
        assert!(files.contains(&known));
        assert!(bytes > 0);
        reclaim(&fixture.0, &files).unwrap();
        assert!(!known.exists());
    }
    #[test]
    #[cfg(unix)]
    fn staging_symlink_and_writable_file_are_never_sweepable() {
        use std::os::unix::{fs::PermissionsExt, fs::symlink};
        let fixture = Fixture::new();
        let staging = fixture.0.join(STAGING);
        directory(&staging).unwrap();
        let outside = fixture.0.parent().unwrap().join("outside");
        create(&outside, b"preserve").unwrap();
        let alias = staging.join("ac".repeat(32));
        symlink(&outside, &alias).unwrap();
        let _fence = pin(&fixture.0, true).unwrap();
        assert!(preflight(&fixture.0).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"preserve");
        fs::remove_file(&alias).unwrap();
        create(&alias, b"private").unwrap();
        fs::set_permissions(&alias, fs::Permissions::from_mode(0o622)).unwrap();
        assert!(preflight(&fixture.0).is_err());
        assert!(alias.exists());
    }
}
