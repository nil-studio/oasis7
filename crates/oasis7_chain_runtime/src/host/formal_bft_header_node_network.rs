//! Explicit production wiring; missing issuer, witness, clock or leases fail closed.
use super::*;
use crate::formal_live_service::full_sync_read_gate::{
    FullSyncV3ReadGate, fresh_provider_incarnation,
    remote_evidence::RemoteFullSyncReadEvidenceSourceV3,
};
use ed25519_dalek::Signer;
use oasis7_client_api::world_service_full_sync::FormalNodeReadGrantV1;
use oasis7_distfs::controlled_authority::formal::full_sync_issuer::{
    FullSyncIssuerAnchorV3, SignedFullSyncIssuerPolicyV2, SignedFullSyncIssuerPolicyV3,
};
use oasis7_net::{Libp2pNetwork, Libp2pNetworkConfig};
use oasis7_node::{
    NodeBftCommittedReadCredentialsV3, NodeBftLibp2pPeerTransportV3,
    NodeBftLibp2pPrivateCandidateExchangeV3, NodeBftPeerExecutionStateV3,
    NodeBftPrivateExchangeConfigV3, NodeBftSharedExecutionHookV3,
};
use std::sync::Mutex;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    noise_signing_key_path: PathBuf,
    read_provider_signing_key_path: PathBuf,
    listen_addrs: Vec<String>,
    bootstrap_peers: Vec<String>,
    peers: Vec<String>,
    signed_v2_policy: SignedFullSyncIssuerPolicyV2,
    signed_v3_policy: SignedFullSyncIssuerPolicyV3,
    minimum_anchor: FullSyncIssuerAnchorV3,
    evidence_endpoint: String,
    candidate_grant_generation: u64,
    private_operation_timeout_ms: u64,
    committed_reader: Option<ReaderConfig>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReaderConfig {
    signing_key_path: PathBuf,
    grant_path: PathBuf,
}
struct Reader {
    key: ed25519_dalek::SigningKey,
    grant: FormalNodeReadGrantV1,
}
impl NodeBftCommittedReadCredentialsV3 for Reader {
    fn grant(&self) -> FormalNodeReadGrantV1 {
        self.grant.clone()
    }
    fn sign_request(&self, bytes: &[u8]) -> Result<String, String> {
        Ok(hex::encode(self.key.sign(bytes).to_bytes()))
    }
}
fn key(path: &Path) -> Result<ed25519_dalek::SigningKey, String> {
    if !path.is_absolute() {
        return Err("network key path must be absolute".into());
    }
    crate::controlled_live_config::read_existing_signing_key(path)
}
pub(super) fn configure(
    runtime: NodeRuntime,
    driver: FormalBftHeaderNodeDriverV3,
    binding: BftSignerBindingV3,
    provider: Arc<RemoteBftSignerProviderV3>,
    config: &Config,
    trust_path: &Path,
) -> Result<NodeRuntime, String> {
    if config.listen_addrs.is_empty()
        || config.listen_addrs.len() > 8
        || config.peers.is_empty()
        || config.peers.len() > 256
        || config.bootstrap_peers.len() > 256
        || !(1..=600_000).contains(&config.private_operation_timeout_ms)
        || config.candidate_grant_generation == 0
    {
        return Err("invalid explicit header Node peer limits".into());
    }
    let listen_addrs = config
        .listen_addrs
        .iter()
        .map(|v| v.parse())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "header Node listen address rejected")?;
    let bootstrap_peers = config
        .bootstrap_peers
        .iter()
        .map(|v| v.parse())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "header Node bootstrap address rejected")?;
    let trust: oasis7::world_service::client::FormalReadTrustConfigV1 =
        serde_json::from_slice(&read_private(trust_path, 2 * 1024 * 1024)?)
            .map_err(|_| "header Node external read trust rejected")?;
    trust.verified_anchor()?;
    let read_provider_key = key(&config.read_provider_signing_key_path)?;
    let verified_v2 = oasis7_distfs::controlled_authority::formal::full_sync_issuer::VerifiedFullSyncIssuerPolicyV2::verify(&config.signed_v2_policy,&trust.initial_trust)
        .map_err(|_| "header Node V2 issuer policy rejected")?;
    let verified_v3 = oasis7_distfs::controlled_authority::formal::full_sync_issuer::VerifiedFullSyncIssuerPolicyV3::verify(&config.signed_v3_policy,&trust.initial_trust,&verified_v2)
        .map_err(|_| "header Node V3 issuer policy rejected")?;
    let source = RemoteFullSyncReadEvidenceSourceV3::new(&config.evidence_endpoint)?
        .with_provider_lease_requests(verified_v2, verified_v3, read_provider_key.clone())?;
    let gate = Arc::new(FullSyncV3ReadGate::new(
        trust.initial_trust,
        config.signed_v2_policy.clone(),
        config.signed_v3_policy.clone(),
        config.minimum_anchor.clone(),
        hex::encode(read_provider_key.verifying_key().to_bytes()),
        fresh_provider_incarnation()?,
        Arc::new(source),
    )?);
    let reader = config
        .committed_reader
        .as_ref()
        .map(
            |r| -> Result<Arc<dyn NodeBftCommittedReadCredentialsV3>, String> {
                if !r.grant_path.is_absolute() {
                    return Err("committed reader grant path must be absolute".into());
                }
                crate::controlled_live_config::require_private_owned(&r.grant_path, false)?;
                let grant: FormalNodeReadGrantV1 =
                    serde_json::from_slice(&read_private(&r.grant_path, 64 * 1024)?)
                        .map_err(|_| "committed reader grant rejected")?;
                let key = key(&r.signing_key_path)?;
                if grant.body.node_public_key != hex::encode(key.verifying_key().to_bytes()) {
                    return Err("committed reader key differs from grant subject".into());
                }
                Ok(Arc::new(Reader { key, grant }) as Arc<dyn NodeBftCommittedReadCredentialsV3>)
            },
        )
        .transpose()?;
    let mut noise_secret = key(&config.noise_signing_key_path)?.to_bytes();
    let network_config = Libp2pNetworkConfig {
        listen_addrs,
        bootstrap_peers,
        enable_autonat: false,
        ..Default::default()
    }
    .with_ed25519_identity(&mut noise_secret)
    .map_err(|_| "header Node Noise identity rejected")?;
    let net = Arc::new(Libp2pNetwork::new(network_config));
    let context =
        oasis7_consensus::tendermint_v3::BftPeerContextV3::from_handoff(binding.handoff());
    let public = Arc::new(NodeBftLibp2pPeerTransportV3::new(
        net.clone(),
        context,
        config.peers.clone(),
    )?);
    let state = Arc::new(NodeBftPeerExecutionStateV3::default());
    let hook: NodeBftSharedExecutionHookV3 = Arc::new(Mutex::new(Box::new(driver)));
    let private = Arc::new(NodeBftLibp2pPrivateCandidateExchangeV3::new(
        net,
        NodeBftPrivateExchangeConfigV3 {
            binding: binding.clone(),
            candidate_policy_digest: config.minimum_anchor.policy_digest.clone(),
            candidate_grant_generation: config.candidate_grant_generation,
            peers: config.peers.clone(),
            operation_timeout: Duration::from_millis(config.private_operation_timeout_ms),
        },
        state.clone(),
        hook.clone(),
        gate,
        reader,
    )?);
    runtime
        .try_with_shared_execution_hook(hook)
        .and_then(|r| r.with_tendermint_v3_network(binding, provider, public, private, state))
        .map_err(|_| "header Node peer runtime configuration rejected".into())
}
