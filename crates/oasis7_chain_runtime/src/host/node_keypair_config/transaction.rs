//! Local identity publication. The persistent intent contains no private key.
//! The lock inode is never removed; an OS lock owns the critical section.
use super::*;
use crate::node_lifecycle::local_io as io;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    version: u32,
    target: PathBuf,
    original_sha256: Option<String>,
    candidate_id: String,
    prepared_sha256: Option<String>,
    complete: bool,
    public_key: Option<String>,
}

pub(super) fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn source(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err("identity path must be a regular file; recovery required".into());
            }
            if metadata.len() > 4 * 1024 * 1024 {
                return Err("identity configuration exceeds size limit".into());
            }
            fs::read(path).map(Some).map_err(|e| e.to_string())
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

fn source_digest(path: &Path) -> Result<Option<String>, String> {
    Ok(source(path)?.as_deref().map(digest))
}

fn validate_input(table: &toml::map::Map<String, toml::Value>) -> Result<bool, String> {
    let Some(node) = table.get(NODE_TABLE_KEY) else {
        return Ok(false);
    };
    let node = node
        .as_table()
        .ok_or("config field 'node' must be a table")?;
    for key in [NODE_PRIVATE_KEY_FIELD, NODE_PUBLIC_KEY_FIELD] {
        if let Some(value) = node.get(key)
            && value.as_str().is_none_or(|v| v.trim().is_empty())
        {
            return Err("invalid existing node key fields; recovery required".into());
        }
    }
    match (
        node.get(NODE_PRIVATE_KEY_FIELD),
        node.get(NODE_PUBLIC_KEY_FIELD),
    ) {
        (Some(private), Some(public)) => {
            validate_node_keypair_hex(
                private.as_str().ok_or("invalid private key")?.trim(),
                public.as_str().ok_or("invalid public key")?.trim(),
            )?;
            Ok(true)
        }
        (Some(private), None) => {
            signing_key_from_hex(private.as_str().ok_or("invalid private key")?.trim())?;
            Ok(false)
        }
        (None, Some(_)) => Err("node.public_key without private key; recovery required".into()),
        (None, None) => Ok(false),
    }
}

