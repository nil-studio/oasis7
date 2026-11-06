//! Versioned non-secret service contract for dev-test-accounts-v1.
use serde::{Deserialize, Serialize};

pub const PROFILE: &str = "dev-test-accounts-v1";
pub const AUDIENCE: &str = "oasis7-dev-test-tools";

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credential {
    pub caller_id: String,
    pub token: String,
    pub expires_at_unix: u64,
    pub revoked: bool,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub profile: String,
    pub socket: std::path::PathBuf,
    pub usage_directory: std::path::PathBuf,
    pub grant_id: String,
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
            || self.grant_id.is_empty()
            || !self
                .grant_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || self.credentials.is_empty()
            || self
                .credentials
                .iter()
                .any(|c| c.token.len() < 32 || c.caller_id.is_empty())
        {
            return Err("invalid_service_configuration");
        }
        let mut callers = std::collections::BTreeSet::new();
        let mut tokens = std::collections::BTreeSet::new();
        if self
            .credentials
            .iter()
            .any(|c| !callers.insert(&c.caller_id) || !tokens.insert(&c.token))
        {
            return Err("ambiguous_credential_mapping");
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
    config: &Config,
    now: u64,
    caller: &AuthenticatedCaller,
    profile: &str,
    audience: &str,
    method: &str,
) -> Response {
    let status = if authenticate_local(config, &caller.caller_id, now).is_err() {
        "unauthorized"
    } else if profile != PROFILE {
        "unsupported_profile"
    } else if audience != AUDIENCE {
        "wrong_audience"
    } else if method != "capabilities" {
        "method_unavailable"
    } else {
        "ok"
    };
    let status = if status == "ok" {
        admit(config, &caller.caller_id).err().unwrap_or("ok")
    } else {
        status
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
            config,
            now,
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

fn usage_key(grant: &str, caller: &str) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(&(grant.len() as u64).to_be_bytes());
    hash.update(grant.as_bytes());
    hash.update(&(caller.len() as u64).to_be_bytes());
    hash.update(caller.as_bytes());
    hash.finalize().to_hex().to_string()
}

fn admit(config: &Config, caller: &str) -> Result<(), &'static str> {
    use std::io::Write;
    use std::os::unix::{
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
    };
    let directory = &config.usage_directory;
    let metadata =
        std::fs::symlink_metadata(directory).map_err(|_| "usage_directory_unavailable")?;
    if !metadata.is_dir()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err("private_usage_directory_required");
    }
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
    let used: u64 = match std::fs::read_to_string(&path) {
        Ok(value) => value.parse().map_err(|_| "invalid_usage_record")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(_) => return Err("usage_unavailable"),
    };
    if used >= config.max_calls {
        return Err("call_limit_exhausted");
    }
    let temporary = directory.join(format!("{key}.pending"));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|_| "usage_write_failed")?;
    file.write_all((used + 1).to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "usage_write_failed")?;
    std::fs::rename(temporary, path).map_err(|_| "usage_write_failed")?;
    std::fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| "usage_write_failed")?;
    Ok(())
}

