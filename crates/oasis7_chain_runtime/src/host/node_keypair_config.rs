use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;

#[path = "node_keypair_config/transaction.rs"]
mod transaction;

#[path = "node_keypair_config/runtime_lease.rs"]
mod runtime_lease;
pub(crate) use runtime_lease::{
    RuntimeIdentityBindingV1, RuntimeIdentityLease, acquire_runtime_identity_lease,
};

pub(super) const NODE_TABLE_KEY: &str = "node";
pub(super) const NODE_PRIVATE_KEY_FIELD: &str = "private_key";
pub(super) const NODE_PUBLIC_KEY_FIELD: &str = "public_key";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NodeKeypairConfig {
    pub private_key_hex: String,
    pub public_key_hex: String,
}

pub(super) const PROVISIONED_NODE_KEYPAIR_FILE: &str = "node-keypair.toml";

/// Read a legacy arbitrary TOML path without provisioning or repairing it.
pub(crate) fn read_existing_legacy_keypair(path: &Path) -> Result<NodeKeypairConfig, String> {
    if !path.is_absolute() {
        return Err("legacy key path must be absolute".into());
    }
    reject_symlink_path_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("legacy key config must be a regular file".into());
    }
    require_exact_mode(path, 0o600, "legacy key config")?;
    validate_complete_keypair_config(path)?;
    let table = load_config_table(path)?;
    let node = table
        .get(NODE_TABLE_KEY)
        .and_then(toml::Value::as_table)
        .ok_or("legacy [node] missing")?;
    let private_key_hex = node
        .get(NODE_PRIVATE_KEY_FIELD)
        .and_then(toml::Value::as_str)
        .ok_or("legacy private key missing")?
        .trim()
        .to_owned();
    let public_key_hex = node
        .get(NODE_PUBLIC_KEY_FIELD)
        .and_then(toml::Value::as_str)
        .ok_or("legacy public key missing")?
        .trim()
        .to_owned();
    transaction::verify_published(path, &public_key_hex)?;
    Ok(NodeKeypairConfig {
        private_key_hex,
        public_key_hex,
    })
}

/// Read an already provisioned identity without creating, repairing, or
/// rewriting any file. Missing or weakly protected identities fail closed.
pub(super) fn read_existing_node_keypair_in_secure_config_dir(
    config_dir: &Path,
) -> Result<(NodeKeypairConfig, PathBuf), String> {
    if !config_dir.is_absolute() {
        return Err("--config-dir must be an absolute path".to_string());
    }
    reject_symlink_path_components(config_dir)?;
    let metadata = fs::metadata(config_dir).map_err(|err| {
        format!(
            "read config directory {} failed: {err}",
            config_dir.display()
        )
    })?;
    if !metadata.is_dir() {
        return Err(format!(
            "config directory {} is not a directory",
            config_dir.display()
        ));
    }
    require_exact_mode(config_dir, 0o700, "config directory")?;
    let key_path = config_dir.join(PROVISIONED_NODE_KEYPAIR_FILE);
    let metadata = fs::symlink_metadata(&key_path).map_err(|err| {
        format!(
            "read existing key config {} failed: {err}",
            key_path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "key config path {} must be a regular file",
            key_path.display()
        ));
    }
    require_exact_mode(&key_path, 0o600, "key config")?;
    validate_complete_keypair_config(&key_path)?;
    let table = load_config_table(&key_path)?;
    let node = table
        .get(NODE_TABLE_KEY)
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("key config {} missing [node] table", key_path.display()))?;
    let private_key_hex = node
        .get(NODE_PRIVATE_KEY_FIELD)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("key config {} missing private key", key_path.display()))?
        .to_string();
    let public_key_hex = node
        .get(NODE_PUBLIC_KEY_FIELD)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("key config {} missing public key", key_path.display()))?
        .to_string();
    transaction::verify_published(&key_path, &public_key_hex)?;
    Ok((
        NodeKeypairConfig {
            private_key_hex,
            public_key_hex,
        },
        key_path,
    ))
}

/// Ensures the dedicated bootstrap key file exists without accepting a partial,
/// redirected, or weakly protected pre-existing identity.
pub(super) fn ensure_node_keypair_in_secure_config_dir(
    config_dir: &Path,
) -> Result<(NodeKeypairConfig, PathBuf), String> {
    if !config_dir.is_absolute() {
        return Err("--config-dir must be an absolute path".to_string());
    }
    reject_symlink_path_components(config_dir)?;
    let metadata = fs::metadata(config_dir).map_err(|err| {
        format!(
            "read config directory {} failed: {err}",
            config_dir.display()
        )
    })?;
    if !metadata.is_dir() {
        return Err(format!(
            "config directory {} is not a directory",
            config_dir.display()
        ));
    }
    set_exact_mode(config_dir, 0o700, "config directory")?;

    let key_path = config_dir.join(PROVISIONED_NODE_KEYPAIR_FILE);
    match fs::symlink_metadata(&key_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "key config path {} must not be a symlink",
                    key_path.display()
                ));
            }
            if !metadata.is_file() {
                return Err(format!(
                    "key config path {} is not a regular file",
                    key_path.display()
                ));
            }
            require_exact_mode(&key_path, 0o600, "key config")?;
            validate_complete_keypair_config(&key_path)?;
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => {
            return Err(format!(
                "inspect key config {} failed: {err}",
                key_path.display()
            ));
        }
    }

    let keypair = transaction::ensure(&key_path, true)?;
    set_exact_mode(&key_path, 0o600, "key config")?;
    Ok((keypair, key_path))
}