pub(super) fn ensure(path: &Path, dedicated: bool) -> Result<NodeKeypairConfig, String> {
    #[cfg(not(unix))]
    {
        let _ = dedicated;
        // Existing complete identities remain readable. Creation and repair
        // require the durable directory/publication backend implemented here.
        let table = load_config_table(path)?;
        if validate_input(&table)? {
            return prepare_config(table).map(|(key, _)| key);
        }
        return Err("crash-safe identity publication requires Unix filesystem semantics".into());
    }
    #[cfg(unix)]
    {
        io::reject_symlinks(path)?;
        let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
        let parent = absolute.parent().ok_or("identity parent missing")?;
        // The local/dev helper historically accepts a missing parent. Create
        // new directories privately; never relax permissions on existing ones.
        create_parent(parent)?;
        trusted_parent(parent)?;
        let path = fs::canonicalize(parent)
            .map_err(|e| e.to_string())?
            .join(absolute.file_name().ok_or("identity filename missing")?);
        let _lock = acquire(&path)?;
        checkpoint("locked")?;
        let intent_path = sidecar(&path, ".identity-v2.json");
        let mut intent: Intent = if source(&intent_path)?.is_some() {
            private_owned(&intent_path)?;
            io::read(&intent_path)?
        } else {
            let table = load_config_table(&path)?;
            if validate_input(&table)? {
                let (key, _) = prepare_config(table)?;
                File::open(&path)
                    .and_then(|f| f.sync_all())
                    .map_err(|e| e.to_string())?;
                let digest = source_digest(&path)?;
                let adopted = Intent {
                    version: 2,
                    target: path.clone(),
                    original_sha256: digest.clone(),
                    candidate_id: io::random_id()?,
                    prepared_sha256: digest,
                    complete: true,
                    public_key: Some(key.public_key_hex.clone()),
                };
                io::write(&intent_path, &adopted)?;
                return Ok(key);
            }
            if dedicated && source(&path)?.is_some() {
                return Err("dedicated identity is incomplete; restore original key".into());
            }
            let intent = Intent {
                version: 2,
                target: path.clone(),
                original_sha256: source_digest(&path)?,
                candidate_id: io::random_id()?,
                prepared_sha256: None,
                complete: false,
                public_key: None,
            };
            io::write(&intent_path, &intent)?;
            checkpoint("intent")?;
            intent
        };
        if intent.version != 2
            || intent.target != path
            || intent.candidate_id.len() != 32
            || !intent.candidate_id.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("identity initialization intent conflict; recovery required".into());
        }
        let candidate = sidecar(&path, &format!(".identity-{}.toml", intent.candidate_id));
        let current = source_digest(&path)?;
        if intent.complete {
            if current.is_none() {
                return Err("completed identity missing; restore original key".into());
            }
            validate_complete_keypair_config(&path)?;
            let (key, _) = prepare_config(load_config_table(&path)?)?;
            if intent.public_key.as_deref() != Some(key.public_key_hex.as_str()) {
                return Err("completed identity changed; restore original key".into());
            }
            File::open(&path)
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
            File::open(&intent_path)
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
            if source(&candidate)?.is_some() {
                private_owned(&candidate)?;
                if source_digest(&candidate)? != intent.prepared_sha256 {
                    return Err("conflicting completed candidate; recovery required".into());
                }
                fs::remove_file(&candidate).map_err(|e| e.to_string())?;
            }
            io::sync_dir(parent)?;
            return Ok(key);
        }
        if current != intent.original_sha256 && current != intent.prepared_sha256 {
            return Err("identity target changed during initialization; recovery required".into());
        }
        if source(&candidate)?.is_some_and(|bytes| bytes.is_empty())
            && intent.prepared_sha256.is_none()
            && current == intent.original_sha256
        {
            private_owned(&candidate)?;
            let quarantine = sidecar(&candidate, ".incomplete");
            if let Some(bytes) = source(&quarantine)? {
                private_owned(&quarantine)?;
                if !bytes.is_empty() {
                    return Err("conflicting quarantine material; recovery required".into());
                }
            } else {
                fs::hard_link(&candidate, &quarantine).map_err(|e| e.to_string())?;
            }
            io::sync_dir(parent)?;
            checkpoint("quarantined")?;
            fs::remove_file(&candidate).map_err(|e| e.to_string())?;
            io::sync_dir(parent)?;
        }
        if source(&candidate)?.is_none() {
            if intent.prepared_sha256.is_some() || current != intent.original_sha256 {
                return Err("prepared identity missing; restore original material".into());
            }
            let table = load_config_table(&path)?;
            validate_input(&table)?;
            let (_, prepared) = prepare_config(table)?;
            let bytes = toml::to_string_pretty(&prepared).map_err(|e| e.to_string())?;
            let mut options = OpenOptions::new();
            use std::os::unix::fs::OpenOptionsExt;
            options.write(true).create_new(true).mode(0o600);
            let mut file = options.open(&candidate).map_err(|e| e.to_string())?;
            checkpoint("candidate-created")?;
            file.write_all(bytes.as_bytes())
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            io::sync_dir(parent)?;
            checkpoint("candidate-synced")?;
        }
        private_owned(&candidate)?;
        validate_complete_keypair_config(&candidate).map_err(|_| {
            "prepared identity incomplete or invalid; recovery required".to_string()
        })?;
        File::open(&candidate)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        io::sync_dir(parent)?;
        let prepared_digest = source_digest(&candidate)?.ok_or("prepared identity missing")?;
        if let Some(expected) = &intent.prepared_sha256 {
            if expected != &prepared_digest {
                return Err("prepared identity changed; recovery required".into());
            }
        } else {
            // Crash before recording the prepared digest: prove the candidate
            // preserves the original config and any pre-existing private key.
            verify_candidate(&path, &candidate)?;
            intent.public_key = Some(
                prepare_config(load_config_table(&candidate)?)?
                    .0
                    .public_key_hex,
            );
            intent.prepared_sha256 = Some(prepared_digest);
            io::write(&intent_path, &intent)?;
        }
        checkpoint("prepared")?;
        File::open(&candidate)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        if current != intent.prepared_sha256 {
            if source_digest(&path)? != intent.original_sha256 {
                return Err("identity target changed before publication".into());
            }
            if intent.original_sha256.is_none() {
                // Atomic no-clobber publication; the recoverable candidate
                // remains in place until the complete record is durable.
                fs::hard_link(&candidate, &path).map_err(|e| e.to_string())?;
            } else {
                let publish = sidecar(&path, ".identity-v2.publish");
                if source(&publish)?.is_some() {
                    private_owned(&publish)?;
                    if source_digest(&publish)? != intent.prepared_sha256 {
                        return Err("conflicting publication material; recovery required".into());
                    }
                } else {
                    fs::hard_link(&candidate, &publish).map_err(|e| e.to_string())?;
                }
                fs::rename(&publish, &path).map_err(|e| e.to_string())?;
            }
        }
        checkpoint("published")?;
        io::sync_dir(parent)?;
        intent.complete = true;
        io::write(&intent_path, &intent)?;
        checkpoint("complete")?;
        fs::remove_file(&candidate).map_err(|e| e.to_string())?;
        io::sync_dir(parent)?;
        prepare_config(load_config_table(&path)?).map(|(key, _)| key)
    }
}

