//! Fixed write tools and one durable transaction for usage and original-operation association.
use crate::dev_test_service::Config;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Arguments {
    Context,
    Move {
        context_request_id: String,
        to: [i64; 3],
    },
    Lookup {
        original_request_id: String,
    },
}
impl Arguments {
    pub fn method(&self) -> &'static str {
        match self {
            Self::Context => "context",
            Self::Move { .. } => "move",
            Self::Lookup { .. } => "lookup",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub request_id: String,
    pub arguments: Arguments,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub account_ref: String,
    pub environment: String,
    pub world: String,
    pub genesis_digest: String,
    pub agent: String,
    pub resource: String,
    pub executor: String,
    pub fence_digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Original {
    pub body_digest: String,
    pub scope: Scope,
    pub arguments: Arguments,
    pub native_context: String,
    pub native_kind: String,
    pub last_observation: Option<serde_json::Value>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ledger {
    version: u32,
    used: u64,
    operations: BTreeMap<String, Original>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Stored {
    Legacy(u64),
    Current(Ledger),
}
pub(crate) fn usage_key(grant: &str, caller: &str) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(&(grant.len() as u64).to_be_bytes());
    hash.update(grant.as_bytes());
    hash.update(&(caller.len() as u64).to_be_bytes());
    hash.update(caller.as_bytes());
    hash.finalize().to_hex().to_string()
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
fn transaction<T>(
    config: &Config,
    caller: &str,
    update: impl FnOnce(&mut Ledger) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    use std::{
        io::{Read, Write},
        os::unix::{
            fs::{MetadataExt, OpenOptionsExt},
            io::AsRawFd,
        },
    };
    let directory = &config.usage_directory;
    let metadata =
        std::fs::symlink_metadata(directory).map_err(|_| "usage_directory_unavailable")?;
    if !metadata.is_dir()
        || metadata.mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err("private_usage_directory_required");
    }
    crate::runtime_broker::trusted_ancestors(&directory.join("ledger"))
        .map_err(|_| "usage_directory_untrusted")?;
    let key = usage_key(&config.grant_id, caller);
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join(format!("{key}.lock")))
        .map_err(|_| "usage_lock_failed")?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err("usage_lock_failed");
    }
    let path = directory.join(format!("{key}.json"));
    let marker = directory.join(format!("{key}.initialized"));
    let mut ledger = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
    {
        Ok(file) => {
            let metadata = file.metadata().map_err(|_| "usage_unavailable")?;
            if !metadata.is_file()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o077 != 0
                || metadata.len() > 4194304
            {
                return Err("invalid_usage_record");
            }
            let mut bytes = Vec::new();
            file.take(4194305)
                .read_to_end(&mut bytes)
                .map_err(|_| "usage_unavailable")?;
            if bytes.len() > 4194304 {
                return Err("invalid_usage_record");
            }
            match serde_json::from_slice::<Stored>(&bytes).map_err(|_| "invalid_usage_record")? {
                Stored::Legacy(used) => Ledger {
                    version: 1,
                    used,
                    operations: BTreeMap::new(),
                },
                Stored::Current(record) if record.version == 1 => record,
                _ => return Err("unsupported_usage_record"),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if std::fs::symlink_metadata(&marker).is_ok() {
                return Err("usage_record_missing_unknown");
            }
            Ledger {
                version: 1,
                ..Ledger::default()
            }
        }
        Err(_) => return Err("usage_unavailable"),
    };
    let result = update(&mut ledger)?;
    let bytes = serde_json::to_vec(&ledger).map_err(|_| "usage_encoding_failed")?;
    if bytes.len() > 4194304 {
        return Err("operation_capacity_exhausted");
    }
    // A durable initialization marker precedes the first ledger commit. A crash
    // in that window fails closed; a missing initialized ledger never means zero.
    if std::fs::symlink_metadata(&marker).is_err() {
        let marker_file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&marker)
            .map_err(|_| "usage_marker_failed")?;
        marker_file.sync_all().map_err(|_| "usage_marker_failed")?;
        std::fs::File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|_| "usage_marker_failed")?;
    }
    let temp = directory.join(format!("{key}.pending"));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temp)
        .map_err(|_| "usage_write_failed")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "usage_write_failed")?;
    std::fs::rename(temp, path).map_err(|_| "usage_write_failed")?;
    std::fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| "usage_write_failed")?;
    Ok(result)
}
pub(crate) fn admit_read(config: &Config, caller: &str) -> Result<(), &'static str> {
    transaction(config, caller, |ledger| {
        if ledger.used >= config.max_calls {
            return Err("call_limit_exhausted");
        }
        ledger.used += 1;
        Ok(())
    })
}
/// Replay never allocates or sends: it returns the same recorded native identity.
pub(crate) fn admit(
    config: &Config,
    caller: &str,
    scope: &Scope,
    call: &Call,
) -> Result<(Original, bool), &'static str> {
    if !valid_id(&call.request_id) {
        return Err("invalid_request_id");
    }
    let digest =
        blake3::hash(&serde_json::to_vec(&call.arguments).map_err(|_| "invalid_operation_body")?)
            .to_hex()
            .to_string();
    transaction(config, caller, |ledger| {
        if let Some(original) = ledger.operations.get(&call.request_id) {
            if original.body_digest != digest || original.scope != *scope {
                return Err("request_body_conflict");
            }
            if ledger.used >= config.max_calls {
                return Err("call_limit_exhausted");
            }
            let original = original.clone();
            ledger.used += 1;
            return Ok((original, false));
        }
        if ledger.used >= config.max_calls {
            return Err("call_limit_exhausted");
        }
        if ledger.operations.len() >= 4096 {
            return Err("operation_capacity_exhausted");
        }
        let (native_context, native_kind) = match &call.arguments {
            Arguments::Context => {
                let domain =
                    serde_json::to_vec(&(&config.grant_id, caller, scope, &call.request_id))
                        .map_err(|_| "invalid_operation_body")?;
                (
                    format!("dev-test-{}", blake3::hash(&domain).to_hex()),
                    "prefix".into(),
                )
            }
            Arguments::Move {
                context_request_id, ..
            } => {
                let context = ledger
                    .operations
                    .get(context_request_id)
                    .ok_or("original_context_unknown")?;
                if context.scope != *scope || !matches!(context.arguments, Arguments::Context) {
                    return Err("original_scope_denied");
                }
                if ledger.operations.values().any(|op| {
                    op.native_context == context.native_context
                        && matches!(op.arguments, Arguments::Move { .. })
                }) {
                    return Err("context_action_already_reserved");
                }
                (context.native_context.clone(), "action".into())
            }
            Arguments::Lookup {
                original_request_id,
            } => {
                let original = ledger
                    .operations
                    .get(original_request_id)
                    .ok_or("original_operation_unknown")?;
                if original.scope != *scope
                    || matches!(original.arguments, Arguments::Lookup { .. })
                {
                    return Err("original_scope_denied");
                }
                (
                    original.native_context.clone(),
                    original.native_kind.clone(),
                )
            }
        };
        let original = Original {
            body_digest: digest,
            scope: scope.clone(),
            arguments: call.arguments.clone(),
            native_context,
            native_kind,
            last_observation: None,
        };
        ledger.used += 1;
        ledger
            .operations
            .insert(call.request_id.clone(), original.clone());
        Ok((original, true))
    })
}
pub(crate) fn observe(
    config: &Config,
    caller: &str,
    call: &Call,
    observation: serde_json::Value,
) -> Result<(), &'static str> {
    transaction(config, caller, |ledger| {
        let original = ledger
            .operations
            .get_mut(&call.request_id)
            .ok_or("original_operation_unknown")?;
        original.last_observation = Some(observation);
        Ok(())
    })
}