fn validate_complete_keypair_config(path: &Path) -> Result<(), String> {
    let table = load_config_table(path)?;
    let node = table
        .get(NODE_TABLE_KEY)
        .and_then(toml::Value::as_table)
        .ok_or_else(|| format!("key config {} must contain a [node] table", path.display()))?;
    let private_key = node
        .get(NODE_PRIVATE_KEY_FIELD)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("key config {} missing node.private_key", path.display()))?;
    let public_key = node
        .get(NODE_PUBLIC_KEY_FIELD)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("key config {} missing node.public_key", path.display()))?;
    validate_node_keypair_hex(private_key, public_key)
}

fn reject_symlink_path_components(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "config directory path contains symlink: {}",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => break,
            Err(err) => {
                return Err(format!(
                    "inspect config directory component {} failed: {err}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn require_exact_mode(path: &Path, expected: u32, label: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let actual = fs::metadata(path)
        .map_err(|err| format!("read {label} {} failed: {err}", path.display()))?
        .permissions()
        .mode()
        & 0o777;
    if actual != expected {
        return Err(format!(
            "{label} {} must have mode {:o}, found {:o}",
            path.display(),
            expected,
            actual
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn set_exact_mode(path: &Path, expected: u32, label: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(expected)).map_err(|err| {
        format!(
            "set {label} {} mode {:o} failed: {err}",
            path.display(),
            expected
        )
    })
}

#[cfg(not(unix))]
fn require_exact_mode(_path: &Path, _expected: u32, _label: &str) -> Result<(), String> {
    Err("identity provisioning requires Unix filesystem permissions".to_string())
}

#[cfg(not(unix))]
fn set_exact_mode(_path: &Path, _expected: u32, _label: &str) -> Result<(), String> {
    Err("identity provisioning requires Unix filesystem permissions".to_string())
}

pub(super) fn ensure_node_keypair_in_config(path: &Path) -> Result<NodeKeypairConfig, String> {
    transaction::ensure(path, false)
}

/// Only the stopped-node migration transaction may relocate completed metadata.
/// The key bytes are copied unchanged; pending identity transactions cannot move.
pub(crate) fn relocated_publication_metadata(
    source: &Path,
    destination: &Path,
) -> Result<Option<(PathBuf, String)>, String> {
    transaction::relocated_metadata(source, destination)
}

fn prepare_config(
    mut table: toml::map::Map<String, toml::Value>,
) -> Result<(NodeKeypairConfig, toml::map::Map<String, toml::Value>), String> {
    let node_table = table
        .entry(NODE_TABLE_KEY.to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let node_table = node_table
        .as_table_mut()
        .ok_or_else(|| "config field 'node' must be a table".to_string())?;

    let existing_private = node_table
        .get(NODE_PRIVATE_KEY_FIELD)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let existing_public = node_table
        .get(NODE_PUBLIC_KEY_FIELD)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let keypair = match (existing_private, existing_public) {
        (Some(private_hex), Some(public_hex)) => {
            validate_node_keypair_hex(private_hex.as_str(), public_hex.as_str())?;
            NodeKeypairConfig {
                private_key_hex: private_hex,
                public_key_hex: public_hex,
            }
        }
        (Some(private_hex), None) => {
            let signing_key = signing_key_from_hex(private_hex.as_str())?;
            let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
            node_table.insert(
                NODE_PUBLIC_KEY_FIELD.to_string(),
                toml::Value::String(public_key_hex.clone()),
            );
            NodeKeypairConfig {
                private_key_hex: private_hex,
                public_key_hex,
            }
        }
        (None, Some(_)) => {
            return Err("node.public_key without private key; recovery required".into());
        }
        (None, None) => {
            if node_table.contains_key(NODE_PRIVATE_KEY_FIELD)
                || node_table.contains_key(NODE_PUBLIC_KEY_FIELD)
            {
                return Err("invalid existing node key fields; recovery required".into());
            }
            let mut private_key_bytes = [0_u8; 32];
            getrandom::fill(&mut private_key_bytes)
                .map_err(|err| format!("generate node signing key failed: {err}"))?;
            let signing_key = SigningKey::from_bytes(&private_key_bytes);
            let private_key_hex = hex::encode(signing_key.to_bytes());
            let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
            node_table.insert(
                NODE_PRIVATE_KEY_FIELD.to_string(),
                toml::Value::String(private_key_hex.clone()),
            );
            node_table.insert(
                NODE_PUBLIC_KEY_FIELD.to_string(),
                toml::Value::String(public_key_hex.clone()),
            );
            NodeKeypairConfig {
                private_key_hex,
                public_key_hex,
            }
        }
    };

    Ok((keypair, table))
}

fn load_config_table(path: &Path) -> Result<toml::map::Map<String, toml::Value>, String> {
    if !path.exists() {
        return Ok(toml::map::Map::new());
    }

    let content = fs::read_to_string(path)
        .map_err(|err| format!("read {} failed: {}", path.display(), err))?;
    if content.trim().is_empty() {
        return Ok(toml::map::Map::new());
    }

    let value: toml::Value = toml::from_str(content.as_str()).map_err(|_| {
        format!(
            "parse {} failed: invalid TOML (content redacted)",
            path.display()
        )
    })?;
    value
        .as_table()
        .cloned()
        .ok_or_else(|| format!("{} root must be a table", path.display()))
}

fn validate_node_keypair_hex(private_key_hex: &str, public_key_hex: &str) -> Result<(), String> {
    let signing_key = signing_key_from_hex(private_key_hex)?;
    let expected_public_hex = hex::encode(signing_key.verifying_key().to_bytes());
    if expected_public_hex != public_key_hex {
        return Err("node.public_key does not match node.private_key".to_string());
    }
    Ok(())
}

fn signing_key_from_hex(private_key_hex: &str) -> Result<SigningKey, String> {
    let private_bytes = hex::decode(private_key_hex)
        .map_err(|_| "node.private_key must be valid hex".to_string())?;
    let private_array: [u8; 32] = private_bytes
        .try_into()
        .map_err(|_| "node.private_key must be 32-byte hex".to_string())?;
    Ok(SigningKey::from_bytes(&private_array))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        fs::canonicalize(std::env::temp_dir())
            .expect("temp directory")
            .join(format!("oasis7-node-keypair-config-{label}-{unique}"))
            .join("config.toml")
    }

    #[cfg(unix)]
    fn secure_config_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::current_dir()
            .expect("current worktree")
            .join(".tmp")
            .join(format!("oasis7-node-keypair-config-{label}-{unique}"))
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_first_writers_share_one_generated_keypair() {
        let config_path = temp_config_path("concurrent");
        let lock_path = transaction::sidecar(&config_path, ".identity-v2.lock");
        let barrier = Arc::new(Barrier::new(8));
        let handles = (0..8)
            .map(|_| {
                let config_path = config_path.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    ensure_node_keypair_in_config(&config_path).expect("ensure keypair")
                })
            })
            .collect::<Vec<_>>();

        let keypairs = handles
            .into_iter()
            .map(|handle| handle.join().expect("join keypair worker"))
            .collect::<Vec<_>>();
        let first = keypairs.first().expect("first keypair");
        assert!(keypairs.iter().all(|keypair| keypair == first));
        assert!(lock_path.exists(), "OS lock carrier must remain stable");

        let persisted =
            ensure_node_keypair_in_config(&config_path).expect("read persisted keypair");
        assert_eq!(persisted, *first);
        let _ = fs::remove_dir_all(config_path.parent().expect("config parent"));
    }

    #[test]
    fn public_only_identity_is_not_silently_replaced() {
        let path = temp_config_path("public-only");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "[node]\npublic_key = 'unknown'\n").unwrap();
        let original = fs::read(&path).unwrap();
        assert!(ensure_node_keypair_in_config(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn secure_config_dir_creates_a_mode_restricted_complete_keypair() {
        use std::os::unix::fs::PermissionsExt;

        let config_dir = secure_config_dir("secure");
        fs::create_dir_all(&config_dir).expect("create config dir");
        let (keypair, key_path) =
            ensure_node_keypair_in_secure_config_dir(&config_dir).expect("provision keypair");

        assert_eq!(key_path, config_dir.join(PROVISIONED_NODE_KEYPAIR_FILE));
        assert_eq!(
            fs::metadata(&config_dir)
                .expect("config dir")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&key_path)
                .expect("key file")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        validate_node_keypair_hex(
            keypair.private_key_hex.as_str(),
            keypair.public_key_hex.as_str(),
        )
        .expect("generated keypair is complete");
        let _ = fs::remove_dir_all(config_dir);
    }

    #[cfg(unix)]
    #[test]
    fn secure_config_dir_rejects_a_symlinked_key_file() {
        use std::os::unix::fs::symlink;

        let config_dir = secure_config_dir("symlink");
        fs::create_dir_all(&config_dir).expect("create config dir");
        let target = config_dir.join("target.toml");
        fs::write(&target, "").expect("write target");
        symlink(&target, config_dir.join(PROVISIONED_NODE_KEYPAIR_FILE)).expect("symlink key");

        let err =
            ensure_node_keypair_in_secure_config_dir(&config_dir).expect_err("must reject symlink");
        assert!(
            err.contains("must not be a symlink"),
            "unexpected error: {err}"
        );
        let _ = fs::remove_dir_all(config_dir);
    }
}
