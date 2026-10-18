//! Explicit engineering startup policy. Configuration is caller trust input,
//! never evidence of issuer authorization or independent hardware topology.
#[cfg(not(test))]
use super::execution_bridge::controlled_bootstrap_anchor::VerifiedBootstrapAnchor;
#[cfg(test)]
use super::execution_bridge_real_tests::real_execution_bridge::controlled_bootstrap_anchor::VerifiedBootstrapAnchor;
use oasis7_distfs::controlled_authority::{
    activation::InitialActivationPolicy, replicated_protocol::HeadAnchor,
};
use oasis7_runtime::{ReleaseSecurityPolicy, blake3_hex};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuardedRemoteEndpoints {
    pub primary: std::net::SocketAddr,
    pub replica: std::net::SocketAddr,
    pub timeout_ms: u64,
}

pub(crate) const ENGINEERING_SCOPE: &str = "guarded_local_execution_prerequisite";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuardedConfiguration {
    pub schema_version: u32,
    pub scope: String,
    pub node_id: String,
    pub node_keypair_directory: PathBuf,
    pub records_directory: PathBuf,
    pub status_bind: std::net::SocketAddr,
    pub node_tick_ms: u64,
    pub trusted_bootstrap_config: PathBuf,
    pub activation_evidence: PathBuf,
    #[serde(default)]
    pub primary_directory: PathBuf,
    #[serde(default)]
    pub replica_directory: PathBuf,
    pub writer_key_file: PathBuf,
    #[serde(default)]
    pub primary_key_file: PathBuf,
    #[serde(default)]
    pub replica_key_file: PathBuf,
    pub minimum_runtime_head: HeadAnchor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_endpoints: Option<GuardedRemoteEndpoints>,
}
pub(crate) struct GuardedAuthority {
    pub config: GuardedConfiguration,
    pub anchor: VerifiedBootstrapAnchor,
    pub policy: InitialActivationPolicy,
    pub release_policy: ReleaseSecurityPolicy,
    pub configuration_digest: String,
    pub frozen_execution_digest: String,
    pub execution_policy: super::controlled_execution_policy::GuardedExecutionPolicy,
}
impl GuardedAuthority {
    pub(crate) fn load(path: &Path) -> Result<Self, String> {
        let bytes = super::controlled_history_cli::read_bounded(path, 64 * 1024)?;
        let mut config: GuardedConfiguration =
            serde_json::from_slice(&bytes).map_err(|e| format!("guarded configuration: {e}"))?;
        if !matches!(config.schema_version, 1 | 2)
            || config.scope != ENGINEERING_SCOPE
            || config.node_id.trim().is_empty()
            || !config.status_bind.ip().is_loopback()
            || !(1..=60_000).contains(&config.node_tick_ms)
        {
            return Err("unsupported guarded engineering configuration".into());
        }
        let parent = path
            .parent()
            .ok_or("guarded configuration parent unavailable")?;
        let remote = config.remote_endpoints.as_ref();
        if let Some(remote) = remote
            && (config.schema_version != 2
                || !remote.primary.ip().is_loopback()
                || !remote.replica.ip().is_loopback()
                || remote.primary == remote.replica
                || !(1..=60_000).contains(&remote.timeout_ms)
                || [
                    &config.primary_directory,
                    &config.replica_directory,
                    &config.primary_key_file,
                    &config.replica_key_file,
                ]
                .iter()
                .any(|p| !p.as_os_str().is_empty()))
        {
            return Err("remote guarded endpoints require v2, distinct loopback tunnels and no local endpoint paths/keys".into());
        }
        let mut paths = vec![
            &mut config.node_keypair_directory,
            &mut config.records_directory,
            &mut config.trusted_bootstrap_config,
            &mut config.activation_evidence,
            &mut config.writer_key_file,
        ];
        if remote.is_none() {
            paths.extend([
                &mut config.primary_directory,
                &mut config.replica_directory,
                &mut config.primary_key_file,
                &mut config.replica_key_file,
            ]);
        }
        for p in paths {
            if p.as_os_str().is_empty() {
                return Err("empty guarded path".into());
            }
            if !p.is_absolute() {
                *p = parent.join(&*p);
            }
            if p.to_string_lossy().len() > 4096 {
                return Err("invalid guarded path".into());
            }
        }
        let (anchor, policy, release_policy) =
            super::controlled_bootstrap_cli::load_verified_bootstrap(
                &config.trusted_bootstrap_config,
                &config.activation_evidence,
            )?;
        let activation_head = anchor.activation().qualified_head();
        if config.minimum_runtime_head.position < activation_head.position
            || (config.minimum_runtime_head.position == activation_head.position
                && &config.minimum_runtime_head != activation_head)
        {
            return Err(
                "guarded minimum head precedes or conflicts with original activation".into(),
            );
        }
        let execution_policy =
            super::controlled_execution_policy::GuardedExecutionPolicy::current();
        let configuration_digest = if config.schema_version == 1 {
            // Preserve already persisted v1 continuation identities exactly.
            blake3_hex(
                &serde_json::to_vec(&(
                    &config,
                    anchor.activation().envelope_digest(),
                    &release_policy,
                ))
                .map_err(|e| e.to_string())?,
            )
        } else {
            blake3_hex(
                &serde_json::to_vec(&(
                    "oasis7.guarded_execution_binding.v2",
                    &config.scope,
                    &config.node_id,
                    config.node_tick_ms,
                    anchor.activation().envelope_digest(),
                    &release_policy,
                    &execution_policy,
                ))
                .map_err(|e| e.to_string())?,
            )
        };
        let frozen_execution_digest = if config.schema_version == 2 {
            blake3_hex(
                &serde_json::to_vec(&(
                    &configuration_digest,
                    super::controlled_execution_policy::executable_digest()?,
                    &execution_policy,
                ))
                .map_err(|e| e.to_string())?,
            )
        } else {
            configuration_digest.clone()
        };
        Ok(Self {
            config,
            anchor,
            policy,
            release_policy,
            configuration_digest,
            frozen_execution_digest,
            execution_policy,
        })
    }
}

