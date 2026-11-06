use super::*;

/// Trusted local daemon entry. Unlock material arrives only on private stdin.
pub fn run(args: Vec<String>) -> Result<(), String> {
    use std::{
        io::{Read, Write},
        os::unix::{
            fs::{MetadataExt, PermissionsExt},
            net::UnixListener,
        },
        time::{Duration, Instant},
    };
    if args.len() == 3 && args[0] == "--apply" {
        let target = std::path::Path::new(&args[1]);
        let _guard = control_lock(target)?;
        let old = load_session_config(target)?;
        let mut next = load_session_config(std::path::Path::new(&args[2]))?;
        if old.service.socket != next.service.socket
            || old.service.usage_directory != next.service.usage_directory
            || old.service.grant_id != next.service.grant_id
        {
            return Err("stable_service_binding_changed".into());
        }
        next.grant_generation_high_water = old.grant_generation_high_water.clone();
        // Changed policies invalidate snapshots; adding an unrelated grant does not.
        for grant in &mut next.grants {
            if let Some(prior) = old.grants.iter().find(|g| g.grant_ref == grant.grant_ref) {
                grant.binding.authorization_version = prior.binding.authorization_version;
                if grant != prior
                    || next.service.max_calls > old.service.max_calls
                    || next
                        .service
                        .credentials
                        .iter()
                        .find(|c| c.caller_id == grant.caller_id)
                        != old
                            .service
                            .credentials
                            .iter()
                            .find(|c| c.caller_id == grant.caller_id)
                {
                    grant.binding.authorization_version = prior
                        .binding
                        .authorization_version
                        .checked_add(1)
                        .ok_or("authorization_version_exhausted")?;
                }
            } else if let Some(high) = old.grant_generation_high_water.get(&grant.grant_ref) {
                grant.binding.authorization_version = (*high)
                    .checked_add(1)
                    .ok_or("authorization_version_exhausted")?;
            }
            next.grant_generation_high_water
                .entry(grant.grant_ref.clone())
                .and_modify(|high| *high = (*high).max(grant.binding.authorization_version))
                .or_insert(grant.binding.authorization_version);
        }
        save_policy(target, &next)?;
        println!("{{\"status\":\"policy_applied\"}}");
        return Ok(());
    }
    if args.len() == 3 && ["--disable-account", "--enable-account"].contains(&args[0].as_str()) {
        let target = std::path::Path::new(&args[1]);
        let _guard = control_lock(target)?;
        let mut config = load_session_config(target)?;
        crate::dev_test_catalog::set_disabled(
            &config.catalog_directory,
            &args[2],
            args[0] == "--disable-account",
        )?;
        for grant in config
            .grants
            .iter_mut()
            .filter(|g| g.binding.account_ref == args[2])
        {
            if args[0] == "--disable-account" {
                grant.enabled = false;
            }
            grant.binding.authorization_version = grant
                .binding
                .authorization_version
                .checked_add(1)
                .ok_or("authorization_version_exhausted")?;
        }
        save_policy(target, &config)?;
        println!("{{\"status\":\"account_state_updated_new_grant_required\"}}");
        return Ok(());
    }
    if args.len() == 2 && args[0] == "--call" {
        let mut body = Vec::new();
        std::io::stdin()
            .take(65537)
            .read_to_end(&mut body)
            .map_err(|_| "request_unavailable")?;
        let request: ToolRequest =
            serde_json::from_slice(&body).map_err(|_| "invalid_tool_request")?;
        drop(request);
        let result = crate::dev_test_service::call_serialized_until(
            std::path::Path::new(&args[1]),
            &body,
            Instant::now() + Duration::from_secs(30),
        )?;
        println!("{}", result);
        return Ok(());
    }
    if args.len() != 2 || args[0] != "--serve" {
        return Err("usage: oasis7_dev_test_sessions --serve PRIVATE_CONFIG".into());
    }
    let config_path = std::path::Path::new(&args[1]);
    let load = || load_session_config(config_path);
    let initial = load()?;
    crate::runtime_broker::trusted_ancestors(&initial.service.socket)?;
    let parent = initial
        .service
        .socket
        .parent()
        .ok_or("socket_parent_required")?;
    let metadata = std::fs::symlink_metadata(parent).map_err(|_| "socket_parent_unavailable")?;
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
        return Err("private_socket_directory_required".into());
    }
    let mut unlock_bytes = zeroize::Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(65537)
        .read_to_end(&mut unlock_bytes)
        .map_err(|_| "private_unlock_unavailable")?;
    if unlock_bytes.len() > 65536 {
        return Err("private_unlock_too_large".into());
    }
    use base64::Engine;
    let encoded: BTreeMap<String, String> =
        serde_json::from_slice(&unlock_bytes).map_err(|_| "private_unlock_invalid")?;
    let mut unlocks = PrivateUnlocks::new();
    for (account_ref, value) in encoded {
        let value = zeroize::Zeroizing::new(value);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(value.as_bytes())
            .map_err(|_| "private_unlock_encoding_invalid")?;
        unlocks.insert(account_ref, zeroize::Zeroizing::new(bytes));
    }
    drop(unlock_bytes);
    let listener = UnixListener::bind(&initial.service.socket).map_err(|_| "socket_bind_failed")?;
    std::fs::set_permissions(
        &initial.service.socket,
        std::fs::Permissions::from_mode(0o600),
    )
    .map_err(|_| "socket_permissions_failed")?;
    let sessions = std::sync::Arc::new(std::sync::Mutex::new(Sessions::new()?));
    let unlocks = std::sync::Arc::new(unlocks);
    let active = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    eprintln!("read_session_service_ready");
    for connection in listener.incoming() {
        let mut stream = connection.map_err(|_| "socket_accept_failed")?;
        let config = load()?;
        if config.service.socket != initial.service.socket
            || config.service.usage_directory != initial.service.usage_directory
            || config.service.grant_id != initial.service.grant_id
        {
            return Err("stable_service_binding_changed".into());
        }
        let started = Instant::now();
        let mut deadline = started + Duration::from_millis(config.service.timeout_ms);
        let mut body = Vec::new();
        if crate::dev_test_service::read_until(
            &mut stream,
            &mut body,
            config.service.max_request_bytes,
            deadline,
        )
        .is_err()
        {
            continue;
        }
        if active.swap(true, std::sync::atomic::Ordering::AcqRel) {
            let _ = stream.set_write_timeout(Some(Duration::from_millis(1)));
            let _ = stream.write_all(br#"{"status":"concurrency_limit_exhausted"}"#);
            continue;
        }
        let guard = match control_lock_until(config_path, deadline) {
            Ok(guard) => guard,
            Err(_) => {
                active.store(false, std::sync::atomic::Ordering::Release);
                continue;
            }
        };
        let current = load()?;
        deadline = deadline.min(started + Duration::from_millis(current.service.timeout_ms));
        let sessions = sessions.clone();
        let unlocks = unlocks.clone();
        let active = active.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            struct Active(std::sync::Arc<std::sync::atomic::AtomicBool>);
            impl Drop for Active {
                fn drop(&mut self) {
                    self.0.store(false, std::sync::atomic::Ordering::Release);
                }
            }
            let _active = Active(active);
            let _guard = guard;
            let result = (|| {
                if body.len() > current.service.max_request_bytes {
                    return Err("request_too_large".into());
                }
                let request: ToolRequest =
                    serde_json::from_slice(&body).map_err(|_| "invalid_tool_request")?;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| "clock_unavailable")?
                    .as_secs();
                handle(
                    &current,
                    &mut *sessions.lock().map_err(|_| "session_state_unavailable")?,
                    request,
                    &unlocks,
                    now,
                    deadline,
                )
            })();
            let _ = sender.send(result);
        });
        let result = deadline
            .checked_duration_since(Instant::now())
            .and_then(|remaining| {
                receiver
                    .recv_timeout(remaining.saturating_sub(Duration::from_millis(2)))
                    .ok()
            })
            .unwrap_or_else(|| Err("downstream_deadline_unknown".into()));
        let value = match result {
            Ok(value) => serde_json::json!({"status":"ok","result":value}),
            Err(error) => serde_json::json!({"status":error}),
        };
        if let Some(remaining) = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
        {
            let _ = stream.set_write_timeout(Some(remaining));
            let _ = stream.write_all(value.to_string().as_bytes());
        }
    }
    Err("session_listener_closed".into())
}

