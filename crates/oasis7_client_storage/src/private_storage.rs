//! Local encrypted record primitives; callers own payload and authorization semantics.
use std::path::Path;
use zeroize::Zeroizing;

const MAX_PRIVATE_GOAL_FILE_BYTES: usize = 32 * 1024;
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{
    Dir, DirBuilder, DirBuilderExt, MetadataExt, OpenOptions, OpenOptionsExt, PermissionsExt,
};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use std::{
    io::{Read, Write},
    path::Component,
};

fn validate_record_name(name: &str) -> Result<(), String> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err("invalid private record name".into());
    }
    Ok(())
}
fn io_error(_: std::io::Error) -> String {
    "private goal filesystem operation failed".into()
}
fn sync(dir: &Dir) -> Result<(), String> {
    // cap-std directory handles are O_PATH on Linux, which cannot fsync.
    // Reopen the held directory itself, without resolving an ambient path,
    // as a readable directory descriptor. Setting only the custom DIRECTORY
    // flag avoids cap-primitives' dir_required -> O_PATH optimization.
    let mut options = OpenOptions::new();
    options
        .read(true)
        .follow(FollowSymlinks::No)
        .custom_flags(libc::O_DIRECTORY);
    dir.open_with(".", &options)
        .map_err(io_error)?
        .sync_all()
        .map_err(io_error)
}
fn private_file(metadata: &cap_std::fs::Metadata) -> Result<(), String> {
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o7777 != 0o600
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
    {
        return Err("private goal file permissions or type invalid".into());
    }
    Ok(())
}
pub fn private_directory(dir: &Dir) -> Result<(), String> {
    let metadata = dir.dir_metadata().map_err(io_error)?;
    if metadata.permissions().mode() & 0o7777 != 0o700
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err("private goal directory must be owned by operator with mode 0700".into());
    }
    Ok(())
}
pub fn open_root(root: &Path) -> Result<Dir, String> {
    if !root.is_absolute() {
        return Err("private goal root must be an absolute operator-private path".into());
    }
    let components = root.components().collect::<Vec<_>>();
    if components.len() < 2
        || components
            .iter()
            .skip(1)
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("invalid private goal root path".into());
    }
    let mut parent = Dir::open_ambient_dir("/", cap_std::ambient_authority()).map_err(io_error)?;
    for component in &components[1..components.len() - 1] {
        parent = parent
            .open_dir_nofollow(component.as_os_str())
            .map_err(io_error)?;
    }
    let name = components
        .last()
        .expect("root has final component")
        .as_os_str();
    let mut builder = DirBuilder::new();
    builder.mode(0o700);
    match parent.create_dir_with(name, &builder) {
        Ok(()) => sync(&parent)?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(io_error(e)),
    }
    let directory = parent.open_dir_nofollow(name).map_err(io_error)?;
    private_directory(&directory)?;
    sync(&directory)?;
    Ok(directory)
}
fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| "private goal randomness unavailable")?;
    Ok(bytes)
}
const KEY_MARKER: &str = ".key-verifier-v1";
const KEY_MAGIC: &[u8; 8] = b"O7KEY001";
const KEY_AAD: &[u8] = b"oasis7/private-goal-key-verifier/v1";
const KEY_BODY: &[u8] = b"oasis7 private goal namespace key v1";

pub fn read_private(dir: &Dir, name: &str, limit: usize) -> Result<Vec<u8>, String> {
    validate_record_name(name)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .follow(FollowSymlinks::No)
        .custom_flags(libc::O_NONBLOCK);
    let file = dir.open_with(name, &options).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    private_file(&metadata)?;
    if metadata.len() > limit as u64 {
        return Err("private goal file exceeds byte limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > limit {
        return Err("private goal file exceeds byte limit".into());
    }
    Ok(bytes)
}

// Shared durable, immutable publication. A collision never replaces the winner.
pub fn publish_immutable(dir: &Dir, name: &str, bytes: &[u8]) -> Result<bool, String> {
    validate_record_name(name)?;
    for _ in 0..8 {
        let temp = format!(".{}.tmp", hex::encode(random::<32>()?));
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .mode(0o600)
            .follow(FollowSymlinks::No);
        let mut file = match dir.open_with(&temp, &options) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_error(e)),
        };
        if let Err(error) = file
            .metadata()
            .map_err(io_error)
            .and_then(|metadata| private_file(&metadata))
        {
            let _ = dir.remove_file(&temp);
            return Err(error);
        }
        let publication = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            dir.hard_link(&temp, dir, name)
        })();
        let cleanup = dir.remove_file(&temp);
        match publication {
            Ok(()) => {
                cleanup.map_err(io_error)?;
                sync(dir)?;
                return Ok(true);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                cleanup.map_err(io_error)?;
                sync(dir)?;
                return Ok(false);
            }
            Err(e) => {
                let _ = cleanup;
                return Err(io_error(e));
            }
        }
    }
    Err("private goal temporary collision retry budget exhausted".into())
}

