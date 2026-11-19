//! Read-only admission of the separate profile-3 clean-Cold CLI schema.
//! Existing profile-2 input is never promoted by accepting another number.
use super::{
    iws_live_limits::*,
    iws_live_work::{ColdWorkConfigV1, IoCeilingsV1},
};
use oasis7::world_service::client::FormalReadTrustConfigV1;
use oasis7_distfs::controlled_authority::{
    fencing::{RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1},
    formal::VerifiedFormalReadAnchorV1,
};
use oasis7_runtime::{ReleaseSecurityPolicy, WorldRuntimeMemoryLimits};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
pub(super) const MODE: &str = "iws_clean_local_v1";
pub(super) const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EndpointConfigV1 {
    pub endpoint_directory: PathBuf,
    pub pages_directory: PathBuf,
    pub provider_address: SocketAddr,
    pub provider_authentication_key_file: PathBuf,
    pub provider_policy_file: PathBuf,
}

/// Operator-pinned configuration DATA, not a deserializable trusted permit.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProviderPolicyDataV1 {
    provider_public_key: String,
    authorized_resource_bindings: Vec<String>,
}
impl ProviderPolicyDataV1 {
    fn build_provider(
        self,
        address: SocketAddr,
        timeout: Duration,
        authentication_key: ed25519_dalek::SigningKey,
    ) -> Result<(RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1), String> {
        let policy = TrustedFencingPolicyV1 {
            provider_public_key: self.provider_public_key,
            authorized_resource_bindings: self.authorized_resource_bindings,
        };
        // The original constructor checks canonical pins, a nonempty resource
        // set, loopback address and timeout before either owner is returned.
        // Actual provider evidence and lease validation still happen at acquire.
        let provider = RemoteMonotonicEndpointProvider::new(
            address,
            timeout,
            authentication_key,
            policy.clone(),
        )
        .map_err(|error| format!("IWS provider configuration: {error}"))?;
        Ok((provider, policy))
    }
}
#[cfg(any(test, feature = "iws-host-native-test-support"))]
impl From<&TrustedFencingPolicyV1> for ProviderPolicyDataV1 {
    fn from(policy: &TrustedFencingPolicyV1) -> Self {
        Self {
            provider_public_key: policy.provider_public_key.clone(),
            authorized_resource_bindings: policy.authorized_resource_bindings.clone(),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CheckpointLimitsDataV1 {
    pub max_projection_bytes: u64,
    pub max_mailbox_entries: u64,
    pub max_path_bytes: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IwsLiveConfigV1 {
    pub schema_version: u32,
    pub runtime_mode: String,
    pub node_id: String,
    pub player_id: String,
    pub node_tick_ms: u64,
    pub start_automatically: bool,
    pub node_keypair_directory: PathBuf,
    pub writer_key_file: PathBuf,
    pub formal_read_trust_file: PathBuf,
    pub writer_directory: PathBuf,
    pub checkpoint_directory: PathBuf,
    pub checkpoint_record: String,
    pub primary: EndpointConfigV1,
    pub replica: EndpointConfigV1,
    pub timeout_ms: u64,
    pub memory_bytes: usize,
    pub recovery_reserve_bytes: usize,
    pub common_steps: u64,
    pub common_read_bytes: u64,
    pub cli_steps: u64,
    pub cli_bytes: u64,
    pub node_queue_bytes: usize,
    pub node_committed_bytes: usize,
    pub node_committed_batches: usize,
    pub runtime_memory: WorldRuntimeMemoryLimits,
    pub worker: WorkerLimitsDataV1,
    pub current: CurrentLimitsDataV1,
    pub endpoint_budgets: EndpointLimitsDataV1,
    pub projection: ProjectionLimitsDataV1,
    pub transaction: TransactionLimitsDataV1,
    pub input: InputLimitsDataV1,
    pub actions: InputLimitsDataV1,
    pub max_action_bytes: usize,
    pub max_grant_payload_bytes: u64,
    pub checkpoint_limits: CheckpointLimitsDataV1,
    pub initial_cold: [ColdWorkConfigV1; 2],
    pub recovery_cold: [ColdWorkConfigV1; 2],
    pub runtime_work: RuntimeLimitsDataV1,
    pub normal_io: [IoCeilingsV1; 2],
    pub normal_sources: [IoCeilingsV1; 2],
    pub input_work: IoCeilingsV1,
    pub cognition_work: IoCeilingsV1,
    pub observation_work: IoCeilingsV1,
    pub max_request_bytes: usize,
    pub max_reply_bytes: usize,
}
pub(super) struct ValidatedIwsLiveConfigV1 {
    pub config: IwsLiveConfigV1,
    pub record: [u8; 32],
    pub anchor: VerifiedFormalReadAnchorV1,
    pub writer: ed25519_dalek::SigningKey,
    pub node_keypair: super::node_keypair_config::NodeKeypairConfig,
    pub providers: [Arc<RemoteMonotonicEndpointProvider>; 2],
    pub policies: [TrustedFencingPolicyV1; 2],
    pub process: ReleaseSecurityPolicy,
}
pub(super) fn private_config_bytes(path: &Path) -> Result<(PathBuf, Vec<u8>), String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    super::controlled_live_config::require_private_owned(&path, false)?;
    let bytes = super::controlled_history_cli::read_bounded(&path, MAX_CONFIG_BYTES)?;
    Ok((path, bytes))
}
fn resolve(base: &Path, path: &mut PathBuf, directory: bool) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("IWS configuration path is empty".into());
    }
    if !path.is_absolute() {
        *path = base.join(&*path)
    }
    if path.as_os_str().as_encoded_bytes().len() > MAX_PATH_BYTES {
        return Err("IWS configuration path bound".into());
    }
    super::controlled_live_config::require_private_owned(path, directory)?;
    *path = std::fs::canonicalize(&*path).map_err(|e| e.to_string())?;
    Ok(())
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
fn finite_numbers(value: &serde_json::Value) -> Result<(), String> {
    // Read-only schema parsing has a fixed 1 MiB ceiling. Prevent sentinel
    // unbounded accounts before any identity or endpoint is opened.
    match value {
        serde_json::Value::Number(v) if v.as_u64() == Some(u64::MAX) => {
            Err("IWS work must have finite non-sentinel limits".into())
        }
        serde_json::Value::Array(values) => {
            for v in values {
                finite_numbers(v)?;
            }
            Ok(())
        }
        serde_json::Value::Object(values) => {
            for v in values.values() {
                finite_numbers(v)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
impl IwsLiveConfigV1 {
    pub(super) fn load(path: &Path, bytes: &[u8]) -> Result<ValidatedIwsLiveConfigV1, String> {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| format!("IWS configuration JSON: {e}"))?;
        finite_numbers(&value)?;
        let mut config: Self =
            serde_json::from_value(value).map_err(|e| format!("IWS configuration: {e}"))?;
        if config.schema_version != 1
            || config.runtime_mode != MODE
            || !identifier(&config.node_id)
            || !identifier(&config.player_id)
            || !(1..=60_000).contains(&config.node_tick_ms)
            || !(1..=60_000).contains(&config.timeout_ms)
            || !(1..=16 * 1024 * 1024).contains(&config.max_request_bytes)
            || !(1..=64 * 1024 * 1024).contains(&config.max_reply_bytes)
            || config.memory_bytes == 0
            || config.recovery_reserve_bytes >= config.memory_bytes
            || config.runtime_memory.max_pending_actions == 0
            || config.node_queue_bytes == 0
            || config.node_committed_bytes == 0
            || config.node_committed_batches == 0
            || config.max_action_bytes == 0
            || config.checkpoint_limits.max_path_bytes > MAX_PATH_BYTES
            || config.max_action_bytes > config.max_request_bytes
        {
            return Err("unsupported IWS local configuration limits or identity".into());
        }
        let mut record = [0u8; 32];
        if config.checkpoint_record.len() != 64
            || !config
                .checkpoint_record
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || hex::decode_to_slice(&config.checkpoint_record, &mut record).is_err()
        {
            return Err("IWS checkpoint record must be lowercase 32-byte hex DATA".into());
        }
        let base = path
            .parent()
            .ok_or("IWS configuration directory unavailable")?;
        for p in [
            &mut config.node_keypair_directory,
            &mut config.writer_directory,
            &mut config.checkpoint_directory,
        ] {
            resolve(base, p, true)?;
        }
        for p in [
            &mut config.writer_key_file,
            &mut config.formal_read_trust_file,
        ] {
            resolve(base, p, false)?;
        }
        for endpoint in [&mut config.primary, &mut config.replica] {
            if !endpoint.provider_address.ip().is_loopback()
                || endpoint.provider_address.port() == 0
            {
                return Err("IWS monotonic provider must use an explicit loopback tunnel".into());
            }
            resolve(base, &mut endpoint.endpoint_directory, true)?;
            resolve(base, &mut endpoint.pages_directory, true)?;
            resolve(base, &mut endpoint.provider_authentication_key_file, false)?;
            resolve(base, &mut endpoint.provider_policy_file, false)?;
        }
        if config.primary.endpoint_directory == config.replica.endpoint_directory
            || config.primary.pages_directory == config.replica.pages_directory
        {
            return Err("IWS primary/replica namespaces must be distinct".into());
        }
        let (node_keypair, _) =
            super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
                &config.node_keypair_directory,
            )
            .map_err(|_| "IWS pre-existing Node keypair unavailable")?;
        let writer =
            super::controlled_live_config::read_existing_signing_key(&config.writer_key_file)?;
        let trust_raw = super::controlled_history_cli::read_bounded(
            &config.formal_read_trust_file,
            4 * 1024 * 1024,
        )?;
        let trust: FormalReadTrustConfigV1 = serde_json::from_slice(&trust_raw)
            .map_err(|e| format!("IWS externally pinned trust: {e}"))?;
        let anchor = trust.verified_anchor()?;
        if anchor.head().verified_record_id() != record {
            return Err(
                "IWS clean startup requires an independent pin of the exact selected record".into(),
            );
        }
        let definition = anchor.authority().definition();
        if definition.profile_version != 3
            || definition.predecessor_authority_id.is_some()
            || anchor.authority().durable_set_activation_digest().is_some()
            || definition.writer_public_key != hex::encode(writer.verifying_key().to_bytes())
        {
            return Err("IWS clean local startup requires the pinned profile-3 initial authority and original writer".into());
        }
        let open_provider = |e: &EndpointConfigV1| -> Result<_, String> {
            let key = super::controlled_live_config::read_existing_signing_key(
                &e.provider_authentication_key_file,
            )?;
            let raw =
                super::controlled_history_cli::read_bounded(&e.provider_policy_file, 64 * 1024)?;
            let policy: ProviderPolicyDataV1 =
                serde_json::from_slice(&raw).map_err(|v| format!("IWS provider policy: {v}"))?;
            let (provider, policy) = policy.build_provider(
                e.provider_address,
                Duration::from_millis(config.timeout_ms),
                key,
            )?;
            Ok((Arc::new(provider), policy))
        };
        let (primary, primary_policy) = open_provider(&config.primary)?;
        let (replica, replica_policy) = open_provider(&config.replica)?;
        Ok(ValidatedIwsLiveConfigV1 {
            config,
            record,
            anchor,
            writer,
            node_keypair,
            providers: [primary, replica],
            policies: [primary_policy, replica_policy],
            process: ReleaseSecurityPolicy::production_hardened(),
        })
    }
}
pub(super) fn acquire_identity(
    validated: &ValidatedIwsLiveConfigV1,
) -> Result<super::node_keypair_config::RuntimeIdentityLease, String> {
    use super::node_keypair_config::{RuntimeIdentityBindingV1, acquire_runtime_identity_lease};
    let config = &validated.config;
    let key_path = config
        .node_keypair_directory
        .join(super::node_keypair_config::PROVISIONED_NODE_KEYPAIR_FILE);
    super::controlled_live_config::require_private_owned(&key_path, false)?;
    let key_path = std::fs::canonicalize(key_path).map_err(|e| e.to_string())?;
    let derived = super::derive_node_libp2p_identity_keypair_config(
        &config.node_id,
        &validated.node_keypair,
    )?;
    let peer_id = oasis7_node::derive_libp2p_identity_keypair(&derived.private_key_hex)
        .map_err(|e| format!("IWS Node identity: {e:?}"))?
        .public()
        .to_peer_id()
        .to_string();
    let world = &validated.anchor.head().record().world;
    let binding = RuntimeIdentityBindingV1 {
        version: 1,
        key_path: key_path.clone(),
        data_directory: config.writer_directory.clone(),
        node_id: config.node_id.clone(),
        root_public_key: validated.node_keypair.public_key_hex.clone(),
        peer_id,
        world_id: world.world_id.clone(),
        chain_id: world.chain_id.clone(),
        genesis_digest: world.genesis_digest.clone(),
        profile: "iws-clean-local-v1".into(),
    };
    let lease = acquire_runtime_identity_lease(&key_path, &binding)?;
    let (current, _) = super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(
        &config.node_keypair_directory,
    )
    .map_err(|_| "IWS Node identity reread after lease failed")?;
    if current.private_key_hex != validated.node_keypair.private_key_hex
        || current.public_key_hex != validated.node_keypair.public_key_hex
    {
        return Err("IWS Node key changed during admission".into());
    }
    Ok(lease)
}

#[cfg(test)]
mod policy_data_tests {
    use super::*;

    fn original() -> TrustedFencingPolicyV1 {
        TrustedFencingPolicyV1 {
            provider_public_key: "ab".repeat(32),
            authorized_resource_bindings: vec!["02".repeat(32), "01".repeat(32), "02".repeat(32)],
        }
    }
    fn build(
        data: ProviderPolicyDataV1,
    ) -> Result<(RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1), String> {
        data.build_provider(
            "127.0.0.1:9".parse().unwrap(),
            Duration::from_millis(10),
            ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
        )
    }

    #[test]
    fn provider_policy_data_preserves_exact_operator_pins() {
        let expected = original();
        let wire = serde_json::to_vec(&ProviderPolicyDataV1::from(&expected)).unwrap();
        let data: ProviderPolicyDataV1 = serde_json::from_slice(&wire).unwrap();
        let (_provider, actual) = build(data).unwrap();
        assert_eq!(actual.provider_public_key, expected.provider_public_key);
        assert_eq!(
            actual.authorized_resource_bindings,
            expected.authorized_resource_bindings
        );
    }

    #[test]
    fn provider_policy_data_rejects_unknown_or_missing_fields() {
        let original = serde_json::to_value(ProviderPolicyDataV1::from(&original())).unwrap();
        for removed in ["provider_public_key", "authorized_resource_bindings"] {
            let mut value = original.clone();
            value.as_object_mut().unwrap().remove(removed);
            assert!(serde_json::from_value::<ProviderPolicyDataV1>(value).is_err());
        }
        let mut unknown = original;
        unknown["unchecked"] = true.into();
        assert!(serde_json::from_value::<ProviderPolicyDataV1>(unknown).is_err());
    }

    #[test]
    fn provider_policy_data_keeps_original_constructor_rejections() {
        for case in 0..4 {
            let mut data = ProviderPolicyDataV1::from(&original());
            match case {
                0 => data.provider_public_key = "AB".repeat(32),
                1 => data.provider_public_key = "ab".repeat(31),
                2 => data.authorized_resource_bindings.clear(),
                _ => data.authorized_resource_bindings[0] = "not-a-resource-digest".into(),
            }
            assert!(build(data).is_err());
        }
        for (address, timeout) in [
            ("192.0.2.1:9", Duration::from_millis(10)),
            ("127.0.0.1:9", Duration::ZERO),
            ("127.0.0.1:9", Duration::from_secs(61)),
        ] {
            assert!(
                ProviderPolicyDataV1::from(&original())
                    .build_provider(
                        address.parse().unwrap(),
                        timeout,
                        ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
                    )
                    .is_err()
            );
        }
    }
}
