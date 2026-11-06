//! Linux-only restricted local transport; authentication uses kernel peer credentials.
//! The caller cannot supply a principal or a credential path in the transport envelope.
#[cfg(target_os = "linux")]
use std::io::Read;
use std::{
    io::Write,
    os::unix::{
        fs::MetadataExt,
        net::{UnixListener, UnixStream},
    },
    path::Path,
    time::Duration,
};

/// Trusted daemon configuration, never accepted from a model request.
pub struct LocalPolicy {
    pub service_uid: u32,
    pub caller_uid: u32,
    pub max_request_bytes: usize,
    pub timeout: Duration,
}

#[cfg(target_os = "linux")]
fn peer_uid(stream: &UnixStream) -> Result<u32, String> {
    use std::os::fd::AsRawFd;
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result != 0 || length as usize != std::mem::size_of::<libc::ucred>() {
        return Err("local peer authentication unavailable".into());
    }
    Ok(credentials.uid)
}

/// Receive one bounded request only after authenticating its OS identity.
#[cfg(target_os = "linux")]
pub fn receive(stream: &mut UnixStream, policy: &LocalPolicy) -> Result<Vec<u8>, String> {
    if policy.service_uid != unsafe { libc::geteuid() }
        || policy.service_uid == policy.caller_uid
        || policy.max_request_bytes == 0
        || policy.max_request_bytes > 65536
        || policy.timeout.is_zero()
        || policy.timeout > Duration::from_secs(30)
    {
        return Err("invalid local isolation policy".into());
    }
    if peer_uid(stream)? != policy.caller_uid {
        return Err("local caller denied".into());
    }
    stream
        .set_read_timeout(Some(policy.timeout))
        .map_err(|_| "local timeout configuration failed")?;
    stream
        .set_write_timeout(Some(policy.timeout))
        .map_err(|_| "local timeout configuration failed")?;
    let mut body = Vec::new();
    stream
        .take(policy.max_request_bytes as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|_| "local request read failed")?;
    if body.len() > policy.max_request_bytes {
        return Err("local request too large".into());
    }
    Ok(body)
}

/// Bind only within an existing service-owned directory without writable ancestors.
/// Operators grant traversal to the model subject; secrets belong in another private directory.
pub fn bind(path: &Path, service_uid: u32) -> Result<UnixListener, String> {
    use std::os::unix::fs::PermissionsExt;
    if !path.is_absolute() {
        return Err("local socket path must be absolute".into());
    }
    let parent = path.parent().ok_or("local socket directory missing")?;
    for ancestor in parent.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)
            .map_err(|_| "local socket directory unavailable")?;
        if !metadata.is_dir()
            || metadata.mode() & 0o022 != 0
            || (metadata.uid() != 0 && metadata.uid() != service_uid)
        {
            return Err("local socket directory untrusted".into());
        }
    }
    let metadata =
        std::fs::symlink_metadata(parent).map_err(|_| "local socket directory unavailable")?;
    if metadata.uid() != service_uid {
        return Err("local socket directory owner invalid".into());
    }
    let listener = UnixListener::bind(path).map_err(|_| "local socket bind failed")?;
    // Peer UID authorization, rather than possession of a readable token, gates requests.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666))
        .map_err(|_| "local socket permission failed")?;
    Ok(listener)
}

pub fn respond(stream: &mut UnixStream, response: &[u8]) -> Result<(), String> {
    if response.len() > 65536 {
        return Err("local response too large".into());
    }
    stream
        .write_all(response)
        .map_err(|_| "local response write failed".into())
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalRequest {
    pub profile: String,
    pub audience: String,
    pub method: String,
}

/// The proxy resolves identity and inserts service authentication only in trusted memory.
/// Forwarding to the common service preserves its single cumulative admission ledger.
#[cfg(target_os = "linux")]
pub fn serve(
    socket: &Path,
    config_path: &Path,
    caller_uid: u32,
    caller_id: &str,
) -> Result<(), String> {
    use crate::dev_test_service::{Config, Request};
    let service_uid = unsafe { libc::geteuid() };
    if service_uid == caller_uid {
        return Err("distinct model uid required".into());
    }
    let listener = bind(socket, service_uid)?;
    for stream in listener.incoming() {
        let mut stream = stream.map_err(|_| "local accept failed")?;
        // Re-read a private administrator-controlled config on each call.
        let metadata =
            std::fs::symlink_metadata(config_path).map_err(|_| "private config unavailable")?;
        if !metadata.is_file() || metadata.uid() != service_uid || metadata.mode() & 0o077 != 0 {
            return Err("private config required".into());
        }
        let config: Config = serde_json::from_slice(
            &std::fs::read(config_path).map_err(|_| "private config unavailable")?,
        )
        .map_err(|_| "invalid private config")?;
        config.validate()?;
        let policy = LocalPolicy {
            service_uid,
            caller_uid,
            max_request_bytes: config.max_request_bytes,
            timeout: Duration::from_millis(config.timeout_ms),
        };
        let result = (|| -> Result<Vec<u8>, String> {
            let body = receive(&mut stream, &policy)?;
            let request: LocalRequest =
                serde_json::from_slice(&body).map_err(|_| "invalid local request")?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "clock unavailable")?
                .as_secs();
            crate::dev_test_service::authenticate_local(&config, caller_id, now)?;
            let credential = config
                .credentials
                .iter()
                .find(|c| c.caller_id == caller_id && !c.revoked && now < c.expires_at_unix)
                .ok_or("unauthorized")?;
            let metadata = std::fs::symlink_metadata(&config.socket)
                .map_err(|_| "private service unavailable")?;
            use std::os::unix::fs::FileTypeExt;
            if !metadata.file_type().is_socket()
                || metadata.uid() != service_uid
                || metadata.mode() & 0o077 != 0
            {
                return Err("private service socket untrusted".into());
            }
            let forwarded = Request {
                profile: request.profile,
                audience: request.audience,
                credential: credential.token.clone(),
                method: request.method,
            };
            let parsed = crate::dev_test_service::call_service(&config.socket, &forwarded)?;
            serde_json::to_vec(&parsed).map_err(|_| "response encoding failed".into())
        })();
        match result {
            Ok(response) => {
                let _ = respond(&mut stream, &response);
            }
            Err(_) => {
                let _ = respond(&mut stream, br#"{"status":"local_request_denied"}"#);
            }
        }
    }
    Err("local listener closed".into())
}
