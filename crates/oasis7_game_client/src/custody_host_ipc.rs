//! Separate custody socket. Existing same-UID Executor broker remains untouched.
use super::*;
use cap_fs_ext::DirExt;
use cap_std::fs::{Dir, MetadataExt, PermissionsExt as CapPermissionsExt};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileTypeExt, MetadataExt as StdMetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Component, Path},
    time::{Duration, Instant},
};
const MAX_FRAME: usize = 32768;
const TOOLS: &str = include_str!("custody_control_tools.v1.json");
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PublicBinding {
    pub api_version: u8,
    pub transport_profile: String,
    pub socket_path: String,
    pub identity_digest: String,
    pub descriptor_digest: String,
    pub session_digest: String,
    pub resource_id: String,
    pub restricted_runtime_identity: String,
    pub(super) isolation: Isolation,
}
fn sha_file(path: &Path, limit: u64) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let meta = std::fs::symlink_metadata(path).map_err(|_| denied())?;
    if !meta.is_file()
        || meta.nlink() != 1
        || meta.permissions().mode() & 0o022 != 0
        || meta.len() > limit
    {
        return Err(denied());
    }
    let mut f = std::fs::File::open(path).map_err(|_| denied())?;
    let opened = f.metadata().map_err(|_| denied())?;
    if meta.dev() != opened.dev() || meta.ino() != opened.ino() {
        return Err(denied());
    }
    let mut digest = Sha256::new();
    let mut buffer = [0; 8192];
    let mut total = 0u64;
    loop {
        let n = f.read(&mut buffer).map_err(|_| denied())?;
        if n == 0 {
            break;
        }
        total = total.checked_add(n as u64).ok_or_else(denied)?;
        if total > limit {
            return Err(denied());
        }
        digest.update(&buffer[..n]);
    }
    Ok(hex::encode(digest.finalize()))
}
pub(super) fn fill_runtime_pins(i: &mut IdentityBinding) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    let build = sha_file(
        &std::env::current_exe().map_err(|_| denied())?,
        512 * 1024 * 1024,
    )?;
    let skill = sha_file(Path::new(&i.skill_manifest_path), 1024 * 1024)?;
    let tools = hex::encode(Sha256::digest(TOOLS.as_bytes()));
    for (field, actual) in [
        (&mut i.descriptor.build_digest, build),
        (&mut i.descriptor.skill_manifest_digest, skill),
        (&mut i.descriptor.tool_manifest_digest, tools),
    ] {
        if field.is_empty() {
            *field = actual;
        } else if *field != actual {
            return Err(denied());
        }
    }
    Ok(())
}
pub(super) fn verify_runtime_pins(i: &IdentityBinding) -> Result<(), String> {
    let mut expected = i.clone();
    fill_runtime_pins(&mut expected)?;
    if expected != *i {
        return Err(denied());
    }
    Ok(())
}
fn peer_uid(stream: &UnixStream) -> Result<u32, String> {
    #[cfg(any(
        target_os = "macos",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    {
        let mut uid: libc::uid_t = 0;
        let mut gid: libc::gid_t = 0;
        // SAFETY: the live UnixStream owns the fd; uid/gid are valid writable outputs.
        if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
            return Err(denied());
        }
        Ok(uid)
    }
    #[cfg(target_os = "linux")]
    {
        let mut cred = std::mem::MaybeUninit::<libc::ucred>::uninit();
        let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        // SAFETY: the fd is live; the buffer and length cover exactly one ucred.
        let result = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                cred.as_mut_ptr().cast(),
                &mut len,
            )
        };
        if result != 0 || len as usize != std::mem::size_of::<libc::ucred>() {
            return Err(denied());
        }
        // SAFETY: successful SO_PEERCRED returned the exact initialized ucred size.
        Ok(unsafe { cred.assume_init() }.uid)
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )))]
    {
        let _ = stream;
        Err(denied())
    }
}
fn directory(path: &Path, host: u32) -> Result<Dir, String> {
    if !path.is_absolute() {
        return Err(denied());
    }
    let mut held =
        Dir::open_ambient_dir("/", cap_std::ambient_authority()).map_err(|_| denied())?;
    for part in path.components().skip(1) {
        if !matches!(part, Component::Normal(_)) {
            return Err(denied());
        }
        held = held
            .open_dir_nofollow(part.as_os_str())
            .map_err(|_| denied())?;
    }
    let m = held.dir_metadata().map_err(|_| denied())?;
    let mode = m.permissions().mode();
    if m.uid() != host || mode & 0o700 != 0o700 || mode & 0o027 != 0 || mode & 0o010 == 0 {
        return Err(denied());
    }
    Ok(held)
}
fn verify_parent(path: &Path, held: &Dir) -> Result<(), String> {
    let a = std::fs::symlink_metadata(path).map_err(|_| denied())?;
    let b = held.dir_metadata().map_err(|_| denied())?;
    if !a.is_dir() || a.dev() != b.dev() || a.ino() != b.ino() {
        return Err(denied());
    }
    Ok(())
}
fn sidecar(socket: &Path) -> Result<std::path::PathBuf, String> {
    let mut name = socket.file_name().ok_or_else(denied)?.to_os_string();
    name.push(".custody.json");
    Ok(socket.with_file_name(name))
}
pub(super) fn binding(host: &Host, socket: &Path) -> Result<PublicBinding, String> {
    host.vault.read(|s| {
        let session = s.session.as_ref().ok_or_else(denied)?;
        session.credential(SessionPurposeV1::ReconcileRead)?;
        if vault::now()? < session.binding.plan.write_deadline.get() {
            session.credential(SessionPurposeV1::Execution)?;
        }
        Ok(PublicBinding {
            api_version: 1,
            transport_profile: "native-broker-custody-v1".into(),
            socket_path: socket.to_str().ok_or_else(denied)?.into(),
            identity_digest: s.identity.digest()?,
            descriptor_digest: s.identity.descriptor.digest().map_err(|_| denied())?,
            session_digest: session.binding.digest()?,
            resource_id: session.binding.resource_id.clone(),
            restricted_runtime_identity: s.identity.descriptor.restricted_runtime_identity.clone(),
            isolation: s.identity.isolation.clone(),
        })
    })
}
fn publish_binding(dir: &Dir, socket: &Path, binding: &PublicBinding) -> Result<(), String> {
    use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
    use cap_std::fs::OpenOptionsExt;
    let name = sidecar(socket)?.file_name().ok_or_else(denied)?.to_owned();
    let bytes = serde_json::to_vec(binding).map_err(|_| denied())?;
    let mut options = cap_std::fs::OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .mode(0o644)
        .follow(FollowSymlinks::No);
    match dir.open_with(&name, &options) {
        Ok(mut f) => {
            f.write_all(&bytes).map_err(|_| denied())?;
            f.sync_all().map_err(|_| denied())?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = read_binding(socket)?;
            if serde_json::to_vec(&existing).map_err(|_| denied())? != bytes {
                return Err(denied());
            }
        }
        Err(_) => return Err(denied()),
    }
    Ok(())
}
fn read_binding(socket: &Path) -> Result<PublicBinding, String> {
    let path = sidecar(socket)?;
    let m = std::fs::symlink_metadata(&path).map_err(|_| denied())?;
    if !m.is_file()
        || m.nlink() != 1
        || m.len() > MAX_FRAME as u64
        || m.permissions().mode() & 0o022 != 0
    {
        return Err(denied());
    }
    let f = std::fs::File::open(&path).map_err(|_| denied())?;
    let opened = f.metadata().map_err(|_| denied())?;
    if opened.dev() != m.dev() || opened.ino() != m.ino() {
        return Err(denied());
    }
    let mut bytes = Vec::new();
    f.take(MAX_FRAME as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| denied())?;
    let b: PublicBinding = serde_json::from_slice(&bytes).map_err(|_| denied())?;
    b.isolation.validate()?;
    if b.api_version != 1
        || b.transport_profile != "native-broker-custody-v1"
        || b.socket_path != socket.to_str().ok_or_else(denied)?
        || m.uid() != b.isolation.host_uid
        || !id(&b.resource_id)
    {
        return Err(denied());
    }
    for d in [&b.identity_digest, &b.descriptor_digest, &b.session_digest] {
        if d.len() != 64 || hex::decode(d).is_err() {
            return Err(denied());
        }
    }
    Ok(b)
}
fn write_frame<T: Serialize>(s: &mut UnixStream, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|_| denied())?;
    if bytes.len() > MAX_FRAME {
        return Err(denied());
    }
    s.write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|_| s.write_all(&bytes))
        .map_err(|_| denied())
}
fn read_frame<T: serde::de::DeserializeOwned>(s: &mut UnixStream) -> Result<T, String> {
    let mut n = [0; 4];
    s.read_exact(&mut n).map_err(|_| denied())?;
    let n = u32::from_be_bytes(n) as usize;
    if n == 0 || n > MAX_FRAME {
        return Err(denied());
    }
    let mut bytes = vec![0; n];
    s.read_exact(&mut bytes).map_err(|_| denied())?;
    serde_json::from_slice(&bytes).map_err(|_| denied())
}
fn socket_metadata(socket: &Path, host: u32) -> Result<std::fs::Metadata, String> {
    let m = std::fs::symlink_metadata(socket).map_err(|_| denied())?;
    if !m.file_type().is_socket() || m.uid() != host || m.mode() & 0o777 != 0o660 {
        return Err(denied());
    }
    Ok(m)
}
pub(crate) fn call(
    socket: &Path,
    request: control::ModelRequest,
) -> Result<control::ModelReply, String> {
    let b = read_binding(socket)?;
    super::registry::check(&b)?;
    if vault::actual_uid() != b.isolation.model_uid || request.resource_id != b.resource_id {
        return Err(denied());
    }
    let parent = socket.parent().ok_or_else(denied)?;
    let held = directory(parent, b.isolation.host_uid)?;
    verify_parent(parent, &held)?;
    let before = socket_metadata(socket, b.isolation.host_uid)?;
    let mut stream = UnixStream::connect(socket).map_err(|_| denied())?;
    if peer_uid(&stream)? != b.isolation.host_uid {
        return Err(denied());
    }
    let after = socket_metadata(socket, b.isolation.host_uid)?;
    if before.dev() != after.dev() || before.ino() != after.ino() {
        return Err(denied());
    }
    verify_parent(parent, &held)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|_| denied())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| denied())?;
    let expected = request.request_id.clone();
    write_frame(&mut stream, &request)?;
    let reply: control::ModelReply = read_frame(&mut stream)?;
    if reply.api_version != 1 || reply.request_id != expected {
        return Err(denied());
    }
    Ok(reply)
}
pub(crate) fn serve(host: Host, socket: &Path) -> Result<(), String> {
    let b = binding(&host, socket)?;
    super::registry::check(&b)?;
    if vault::actual_uid() != b.isolation.host_uid {
        return Err(denied());
    }
    let parent = socket.parent().ok_or_else(denied)?;
    let held = directory(parent, b.isolation.host_uid)?;
    verify_parent(parent, &held)?;
    publish_binding(&held, socket, &b)?;
    let listener = UnixListener::bind(socket).map_err(|_| denied())?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o660))
        .map_err(|_| denied())?;
    let own = socket_metadata(socket, b.isolation.host_uid)?;
    verify_parent(parent, &held)?;
    listener.set_nonblocking(true).map_err(|_| denied())?;
    let mut maintenance = Instant::now();
    loop {
        super::registry::check(&b)?;
        let alive = host.vault.read(|s| {
            Ok(vault::now()?
                < s.session
                    .as_ref()
                    .ok_or_else(denied)?
                    .binding
                    .plan
                    .read_until
                    .get())
        })?;
        if !alive {
            break;
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                if peer_uid(&stream).ok() != Some(b.isolation.model_uid) {
                    continue;
                }
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .map_err(|_| denied())?;
                stream
                    .set_write_timeout(Some(Duration::from_secs(3)))
                    .map_err(|_| denied())?;
                if let Ok(q) = read_frame::<control::ModelRequest>(&mut stream) {
                    let id = q.request_id.clone();
                    super::registry::check(&b)?;
                    let r = host
                        .control(q)
                        .unwrap_or_else(|_| control::ModelReply::unavailable(id));
                    let _ = write_frame(&mut stream, &r);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(_) => return Err(denied()),
        }
        if maintenance.elapsed() >= Duration::from_secs(30) {
            super::registry::check(&b)?;
            let _ = host.heartbeat_if_due();
            maintenance = Instant::now();
        }
    }
    drop(listener);
    if let Ok(now) = held.symlink_metadata(socket.file_name().ok_or_else(denied)?)
        && now.dev() == own.dev()
        && now.ino() == own.ino()
    {
        held.remove_file(socket.file_name().ok_or_else(denied)?)
            .map_err(|_| denied())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kernel_peer_observation_does_not_accept_a_different_claimed_uid() {
        let (a, b) = UnixStream::pair().unwrap();
        let actual = vault::actual_uid();
        assert_eq!(peer_uid(&a).unwrap(), actual);
        assert_eq!(peer_uid(&b).unwrap(), actual);
        let claimed = if actual == 60001 { 60002 } else { 60001 };
        assert_ne!(peer_uid(&a).unwrap(), claimed);
    }
}
