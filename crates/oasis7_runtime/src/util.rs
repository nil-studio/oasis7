//! Utility functions for the runtime module.

use serde::de::{DeserializeOwned, Deserializer, Error as DeError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use super::error::WorldError;

#[cfg(windows)]
fn platform_atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn platform_atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(unix)]
pub(crate) fn sync_directory(path: &Path) -> Result<(), WorldError> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn sync_directory(_path: &Path) -> Result<(), WorldError> {
    Ok(())
}

pub fn deserialize_btreemap_u64_keys<'de, D, V>(
    deserializer: D,
) -> Result<BTreeMap<u64, V>, D::Error>
where
    D: Deserializer<'de>,
    V: Deserialize<'de>,
{
    #[derive(Deserialize, PartialEq, Eq, PartialOrd, Ord)]
    #[serde(untagged)]
    enum NumericMapKey {
        String(String),
        U64(u64),
        I64(i64),
    }

    let raw = BTreeMap::<NumericMapKey, V>::deserialize(deserializer)?;
    raw.into_iter()
        .map(|(key, value)| {
            let parsed = match key {
                NumericMapKey::String(key) => key.parse::<u64>().map_err(|err| {
                    D::Error::custom(format!("invalid numeric map key `{key}`: {err}"))
                })?,
                NumericMapKey::U64(key) => key,
                NumericMapKey::I64(key) => u64::try_from(key).map_err(|_| {
                    D::Error::custom(format!("invalid numeric map key `{key}`: must be >= 0"))
                })?,
            };
            Ok((parsed, value))
        })
        .collect()
}

/// Compute SHA256 hash of a serializable value.
pub fn hash_json<T: Serialize>(value: &T) -> Result<String, WorldError> {
    let bytes = serde_json::to_vec(value)?;
    Ok(sha256_hex(&bytes))
}

/// Serialize a value into canonical CBOR bytes using deterministic ordering.
pub fn to_canonical_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, WorldError> {
    let mut buf = Vec::with_capacity(256);
    let canonical_value = serde_cbor::value::to_value(value)?;
    let mut serializer = serde_cbor::ser::Serializer::new(&mut buf);
    serializer.self_describe()?;
    canonical_value.serialize(&mut serializer)?;
    Ok(buf)
}

/// Compute SHA256 hash of bytes and return as hex string.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Write a serializable value to a JSON file.
pub fn write_json_to_path<T: Serialize>(value: &T, path: &Path) -> Result<(), WorldError> {
    let data = serde_json::to_vec_pretty(value)?;
    fs::write(path, data)?;
    Ok(())
}

/// Atomically replace a file and make both its contents and directory entry durable.
pub fn atomic_write_bytes_to_path(data: &[u8], path: &Path) -> Result<(), WorldError> {
    let parent = path
        .parent()
        .ok_or_else(|| WorldError::DistributedValidationFailed {
            reason: format!("atomic write path has no parent: {}", path.display()),
        })?;
    fs::create_dir_all(parent)?;
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(tmp.as_path())?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    platform_atomic_replace(tmp.as_path(), path)?;
    sync_directory(parent)?;
    Ok(())
}

pub fn atomic_write_json_to_path<T: Serialize>(value: &T, path: &Path) -> Result<(), WorldError> {
    atomic_write_bytes_to_path(serde_json::to_vec_pretty(value)?.as_slice(), path)
}

/// Read a JSON file and deserialize it.
pub fn read_json_from_path<T: DeserializeOwned>(path: &Path) -> Result<T, WorldError> {
    let data = fs::read(path)?;
    Ok(serde_json::from_slice(&data)?)
}

#[cfg(test)]
mod tests {
    use super::{atomic_write_bytes_to_path, sync_directory};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("oasis7-runtime-util-{label}-{unique}"));
        fs::create_dir_all(&path).expect("create tempdir");
        path
    }

    #[test]
    fn atomic_write_replaces_an_existing_destination() {
        let temp = temp_dir("replace");
        let path = temp.join("state.json");
        fs::write(&path, b"old").expect("write existing destination");

        atomic_write_bytes_to_path(b"new", &path).expect("replace existing destination");

        assert_eq!(fs::read(path).expect("read replaced destination"), b"new");
        fs::remove_dir_all(temp).expect("remove tempdir");
    }

    #[test]
    fn syncing_a_directory_uses_supported_platform_semantics() {
        let temp = temp_dir("sync-directory");
        sync_directory(&temp).expect("sync directory");
        fs::remove_dir_all(temp).expect("remove tempdir");
    }
}