pub(crate) fn read_existing_signing_key(path: &Path) -> Result<ed25519_dalek::SigningKey, String> {
    require_private_owned(path, false)?;
    let bytes = super::controlled_history_cli::read_bounded(path, 128)?;
    let encoded =
        std::str::from_utf8(&bytes).map_err(|_| "invalid existing guarded signing key encoding")?;
    let decoded =
        hex::decode(encoded.trim()).map_err(|_| "invalid existing guarded signing key encoding")?;
    let secret: [u8; 32] = decoded
        .try_into()
        .map_err(|_| "invalid existing guarded signing key length")?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&secret))
}

/// Process-local publication fence. It is advanced only after independently
/// verified immutable publication; readers cannot derive it from a latest file.
#[derive(Clone, Debug)]
pub(crate) struct GuardedReadAuthority {
    configuration_digest: String,
    head: std::sync::Arc<
        std::sync::RwLock<oasis7_distfs::controlled_authority::replicated_protocol::HeadAnchor>,
    >,
}
impl GuardedReadAuthority {
    pub(crate) fn from_verified(
        head: oasis7_distfs::controlled_authority::replicated_protocol::HeadAnchor,
        configuration_digest: String,
    ) -> Self {
        Self {
            configuration_digest,
            head: std::sync::Arc::new(std::sync::RwLock::new(head)),
        }
    }
    pub(crate) fn check_configuration(&self, expected: &str) -> Result<(), String> {
        if self.configuration_digest != expected {
            return Err("guarded read configuration changed".into());
        }
        Ok(())
    }
    pub(crate) fn current(
        &self,
    ) -> Result<oasis7_distfs::controlled_authority::replicated_protocol::HeadAnchor, String> {
        self.head
            .read()
            .map(|h| h.clone())
            .map_err(|_| "guarded publication fence unavailable".into())
    }
    pub(crate) fn publish_verified(
        &self,
        next: oasis7_distfs::controlled_authority::replicated_protocol::HeadAnchor,
    ) -> Result<(), String> {
        let mut head = self
            .head
            .write()
            .map_err(|_| "guarded publication fence unavailable")?;
        if next.position < head.position || (next.position == head.position && next != *head) {
            return Err("guarded publication fence cannot regress or change".into());
        }
        *head = next;
        Ok(())
    }
}

impl PartialEq for GuardedReadAuthority {
    fn eq(&self, other: &Self) -> bool {
        self.configuration_digest == other.configuration_digest
            && std::sync::Arc::ptr_eq(&self.head, &other.head)
    }
}
impl Eq for GuardedReadAuthority {}

#[cfg(unix)]
pub(crate) fn require_private_owned(path: &Path, directory: bool) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    for component in path.ancestors() {
        let metadata = std::fs::symlink_metadata(component).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("guarded private input symlink component refused".into());
        }
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    // SAFETY: geteuid takes no arguments and reads the process identity only.
    let current_uid = unsafe { libc::geteuid() };
    let expected_mode = if directory { 0o700 } else { 0o600 };
    if metadata.file_type().is_symlink()
        || metadata.uid() != current_uid
        || metadata.permissions().mode() & 0o777 != expected_mode
        || (directory && !metadata.is_dir())
        || (!directory && !metadata.is_file())
    {
        return Err("guarded storage/key input requires an owned private regular path".into());
    }
    Ok(())
}
#[cfg(not(unix))]
pub(crate) fn require_private_owned(_: &Path, _: bool) -> Result<(), String> {
    Err("guarded private key/storage ownership requires Unix".into())
}
