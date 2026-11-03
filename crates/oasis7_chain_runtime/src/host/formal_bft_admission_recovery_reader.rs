//! Explicit Node-to-provider retained-original reader. It is independent of the
//! provider-to-Node ingress subject and of legacy committed-state credentials.
use super::client::{
    AdmissionGrantClientConfigV1, AdmissionGrantTargetV1, RemoteAdmissionGrantCredentialsV1,
};
use oasis7_distfs::protected_bft_v3::BftSignerBindingV3;
use oasis7_net::Libp2pNetwork;
use oasis7_node::{
    NodeBftAdmissionInputPublisherV1, NodeBftLibp2pAdmissionInputReaderV2,
    NodeBftLibp2pPrivateCandidateExchangeV3,
};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdmissionRecoveryReaderConfigV1 {
    pub serving_provider_peer_id: String,
    pub subject_signing_key_path: PathBuf,
    pub issuer_endpoint: String,
    pub issuer_public_key: String,
    pub target: AdmissionGrantTargetV1,
}
pub(crate) fn publisher(
    config: Option<&AdmissionRecoveryReaderConfigV1>,
    network: Arc<Libp2pNetwork>,
    private: &NodeBftLibp2pPrivateCandidateExchangeV3,
    binding: &BftSignerBindingV3,
    allowed_provider_peers: &[String],
    forbidden_keys: &[String],
) -> Result<NodeBftAdmissionInputPublisherV1, String> {
    let buffers = private.transport_buffers();
    let publisher = NodeBftAdmissionInputPublisherV1::new(buffers.clone());
    let Some(config) = config else {
        return Ok(publisher);
    };
    if !allowed_provider_peers.contains(&config.serving_provider_peer_id)
        || !config.subject_signing_key_path.is_absolute()
    {
        return Err("admission recovery reader requires explicit provider route and absolute subject key path".into());
    }
    let subject =
        crate::controlled_live_config::read_existing_signing_key(&config.subject_signing_key_path)?;
    let public = hex::encode(subject.verifying_key().to_bytes());
    if public == binding.requester_auth_public_key
        || public == config.issuer_public_key
        || binding
            .provisioned_signing_keys
            .iter()
            .any(|key| key.public_key_hex == public)
        || forbidden_keys.contains(&public)
    {
        return Err(
            "admission recovery subject must differ from existing authority and read keys".into(),
        );
    }
    let credentials = Arc::new(RemoteAdmissionGrantCredentialsV1::new(
        AdmissionGrantClientConfigV1 {
            endpoint: config.issuer_endpoint.clone(),
            issuer_public_key: config.issuer_public_key.clone(),
            target: config.target.clone(),
        },
        network.clone(),
        subject,
    )?);
    let reader = Arc::new(NodeBftLibp2pAdmissionInputReaderV2::new(
        network,
        config.serving_provider_peer_id.clone(),
        credentials,
        buffers,
    )?);
    Ok(publisher.with_original_reader(reader))
}
