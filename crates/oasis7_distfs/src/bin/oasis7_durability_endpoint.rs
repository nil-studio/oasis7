//! Independently keyed engineering or protected-signer formal endpoint.
#[path = "oasis7_durability_endpoint/formal.rs"]
mod formal_service;
#[path = "oasis7_durability_endpoint/writer_custody.rs"]
mod writer_custody_service;
use oasis7_distfs::controlled_authority::replicated_protocol::{
    EndpointRole, FileEndpoint, FixedTrust, HeadAnchor, transport::EndpointServer,
};
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u32,
    trust: FixedTrust,
    role: EndpointRole,
    data_dir: PathBuf,
    signing_key_file: PathBuf,
    minimum_head: HeadAnchor,
    listen: std::net::SocketAddr,
    timeout_ms: u64,
}
fn read(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err("endpoint input must be a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("endpoint input size limit".into());
    }
    Ok(bytes)
}
#[cfg(unix)]
fn key(path: &Path) -> Result<ed25519_dalek::SigningKey, String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    // SAFETY: geteuid reads the current process identity and has no arguments.
    let owner = unsafe { libc::geteuid() };
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 || metadata.uid() != owner
    {
        return Err("endpoint key must be owned and private".into());
    }
    let bytes = read(path, 128)?;
    let secret: [u8; 32] = hex::decode(
        std::str::from_utf8(&bytes)
            .map_err(|_| "key encoding")?
            .trim(),
    )
    .map_err(|_| "key encoding")?
    .try_into()
    .map_err(|_| "key length")?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&secret))
}
#[cfg(unix)]
fn private_data_dir(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    // SAFETY: geteuid reads the process identity only.
    let owner = unsafe { libc::geteuid() };
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != owner
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err("endpoint data directory must be owned and private".into());
    }
    Ok(())
}
#[cfg(not(unix))]
fn private_data_dir(_: &Path) -> Result<(), String> {
    Err("endpoint requires Unix durability".into())
}
#[cfg(not(unix))]
fn key(_: &Path) -> Result<ed25519_dalek::SigningKey, String> {
    Err("endpoint requires Unix durability".into())
}

fn run(arguments: &[String]) -> Result<(), String> {
    if arguments == ["--help".to_string()] {
        println!(
            "Usage: oasis7_durability_endpoint --config <file>\n       oasis7_durability_endpoint --writer-custody-config <file>\nEndpoint schema 1: independent-key engineering endpoint. Endpoint schema 2: formal endpoint with externally pinned initial authorization and protected monotonic oracle; no local receipt/writer keys. The separate writer custody mode owns its writer key and durable plan/seal ledger in a CONTROLLED_ONLY file service; it does not establish physical rollback protection. Loopback only; cross-host use requires an authenticated encrypted tunnel."
        );
        return Ok(());
    }
    if arguments.len() == 2
        && arguments[0] == "--writer-custody-config"
        && !arguments[1].starts_with("--")
    {
        let path = Path::new(&arguments[1]);
        let bytes = read(path, 4 * 1024 * 1024)?;
        return writer_custody_service::run(&bytes, path.parent().unwrap_or(Path::new(".")));
    }
    if arguments.len() != 2 || arguments[0] != "--config" || arguments[1].starts_with("--") {
        return Err("Usage: oasis7_durability_endpoint --config <file>".into());
    }
    let path = Path::new(&arguments[1]);
    let config_bytes = read(path, 4 * 1024 * 1024)?;
    let schema: serde_json::Value =
        serde_json::from_slice(&config_bytes).map_err(|e| e.to_string())?;
    if schema
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        == Some(2)
    {
        return formal_service::run(&config_bytes, path.parent().unwrap_or(Path::new(".")));
    }
    if config_bytes.len() > 64 * 1024 {
        return Err("engineering endpoint configuration size limit".into());
    }
    let mut config: Config = serde_json::from_slice(&config_bytes).map_err(|e| e.to_string())?;
    if config.schema_version != 1
        || !config.listen.ip().is_loopback()
        || !(1..=60_000).contains(&config.timeout_ms)
    {
        return Err("invalid endpoint configuration".into());
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    for value in [&mut config.data_dir, &mut config.signing_key_file] {
        if !value.is_absolute() {
            *value = parent.join(&*value);
        }
    }
    let signer = key(&config.signing_key_file)?;
    if config.data_dir.exists() {
        private_data_dir(&config.data_dir)?;
    }
    let listener = std::net::TcpListener::bind(config.listen).map_err(|e| e.to_string())?;
    let endpoint = FileEndpoint::open(
        config.data_dir,
        config.trust,
        config.role,
        signer,
        &config.minimum_head,
    )
    .map_err(|e| e.to_string())?;
    let mut server = EndpointServer::new(endpoint, Duration::from_millis(config.timeout_ms))
        .map_err(|e| e.to_string())?;
    println!(
        "endpoint_listening={}",
        listener.local_addr().map_err(|e| e.to_string())?
    );
    loop {
        // An untrusted request cannot stop the endpoint or reveal key/record data.
        if server.serve_one(&listener).is_err() {
            eprintln!("endpoint RPC rejected or incomplete; reconcile original request");
        }
    }
}
fn main() {
    if let Err(error) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn help_and_bad_arguments_do_not_open_endpoint_storage() {
        assert!(run(&["--help".into()]).is_ok());
        assert!(run(&[]).is_err());
        assert!(run(&["--config".into(), "--help".into()]).is_err());
    }
    #[test]
    #[cfg(unix)]
    fn signer_requires_private_regular_file_and_data_dir() {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let directory =
            std::env::temp_dir().join(format!("oasis7-endpoint-key-test-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = directory.join("key");
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        file.write_all(hex::encode([2u8; 32]).as_bytes()).unwrap();
        drop(file);
        assert!(key(&path).is_ok());
        assert!(private_data_dir(&directory).is_ok());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(key(&path).is_err());
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(private_data_dir(&directory).is_err());
        let link = directory.join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(key(&link).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
