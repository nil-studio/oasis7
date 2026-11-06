//! Versioned non-secret service contract for dev-test-accounts-v1.
use serde::{Deserialize, Serialize};

pub const PROFILE: &str = "dev-test-accounts-v1";
pub const AUDIENCE: &str = "oasis7-dev-test-tools";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credential {
    pub caller_id: String,
    pub token: String,
    pub expires_at_unix: u64,
    pub revoked: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub profile: String,
    pub socket: std::path::PathBuf,
    pub max_request_bytes: usize,
    pub timeout_ms: u64,
    pub max_calls: u64,
    pub credentials: Vec<Credential>,
}
impl Config {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.profile != PROFILE
            || !(256..=65536).contains(&self.max_request_bytes)
            || !(1..=30000).contains(&self.timeout_ms)
            || self.max_calls == 0
            || self.credentials.is_empty()
            || self
                .credentials
                .iter()
                .any(|c| c.token.len() < 32 || c.caller_id.is_empty())
        {
            return Err("invalid_service_configuration");
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub profile: String,
    pub audience: String,
    pub credential: String,
    pub method: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub profile: String,
    pub status: String,
    pub caller_id: Option<String>,
    pub methods: Vec<String>,
    pub accounts_available: bool,
    pub writes_available: bool,
}
/// Constructed only after trusted adapter authentication; never deserialized.
pub struct AuthenticatedCaller {
    caller_id: String,
}
pub fn authenticate_token(
    config: &Config,
    token: &str,
    now: u64,
) -> Result<AuthenticatedCaller, &'static str> {
    config.validate()?;
    config
        .credentials
        .iter()
        .find(|c| c.token == token && !c.revoked && now < c.expires_at_unix)
        .map(|c| AuthenticatedCaller {
            caller_id: c.caller_id.clone(),
        })
        .ok_or("unauthorized")
}
/// The trusted local adapter supplies the principal resolved from its peer-UID allowlist.
pub fn authenticate_local(
    config: &Config,
    caller_id: &str,
    now: u64,
) -> Result<AuthenticatedCaller, &'static str> {
    config.validate()?;
    config
        .credentials
        .iter()
        .find(|c| c.caller_id == caller_id && !c.revoked && now < c.expires_at_unix)
        .map(|c| AuthenticatedCaller {
            caller_id: c.caller_id.clone(),
        })
        .ok_or("unauthorized")
}
pub fn dispatch_authenticated(
    caller: &AuthenticatedCaller,
    profile: &str,
    audience: &str,
    method: &str,
) -> Response {
    let status = if profile != PROFILE {
        "unsupported_profile"
    } else if audience != AUDIENCE {
        "wrong_audience"
    } else if method != "capabilities" {
        "method_unavailable"
    } else {
        "ok"
    };
    Response {
        profile: PROFILE.into(),
        status: status.into(),
        caller_id: (status == "ok").then(|| caller.caller_id.clone()),
        methods: if status == "ok" {
            vec!["capabilities".into()]
        } else {
            vec![]
        },
        accounts_available: false,
        writes_available: false,
    }
}
pub fn dispatch(config: &Config, request: &Request, now: u64) -> Response {
    match authenticate_token(config, &request.credential, now) {
        Ok(caller) => dispatch_authenticated(
            &caller,
            &request.profile,
            &request.audience,
            &request.method,
        ),
        Err(status) => Response {
            profile: PROFILE.into(),
            status: status.into(),
            caller_id: None,
            methods: vec![],
            accounts_available: false,
            writes_available: false,
        },
    }
}

#[cfg(unix)]
pub fn serve(config: Config) -> Result<(), String> {
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    config.validate()?;
    let parent = config.socket.parent().ok_or("socket_parent_required")?;
    let metadata = std::fs::symlink_metadata(parent).map_err(|_| "socket_parent_unavailable")?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("private_socket_directory_required".into());
    }
    let listener = UnixListener::bind(&config.socket).map_err(|_| "socket_bind_failed")?;
    std::fs::set_permissions(&config.socket, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "socket_permissions_failed")?;
    let usage_path = config.socket.with_extension("usage");
    let mut calls: u64 = match std::fs::read_to_string(&usage_path) {
        Ok(value) => value.parse().map_err(|_| "invalid_usage_record")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(_) => return Err("usage_unavailable".into()),
    };
    for stream in listener.incoming() {
        let mut stream = stream.map_err(|_| "socket_accept_failed")?;
        let timeout = Some(std::time::Duration::from_millis(config.timeout_ms));
        stream
            .set_read_timeout(timeout)
            .map_err(|_| "timeout_configuration_failed")?;
        stream
            .set_write_timeout(timeout)
            .map_err(|_| "timeout_configuration_failed")?;
        let mut body = Vec::new();
        if (&mut stream)
            .take(config.max_request_bytes as u64 + 1)
            .read_to_end(&mut body)
            .is_err()
        {
            continue;
        }
        if body.len() > config.max_request_bytes {
            continue;
        }
        let Ok(request) = serde_json::from_slice::<Request>(&body) else {
            continue;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "clock_unavailable")?
            .as_secs();
        let mut result = dispatch(&config, &request, now);
        if result.status == "ok" {
            if calls >= config.max_calls {
                result.status = "call_limit_exhausted".into();
                result.methods.clear();
            } else {
                calls += 1;
                let temporary = usage_path.with_extension("pending");
                let mut file =
                    std::fs::File::create(&temporary).map_err(|_| "usage_write_failed")?;
                file.write_all(calls.to_string().as_bytes())
                    .map_err(|_| "usage_write_failed")?;
                file.sync_all().map_err(|_| "usage_write_failed")?;
                std::fs::rename(&temporary, &usage_path).map_err(|_| "usage_write_failed")?;
                std::fs::File::open(parent)
                    .and_then(|file| file.sync_all())
                    .map_err(|_| "usage_write_failed")?;
            }
        }
        let response = serde_json::to_vec(&result).map_err(|_| "response_encoding_failed")?;
        let _ = stream.write_all(&response);
    }
    Err("service_listener_closed".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capabilities_enforce_credential_and_closed_methods() {
        let config = Config {
            profile: PROFILE.into(),
            socket: "/unused".into(),
            max_request_bytes: 1024,
            timeout_ms: 1000,
            max_calls: 10,
            credentials: vec![Credential {
                caller_id: "tester".into(),
                token: "x".repeat(32),
                expires_at_unix: 100,
                revoked: false,
            }],
        };
        let mut request = Request {
            profile: PROFILE.into(),
            audience: AUDIENCE.into(),
            credential: "x".repeat(32),
            method: "capabilities".into(),
        };
        assert_eq!(dispatch(&config, &request, 99).status, "ok");
        assert_eq!(dispatch(&config, &request, 100).status, "unauthorized");
        request.method = "register_account".into();
        assert_eq!(dispatch(&config, &request, 99).status, "method_unavailable");
        request.audience = "manager".into();
        assert_eq!(dispatch(&config, &request, 99).status, "wrong_audience");
        request.credential.clear();
        assert_eq!(dispatch(&config, &request, 99).status, "unauthorized");
        assert!(serde_json::from_str::<Request>(r#"{"profile":"dev-test-accounts-v1","audience":"x","credential":"x","method":"capabilities","path":"/secret"}"#).is_err());
    }
}