pub(crate) fn execute(
    client: &crate::device_client::Client,
    original: &Original,
    fresh: bool,
    deadline: std::time::Instant,
) -> Result<serde_json::Value, String> {
    use crate::IntentOutcome;
    use crate::operation_store::ClientStateKind;
    let operation = &original.native_context;
    if fresh {
        match &original.arguments {
            Arguments::Context => {
                let prefix = client.prefix_until(operation, deadline)?;
                let result = client.submit_and_poll_until(&prefix, deadline)?;
                if matches!(result.outcome, IntentOutcome::Committed { .. }) {
                    let captured = client.captured_until(operation, deadline)?;
                    return Ok(
                        serde_json::json!({"native_context":operation,"operation":result,"context":captured.prepared.identity,"actor_observation":captured.prepared.snapshot.actor_observation,"native_authority":captured.current_public_context.native_authority}),
                    );
                }
                return serde_json::to_value(result).map_err(|_| "response_unavailable".into());
            }
            Arguments::Move { to, .. } => {
                let action = oasis7_agent_api::world_action::Action::MoveAgent {
                    agent_id: client.agent.clone(),
                    to: oasis7_agent_api::world_action::GeoPos::new(to[0], to[1], to[2]),
                };
                return serde_json::to_value(
                    client.submit_action_until(operation, action, deadline)?,
                )
                .map_err(|_| "response_unavailable".into());
            }
            Arguments::Lookup { .. } => {}
        }
    }
    let kind = if original.native_kind == "prefix" {
        ClientStateKind::Prefix
    } else {
        ClientStateKind::Action
    };
    let Some(request) = client.cached_intent(kind, operation)? else {
        return Ok(
            serde_json::json!({"status":"unknown","native_context":operation,"next":"retain_original_operation"}),
        );
    };
    crate::device_client::Client::check_tool_deadline(Some(deadline))?;
    let response = client.lookup(&request)?;
    if original.native_kind == "prefix"
        && matches!(response.outcome, IntentOutcome::Committed { .. })
        && matches!(original.arguments, Arguments::Context)
    {
        let captured = client.captured_until(operation, deadline)?;
        return Ok(
            serde_json::json!({"native_context":operation,"operation":response,"context":captured.prepared.identity,"actor_observation":captured.prepared.snapshot.actor_observation,"native_authority":captured.current_public_context.native_authority}),
        );
    }
    serde_json::to_value(response).map_err(|_| "response_unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    fn fixture(limit: u64) -> (Config, Scope) {
        let mut entropy = [0; 16];
        getrandom::fill(&mut entropy).unwrap();
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("dev-test-op-{}", hex::encode(entropy)));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = Config {
            profile: crate::dev_test_service::PROFILE.into(),
            socket: root.join("socket"),
            usage_directory: root,
            grant_id: "stable".into(),
            max_request_bytes: 4096,
            timeout_ms: 100,
            max_calls: limit,
            credentials: vec![],
        };
        let scope = Scope {
            account_ref: "a".into(),
            environment: "e".into(),
            world: "w".into(),
            genesis_digest: "g".into(),
            agent: "agent".into(),
            resource: "r".into(),
            executor: "executor-key".into(),
            fence_digest: "fence".into(),
        };
        (config, scope)
    }
    fn context() -> Call {
        Call {
            request_id: "context-1".into(),
            arguments: Arguments::Context,
        }
    }
    fn ledger(config: &Config) -> std::path::PathBuf {
        config
            .usage_directory
            .join(format!("{}.json", usage_key(&config.grant_id, "caller")))
    }
    #[test]
    fn migration_replay_conflict_and_lookup_share_exact_persistent_budget() {
        let (config, scope) = fixture(6);
        std::fs::write(ledger(&config), b"2").unwrap();
        std::fs::set_permissions(ledger(&config), std::fs::Permissions::from_mode(0o600)).unwrap();
        let (first, fresh) = admit(&config, "caller", &scope, &context()).unwrap();
        assert!(fresh);
        let mut restarted = config.clone();
        restarted.socket = config.usage_directory.join("new-socket");
        let (again, fresh) = admit(&restarted, "caller", &scope, &context()).unwrap();
        assert!(!fresh);
        assert_eq!(first.native_context, again.native_context);
        let changed = Call {
            request_id: "context-1".into(),
            arguments: Arguments::Move {
                context_request_id: "context-1".into(),
                to: [1, 0, 0],
            },
        };
        assert_eq!(
            admit(&config, "caller", &scope, &changed).unwrap_err(),
            "request_body_conflict"
        );
        let lookup = Call {
            request_id: "lookup-1".into(),
            arguments: Arguments::Lookup {
                original_request_id: "context-1".into(),
            },
        };
        assert_eq!(
            admit(&config, "caller", &scope, &lookup)
                .unwrap()
                .0
                .native_context,
            first.native_context
        );
        admit_read(&config, "caller").unwrap();
        assert_eq!(
            admit(&config, "caller", &scope, &context()).unwrap_err(),
            "call_limit_exhausted"
        );
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(ledger(&config)).unwrap()).unwrap();
        assert_eq!(stored["used"], 6);
        assert_eq!(stored["operations"].as_object().unwrap().len(), 2);
        std::fs::remove_dir_all(config.usage_directory).unwrap();
    }
    #[test]
    fn admission_is_durable_before_send_and_failure_cannot_publish_association() {
        let (config, scope) = fixture(8);
        admit_read(&config, "caller").unwrap();
        let before = std::fs::read(ledger(&config)).unwrap();
        let temp = config
            .usage_directory
            .join(format!("{}.pending", usage_key(&config.grant_id, "caller")));
        symlink(ledger(&config), &temp).unwrap();
        assert_eq!(
            admit(&config, "caller", &scope, &context()).unwrap_err(),
            "usage_write_failed"
        );
        assert_eq!(std::fs::read(ledger(&config)).unwrap(), before);
        std::fs::remove_file(temp).unwrap();
        let (original, fresh) = admit(&config, "caller", &scope, &context()).unwrap();
        assert!(fresh);
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(ledger(&config)).unwrap()).unwrap();
        assert_eq!(stored["used"], 2);
        assert_eq!(
            stored["operations"]["context-1"]["native_context"],
            original.native_context
        );
        std::fs::remove_file(ledger(&config)).unwrap();
        assert_eq!(
            admit(&config, "caller", &scope, &context()).unwrap_err(),
            "usage_record_missing_unknown"
        );
        std::fs::remove_dir_all(config.usage_directory).unwrap();
    }
    #[test]
    fn duplicate_concurrency_and_foreign_context_never_allocate_a_second_action() {
        let (config, scope) = fixture(16);
        let a = config.clone();
        let b = scope.clone();
        let worker = std::thread::spawn(move || admit(&a, "caller", &b, &context()).unwrap().1);
        let fresh = admit(&config, "caller", &scope, &context()).unwrap().1;
        assert_ne!(fresh, worker.join().unwrap());
        let mut other = scope.clone();
        other.account_ref = "b".into();
        let lookup = Call {
            request_id: "bad-read".into(),
            arguments: Arguments::Lookup {
                original_request_id: "context-1".into(),
            },
        };
        assert_eq!(
            admit(&config, "caller", &other, &lookup).unwrap_err(),
            "original_scope_denied"
        );
        let movement = Call {
            request_id: "move-1".into(),
            arguments: Arguments::Move {
                context_request_id: "context-1".into(),
                to: [1, 0, 0],
            },
        };
        let (action, _) = admit(&config, "caller", &scope, &movement).unwrap();
        assert_eq!(
            action.native_context,
            admit(&config, "caller", &scope, &context())
                .unwrap()
                .0
                .native_context
        );
        let competing = Call {
            request_id: "move-2".into(),
            arguments: movement.arguments,
        };
        assert_eq!(
            admit(&config, "caller", &scope, &competing).unwrap_err(),
            "context_action_already_reserved"
        );
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(ledger(&config)).unwrap()).unwrap();
        assert_eq!(stored["used"], 4);
        std::fs::remove_dir_all(config.usage_directory).unwrap();
    }
}