#[cfg(unix)]
pub fn serve(config: Config) -> Result<(), String> {
    use std::io::Write;
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
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(config.timeout_ms);
        if read_until(&mut stream, &mut body, config.max_request_bytes, deadline).is_err() {
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
        let result = dispatch(&config, &request, now);
        let response = serde_json::to_vec(&result).map_err(|_| "response_encoding_failed")?;
        let _ = stream.write_all(&response);
    }
    Err("service_listener_closed".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completed_peer_response_is_read_through_eof() {
        use std::{
            io::Write,
            time::{Duration, Instant},
        };
        let (mut reader, mut writer) = std::os::unix::net::UnixStream::pair().unwrap();
        writer.write_all(b"response").unwrap();
        drop(writer);
        let mut output = Vec::new();
        read_until(
            &mut reader,
            &mut output,
            1024,
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(output, b"response");
    }
    #[test]
    fn slow_drip_cannot_extend_absolute_read_deadline() {
        use std::{
            io::Write,
            time::{Duration, Instant},
        };
        let (mut reader, mut writer) = std::os::unix::net::UnixStream::pair().unwrap();
        let sender = std::thread::spawn(move || {
            for _ in 0..20 {
                if writer.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        let started = Instant::now();
        assert!(
            read_until(
                &mut reader,
                &mut Vec::new(),
                1024,
                started + Duration::from_millis(75)
            )
            .is_err()
        );
        assert!(started.elapsed() < Duration::from_millis(250));
        drop(reader);
        sender.join().unwrap();
    }
    #[test]
    fn capabilities_enforce_credential_and_closed_methods() {
        use std::os::unix::fs::PermissionsExt;
        let directory =
            std::env::temp_dir().join(format!("oasis7-service-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_ne!(usage_key("a", "b\0c"), usage_key("a\0b", "c"));
        let request = Request {
            profile: PROFILE.into(),
            audience: AUDIENCE.into(),
            credential: "x".repeat(32),
            method: "capabilities".into(),
        };
        let path = directory.join("fake-socket");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(call_service(&path, &request).is_err());
        drop(listener);
        let mut config = Config {
            profile: PROFILE.into(),
            socket: "/unused".into(),
            usage_directory: directory.clone(),
            grant_id: "test-grant".into(),
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
        let mut ambiguous = config.clone();
        ambiguous.credentials.push(ambiguous.credentials[0].clone());
        assert_eq!(ambiguous.validate(), Err("ambiguous_credential_mapping"));
        let mut request = Request {
            profile: PROFILE.into(),
            audience: AUDIENCE.into(),
            credential: "x".repeat(32),
            method: "capabilities".into(),
        };
        assert_eq!(dispatch(&config, &request, 99).status, "ok");
        assert_eq!(dispatch(&config, &request, 100).status, "unauthorized");
        let successes = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..20)
                .map(|_| {
                    let config = &config;
                    let request = &request;
                    scope.spawn(move || dispatch(config, request, 99).status == "ok")
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| usize::from(handle.join().unwrap()))
                .sum::<usize>()
        });
        assert_eq!(successes, 9);
        config.max_calls = 1;
        config.socket = "/another-socket".into();
        let caller = authenticate_local(&config, "tester", 99).unwrap();
        assert_eq!(
            dispatch_authenticated(&config, 99, &caller, PROFILE, AUDIENCE, "capabilities").status,
            "call_limit_exhausted"
        );
        assert_eq!(
            dispatch(&config, &request, 99).status,
            "call_limit_exhausted"
        );
        request.method = "register_account".into();
        assert_eq!(dispatch(&config, &request, 99).status, "method_unavailable");
        request.audience = "manager".into();
        assert_eq!(dispatch(&config, &request, 99).status, "wrong_audience");
        request.credential.clear();
        assert_eq!(dispatch(&config, &request, 99).status, "unauthorized");
        std::fs::remove_dir_all(directory).unwrap();
        assert!(serde_json::from_str::<Request>(r#"{"profile":"dev-test-accounts-v1","audience":"x","credential":"x","method":"capabilities","path":"/secret"}"#).is_err());
    }
}

pub fn call_service(socket: &std::path::Path, request: &Request) -> Result<Response, String> {
    call_service_until(
        socket,
        request,
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    )
}
pub fn call_service_until(
    socket: &std::path::Path,
    request: &Request,
    deadline: std::time::Instant,
) -> Result<Response, String> {
    let body = serde_json::to_vec(request).map_err(|_| "request_encoding_failed")?;
    if body.len() > 65536 {
        return Err("request_too_large".into());
    }
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    crate::runtime_broker::trusted_ancestors(socket)?;
    let metadata = std::fs::symlink_metadata(socket).map_err(|_| "socket_unavailable")?;
    if !metadata.file_type().is_socket()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o777 != 0o600
    {
        return Err("untrusted_socket".into());
    }
    let mut stream = connect_until(socket, deadline)?;
    #[cfg(target_os = "linux")]
    {
        let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
        let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        if unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut credentials as *mut libc::ucred).cast(),
                &mut length,
            )
        } != 0
            || credentials.uid != unsafe { libc::geteuid() }
        {
            return Err("untrusted_service_peer".into());
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut uid = 0;
        let mut gid = 0;
        if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0
            || uid != unsafe { libc::geteuid() }
        {
            return Err("untrusted_service_peer".into());
        }
    }
    let timeout = Some(
        deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or("service_deadline_exceeded")?,
    );
    stream
        .set_read_timeout(timeout)
        .map_err(|_| "timeout_configuration_failed")?;
    stream
        .set_write_timeout(timeout)
        .map_err(|_| "timeout_configuration_failed")?;
    let mut sent = 0;
    while sent < body.len() {
        use std::io::Write;
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or("service_deadline_exceeded")?;
        stream
            .set_write_timeout(Some(remaining))
            .map_err(|_| "timeout_configuration_failed")?;
        let count = stream.write(&body[sent..]).map_err(|_| "request_failed")?;
        if count == 0 {
            return Err("request_failed".into());
        }
        sent += count;
    }
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|_| "request_failed")?;
    let mut response = Vec::new();
    read_until(&mut stream, &mut response, 65536, deadline)?;
    serde_json::from_slice(&response).map_err(|_| "invalid_response".into())
}

fn read_until(
    stream: &mut std::os::unix::net::UnixStream,
    output: &mut Vec<u8>,
    limit: usize,
    deadline: std::time::Instant,
) -> Result<(), String> {
    use std::{io::Read, os::fd::AsRawFd};
    stream
        .set_nonblocking(true)
        .map_err(|_| "socket_configuration_failed")?;
    let result = (|| {
        loop {
            let remaining = deadline
                .checked_duration_since(std::time::Instant::now())
                .filter(|r| !r.is_zero())
                .ok_or("service_deadline_exceeded")?;
            let mut poll = libc::pollfd {
                fd: stream.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe {
                libc::poll(
                    &mut poll,
                    1,
                    remaining.as_millis().min(i32::MAX as u128).max(1) as i32,
                )
            };
            if ready == 0 {
                return Err("service_deadline_exceeded".into());
            }
            if ready < 0 {
                if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err("socket_poll_failed".into());
            }
            let mut buffer = [0; 4096];
            match stream.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(count) => {
                    if output.len() + count > limit {
                        return Err("request_too_large".into());
                    }
                    output.extend_from_slice(&buffer[..count]);
                }
                Err(error)
                    if [
                        std::io::ErrorKind::WouldBlock,
                        std::io::ErrorKind::Interrupted,
                    ]
                    .contains(&error.kind()) =>
                {
                    continue;
                }
                Err(_) => return Err("request_transport_failed".into()),
            }
        }
    })();
    stream
        .set_nonblocking(false)
        .map_err(|_| "socket_configuration_failed")?;
    result
}

pub(crate) fn connect_until(
    path: &std::path::Path,
    deadline: std::time::Instant,
) -> Result<std::os::unix::net::UnixStream, String> {
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    };
    let bytes = path.as_os_str().as_bytes();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err("invalid_socket_path".into());
    }
    address.sun_family = libc::AF_UNIX as _;
    #[cfg(target_os = "macos")]
    {
        address.sun_len = std::mem::size_of_val(&address) as _;
    }
    for (slot, byte) in address.sun_path.iter_mut().zip(bytes) {
        *slot = *byte as _;
    }
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err("socket_creation_failed".into());
    }
    let stream = unsafe { std::os::unix::net::UnixStream::from_raw_fd(fd) };
    stream
        .set_nonblocking(true)
        .map_err(|_| "socket_configuration_failed")?;
    let result = unsafe {
        libc::connect(
            fd,
            (&address as *const libc::sockaddr_un).cast(),
            std::mem::size_of_val(&address) as _,
        )
    };
    if result != 0 {
        let error = std::io::Error::last_os_error().raw_os_error();
        if ![Some(libc::EINPROGRESS), Some(libc::EAGAIN)].contains(&error) {
            return Err("service_unavailable".into());
        }
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or("service_deadline_exceeded")?;
        let mut poll = libc::pollfd {
            fd: stream.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        if unsafe {
            libc::poll(
                &mut poll,
                1,
                remaining.as_millis().min(i32::MAX as u128).max(1) as i32,
            )
        } <= 0
        {
            return Err("service_deadline_exceeded".into());
        }
        if stream
            .take_error()
            .map_err(|_| "service_unavailable")?
            .is_some()
        {
            return Err("service_unavailable".into());
        }
    }
    if std::time::Instant::now() >= deadline {
        return Err("service_deadline_exceeded".into());
    }
    stream
        .set_nonblocking(false)
        .map_err(|_| "socket_configuration_failed")?;
    Ok(stream)
}
