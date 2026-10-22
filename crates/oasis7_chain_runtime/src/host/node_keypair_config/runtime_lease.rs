//! Runtime identity ownership is an OS lease, not a PID or an authority grant.
use super::*;
#[cfg(unix)]
use crate::node_lifecycle::local_io as io;
use serde::{Deserialize, Serialize};
use std::fs::File;

const KEY_LOCK: &str = ".runtime-identity-v1.lock";
const KEY_BINDING: &str = ".runtime-identity-v1.json";
const DIRECTORY_LOCK: &str = ".node-runtime-identity-v1.lock";
const DIRECTORY_BINDING: &str = ".node-runtime-identity-v1.json";
const MAX_BINDING_BYTES: u64 = 16 * 1024;

/// Immutable nonsecret identity/domain binding. A role is not identity or authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuntimeIdentityBindingV1 {
    pub version: u32,
    pub key_path: PathBuf,
    pub data_directory: PathBuf,
    pub node_id: String,
    pub root_public_key: String,
    pub peer_id: String,
    pub world_id: String,
    pub chain_id: String,
    pub genesis_digest: String,
    pub profile: String,
}

/// The root identity is bound to its domain, independently of which stopped
/// writer directory a qualified restore or signer handoff will next open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeIdentityDomainV1 {
    version: u32,
    key_path: PathBuf,
    node_id: String,
    root_public_key: String,
    peer_id: String,
    world_id: String,
    chain_id: String,
    genesis_digest: String,
    profile: String,
}

impl From<&RuntimeIdentityBindingV1> for RuntimeIdentityDomainV1 {
    fn from(binding: &RuntimeIdentityBindingV1) -> Self {
        Self {
            version: binding.version,
            key_path: binding.key_path.clone(),
            node_id: binding.node_id.clone(),
            root_public_key: binding.root_public_key.clone(),
            peer_id: binding.peer_id.clone(),
            world_id: binding.world_id.clone(),
            chain_id: binding.chain_id.clone(),
            genesis_digest: binding.genesis_digest.clone(),
            profile: binding.profile.clone(),
        }
    }
}

/// Retain this value (or an Arc) in the real worker until it has fully stopped.
/// File ownership releases locks on exit, including SIGKILL; lock inodes remain.
#[derive(Debug)]
pub(crate) struct RuntimeIdentityLease {
    _key_lock: File,
    _directory_lock: File,
}

pub(crate) fn acquire_runtime_identity_lease(
    key_path: &Path,
    binding: &RuntimeIdentityBindingV1,
) -> Result<RuntimeIdentityLease, String> {
    #[cfg(not(unix))]
    {
        let _ = (key_path, binding);
        Err("runtime identity lease requires Unix ownership and durability semantics".into())
    }
    #[cfg(unix)]
    {
        validate_paths(key_path, binding)?;
        match fs::symlink_metadata(transaction::sidecar(key_path, ".lock")) {
            Ok(_) => {
                return Err(
                    "legacy identity lock present; stop old writers and verify offline migration"
                        .into(),
                );
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        let key_lock = lock(&transaction::sidecar(key_path, KEY_LOCK))?;
        let directory_lock = lock(&binding.data_directory.join(DIRECTORY_LOCK))?;
        // Read under both lifetime locks. Never provision, repair, or regenerate here.
        if fs::symlink_metadata(key_path)
            .map_err(|error| error.to_string())?
            .len()
            > 4 * 1024 * 1024
        {
            return Err("runtime identity key configuration exceeds size limit".into());
        }
        let key = read_existing_legacy_keypair(key_path).map_err(|error| {
            if error.starts_with("parse ") {
                "runtime existing key contains invalid TOML (content redacted); restore original identity".into()
            } else {
                error
            }
        })?;
        if key.public_key_hex != binding.root_public_key {
            return Err(
                "runtime identity root public key mismatch; restore original identity".into(),
            );
        }
        let derived =
            super::super::derive_node_libp2p_identity_keypair_config(&binding.node_id, &key)?;
        let peer = oasis7_node::derive_libp2p_identity_keypair(&derived.private_key_hex)
            .map_err(|error| format!("runtime peer derivation failed: {error:?}"))?
            .public()
            .to_peer_id()
            .to_string();
        if peer != binding.peer_id {
            return Err("runtime peer identity mismatch; restore original binding".into());
        }
        let key_record = transaction::sidecar(key_path, KEY_BINDING);
        let directory_record = binding.data_directory.join(DIRECTORY_BINDING);
        // Check BOTH before writing either; conflicts never alter original records.
        let domain = RuntimeIdentityDomainV1::from(binding);
        let key_missing = inspect(&key_record, &domain)?;
        let directory_missing = inspect(&directory_record, binding)?;
        if key_missing {
            publish(&key_record, &domain)?;
        }
        if directory_missing {
            publish(&directory_record, binding)?;
        }
        Ok(RuntimeIdentityLease {
            _key_lock: key_lock,
            _directory_lock: directory_lock,
        })
    }
}

#[cfg(unix)]
fn validate_paths(key_path: &Path, binding: &RuntimeIdentityBindingV1) -> Result<(), String> {
    if binding.version != 1 || binding.profile != "controlled-v2" {
        return Err("unsupported runtime identity binding version or profile".into());
    }
    for value in [
        &binding.node_id,
        &binding.world_id,
        &binding.chain_id,
        &binding.peer_id,
    ] {
        if value.is_empty()
            || value.len() > 256
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err("runtime identity binding identifier is invalid".into());
        }
    }
    for value in [&binding.genesis_digest, &binding.root_public_key] {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("runtime identity binding digest/key must be lowercase hex".into());
        }
    }
    for path in [
        key_path,
        binding.key_path.as_path(),
        binding.data_directory.as_path(),
    ] {
        if !path.is_absolute() {
            return Err("runtime identity paths must be absolute".into());
        }
        io::reject_symlinks(path)?;
        if fs::canonicalize(path).map_err(|error| error.to_string())? != path {
            return Err("runtime identity paths must be canonical".into());
        }
    }
    if key_path != binding.key_path {
        return Err("runtime key path binding mismatch".into());
    }
    let parent = key_path.parent().ok_or("runtime key parent missing")?;
    transaction::trusted_parent(parent)?;
    transaction::trusted_parent(&binding.data_directory)?;
    crate::controlled_live_config::require_private_owned(&binding.data_directory, true)?;
    Ok(())
}

