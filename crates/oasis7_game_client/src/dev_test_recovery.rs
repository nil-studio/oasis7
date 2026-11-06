//! Trusted operator backup/restore. This is a fail-closed workflow marker, not an
//! authority against a malicious operator. Native keys/world state are never restored.
use crate::dev_test_sessions::ServiceConfig;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        io::AsRawFd,
    },
    path::Path,
};
const STATE: &str = "recovery-state.json";
const MAX_FILE: u64 = 4 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    source_policy_digest: String,
    files: Vec<(String, String)>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreState {
    version: u32,
    snapshot_digest: String,
    restore_pending: bool,
    retired_grant_ids: BTreeSet<String>,
    retired_grant_refs: BTreeSet<String>,
    retired_credentials: BTreeSet<String>,
    history_files: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    current_identity_and_permissions_checked: bool,
    effects_and_pending_originals_checked: bool,
    missing_usage_is_unknown: bool,
}
fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
fn private_dir(path: &Path) -> Result<(), String> {
    crate::runtime_broker::trusted_ancestors(&path.join("check"))?;
    let m = std::fs::symlink_metadata(path).map_err(|_| "private_directory_unavailable")?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err("private_directory_required".into());
    }
    Ok(())
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    private_dir(path.parent().ok_or("private_parent_required")?)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "recovery_file_unavailable")?;
    let m = file.metadata().map_err(|_| "recovery_file_unavailable")?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
        || m.nlink() != 1
        || m.len() > MAX_FILE
    {
        return Err("invalid_recovery_file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "recovery_read_failed")?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("recovery_file_too_large".into());
    }
    Ok(bytes)
}
fn sync_dir(path: &Path) -> Result<(), String> {
    std::fs::File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| "recovery_sync_failed".into())
}
fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    private_dir(path.parent().ok_or("private_parent_required")?)?;
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| "recovery_entropy_unavailable")?;
    let temporary = path.with_extension(format!("recovery-pending-{}", hex::encode(nonce)));
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|_| "recovery_write_failed")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "recovery_write_failed")?;
    std::fs::rename(&temporary, path).map_err(|_| "recovery_write_failed")?;
    sync_dir(path.parent().unwrap())
}
fn json_write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    write(
        path,
        &serde_json::to_vec(value).map_err(|_| "recovery_encode_failed")?,
    )
}
fn lock(path: &Path) -> Result<std::fs::File, String> {
    private_dir(path.parent().ok_or("private_parent_required")?)?;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| "recovery_lock_failed")?;
    let metadata = f.metadata().map_err(|_| "recovery_lock_failed")?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
        || metadata.nlink() != 1
    {
        return Err("recovery_lock_untrusted".into());
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(f);
        }
        if std::io::Error::last_os_error().kind() != std::io::ErrorKind::WouldBlock
            || std::time::Instant::now() >= deadline
        {
            return Err("recovery_lock_unavailable".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
fn mkdir(path: &Path) -> Result<(), String> {
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .map_err(|_| "new_private_directory_required")?;
    sync_dir(path.parent().ok_or("private_parent_required")?)
}
fn state(config: &ServiceConfig) -> Result<Option<RestoreState>, String> {
    let path = config.service.usage_directory.join(STATE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("recovery_state_unknown".into()),
        Ok(_) => {
            let value: RestoreState =
                serde_json::from_slice(&read(&path)?).map_err(|_| "recovery_state_unknown")?;
            if value.version != 1 {
                return Err("recovery_state_unknown".into());
            }
            Ok(Some(value))
        }
    }
}
/// Hook for every load/save of a policy. Ordinary restarts without a restore marker
/// keep existing admission. Restored identities can never be enabled again.
pub fn validate_policy(config: &ServiceConfig) -> Result<(), String> {
    let Some(state) = state(config)? else {
        return Ok(());
    };
    if state.restore_pending {
        return Err("restore_pending_operator_reconciliation_required".into());
    }
    if config.service.credentials.iter().any(|c| {
        !c.revoked
            && state
                .retired_credentials
                .contains(&hash(c.token.as_bytes()))
    }) || config.grants.iter().any(|g| {
        g.enabled
            && (state.retired_grant_ids.contains(&g.grant_id)
                || state.retired_grant_refs.contains(&g.grant_ref))
    }) {
        return Err("restored_authorization_permanently_retired".into());
    }
    Ok(())
}
/// Copy only tool configuration/catalog/usage associations. Native credentials,
/// SDK caches and the world remain current external dependencies, never rolled back.
pub fn backup(policy: &Path, snapshot: &Path) -> Result<(), String> {
    let _control = lock(&policy.with_extension("control-lock"))?;
    let bytes = read(policy)?;
    let config: ServiceConfig =
        serde_json::from_slice(&bytes).map_err(|_| "invalid_session_configuration")?;
    config.service.validate()?;
    validate_policy(&config)?;
    private_dir(&config.service.usage_directory)?;
    private_dir(&config.catalog_directory)?;
    let _catalog = lock(&config.catalog_directory.join("catalog.lock"))?;
    let mut guards = Vec::new();
    let mut names = Vec::new();
    for credential in &config.service.credentials {
        let key =
            crate::dev_test_operations::usage_key(&config.service.grant_id, &credential.caller_id);
        guards.push(lock(
            &config.service.usage_directory.join(format!("{key}.lock")),
        )?);
        for ext in ["json", "initialized"] {
            let name = format!("{key}.{ext}");
            if std::fs::symlink_metadata(config.service.usage_directory.join(&name)).is_ok() {
                names.push(name)
            }
        }
    }
    mkdir(snapshot)?;
    mkdir(&snapshot.join("usage"))?;
    mkdir(&snapshot.join("history"))?;
    let mut files = Vec::new();
    let mut copy = |relative: &str, source: &Path| -> Result<(), String> {
        let body = read(source)?;
        write(&snapshot.join(relative), &body)?;
        files.push((relative.to_owned(), hash(&body)));
        Ok(())
    };
    copy("policy.json", policy)?;
    copy(
        "catalog.json",
        &config.catalog_directory.join("catalog.json"),
    )?;
    for name in names {
        copy(
            &format!("usage/{name}"),
            &config.service.usage_directory.join(name),
        )?;
    }
    // Preserve prior associations in subsequent snapshots; never promote used.
    if let Some(prior) = state(&config)? {
        let history = config
            .service
            .usage_directory
            .parent()
            .ok_or("history_unknown")?
            .join("history");
        for name in prior.history_files {
            if !permitted(&format!("history/{name}")) {
                return Err("history_unknown".into());
            }
            if history.join(&name).exists() {
                copy(&format!("history/{name}"), &history.join(name))?;
            }
        }
    }
    // Existing retirement state is also part of a subsequent snapshot.
    if config.service.usage_directory.join(STATE).exists() {
        copy(
            &format!("usage/{STATE}"),
            &config.service.usage_directory.join(STATE),
        )?;
    }
    json_write(
        &snapshot.join("manifest.json"),
        &Manifest {
            version: 1,
            source_policy_digest: hash(&bytes),
            files,
        },
    )?;
    drop(guards);
    Ok(())
}
fn permitted(name: &str) -> bool {
    name == "policy.json"
        || name == "catalog.json"
        || name == format!("usage/{STATE}")
        || name
            .strip_prefix("usage/")
            .or_else(|| name.strip_prefix("history/"))
            .is_some_and(|file| {
                let Some((stem, ext)) = file.rsplit_once('.') else {
                    return false;
                };
                stem.len() == 64
                    && stem.bytes().all(|b| b.is_ascii_hexdigit())
                    && ["json", "initialized"].contains(&ext)
            })
}
/// New target only. An interrupted restore leaves a durable pending marker; no
/// service may consume its policy until the operator repeats this exact snapshot.
pub fn restore(snapshot: &Path, target: &Path) -> Result<(), String> {
    private_dir(snapshot)?;
    let manifest_bytes = read(&snapshot.join("manifest.json"))?;
    let manifest: Manifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| "invalid_snapshot")?;
    if manifest.version != 1 || manifest.files.len() > 4096 {
        return Err("invalid_snapshot".into());
    }
    let mut seen = BTreeSet::new();
    let mut payloads = Vec::new();
    let mut total = 0;
    for (name, digest) in &manifest.files {
        if !permitted(name) || !seen.insert(name) {
            return Err("invalid_snapshot_path".into());
        }
        let bytes = read(&snapshot.join(name))?;
        total += bytes.len();
        if total > 64 * 1024 * 1024 || hash(&bytes) != *digest {
            return Err("snapshot_integrity_unknown".into());
        }
        payloads.push((name.clone(), bytes));
    }
    let policy_bytes = &payloads
        .iter()
        .find(|(name, _)| name == "policy.json")
        .ok_or("snapshot_policy_missing")?
        .1;
    if hash(policy_bytes) != manifest.source_policy_digest {
        return Err("snapshot_policy_unknown".into());
    }
    let mut config: ServiceConfig =
        serde_json::from_slice(policy_bytes).map_err(|_| "invalid_session_configuration")?;
    config.service.validate()?;
    let snapshot_digest = hash(&manifest_bytes);
    if target.exists() {
        private_dir(target)?;
        let current: RestoreState =
            serde_json::from_slice(&read(&target.join("usage").join(STATE))?)
                .map_err(|_| "restore_target_not_pending")?;
        if !current.restore_pending || current.snapshot_digest != snapshot_digest {
            return Err("new_restore_target_required".into());
        }
    } else {
        mkdir(target)?;
        mkdir(&target.join("usage"))?;
        mkdir(&target.join("history"))?;
        mkdir(&target.join("catalog"))?;
    }
    let mut restored = payloads
        .iter()
        .find(|(name, _)| name == &format!("usage/{STATE}"))
        .map(|(_, body)| serde_json::from_slice::<RestoreState>(body))
        .transpose()
        .map_err(|_| "recovery_state_unknown")?
        .unwrap_or(RestoreState {
            version: 1,
            snapshot_digest: snapshot_digest.clone(),
            restore_pending: true,
            retired_grant_ids: BTreeSet::new(),
            retired_grant_refs: BTreeSet::new(),
            retired_credentials: BTreeSet::new(),
            history_files: Vec::new(),
        });
    restored.snapshot_digest = snapshot_digest;
    restored.restore_pending = true;
    restored
        .retired_grant_ids
        .insert(config.service.grant_id.clone());
    for grant in &mut config.grants {
        restored.retired_grant_ids.insert(grant.grant_id.clone());
        restored.retired_grant_refs.insert(grant.grant_ref.clone());
        grant.enabled = false;
    }
    for credential in &mut config.service.credentials {
        restored
            .retired_credentials
            .insert(hash(credential.token.as_bytes()));
        credential.revoked = true;
    }
    let state_path = target.join("usage").join(STATE);
    json_write(&state_path, &restored)?;
    for (name, body) in &payloads {
        if name == "catalog.json" {
            write(&target.join("catalog/catalog.json"), body)?;
        } else if (name.starts_with("usage/") || name.starts_with("history/"))
            && name.ends_with(".json")
            && name != &format!("usage/{STATE}")
        {
            let file = name.split_once('/').unwrap().1;
            write(&target.join("history").join(file), body)?;
            if !restored.history_files.iter().any(|n| n == file) {
                restored.history_files.push(file.into());
            }
        }
    }
    config.service.socket = target.join("service.sock");
    config.service.usage_directory = target.join("usage");
    config.catalog_directory = target.join("catalog");
    json_write(&target.join("policy.json"), &config)?;
    restored.restore_pending = false;
    json_write(&state_path, &restored)?;
    Ok(())
}
/// A trusted operator supplies a new policy only after current native review.
/// Old usage remains unknown history; this is a distinct, explicitly budgeted grant.
pub fn reauthorize(policy: &Path, candidate: &Path, review: &Path) -> Result<(), String> {
    let _guard = lock(&policy.with_extension("control-lock"))?;
    let config: ServiceConfig =
        serde_json::from_slice(&read(policy)?).map_err(|_| "invalid_session_configuration")?;
    validate_policy(&config)?;
    let retired = state(&config)?.ok_or("restored_policy_required")?;
    let review: Review =
        serde_json::from_slice(&read(review)?).map_err(|_| "explicit_current_review_required")?;
    if !review.current_identity_and_permissions_checked
        || !review.effects_and_pending_originals_checked
        || !review.missing_usage_is_unknown
    {
        return Err("explicit_current_review_required".into());
    }
    let next: ServiceConfig =
        serde_json::from_slice(&read(candidate)?).map_err(|_| "invalid_session_configuration")?;
    next.service.validate()?;
    if next.service.socket != config.service.socket
        || next.service.usage_directory != config.service.usage_directory
        || next.catalog_directory != config.catalog_directory
        || retired.retired_grant_ids.contains(&next.service.grant_id)
        || next.grants.is_empty()
    {
        return Err("explicit_new_authorization_required".into());
    }
    validate_policy(&next)?;
    json_write(policy, &next)
}
/// Only lookup admission may call this. Current authorization/scope checks remain
/// mandatory in the common service; missing/unverifiable associations stay unknown.
pub fn historical_original(
    config: &ServiceConfig,
    scope: &crate::dev_test_operations::Scope,
    request_id: &str,
) -> Result<Option<crate::dev_test_operations::Original>, String> {
    validate_policy(config)?;
    let Some(state) = state(config)? else {
        return Ok(None);
    };
    let history = config
        .service
        .usage_directory
        .parent()
        .ok_or("history_unknown")?
        .join("history");
    let mut result = None;
    for name in state.history_files {
        if !permitted(&format!("usage/{name}")) {
            return Err("history_unknown".into());
        }
        let Ok(bytes) = read(&history.join(name)) else {
            continue;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        if value.get("version") != Some(&serde_json::json!(1)) {
            continue;
        }
        let Some(original) = value.get("operations").and_then(|ops| ops.get(request_id)) else {
            continue;
        };
        let Ok(original) =
            serde_json::from_value::<crate::dev_test_operations::Original>(original.clone())
        else {
            continue;
        };
        if original.scope != *scope
            || matches!(
                original.arguments,
                crate::dev_test_operations::Arguments::Lookup { .. }
            )
        {
            continue;
        }
        if let Some(prior) = &result {
            if serde_json::to_value(prior).ok() != serde_json::to_value(&original).ok() {
                return Err("historical_original_conflict_unknown".into());
            }
        }
        result = Some(original);
    }
    Ok(result)
}
pub fn run(args: Vec<String>) -> Result<(), String> {
    if args.len() == 3 && args[0] == "backup" {
        backup(Path::new(&args[1]), Path::new(&args[2]))?;
    } else if args.len() == 3 && args[0] == "restore" {
        restore(Path::new(&args[1]), Path::new(&args[2]))?;
    } else if args.len() == 4 && args[0] == "reauthorize" {
        reauthorize(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
        )?;
    } else {
        return Err("usage: operator backup POLICY SNAPSHOT | restore SNAPSHOT NEW_PRIVATE_TARGET | reauthorize POLICY CANDIDATE PRIVATE_REVIEW".into());
    }
    println!(
        "{{\"status\":\"operator_workflow_completed\",\"historical_usage\":\"unknown\",\"world_state\":\"unchanged_requires_current_observation\"}}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev_test_operations::{Arguments, Original, Scope};
    use serde_json::json;
    struct Fixture(PathBuf);
    use std::path::PathBuf;
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> (Fixture, PathBuf, ServiceConfig) {
        let mut entropy = [0; 16];
        getrandom::fill(&mut entropy).unwrap();
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("oasis7-recovery-{}", hex::encode(entropy)));
        mkdir(&root).unwrap();
        mkdir(&root.join("usage")).unwrap();
        mkdir(&root.join("catalog")).unwrap();
        let config:ServiceConfig=serde_json::from_value(json!({"service":{"profile":"dev-test-accounts-v1","socket":root.join("s"),"usage_directory":root.join("usage"),"grant_id":"old-grant","max_request_bytes":4096,"timeout_ms":1000,"max_calls":10,"credentials":[{"caller_id":"caller","token":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx","expires_at_unix":u64::MAX,"revoked":false}]},"catalog_directory":root.join("catalog"),"grants":[{"grant_ref":"old-ref","grant_id":"old-grant","caller_id":"caller","binding":{"account_ref":"a","environment":"e","world":"w","genesis_digest":"g","agent_id":"agent","resource":"r","executor_config_digest":"pin","authorization_version":1},"expires_at_unix":u64::MAX,"enabled":true,"methods":["lookup"],"move_targets":[]}],"resources":{}})).unwrap();
        let policy = root.join("policy.json");
        json_write(&policy, &config).unwrap();
        json_write(&root.join("catalog/catalog.json"), &json!([])).unwrap();
        (Fixture(root), policy, config)
    }
    fn scope() -> Scope {
        Scope {
            account_ref: "a".into(),
            environment: "e".into(),
            world: "w".into(),
            genesis_digest: "g".into(),
            agent: "agent".into(),
            resource: "r".into(),
            executor: "executor".into(),
            fence_digest: "fence".into(),
        }
    }
    #[test]
    fn restored_old_active_policy_is_retired_and_usage_is_only_history() {
        let (root, policy, source) = fixture();
        let key = crate::dev_test_operations::usage_key("old-grant", "caller");
        let original = Original {
            body_digest: hash(&serde_json::to_vec(&Arguments::Context).unwrap()),
            scope: scope(),
            arguments: Arguments::Context,
            native_context: "exact-old-context".into(),
            native_kind: "prefix".into(),
            native_action: None,
            native_context_identity: None,
            last_observation: None,
        };
        json_write(
            &source.service.usage_directory.join(format!("{key}.json")),
            &json!({"version":1,"used":1,"operations":{"context-A":original}}),
        )
        .unwrap();
        let snapshot = root.0.join("snapshot");
        backup(&policy, &snapshot).unwrap();
        json_write(
            &source.service.usage_directory.join(format!("{key}.json")),
            &json!({"version":1,"used":9,"operations":{}}),
        )
        .unwrap();
        let target = root.0.join("restored");
        restore(&snapshot, &target).unwrap();
        let restored: ServiceConfig =
            serde_json::from_slice(&read(&target.join("policy.json")).unwrap()).unwrap();
        assert!(restored.grants.iter().all(|g| !g.enabled));
        assert!(restored.service.credentials.iter().all(|c| c.revoked));
        validate_policy(&restored).unwrap();
        assert!(!target.join("usage").join(format!("{key}.json")).exists());
        let found = historical_original(&restored, &scope(), "context-A")
            .unwrap()
            .unwrap();
        assert_eq!(found.native_context, "exact-old-context");
        let mut wrong = scope();
        wrong.account_ref = "B".into();
        assert!(
            historical_original(&restored, &wrong, "context-A")
                .unwrap()
                .is_none()
        );
        assert!(
            historical_original(&restored, &scope(), "missing")
                .unwrap()
                .is_none()
        );
        let mut revived = restored;
        revived.grants[0].enabled = true;
        assert!(validate_policy(&revived).is_err());
        revived.grants[0].enabled = false;
        revived.service.credentials[0].revoked = false;
        assert!(validate_policy(&revived).is_err());
        assert!(restore(&snapshot, &target).is_err());
    }
    #[test]
    fn pending_restart_denies_and_explicit_new_grant_cannot_reuse_old_identity() {
        let (root, policy, _) = fixture();
        let snapshot = root.0.join("snapshot");
        backup(&policy, &snapshot).unwrap();
        let target = root.0.join("restored");
        restore(&snapshot, &target).unwrap();
        let restored_policy = target.join("policy.json");
        let mut next: ServiceConfig =
            serde_json::from_slice(&read(&restored_policy).unwrap()).unwrap();
        let path = next.service.usage_directory.join(STATE);
        let mut pending: RestoreState = serde_json::from_slice(&read(&path).unwrap()).unwrap();
        pending.restore_pending = true;
        json_write(&path, &pending).unwrap();
        assert!(validate_policy(&next).is_err());
        restore(&snapshot, &target).unwrap();
        validate_policy(&next).unwrap();
        let review = target.join("review.json");
        json_write(&review,&json!({"current_identity_and_permissions_checked":true,"effects_and_pending_originals_checked":true,"missing_usage_is_unknown":true})).unwrap();
        next.service.grant_id = "explicit-new-grant".into();
        next.service.max_calls = 5;
        next.grants[0].grant_id = "explicit-new-grant".into();
        next.grants[0].grant_ref = "explicit-new-ref".into();
        next.grants[0].enabled = true;
        next.service.credentials[0].revoked = false;
        let candidate = target.join("candidate.json");
        json_write(&candidate, &next).unwrap();
        assert!(reauthorize(&restored_policy, &candidate, &review).is_err());
        next.service.credentials[0].token = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".into();
        json_write(&candidate, &next).unwrap();
        reauthorize(&restored_policy, &candidate, &review).unwrap();
        let active: ServiceConfig =
            serde_json::from_slice(&read(&restored_policy).unwrap()).unwrap();
        validate_policy(&active).unwrap();
        assert_eq!(active.service.max_calls, 5);
        assert!(
            active
                .service
                .usage_directory
                .read_dir()
                .unwrap()
                .all(|entry| entry.unwrap().file_name() == STATE)
        );
    }
    #[test]
    fn altered_snapshot_is_rejected_before_target_policy_exists() {
        let (root, policy, _) = fixture();
        let snapshot = root.0.join("snapshot");
        backup(&policy, &snapshot).unwrap();
        json_write(&snapshot.join("catalog.json"), &json!(["altered"])).unwrap();
        let target = root.0.join("restore");
        assert!(restore(&snapshot, &target).is_err());
        assert!(!target.exists());
    }
}