fn verify_candidate(path: &Path, candidate: &Path) -> Result<(), String> {
    let original = load_config_table(path)?;
    let prepared = load_config_table(candidate)?;
    // Re-deriving a public key is deterministic for private-only configs.
    if original
        .get(NODE_TABLE_KEY)
        .and_then(toml::Value::as_table)
        .is_some_and(|node| node.contains_key(NODE_PRIVATE_KEY_FIELD))
    {
        if prepare_config(original)?.1 != prepared {
            return Err("prepared identity differs from original config".into());
        }
    } else {
        let mut stripped = prepared;
        let node = stripped
            .get_mut(NODE_TABLE_KEY)
            .and_then(toml::Value::as_table_mut)
            .ok_or("prepared node table missing")?;
        node.remove(NODE_PRIVATE_KEY_FIELD);
        node.remove(NODE_PUBLIC_KEY_FIELD);
        if !original.contains_key(NODE_TABLE_KEY) && node.is_empty() {
            stripped.remove(NODE_TABLE_KEY);
        }
        if original != stripped {
            return Err("prepared identity changes original config fields".into());
        }
    }
    Ok(())
}

fn private_owned(path: &Path) -> Result<(), String> {
    crate::controlled_live_config::require_private_owned(path, false)
}

fn acquire(path: &Path) -> Result<File, String> {
    // v1 writers only understand the old create-new marker. Its presence is
    // ambiguous: require an offline migration instead of stealing that lock.
    if fs::symlink_metadata(sidecar(path, ".lock")).is_ok() {
        return Err(
            "legacy identity lock present; stop old writers and verify offline migration".into(),
        );
    }
    let lock_path = sidecar(path, ".identity-v2.lock");
    let file = io::private_file(&lock_path)?;
    file.sync_all().map_err(|e| e.to_string())?;
    io::sync_dir(path.parent().ok_or("identity parent missing")?)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(format!("identity OS lock busy or unavailable: {e}")),
        }
    }
}

#[cfg(not(test))]
fn checkpoint(_stage: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
fn checkpoint(stage: &str) -> Result<(), String> {
    if std::env::var("OASIS7_IDENTITY_TEST_FAIL_AT").as_deref() == Ok(stage) {
        return Err(format!("injected identity failure at {stage}"));
    }
    if std::env::var("OASIS7_IDENTITY_TEST_STOP_AT").as_deref() == Ok(stage) {
        let path = std::env::var_os("OASIS7_IDENTITY_TEST_PATH").ok_or("test path missing")?;
        fs::write(sidecar(Path::new(&path), ".test-ready"), stage).map_err(|e| e.to_string())?;
        loop {
            thread::park();
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "transaction_tests.rs"]
mod tests;

#[cfg(unix)]
fn create_parent(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_err() {
        create_parent(path.parent().ok_or("identity parent missing")?)?;
        io::private_dir(path)?;
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn trusted_parent(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: geteuid only reads the identity of the current process.
    let uid = unsafe { libc::geteuid() };
    for (index, ancestor) in path.ancestors().enumerate() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|e| e.to_string())?;
        if !metadata.is_dir()
            || (metadata.uid() != uid && metadata.uid() != 0)
            || (index == 0 && (metadata.uid() != uid || metadata.mode() & 0o022 != 0))
            || (metadata.mode() & 0o022 != 0 && metadata.mode() & 0o1000 == 0)
        {
            return Err(
                "identity directory and ancestors must prevent untrusted path replacement".into(),
            );
        }
    }
    Ok(())
}

/// Read-only callers must not export an identity from an unfinished v2
/// publication. Pre-v2 complete keys remain valid migration inputs.
pub(super) fn verify_published(path: &Path, public_key: &str) -> Result<(), String> {
    let intent_path = sidecar(path, ".identity-v2.json");
    if source(&intent_path)?.is_none() {
        return Ok(());
    }
    private_owned(&intent_path)?;
    let intent: Intent = io::read(&intent_path)?;
    let target = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if intent.version != 2
        || !intent.complete
        || intent.target != target
        || intent.public_key.as_deref() != Some(public_key)
    {
        return Err(
            "identity publication incomplete or conflicting; resume original initialization".into(),
        );
    }
    Ok(())
}

pub(super) fn relocated_metadata(
    source_path: &Path,
    destination: &Path,
) -> Result<Option<(PathBuf, String)>, String> {
    let intent_path = sidecar(source_path, ".identity-v2.json");
    if source(&intent_path)?.is_none() {
        return Ok(None);
    }
    if !destination.is_absolute() {
        return Err("identity relocation requires an absolute destination".into());
    }
    io::reject_symlinks(destination)?;
    let key = read_existing_legacy_keypair(source_path)?;
    private_owned(&intent_path)?;
    let mut intent: Intent = io::read(&intent_path)?;
    verify_published(source_path, &key.public_key_hex)?;
    let candidate = sidecar(
        source_path,
        &format!(".identity-{}.toml", intent.candidate_id),
    );
    if source(&candidate)?.is_some() {
        return Err(
            "identity publication cleanup pending; resume original initialization before migration"
                .into(),
        );
    }
    // A complete record has no live candidate. Its original target/digest and
    // candidate ID remain in the migration source/receipt for audit. Carry the
    // same public key to the explicitly mapped destination, never generate one.
    intent.target = destination.to_path_buf();
    let bytes = serde_json::to_string_pretty(&intent).map_err(|e| e.to_string())?;
    Ok(Some((intent_path, bytes)))
}