#[cfg(unix)]
fn lock(path: &Path) -> Result<File, String> {
    let file = io::private_file(path)?;
    file.try_lock()
        .map_err(|_| "runtime identity already in use or OS lease unavailable".to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    io::sync_dir(path.parent().ok_or("runtime lock parent missing")?)?;
    Ok(file)
}

#[cfg(unix)]
fn inspect<T: serde::de::DeserializeOwned + PartialEq>(
    path: &Path,
    binding: &T,
) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.to_string()),
        Ok(metadata) => {
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() > MAX_BINDING_BYTES
            {
                return Err(
                    "invalid runtime identity binding file; restore original binding".into(),
                );
            }
        }
    }
    crate::controlled_live_config::require_private_owned(path, false)?;
    let stored: T = io::read(path)
        .map_err(|_| "corrupt runtime identity binding; restore original binding".to_string())?;
    if &stored != binding {
        return Err("runtime identity binding conflict; restore original identity/domain or migrate offline".into());
    }
    // A previous publisher may have linked this exact record then failed its
    // directory sync. Retry explicitly completes durability before returning.
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| error.to_string())?;
    io::sync_dir(path.parent().ok_or("runtime binding parent missing")?)?;
    Ok(false)
}

#[cfg(unix)]
fn publish<T: Serialize>(path: &Path, binding: &T) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let parent = path.parent().ok_or("runtime binding parent missing")?;
    let temporary = parent.join(format!(".runtime-binding-{}", io::random_id()?));
    let bytes = serde_json::to_vec(binding).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BINDING_BYTES {
        return Err("runtime identity binding exceeds size limit".into());
    }
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        // No clobber even if a noncooperating writer created a record under our lock.
        fs::hard_link(&temporary, path).map_err(|error| error.to_string())?;
        #[cfg(test)]
        FAIL_BEFORE_BINDING_DIRECTORY_SYNC.with(|fail| {
            if fail.replace(false) {
                Err("injected binding directory sync failure")
            } else {
                Ok(())
            }
        })?;
        io::sync_dir(parent)
    })();
    let cleanup = fs::remove_file(&temporary).map_err(|error| error.to_string());
    result?;
    cleanup?;
    io::sync_dir(parent)
}

#[cfg(all(test, unix))]
#[path = "runtime_lease_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
std::thread_local! {
    static FAIL_BEFORE_BINDING_DIRECTORY_SYNC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
