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
    directory_with_step(path, &mut || Ok(()))
}
pub(crate) fn directory_with_step(
    path: &Path,
    step: &mut impl FnMut() -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    step()?;
    metadata(&fs::symlink_metadata(path).map_err(io)?, true)
}
/// Check every controlled inner directory through its storage root. We do not
/// require ownership of system ancestors such as /tmp or /Users.
pub(crate) fn parents(path: &Path) -> Result<(), FormalError> {
    parents_with_step(path, &mut || Ok(()))
}
pub(crate) fn parents_with_step(
    path: &Path,
    step: &mut impl FnMut() -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    let mut cursor = path
        .parent()
        .ok_or_else(|| invalid("physical path parent"))?;
    loop {
        directory_with_step(cursor, step)?;
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
    sync_with_step(path, &mut || Ok(()))
}
pub(crate) fn sync_with_step(
    path: &Path,
    step: &mut impl FnMut() -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    step()?;
    let expected = fs::symlink_metadata(path).map_err(io)?;
    metadata(&expected, expected.is_dir())?;
    if expected.is_dir() {
        parents_with_step(&path.join(".directory-check"), step)?;
    } else {
        parents_with_step(path, step)?;
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    step()?;
    let file = options.open(path).map_err(io)?;
    step()?;
    metadata(&file.metadata().map_err(io)?, expected.is_dir())?;
    step()?;
    file.sync_all().map_err(io)?;
    step()
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
        directory(&fixture.0).unwrap();
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
        // Use actual ownership, not the assumption that a non-root process
        // cannot own /. User-namespace containers can map / to the caller.
        // SAFETY: geteuid has no caller preconditions.
        let owner = unsafe { libc::geteuid() };
        let mut foreign_owner_checked = false;
        for path in ["/", "/etc", "/usr"] {
            let path = Path::new(path);
            let Ok(metadata) = fs::symlink_metadata(path) else {
                continue;
            };
            // Exclude links, wrong types and writable paths so only the
            // foreign-owner predicate can explain this rejection.
            if metadata.is_dir() && metadata.uid() != owner && metadata.mode() & 0o022 == 0 {
                let error = directory(path).unwrap_err();
                assert!(
                    matches!(&error, FormalError::Invalid(reason) if reason == "physical registered owner/write mode"),
                    "{path:?}: {error}"
                );
                foreign_owner_checked = true;
            }
        }
        if !foreign_owner_checked {
            eprintln!(
                "foreign-owner directory fixture unavailable; owner-mismatch subcase not exercised"
            );
        }
    }
}
