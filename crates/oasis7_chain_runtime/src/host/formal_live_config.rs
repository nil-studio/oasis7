//! Read-only boundary for the production formal writer configuration.
//!
//! This loader consumes pre-existing operator inputs only. It does not provision
//! identities, create storage, discover trust from an endpoint, or promote the
//! engineering configuration into a formal profile.
use ed25519_dalek::SigningKey;
use oasis7::world_service::client::FormalReadTrustConfigV1;
use oasis7_distfs::controlled_authority::formal::{
    FormalCompletionBranchV1, FormalQualifiedCommitProofV1, FormalRecordKindV1,
    VerifiedFormalReadAnchorV1, endpoint::FormalCandidateMaterialV1, verify_formal_candidate,
    verify_formal_qualified, verify_record_material,
};
use oasis7_runtime::ReleaseSecurityPolicy;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf, time::Duration};

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_TRUST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_NODE_KEYPAIR_BYTES: u64 = 16 * 1024;
const MAX_INITIAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;
const MAX_NODE_ID_BYTES: usize = 256;

/// Exact schema-1 configuration. The names and required fields are intentionally
/// separate from `GuardedConfiguration`; legacy engineering files cannot be
/// interpreted as formal production configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalLiveConfigV1 {
    pub schema_version: u32,
    pub node_id: String,
    pub node_keypair_directory: PathBuf,
    pub writer_key_file: PathBuf,
    pub formal_read_trust_file: PathBuf,
    pub writer_directory: PathBuf,
    pub primary: SocketAddr,
    pub replica: SocketAddr,
    pub timeout_ms: u64,
    pub status_bind: SocketAddr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_initial_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_permission_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_issuer_minimum_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_origin_history_directory: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_delivery: Option<FormalEffectDeliveryConfigV1>,
}

/// Routes are usable only when their complete contract digest is already
/// selected by an effect in a qualified outbox. Loading never sends effects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalEffectDeliveryConfigV1 {
    pub routes: Vec<super::formal_effect_transport::FormalEffectReceiverContractV1>,
}

/// Complete, typed inputs for the formal runtime. Paths have been resolved
/// against the config file directory; no path contributes to the World epoch.
pub(super) struct ValidatedFormalLiveConfigV1 {
    pub config: FormalLiveConfigV1,
    pub node_keypair: super::node_keypair_config::NodeKeypairConfig,
    pub writer_key: SigningKey,
    pub trust: FormalReadTrustConfigV1,
    pub anchor: VerifiedFormalReadAnchorV1,
    pub approved_initial: Option<ApprovedInitialPublicationV1>,
    pub recovery: Option<oasis7_distfs::controlled_authority::formal::VerifiedRecoveryBootstrapV1>,
    pub release_policy: ReleaseSecurityPolicy,
    pub timeout: Duration,
}

/// The same complete initial publication envelope persisted by the formal
/// writer: an initial candidate closure plus its independently signed proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ApprovedInitialPublicationV1 {
    pub schema_version: u32,
    pub candidate: FormalCandidateMaterialV1,
    pub proof: FormalQualifiedCommitProofV1,
}

impl FormalLiveConfigV1 {
    /// Validate pre-existing private inputs and the externally pinned trust
    /// checkpoint. This function is deliberately read-only.
    pub(super) fn load(path: &std::path::Path) -> Result<ValidatedFormalLiveConfigV1, String> {
        let config_path = absolute_path(path)?;
        super::controlled_live_config::require_private_owned(&config_path, false)?;
        let bytes = super::controlled_history_cli::read_bounded(&config_path, MAX_CONFIG_BYTES)?;
        let mut config: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("formal live configuration: {error}"))?;
        if let Some(delivery) = &config.effect_delivery {
            super::formal_effect_transport::validate_receiver_contracts(&delivery.routes)?;
        }
        if config.schema_version != 1
            || config.node_id.trim().is_empty()
            || config.node_id.len() > MAX_NODE_ID_BYTES
            || config.node_id.chars().any(char::is_control)
            || !config.status_bind.ip().is_loopback()
            || config.status_bind.port() == 0
            || !config.primary.ip().is_loopback()
            || !config.replica.ip().is_loopback()
            || config.primary.port() == 0
            || config.replica.port() == 0
            || config.primary == config.replica
            || config.status_bind == config.primary
            || config.status_bind == config.replica
            || !(1..=60_000).contains(&config.timeout_ms)
        {
            return Err(
                "unsupported formal live schema, identity, loopback address, or timeout".into(),
            );
        }

        let config_directory = config_path
            .parent()
            .ok_or("formal live configuration directory unavailable")?;
        config.node_keypair_directory = resolve_path(
            config_directory,
            &config.node_keypair_directory,
            "node keypair directory",
        )?;
        config.writer_key_file =
            resolve_path(config_directory, &config.writer_key_file, "writer key file")?;
        config.formal_read_trust_file = resolve_path(
            config_directory,
            &config.formal_read_trust_file,
            "external formal-read trust file",
        )?;
        config.writer_directory = resolve_path(
            config_directory,
            &config.writer_directory,
            "formal writer directory",
        )?;
        config.approved_initial_file = config
            .approved_initial_file
            .as_deref()
            .map(|initial| resolve_path(config_directory, initial, "approved initial file"))
            .transpose()?;

