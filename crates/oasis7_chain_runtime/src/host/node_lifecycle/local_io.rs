use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

const MAX_JSON: u64 = 4 * 1024 * 1024;

pub(crate) fn reject_symlinks(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err("parent-directory path components are unsupported".into());
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(m) if m.file_type().is_symlink() => {
                return Err(format!("symlink refused: {}", current.display()));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
pub(crate) fn regular(path: &Path) -> Result<PathBuf, String> {
    reject_symlinks(path)?;
    if !fs::metadata(path).map_err(|e| e.to_string())?.is_file() {
        return Err(format!("not a regular file: {}", path.display()));
    }
    fs::canonicalize(path).map_err(|e| e.to_string())
}
pub(crate) fn private_dir(path: &Path) -> Result<(), String> {
    reject_symlinks(path)?;
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    match builder.create(path) {
        Ok(()) => {
            sync_dir(path)?;
            sync_dir(path.parent().ok_or("directory parent missing")?)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.to_string()),
    }
    super::super::controlled_live_config::require_private_owned(path, true)
}
pub(crate) fn private_file(path: &Path) -> Result<File, String> {
    reject_symlinks(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path).map_err(|e| e.to_string())?;
    super::super::controlled_live_config::require_private_owned(path, false)?;
    Ok(file)
}
pub(crate) fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let path = regular(path)?;
    let file = File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_JSON {
        return Err("node metadata size limit".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_JSON + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_JSON {
        return Err("node metadata size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
pub(crate) fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    reject_symlinks(path)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_JSON {
        return Err("node metadata size limit".into());
    }
    let parent = path.parent().ok_or("node metadata parent missing")?;
    let temporary = parent.join(format!(".node-write-{}", random_id()?));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temporary, path).map_err(|e| e.to_string())?;
        sync_dir(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
pub(crate) fn sha256(path: &Path) -> Result<String, String> {
    let path = regular(path)?;
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}
pub(crate) fn digest<T: Serialize>(value: &T) -> Result<String, String> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(value).map_err(|e| e.to_string())?,
    )))
}
pub(crate) fn random_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(hex::encode(bytes))
}
pub(crate) fn sync_dir(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}
