//! Immutable read-only sessions; references never authenticate a caller.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub account_ref: String,
    pub environment: String,
    pub world: String,
    pub genesis_digest: String,
    pub agent_id: String,
    pub resource: String,
    pub authorization_version: u64,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub grant_ref: String,
    pub grant_id: String,
    pub caller_id: String,
    pub binding: Binding,
    pub expires_at_unix: u64,
    pub enabled: bool,
    pub methods: Vec<String>,
}
struct Session {
    grant_ref: String,
    grant_id: String,
    caller_id: String,
    binding: Binding,
    expires: u64,
    methods: Vec<String>,
}
pub struct Sessions {
    epoch: String,
    next: u64,
    sessions: BTreeMap<String, Session>,
}
impl Sessions {
    pub fn new() -> Result<Self, String> {
        let mut entropy = [0; 32];
        getrandom::fill(&mut entropy).map_err(|_| "session_entropy_unavailable")?;
        Ok(Self {
            epoch: hex::encode(entropy),
            next: 0,
            sessions: BTreeMap::new(),
        })
    }
    pub fn open(
        &mut self,
        caller: &str,
        grant: &Grant,
        expires: u64,
        now: u64,
    ) -> Result<String, String> {
        if caller != grant.caller_id
            || !grant.enabled
            || expires <= now
            || expires > grant.expires_at_unix
            || grant.methods.is_empty()
            || grant
                .methods
                .iter()
                .any(|m| !["describe", "task", "capabilities"].contains(&m.as_str()))
        {
            return Err("session_scope_denied".into());
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or("session_capacity_exhausted")?;
        let reference = format!("{}-{}", self.epoch, self.next);
        self.sessions.insert(
            reference.clone(),
            Session {
                grant_ref: grant.grant_ref.clone(),
                grant_id: grant.grant_id.clone(),
                caller_id: caller.into(),
                binding: grant.binding.clone(),
                expires,
                methods: grant.methods.clone(),
            },
        );
        Ok(reference)
    }
    pub fn authorize(
        &self,
        reference: &str,
        caller: &str,
        current: &Grant,
        method: &str,
        now: u64,
    ) -> Result<&Binding, String> {
        let session = self.sessions.get(reference).ok_or("session_unavailable")?;
        if caller != session.caller_id
            || caller != current.caller_id
            || session.grant_ref != current.grant_ref
            || session.grant_id != current.grant_id
            || session.binding != current.binding
            || !current.enabled
            || now >= session.expires
            || now >= current.expires_at_unix
            || !session.methods.iter().any(|m| m == method)
            || !current.methods.iter().any(|m| m == method)
        {
            return Err("session_scope_denied".into());
        }
        Ok(&session.binding)
    }
    pub fn close(&mut self, reference: &str, caller: &str) -> Result<(), String> {
        if self
            .sessions
            .get(reference)
            .is_none_or(|s| s.caller_id != caller)
        {
            return Err("session_unavailable".into());
        }
        self.sessions.remove(reference);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switch_close_and_restart_never_rebind() {
        let mut sessions = Sessions::new().unwrap();
        let grant = Grant {
            grant_ref: "a-read".into(),
            grant_id: "stable".into(),
            caller_id: "caller".into(),
            binding: Binding {
                account_ref: "a".into(),
                environment: "e".into(),
                world: "w".into(),
                genesis_digest: "g".into(),
                agent_id: "agent".into(),
                resource: "r".into(),
                authorization_version: 1,
            },
            expires_at_unix: 100,
            enabled: true,
            methods: vec!["task".into()],
        };
        let a = sessions.open("caller", &grant, 99, 1).unwrap();
        let mut b = grant.clone();
        b.binding.account_ref = "b".into();
        let bref = sessions.open("caller", &b, 99, 1).unwrap();
        assert_eq!(
            sessions
                .authorize(&a, "caller", &grant, "task", 2)
                .unwrap()
                .account_ref,
            "a"
        );
        assert!(sessions.authorize(&a, "caller", &b, "task", 2).is_err());
        assert!(sessions.authorize(&bref, "other", &b, "task", 2).is_err());
        let mut revoked = grant.clone();
        revoked.enabled = false;
        assert!(sessions
            .authorize(&a, "caller", &revoked, "task", 2)
            .is_err());
        assert!(Sessions::new()
            .unwrap()
            .authorize(&a, "caller", &grant, "task", 2)
            .is_err());
        sessions.close(&a, "caller").unwrap();
        assert!(sessions.authorize(&a, "caller", &grant, "task", 2).is_err());
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub account_ref: String,
    pub executor_config: std::path::PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
    pub service: crate::dev_test_service::Config,
    pub catalog_directory: std::path::PathBuf,
    pub grants: Vec<Grant>,
    pub resources: BTreeMap<String, Resource>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRequest {
    pub credential: String,
    pub method: String,
    pub grant_ref: Option<String>,
    pub session_ref: Option<String>,
    pub expires_at_unix: Option<u64>,
}
fn current_client(
    config: &ServiceConfig,
    grant: &Grant,
    unlocks: &BTreeMap<String, String>,
) -> Result<crate::device_client::Client, String> {
    let account =
        crate::dev_test_catalog::lookup(&config.catalog_directory, &grant.binding.account_ref)?;
    if account.disabled {
        return Err("account_disabled".into());
    }
    if account.environment != grant.binding.environment {
        return Err("account_environment_mismatch".into());
    }
    let unlock = unlocks
        .get(&account.account_ref)
        .ok_or("private_unlock_unavailable")?;
    account
        .verify_current(unlock.as_bytes())
        .map_err(|_| "native_identity_invalid")?;
    let resource = config
        .resources
        .get(&grant.binding.resource)
        .ok_or("resource_unavailable")?;
    if resource.account_ref != account.account_ref {
        return Err("resource_account_mismatch".into());
    }
    let executor_bytes =
        crate::dev_test_private_config::read_private_config(&resource.executor_config)?;
    let device: crate::device_client::ExecutorDeviceConfig =
        serde_json::from_slice(&executor_bytes).map_err(|_| "executor_configuration_invalid")?;
    let environment = serde_json::to_string(&(
        &device.identity.identity_service_id,
        &device.identity.environment_id,
    ))
    .map_err(|_| "identity_environment_invalid")?;
    if environment != account.environment
        || device.fence.world_id != grant.binding.world
        || device.fence.genesis_digest != grant.binding.genesis_digest
        || device.fence.agent_id != grant.binding.agent_id
    {
        return Err("resource_scope_mismatch".into());
    }
    let repeated: crate::device_client::ExecutorDeviceConfig =
        serde_json::from_slice(&executor_bytes).map_err(|_| "executor_configuration_invalid")?;
    let identity =
        crate::identity_client::IdentityClient::new(repeated.identity, unlock.as_bytes())
            .map_err(|_| "native_identity_invalid")?;
    if identity
        .current_account(&device.login_operation)
        .map_err(|_| "native_identity_invalid")?
        != account.account_id
        || identity.public_key() != account.public_key
    {
        return Err("resource_identity_mismatch".into());
    }
    // The executor gateway additionally checks current native login and fence.
    crate::device_client::Client::from_authenticated_config(device, unlock.as_bytes())
        .map_err(|_| "native_resource_unavailable".into())
}
pub fn handle(
    config: &ServiceConfig,
    sessions: &mut Sessions,
    request: ToolRequest,
    unlocks: &BTreeMap<String, String>,
    now: u64,
) -> Result<serde_json::Value, String> {
    use crate::dev_test_service::{authenticate_token, dispatch_authenticated, AUDIENCE, PROFILE};
    let caller = authenticate_token(&config.service, &request.credential, now)?;
    if request.method == "accounts" {
        if request.grant_ref.is_some()
            || request.session_ref.is_some()
            || request.expires_at_unix.is_some()
        {
            return Err("invalid_tool_arguments".into());
        }
        let mut views = Vec::new();
        for grant in config
            .grants
            .iter()
            .filter(|g| g.caller_id == caller.caller_id && g.enabled && now < g.expires_at_unix)
        {
            let account = crate::dev_test_catalog::lookup(
                &config.catalog_directory,
                &grant.binding.account_ref,
            )?;
            if !account.disabled {
                views.push(serde_json::json!({"grant_ref":grant.grant_ref,"account_ref":account.account_ref,"account_id":account.account_id,"environment":account.environment,"alias":account.alias,"binding":grant.binding}));
            }
        }
        let admitted = dispatch_authenticated(
            &config.service,
            now,
            &caller,
            PROFILE,
            AUDIENCE,
            "capabilities",
        );
        if admitted.status != "ok" {
            return Err(admitted.status);
        }
        return Ok(serde_json::json!({"accounts":views}));
    }
    if request.method == "close" {
        if request.grant_ref.is_some() || request.expires_at_unix.is_some() {
            return Err("invalid_tool_arguments".into());
        }
        sessions.close(
            request.session_ref.as_deref().ok_or("session_required")?,
            &caller.caller_id,
        )?;
        return Ok(serde_json::json!({"status":"closed_new_calls_denied"}));
    }
    let grant = if request.method == "open" {
        if request.session_ref.is_some() {
            return Err("invalid_tool_arguments".into());
        }
        config
            .grants
            .iter()
            .find(|g| {
                Some(g.grant_ref.as_str()) == request.grant_ref.as_deref()
                    && g.caller_id == caller.caller_id
            })
            .ok_or("grant_unavailable")?
    } else {
        if request.grant_ref.is_some() || request.expires_at_unix.is_some() {
            return Err("invalid_tool_arguments".into());
        }
        let reference = request.session_ref.as_deref().ok_or("session_required")?;
        let session = sessions
            .sessions
            .get(reference)
            .ok_or("session_unavailable")?;
        let grant = config
            .grants
            .iter()
            .find(|g| g.grant_ref == session.grant_ref && g.caller_id == caller.caller_id)
            .ok_or("grant_unavailable")?;
        sessions.authorize(reference, &caller.caller_id, grant, &request.method, now)?;
        grant
    };
    if !grant.enabled || now >= grant.expires_at_unix {
        return Err("grant_inactive".into());
    }
    if grant.grant_id != config.service.grant_id {
        return Err("grant_configuration_mismatch".into());
    }
    let client = current_client(config, grant, unlocks)?;
    if request.method == "open" {
        let expires = request.expires_at_unix.ok_or("session_expiry_required")?;
        // Open validates immutable scope before consuming the persistent grant.
        let reference = sessions.open(&caller.caller_id, grant, expires, now)?;
        let admitted = dispatch_authenticated(
            &config.service,
            now,
            &caller,
            PROFILE,
            AUDIENCE,
            "capabilities",
        );
        if admitted.status != "ok" {
            sessions.close(&reference, &caller.caller_id)?;
            return Err(admitted.status);
        }
        return Ok(
            serde_json::json!({"session_ref":reference,"binding":grant.binding,"expires_at_unix":expires}),
        );
    }
    let admitted = dispatch_authenticated(
        &config.service,
        now,
        &caller,
        PROFILE,
        AUDIENCE,
        "capabilities",
    );
    if admitted.status != "ok" {
        return Err(admitted.status);
    }
    let result = match request.method.as_str() {
        "describe" => {
            let fence = client
                .task_fence()
                .map_err(|_| "downstream_read_rejected")?;
            serde_json::json!({"account_ref":grant.binding.account_ref,"world":fence.world_id,"agent":fence.agent_id})
        }
        "task" => serde_json::to_value(
            client
                .task_fence()
                .map_err(|_| "downstream_read_rejected")?,
        )
        .map_err(|_| "response_unavailable")?,
        "capabilities" => client
            .capabilities()
            .map_err(|_| "downstream_read_rejected")?,
        _ => return Err("method_unavailable".into()),
    };
    Ok(serde_json::json!({"binding":grant.binding,"result":result}))
}

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
    let load = || -> Result<ServiceConfig, String> {
        let config: ServiceConfig = serde_json::from_slice(
            &crate::dev_test_private_config::read_private_config(config_path)?,
        )
        .map_err(|_| "invalid_session_configuration")?;
        config.service.validate()?;
        let mut refs = std::collections::BTreeSet::new();
        if config.grants.iter().any(|g| {
            g.grant_ref.is_empty()
                || !refs.insert(&g.grant_ref)
                || g.grant_id != config.service.grant_id
        }) {
            return Err("ambiguous_grant_configuration".into());
        }
        Ok(config)
    };
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
    let unlocks: BTreeMap<String, String> =
        serde_json::from_slice(&unlock_bytes).map_err(|_| "private_unlock_invalid")?;
    let listener = UnixListener::bind(&initial.service.socket).map_err(|_| "socket_bind_failed")?;
    std::fs::set_permissions(
        &initial.service.socket,
        std::fs::Permissions::from_mode(0o600),
    )
    .map_err(|_| "socket_permissions_failed")?;
    let mut sessions = Sessions::new()?;
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
        let deadline = Instant::now() + Duration::from_millis(config.service.timeout_ms);
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
        let result = (|| {
            let request: ToolRequest =
                serde_json::from_slice(&body).map_err(|_| "invalid_tool_request")?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "clock_unavailable")?
                .as_secs();
            handle(&config, &mut sessions, request, &unlocks, now)
        })();
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