        config.recovery_permission_file = config
            .recovery_permission_file
            .as_deref()
            .map(|path| resolve_path(config_directory, path, "formal recovery permission file"))
            .transpose()?;
        config.recovery_issuer_minimum_file = config
            .recovery_issuer_minimum_file
            .as_deref()
            .map(|path| {
                resolve_path(
                    config_directory,
                    path,
                    "formal recovery issuer minimum file",
                )
            })
            .transpose()?;
        config.recovery_origin_history_directory = config
            .recovery_origin_history_directory
            .as_deref()
            .map(|path| resolve_path(config_directory, path, "recovery origin history directory"))
            .transpose()?;
        if config.recovery_origin_history_directory.is_some()
            && config.recovery_permission_file.is_none()
        {
            return Err("recovery origin history requires a recovery permission".into());
        }
        if config.recovery_permission_file.is_some()
            != config.recovery_issuer_minimum_file.is_some()
        {
            return Err(
                "formal recovery permission requires a separately provisioned issuer minimum file"
                    .into(),
            );
        }
        if config.recovery_permission_file.is_some() && config.approved_initial_file.is_some() {
            return Err("recovery cannot manufacture an initial publication".into());
        }
        super::controlled_live_config::require_private_owned(&config.node_keypair_directory, true)?;
        super::controlled_live_config::require_private_owned(&config.writer_directory, true)?;
        let node_keypair_file = config
            .node_keypair_directory
            .join(super::node_keypair_config::PROVISIONED_NODE_KEYPAIR_FILE);
        super::controlled_live_config::require_private_owned(&node_keypair_file, false)?;
        let node_keypair_bytes = super::controlled_history_cli::read_bounded(
            &node_keypair_file,
            MAX_NODE_KEYPAIR_BYTES,
        )?;
        let node_keypair = parse_node_keypair(&node_keypair_bytes)?;

        let writer_key =
            super::controlled_live_config::read_existing_signing_key(&config.writer_key_file)?;
        super::controlled_live_config::require_private_owned(
            &config.formal_read_trust_file,
            false,
        )?;
        let trust_bytes = super::controlled_history_cli::read_bounded(
            &config.formal_read_trust_file,
            MAX_TRUST_BYTES,
        )?;
        let trust: FormalReadTrustConfigV1 = serde_json::from_slice(&trust_bytes)
            .map_err(|error| format!("formal read trust configuration: {error}"))?;
        let computed_checkpoint_digest = trust
            .checkpoint
            .digest()
            .map_err(|error| format!("formal read checkpoint digest: {error}"))?;
        if computed_checkpoint_digest != trust.checkpoint_digest {
            return Err("externally pinned formal checkpoint mismatch".into());
        }
        if trust.checkpoint.active_authority.profile_version != 2 {
            return Err(
                "unsupported formal authority profile version: production requires profile_version 2".into(),
            );
        }
        let anchor = trust.verified_anchor()?;
        if anchor.authority().definition().profile_version != 2 {
            return Err(
                "unsupported formal authority profile version: production requires profile_version 2".into(),
            );
        }
        let recovery = config
            .recovery_permission_file
            .as_deref()
            .map(|path| {
                recovery_config::load_recovery_permission(
                    path,
                    config
                        .recovery_issuer_minimum_file
                        .as_deref()
                        .ok_or("recovery issuer minimum file missing")?,
                    config.recovery_origin_history_directory.as_deref(),
                    &trust,
                    &anchor,
                )
            })
            .transpose()?;
        let active = recovery
            .as_ref()
            .map_or(anchor.authority(), |context| context.recovery().successor());
        if hex::encode(writer_key.verifying_key().to_bytes())
            != active.definition().writer_public_key
        {
            return Err("formal writer key differs from externally pinned active authority".into());
        }

        let approved_initial = config
            .approved_initial_file
            .as_deref()
            .map(|initial_path| load_approved_initial(initial_path, &anchor))
            .transpose()?;

        Ok(ValidatedFormalLiveConfigV1 {
            timeout: Duration::from_millis(config.timeout_ms),
            config,
            node_keypair,
            writer_key,
            trust,
            anchor,
            approved_initial,
            recovery,
            release_policy: ReleaseSecurityPolicy::production_hardened(),
        })
    }
}

impl ValidatedFormalLiveConfigV1 {
    /// The caller supplies the digest computed from the loaded executable and
    /// the runtime's actual current policy after constructing that policy.
    pub(super) fn require_execution_policy_digest(&self, actual: &str) -> Result<(), String> {
        if actual != self.anchor.head().record().execution_policy_digest {
            return Err(
                "formal runtime execution policy differs from externally pinned anchor".into(),
            );
        }
        Ok(())
    }
}

