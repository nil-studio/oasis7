//! Trusted operator-only account catalog; never registered as a model tool.
use serde::{Deserialize, Serialize};
use std::os::unix::{
    fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    io::AsRawFd,
};
use std::{
    io::{IsTerminal, Read, Write},
    path::Path,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub account_ref: String,
    pub account_id: String,
    pub environment: String,
    pub public_key: String,
    pub alias: String,
    credential_ref: String,
    login_operation: String,
}
fn private_directory(path: &Path) -> Result<(), String> {
    crate::runtime_broker::trusted_ancestors(&path.join("catalog"))?;
    let m = std::fs::symlink_metadata(path).map_err(|_| "management_directory_unavailable")?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.permissions().mode() & 0o077 != 0 {
        return Err("operator_private_directory_required".into());
    }
    Ok(())
}
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    if args.len() < 3 || args[1] != "--directory" {
        return Err("dev-test-catalog-list --directory PATH | dev-test-catalog-register --directory PATH --identity-config PATH --operation ID --alias NAME".into());
    }
    let directory = Path::new(&args[2]);
    private_directory(directory)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join("catalog.lock"))
        .map_err(|_| "catalog_lock_failed")?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err("catalog_lock_failed".into());
    }
    let path = directory.join("catalog.json");
    let mut accounts: Vec<Account> = match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "catalog_corrupt")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(_) => return Err("catalog_unavailable".into()),
    };
    if args[0] == "dev-test-catalog-list" && args.len() == 3 {
        let views: Vec<_> = accounts.iter().map(|a| serde_json::json!({"account_ref":a.account_ref,"account_id":a.account_id,"environment":a.environment,"public_key":a.public_key,"alias":a.alias,"state":"registered_identity_requires_current_session_check"})).collect();
        return crate::device_client::print(&views);
    }
    if args.len() != 9
        || args[0] != "dev-test-catalog-register"
        || args[3] != "--identity-config"
        || args[5] != "--operation"
        || args[7] != "--alias"
    {
        return Err("invalid_catalog_command".into());
    }
    let credential =
        std::fs::canonicalize(&args[4]).map_err(|_| "identity_configuration_unavailable")?;
    crate::runtime_broker::trusted_ancestors(&credential)?;
    let m =
        std::fs::symlink_metadata(&credential).map_err(|_| "identity_configuration_unavailable")?;
    if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err("private_identity_configuration_required".into());
    }
    if m.len() > 65536 {
        return Err("identity_configuration_too_large".into());
    }
    let config: crate::identity_client::IdentityClientConfig = serde_json::from_slice(
        &std::fs::read(&credential).map_err(|_| "identity_configuration_unavailable")?,
    )
    .map_err(|_| "invalid_identity_configuration")?;
    let environment = format!("{}:{}", config.identity_service_id, config.environment_id);
    if std::io::stdin().is_terminal() {
        return Err("trusted_private_stdin_required".into());
    }
    let mut unlock = zeroize::Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(1025)
        .read_to_end(&mut unlock)
        .map_err(|_| "private_unlock_unavailable")?;
    if !(12..=1024).contains(&unlock.len()) {
        return Err("private_unlock_required".into());
    }
    let client = crate::identity_client::IdentityClient::new(config, &unlock)?;
    let account_id = client.current_account(&args[6])?;
    let public_key = client.public_key();
    let key =
        serde_json::to_vec(&(&environment, &account_id)).map_err(|_| "account_binding_failed")?;
    let account_ref = blake3::hash(&key).to_hex().to_string();
    if let Some(existing) = accounts.iter_mut().find(|a| a.account_ref == account_ref) {
        if existing.public_key != public_key
            || existing.credential_ref != credential.to_string_lossy()
            || existing.login_operation != args[6]
        {
            return Err("credential_binding_conflict".into());
        }
        existing.alias = args[8].clone();
    } else {
        accounts.push(Account {
            account_ref: account_ref.clone(),
            account_id,
            public_key,
            environment,
            alias: args[8].clone(),
            credential_ref: credential.to_string_lossy().into(),
            login_operation: args[6].clone(),
        });
    }
    let temp = directory.join("catalog.pending");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temp)
        .map_err(|_| "catalog_write_failed")?;
    file.write_all(&serde_json::to_vec(&accounts).map_err(|_| "catalog_encoding_failed")?)
        .and_then(|_| file.sync_all())
        .map_err(|_| "catalog_write_failed")?;
    std::fs::rename(temp, path).map_err(|_| "catalog_write_failed")?;
    std::fs::File::open(directory)
        .and_then(|f| f.sync_all())
        .map_err(|_| "catalog_write_failed")?;
    crate::device_client::print(
        &serde_json::json!({"account_ref":account_ref,"state":"registered"}),
    )
}
