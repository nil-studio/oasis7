//! Authenticated parent directory identities plus live NOFOLLOW descriptor pins.
use super::*;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::os::unix::fs::MetadataExt;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct DirectoryIdentity {
    pub(super) device: u64,
    pub(super) inode: u64,
}
impl DirectoryIdentity {
    pub(super) fn read(path: &Path) -> Result<Self, String> {
        crate::controlled_live_config::require_private_owned(path, true)?;
        let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.ino() == 0 {
            return Err("active private directory identity invalid".into());
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}
pub(super) fn paths(segment: &Path) -> Result<[PathBuf; 5], String> {
    let parent = segment.parent().ok_or("active segment parent absent")?;
    let records = parent.parent().ok_or("active records parent absent")?;
    Ok([
        records.into(),
        parent.into(),
        segment.into(),
        segment.join(DIRECTORY),
        segment
            .join(DIRECTORY)
            .join(storage::NODE_PRIVATE_DIRECTORY),
    ])
}
pub(crate) struct WindowDirectoryPins {
    directories: Vec<(PathBuf, File, DirectoryIdentity)>,
}
impl WindowDirectoryPins {
    pub(crate) fn open(segment: &Path, origin: &WindowOrigin) -> Result<Self, String> {
        let mut directories = Vec::new();
        for (path, expected) in paths(segment)?
            .into_iter()
            .zip(origin.directory_identities())
        {
            let descriptor = super::super::successor_carriers::open_absolute_directory(&path)?;
            let metadata = descriptor.metadata().map_err(|e| e.to_string())?;
            let actual = DirectoryIdentity {
                device: metadata.dev(),
                inode: metadata.ino(),
            };
            if &actual != expected || DirectoryIdentity::read(&path)? != actual {
                return Err("active private directory differs from protected Origin".into());
            }
            directories.push((path, descriptor, actual));
        }
        let pins = Self { directories };
        pins.verify()?;
        Ok(pins)
    }
    pub(crate) fn verify(&self) -> Result<(), String> {
        for (path, descriptor, expected) in &self.directories {
            let metadata = descriptor.metadata().map_err(|e| e.to_string())?;
            let current = super::super::successor_carriers::open_absolute_directory(path)?;
            let current = current.metadata().map_err(|e| e.to_string())?;
            if metadata.dev() != expected.device
                || metadata.ino() != expected.inode
                || current.dev() != expected.device
                || current.ino() != expected.inode
                || DirectoryIdentity::read(path)? != *expected
            {
                return Err("active private parent directory replaced after open".into());
            }
        }
        Ok(())
    }
}