fn load_session_config(path: &std::path::Path) -> Result<ServiceConfig, String> {
    let mut config: ServiceConfig =
        serde_json::from_slice(&crate::dev_test_private_config::read_private_config(path)?)
            .map_err(|_| "invalid_session_configuration")?;
    config.service.validate()?;
    for grant in &config.grants {
        config
            .grant_generation_high_water
            .entry(grant.grant_ref.clone())
            .and_modify(|high| *high = (*high).max(grant.binding.authorization_version))
            .or_insert(grant.binding.authorization_version);
    }
    let mut refs = std::collections::BTreeSet::new();
    if config.grants.iter().any(|g| {
        g.grant_ref.is_empty()
            || !refs.insert(&g.grant_ref)
            || g.grant_id != config.service.grant_id
    }) {
        return Err("ambiguous_grant_configuration".into());
    }
    Ok(config)
}
fn control_lock(path: &std::path::Path) -> Result<std::fs::File, String> {
    control_lock_until(
        path,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
}
fn control_lock_until(
    path: &std::path::Path,
    deadline: std::time::Instant,
) -> Result<std::fs::File, String> {
    use std::os::unix::{fs::OpenOptionsExt, io::AsRawFd};
    crate::dev_test_private_config::read_private_config(path)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path.with_extension("control-lock"))
        .map_err(|_| "control_lock_unavailable")?;
    loop {
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(lock);
        }
        if std::time::Instant::now() >= deadline {
            return Err("control_deadline_exceeded".into());
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::WouldBlock {
            return Err("control_lock_unavailable".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
fn save_policy(path: &std::path::Path, config: &ServiceConfig) -> Result<(), String> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let temporary = path.with_extension("policy-pending");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|_| "policy_write_failed")?;
    file.write_all(&serde_json::to_vec(config).map_err(|_| "policy_encode_failed")?)
        .and_then(|_| file.sync_all())
        .map_err(|_| "policy_write_failed")?;
    std::fs::rename(temporary, path).map_err(|_| "policy_write_failed")?;
    std::fs::File::open(path.parent().ok_or("policy_parent_required")?)
        .and_then(|f| f.sync_all())
        .map_err(|_| "policy_write_failed")?;
    Ok(())
}

#[cfg(test)]
mod control_tests {
    use super::*;
    use std::{
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        time::{Duration, Instant},
    };
    fn fixture() -> (PathBuf, ServiceConfig) {
        let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "session-control-{}",
            Sessions::new().unwrap().epoch
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let binding = Binding {
            account_ref: "a".into(),
            environment: "e".into(),
            world: "w".into(),
            genesis_digest: "g".into(),
            agent_id: "agent".into(),
            resource: "r".into(),
            executor_config_digest: blake3::hash(b"fixture").to_hex().to_string(),
            authorization_version: 1,
        };
        let config:ServiceConfig=serde_json::from_value(serde_json::json!({"service":{"profile":"dev-test-accounts-v1","socket":root.join("s"),"usage_directory":root,"grant_id":"stable","max_request_bytes":4096,"timeout_ms":100,"max_calls":10,"credentials":[{"caller_id":"caller","token":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx","expires_at_unix":100,"revoked":false}]},"catalog_directory":root,"grants":[{"grant_ref":"a-read","grant_id":"stable","caller_id":"caller","binding":binding,"expires_at_unix":99,"enabled":true,"methods":["task"]}],"resources":{}})).unwrap();
        let catalog = root.join("catalog.json");
        std::fs::write(&catalog,serde_json::to_vec(&serde_json::json!([{"account_ref":"a","account_id":"fixture","environment":"e","public_key":"fixture","alias":"a","credential_ref":"/unused","login_operation":"unused"}])).unwrap()).unwrap();
        std::fs::set_permissions(&catalog, std::fs::Permissions::from_mode(0o600)).unwrap();
        let path = root.join("policy");
        save_policy(&path, &config).unwrap();
        (path, config)
    }
    #[test]
    fn managed_controls_invalidate_only_affected_sessions() {
        let (path, mut config) = fixture();
        let mut sessions = Sessions::new().unwrap();
        let reference = sessions.open("caller", &config.grants[0], 90, 1).unwrap();
        let candidate = path.with_extension("candidate");
        let mut c = config.grants[0].clone();
        c.grant_ref = "c-read".into();
        c.binding.account_ref = "c".into();
        config.grants.push(c);
        save_policy(&candidate, &config).unwrap();
        run(vec![
            "--apply".into(),
            path.to_string_lossy().into(),
            candidate.to_string_lossy().into(),
        ])
        .unwrap();
        let current = load_session_config(&path).unwrap();
        assert!(sessions
            .authorize(&reference, "caller", &current.grants[0], "task", 2)
            .is_ok());
        run(vec![
            "--disable-account".into(),
            path.to_string_lossy().into(),
            "a".into(),
        ])
        .unwrap();
        let disabled = load_session_config(&path).unwrap();
        assert!(!disabled.grants[0].enabled);
        assert!(sessions
            .authorize(&reference, "caller", &disabled.grants[0], "task", 2)
            .is_err());
        run(vec![
            "--apply".into(),
            path.to_string_lossy().into(),
            candidate.to_string_lossy().into(),
        ])
        .unwrap();
        let restored = load_session_config(&path).unwrap();
        assert!(restored.grants[0].enabled);
        assert!(sessions
            .authorize(&reference, "caller", &restored.grants[0], "task", 2)
            .is_err());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn removed_grant_cannot_revive_old_reference() {
        let (path, mut config) = fixture();
        let original = config.grants[0].clone();
        let mut sessions = Sessions::new().unwrap();
        let reference = sessions.open("caller", &original, 90, 1).unwrap();
        let candidate = path.with_extension("candidate");
        config.grants.clear();
        save_policy(&candidate, &config).unwrap();
        run(vec![
            "--apply".into(),
            path.to_string_lossy().into(),
            candidate.to_string_lossy().into(),
        ])
        .unwrap();
        config.grants.push(original);
        save_policy(&candidate, &config).unwrap();
        run(vec![
            "--apply".into(),
            path.to_string_lossy().into(),
            candidate.to_string_lossy().into(),
        ])
        .unwrap();
        let current = load_session_config(&path).unwrap();
        assert!(sessions
            .authorize(&reference, "caller", &current.grants[0], "task", 2)
            .is_err());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn control_and_admission_share_finite_lock() {
        let (path, _) = fixture();
        let guard = control_lock(&path).unwrap();
        let start = Instant::now();
        assert!(control_lock_until(&path, start + Duration::from_millis(20)).is_err());
        assert!(start.elapsed() < Duration::from_millis(150));
        drop(guard);
        assert!(control_lock_until(&path, Instant::now() + Duration::from_millis(20)).is_ok());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