pub fn verify_key(dir: &Dir, key: &[u8; 32]) -> Result<(), String> {
    let bytes = read_private(dir, KEY_MARKER, 256)?;
    if bytes.len() < 48 || &bytes[..8] != KEY_MAGIC {
        return Err("invalid private goal key verifier".into());
    }
    let plaintext = XChaCha20Poly1305::new(key.into())
        .decrypt(
            &XNonce::try_from(&bytes[8..32]).map_err(|_| "invalid private goal nonce")?,
            Payload {
                msg: &bytes[32..],
                aad: KEY_AAD,
            },
        )
        .map_err(|_| "private goal namespace key mismatch or corruption")?;
    if plaintext.len() != KEY_BODY.len() + 32 || !plaintext.starts_with(KEY_BODY) {
        return Err("invalid private goal key verifier body".into());
    }
    Ok(())
}

fn verify_key_durable(dir: &Dir, key: &[u8; 32]) -> Result<(), String> {
    verify_key(dir, key)?;
    // A racing initializer may have linked and removed its temporary without
    // yet syncing the directory. Every successful open completes publication.
    sync(dir)
}

// Prepared operations use their own envelope/domain, while sharing exactly
// the same permission, key-verifier and durable publication boundary.
pub fn publish_record(
    dir: &Dir,
    key: &[u8; 32],
    name: &str,
    magic: &[u8; 8],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<bool, String> {
    publish_bounded_record(
        dir,
        key,
        name,
        magic,
        aad,
        plaintext,
        MAX_PRIVATE_GOAL_FILE_BYTES,
    )
}
pub fn publish_bounded_record(
    dir: &Dir,
    key: &[u8; 32],
    name: &str,
    magic: &[u8; 8],
    aad: &[u8],
    plaintext: &[u8],
    maximum: usize,
) -> Result<bool, String> {
    validate_record_name(name)?;
    private_directory(dir)?;
    verify_key(dir, key)?;
    let bytes = encode_record(key, magic, aad, plaintext, maximum)?;
    publish_immutable(dir, name, &bytes)
}

fn encode_record(
    key: &[u8; 32],
    magic: &[u8; 8],
    aad: &[u8],
    plaintext: &[u8],
    maximum: usize,
) -> Result<Vec<u8>, String> {
    let nonce = random::<24>()?;
    let ciphertext = XChaCha20Poly1305::new(key.into())
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| "private prepared goal encryption failed")?;
    let mut bytes = Vec::with_capacity(32 + ciphertext.len());
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(&nonce);
    bytes.extend_from_slice(&ciphertext);
    if bytes.len() > maximum {
        return Err("private prepared record file exceeds limit".into());
    }
    Ok(bytes)
}

/// Lock a private regular file relative to the held directory. The returned
/// handle must remain alive through the complete local read-modify-write.
pub fn lock_record(dir: &Dir, key: &[u8; 32], name: &str) -> Result<(std::fs::File, bool), String> {
    validate_record_name(name)?;
    private_directory(dir)?;
    verify_key(dir, key)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .follow(FollowSymlinks::No)
        .custom_flags(libc::O_NONBLOCK);
    let (file, created) = match dir.open_with(name, &options) {
        Ok(file) => (file, true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let mut existing = OpenOptions::new();
            existing
                .read(true)
                .write(true)
                .follow(FollowSymlinks::No)
                .custom_flags(libc::O_NONBLOCK);
            (dir.open_with(name, &existing).map_err(io_error)?, false)
        }
        Err(error) => return Err(io_error(error)),
    };
    private_file(&file.metadata().map_err(io_error)?)?;
    let file = file.into_std();
    file.lock().map_err(io_error)?;
    sync(dir)?;
    Ok((file, created))
}

/// Atomic encrypted replacement for executor-local counters. Callers must
/// hold lock_record for this namespace; immutable operation records use publish.
pub fn replace_bounded_record(
    dir: &Dir,
    key: &[u8; 32],
    name: &str,
    magic: &[u8; 8],
    aad: &[u8],
    plaintext: &[u8],
    maximum: usize,
) -> Result<(), String> {
    validate_record_name(name)?;
    private_directory(dir)?;
    verify_key(dir, key)?;
    let bytes = encode_record(key, magic, aad, plaintext, maximum)?;
    let temp = format!(".{}.tmp", hex::encode(random::<32>()?));
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .mode(0o600)
        .follow(FollowSymlinks::No);
    let mut file = dir.open_with(&temp, &options).map_err(io_error)?;
    let result = (|| {
        private_file(&file.metadata().map_err(io_error)?)?;
        file.write_all(&bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        dir.rename(&temp, dir, name).map_err(io_error)?;
        sync(dir)
    })();
    if result.is_err() {
        let _ = dir.remove_file(&temp);
    }
    result
}

pub fn read_record(
    dir: &Dir,
    key: &[u8; 32],
    name: &str,
    magic: &[u8; 8],
    aad: &[u8],
) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    read_bounded_record(dir, key, name, magic, aad, MAX_PRIVATE_GOAL_FILE_BYTES)
}
pub fn read_bounded_record(
    dir: &Dir,
    key: &[u8; 32],
    name: &str,
    magic: &[u8; 8],
    aad: &[u8],
    maximum: usize,
) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    validate_record_name(name)?;
    private_directory(dir)?;
    verify_key(dir, key)?;
    match dir.symlink_metadata(name) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(error)),
        Ok(_) => (),
    }
    let bytes = read_private(dir, name, maximum)?;
    if bytes.len() < 48 || &bytes[..8] != magic {
        return Err("invalid private prepared goal envelope".into());
    }
    let plaintext = XChaCha20Poly1305::new(key.into())
        .decrypt(
            &XNonce::try_from(&bytes[8..32]).map_err(|_| "invalid private goal nonce")?,
            Payload {
                msg: &bytes[32..],
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| "private prepared goal authentication failed")?;
    // A retry may observe the concurrent publisher after linking, before its
    // directory fsync. Every successful read completes durable publication.
    sync(dir)?;
    Ok(Some(plaintext))
}

