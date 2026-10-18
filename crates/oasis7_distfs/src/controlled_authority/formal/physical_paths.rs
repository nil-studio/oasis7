//! Registered storage paths are owned regular files/directories, never links.
//! Same-user concurrent mutation remains outside the endpoint lease contract.
//! These checks reject an existing unsafe path; final files still use O_NOFOLLOW.
use super::*;
pub(crate) fn metadata(metadata: &fs::Metadata, directory: bool) -> Result<(), FormalError> {
    if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err(invalid("physical registered path type"));
    }
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no caller preconditions.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
            return Err(invalid("physical registered owner/write mode"));
        }
    }
    Ok(())
}
pub(super) fn directory(path: &Path) -> Result<(), FormalError> {
    metadata(&fs::symlink_metadata(path).map_err(io)?, true)
}
/// Check every controlled inner directory through its storage root. We do not
/// require ownership of system ancestors such as /tmp or /Users.
pub(crate) fn parents(path: &Path) -> Result<(), FormalError> {
    let mut cursor = path
        .parent()
        .ok_or_else(|| invalid("physical path parent"))?;
    loop {
        directory(cursor)?;
        let name = cursor
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let parent = cursor.parent();
        let in_generations = parent
            .and_then(|p| p.file_name())
            .is_some_and(|name| name == ".generations-v1");
        if [
            SELECTORS,
            FRAMES,
            ".generations-v1",
            ".catalog-v2",
            ".catalog-v3",
            ".physical-staging-v1",
            ".retained-logical-v1",
        ]
        .contains(&name)
            || in_generations
        {
            cursor = parent.ok_or_else(|| invalid("physical controlled directory parent"))?;
        } else {
            break;
        }
    }
    Ok(())
}
pub(super) fn sync(path: &Path) -> Result<(), FormalError> {
    let expected = fs::symlink_metadata(path).map_err(io)?;
    metadata(&expected, expected.is_dir())?;
    if expected.is_dir() {
        parents(&path.join(".directory-check"))?;
    } else {
        parents(path)?;
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    let file = options.open(path).map_err(io)?;
    metadata(&file.metadata().map_err(io)?, expected.is_dir())?;
    file.sync_all().map_err(io)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{Fixture, object};
    use super::*;
    #[test]
    #[cfg(unix)]
    fn real_directory_symlink_and_writable_owner_paths_are_rejected() {
        use std::os::unix::{fs::PermissionsExt, fs::symlink};
        let fixture = Fixture::new();
        let value = object(vec![17; 100000]);
        persist(&fixture.0, std::slice::from_ref(&value)).unwrap();
        let frames = fixture.0.join(FRAMES);
        let moved = fixture.0.join("private-original-frames");
        fs::rename(&frames, &moved).unwrap();
        symlink(&moved, &frames).unwrap();
        assert!(read_object(&fixture.0.join(&value.0), &value.0, false).is_err());
        fs::remove_file(&frames).unwrap();
        fs::rename(&moved, &frames).unwrap();
        fs::set_permissions(&frames, fs::Permissions::from_mode(0o722)).unwrap();
        assert!(read_object(&fixture.0.join(&value.0), &value.0, false).is_err());
        fs::set_permissions(&frames, fs::Permissions::from_mode(0o700)).unwrap();
        let selected = fixture.0.join(SELECTORS).join(&value.0);
        fs::set_permissions(&selected, fs::Permissions::from_mode(0o622)).unwrap();
        assert!(read_object(&fixture.0.join(&value.0), &value.0, false).is_err());
        let raw = object(vec![33; 100]);
        fs::write(fixture.0.join(&raw.0), &raw.1).unwrap();
        fs::set_permissions(fixture.0.join(&raw.0), fs::Permissions::from_mode(0o622)).unwrap();
        assert!(
            super::super::super::business_pack::read_object(&fixture.0.join(&raw.0), &raw.0)
                .is_err()
        );
        // Real foreign-owner metadata, without requiring elevated chown.
        // SAFETY: geteuid has no caller preconditions.
        if unsafe { libc::geteuid() } != 0 {
            assert!(directory(Path::new("/")).is_err());
        }
    }
}