fn load_approved_initial(
    path: &std::path::Path,
    anchor: &VerifiedFormalReadAnchorV1,
) -> Result<ApprovedInitialPublicationV1, String> {
    super::controlled_live_config::require_private_owned(path, false)?;
    let bytes = super::controlled_history_cli::read_bounded(path, MAX_INITIAL_BYTES)?;
    oasis7_runtime::World::validate_offline_cbor_budget(
        &bytes,
        oasis7_runtime::ObserverReadLimits::default(),
    )
    .map_err(|error| format!("approved initial publication budget: {error:?}"))?;
    let publication: ApprovedInitialPublicationV1 = serde_cbor::from_slice(&bytes)
        .map_err(|error| format!("approved initial publication: {error}"))?;
    if publication.schema_version != 1
        || anchor.head().record().kind != FormalRecordKindV1::InitialActivation
        || anchor.head().record().position != 1
        || publication.candidate.candidate.record != *anchor.head().record()
        || publication.candidate.branch != FormalCompletionBranchV1::Normal
        || publication.candidate.frozen.is_some()
        || publication.candidate.frozen_material.is_some()
        || publication.proof.decision.record != publication.candidate.candidate.record
        || publication.proof.decision.writer_signature_hex
            != publication.candidate.candidate.writer_signature_hex
        || publication.proof.decision.branch != FormalCompletionBranchV1::Normal
        || publication.proof.decision.frozen.is_some()
    {
        return Err(
            "approved initial publication differs from exact pinned initial activation".into(),
        );
    }
    verify_formal_candidate(
        &publication.candidate.candidate,
        None,
        anchor.authority(),
        None,
        None,
        FormalCompletionBranchV1::Normal,
    )
    .map_err(|error| format!("approved initial candidate: {error}"))?;
    verify_record_material(
        &publication.candidate.candidate.record,
        &publication.candidate.material,
    )
    .map_err(|error| format!("approved initial candidate closure: {error}"))?;
    let verified = verify_formal_qualified(&publication.proof, anchor.authority(), None, None)
        .map_err(|error| format!("approved initial proof: {error}"))?;
    if verified.head() != anchor.head().head() {
        return Err("approved initial proof differs from externally pinned checkpoint".into());
    }
    Ok(publication)
}

fn absolute_path(path: &std::path::Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    validate_path_length(&absolute, "configuration path")?;
    Ok(absolute)
}

fn resolve_path(
    base: &std::path::Path,
    path: &std::path::Path,
    description: &str,
) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err(format!("{description} path is empty"));
    }
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    validate_path_length(&resolved, description)?;
    Ok(resolved)
}

fn validate_path_length(path: &std::path::Path, description: &str) -> Result<(), String> {
    if path.as_os_str().to_string_lossy().len() > MAX_PATH_BYTES {
        return Err(format!("{description} exceeds path length bound"));
    }
    Ok(())
}

fn parse_node_keypair(
    bytes: &[u8],
) -> Result<super::node_keypair_config::NodeKeypairConfig, String> {
    let content = std::str::from_utf8(bytes).map_err(|_| "node keypair TOML is not UTF-8")?;
    let table: toml::Value =
        toml::from_str(content).map_err(|error| format!("node keypair TOML: {error}"))?;
    let node = table
        .get(super::node_keypair_config::NODE_TABLE_KEY)
        .and_then(toml::Value::as_table)
        .ok_or("node keypair TOML missing [node] table")?;
    let private_key_hex = node
        .get(super::node_keypair_config::NODE_PRIVATE_KEY_FIELD)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("node keypair TOML missing node.private_key")?;
    let public_key_hex = node
        .get(super::node_keypair_config::NODE_PUBLIC_KEY_FIELD)
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("node keypair TOML missing node.public_key")?;
    let private_bytes =
        hex::decode(private_key_hex).map_err(|_| "node.private_key must be valid hex")?;
    let private_bytes: [u8; 32] = private_bytes
        .try_into()
        .map_err(|_| "node.private_key must be 32-byte hex")?;
    let signing_key = SigningKey::from_bytes(&private_bytes);
    if hex::encode(signing_key.verifying_key().to_bytes()) != public_key_hex {
        return Err("node.public_key does not match node.private_key".into());
    }
    Ok(super::node_keypair_config::NodeKeypairConfig {
        private_key_hex: private_key_hex.to_owned(),
        public_key_hex: public_key_hex.to_owned(),
    })
}

#[cfg(all(test, unix))]
#[path = "formal_live_config_tests.rs"]
mod tests;

#[path = "formal_live_recovery_config.rs"]
mod recovery_config;
impl ValidatedFormalLiveConfigV1 {
    pub(super) fn active_authority(
        &self,
    ) -> &oasis7_distfs::controlled_authority::formal::VerifiedAuthorityV1 {
        self.recovery
            .as_ref()
            .map_or(self.anchor.authority(), |context| {
                context.recovery().successor()
            })
    }
}