/// A marker-less nonempty namespace is never repaired or re-keyed. This includes
/// interrupted initialization temporaries: operator inspection is required.
pub fn initialize_key(dir: &Dir, key: &[u8; 32]) -> Result<(), String> {
    match dir.symlink_metadata(KEY_MARKER) {
        Ok(_) => return verify_key_durable(dir, key),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(io_error(e)),
    }
    if dir.entries().map_err(io_error)?.next().is_some() {
        // A concurrent initializer may have published while we inspected.
        // Verify it if present; never adopt any other nonempty namespace.
        return verify_key_durable(dir, key);
    }
    let nonce = random::<24>()?;
    let mut challenge = KEY_BODY.to_vec();
    challenge.extend_from_slice(&random::<32>()?);
    let encrypted = XChaCha20Poly1305::new(key.into())
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &challenge,
                aad: KEY_AAD,
            },
        )
        .map_err(|_| "private goal key initialization failed")?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(KEY_MAGIC);
    bytes.extend_from_slice(&nonce);
    bytes.extend_from_slice(&encrypted);
    publish_immutable(dir, KEY_MARKER, &bytes)?;
    // The collision winner is authoritative, including its fsynced key marker.
    verify_key_durable(dir, key)
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::{random, sync};
    use cap_std::fs::Dir;

    #[test]
    fn private_goal_directory_sync_uses_held_directory_after_rename() {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-private-goal-sync-{}",
            hex::encode(random::<16>().unwrap())
        ));
        let moved = root.with_extension("moved");
        std::fs::create_dir(&root).unwrap();
        let directory = Dir::open_ambient_dir(&root, cap_std::ambient_authority()).unwrap();
        std::fs::rename(&root, &moved).unwrap();
        // Linux cap-std opened an O_PATH handle. The old into_std_file +
        // sync_all implementation fails EBADF here; capability-relative reopen
        // must sync the same directory even after its ambient path moved.
        sync(&directory).expect("fsync held Linux directory through a readable descriptor");
        std::fs::remove_dir(&moved).unwrap();
    }
}

#[cfg(test)]
mod name_tests {
    use super::validate_record_name;

    #[test]
    fn records_are_single_relative_names() {
        for name in ["", ".", "..", "../record", "record/child", "/record"] {
            assert!(validate_record_name(name).is_err(), "{name:?}");
        }
        assert!(validate_record_name(".key-verifier-v1").is_ok());
        assert!(validate_record_name("0123456789abcdef").is_ok());
    }
}

#[cfg(test)]
mod record_tests {
    use super::*;

    #[test]
    fn encrypted_records_preserve_namespace_key_and_authenticated_context() {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-sdk-private-{}",
            hex::encode(random::<16>().unwrap())
        ));
        let directory = open_root(&root).unwrap();
        let key = [7; 32];
        initialize_key(&directory, &key).unwrap();
        assert!(initialize_key(&directory, &[8; 32]).is_err());
        let magic = b"O7TEST01";
        assert!(
            publish_bounded_record(
                &directory, &key, "record", magic, b"context", b"secret", 128
            )
            .unwrap()
        );
        assert!(
            !publish_bounded_record(&directory, &key, "record", magic, b"context", b"other", 128)
                .unwrap()
        );
        let body = read_bounded_record(&directory, &key, "record", magic, b"context", 128)
            .unwrap()
            .unwrap();
        assert_eq!(&*body, b"secret");
        assert!(read_bounded_record(&directory, &key, "record", magic, b"foreign", 128).is_err());
        assert!(
            publish_bounded_record(
                &directory,
                &key,
                "../escape",
                magic,
                b"context",
                b"secret",
                128
            )
            .is_err()
        );
        drop(directory);
        std::fs::remove_dir_all(root).unwrap();
    }
}
