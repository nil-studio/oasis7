//! Executable composition over one externally opened backend. This does not
//! provide a durable backend implementation or attest physical fault domains.
use super::{
    backend_owner::SharedBftSignerBackendOwnerV3,
    client::{
        AdmissionGrantClientConfigV1, AdmissionGrantTargetV1, RemoteAdmissionGrantCredentialsV1,
    },
};
use crate::formal_live_service::full_sync_read_gate::{
    FullSyncV3ReadGate, fresh_provider_incarnation,
    remote_evidence::RemoteFullSyncReadEvidenceSourceV3,
};
use ed25519_dalek::SigningKey;
use oasis7_distfs::controlled_authority::formal::{
    InitialActivationTrustV1,
    full_sync_issuer::{
        FullSyncIssuerAnchorV3, SignedFullSyncIssuerPolicyV2, SignedFullSyncIssuerPolicyV3,
        VerifiedFullSyncIssuerPolicyV2, VerifiedFullSyncIssuerPolicyV3,
    },
};
use oasis7_distfs::protected_bft_v3::*;
use oasis7_net::Libp2pNetwork;
use oasis7_node::{
    NodeBftAdmissionInputServerConfigV2, NodeBftBackendAdmissionInputSourceV1,
    NodeBftLibp2pAdmissionInputReaderV2, NodeBftLibp2pAdmissionInputServerV2,
    NodeBftPrivateTransportScopeV1,
};
use std::{sync::Arc, time::Duration};

/// These are independently pinned serving-authority inputs, not new issuer or
/// witness state. The external relay must return complete signed evidence.
pub struct AdmissionProviderIssuerConfigV1 {
    pub initial_trust: InitialActivationTrustV1,
    pub signed_v2_policy: SignedFullSyncIssuerPolicyV2,
    pub signed_v3_policy: SignedFullSyncIssuerPolicyV3,
    pub minimum_anchor: FullSyncIssuerAnchorV3,
    pub evidence_endpoint: String,
    pub read_provider_key: SigningKey,
}
/// The configured provider Noise network is independent of the Node network.
/// No route, network identity or BFT signer key supplies original-input rights.
pub struct AdmissionProviderNetworkConfigV1 {
    pub network: Arc<Libp2pNetwork>,
    pub node_peer_id: String,
    pub operation_timeout: Duration,
    pub ingress_subject_key: SigningKey,
    pub grant_endpoint: String,
    pub grant_issuer_public_key: String,
    pub grant_epoch: u64,
    pub grant_not_before_unix_ms: u64,
    pub grant_expires_unix_ms: u64,
    pub issuer: AdmissionProviderIssuerConfigV1,
}
/// Exactly one already-opened, externally provisioned namespace. In particular
/// this entry never creates a software Memory backend or reopens a writer.
pub struct AdmissionProviderServiceInputsV1 {
    pub binding: BftSignerBindingV3,
    pub provider_key: Box<dyn BftSigningKeyHandleV3>,
    pub validator_keys: Vec<Box<dyn BftSigningKeyHandleV3>>,
    pub backend: Box<dyn BftSignerBackendV3>,
    pub successor: Option<BftSuccessorWindowSupportV1>,
    pub retention: BftPrivateRetentionSupportV1,
}
/// Retain this owner while serving the signer's original TCP protocol. Dropping
/// it revokes retained-input egress; callers still own signer TCP connection I/O.
pub struct OpenedAdmissionProviderV1 {
    service: Arc<BftSignerServiceV3>,
    _retained_input: NodeBftLibp2pAdmissionInputServerV2,
    _network: Arc<Libp2pNetwork>,
}
impl OpenedAdmissionProviderV1 {
    pub fn service(&self) -> Arc<BftSignerServiceV3> {
        self.service.clone()
    }
}
/// Compose both actual encrypted directions. Only provider egress charges the
/// existing protected transfer counter; Node ingress retains its original
/// shared transport/material accounting and never reenters the signer RPC.
pub fn open_admission_provider_v1(
    inputs: AdmissionProviderServiceInputsV1,
    config: AdmissionProviderNetworkConfigV1,
) -> Result<OpenedAdmissionProviderV1, String> {
    let subject_public = hex::encode(config.ingress_subject_key.verifying_key().to_bytes());
    let read_provider_public =
        hex::encode(config.issuer.read_provider_key.verifying_key().to_bytes());
    if subject_public == inputs.provider_key.public_key_hex()
        || inputs
            .validator_keys
            .iter()
            .any(|key| key.public_key_hex() == subject_public)
        || subject_public == read_provider_public
        || subject_public == config.grant_issuer_public_key
        || subject_public == inputs.binding.requester_auth_public_key
    {
        return Err("provider ingress subject must be independently provisioned".into());
    }
    let v2 = VerifiedFullSyncIssuerPolicyV2::verify(
        &config.issuer.signed_v2_policy,
        &config.issuer.initial_trust,
    )
    .map_err(|_| "provider input V2 issuer policy rejected")?;
    let v3 = VerifiedFullSyncIssuerPolicyV3::verify(
        &config.issuer.signed_v3_policy,
        &config.issuer.initial_trust,
        &v2,
    )
    .map_err(|_| "provider input V3 issuer policy rejected")?;
    let evidence = RemoteFullSyncReadEvidenceSourceV3::new(&config.issuer.evidence_endpoint)?
        .with_provider_lease_requests(v2, v3, config.issuer.read_provider_key)?;
    let gate = Arc::new(FullSyncV3ReadGate::new(
        config.issuer.initial_trust,
        config.issuer.signed_v2_policy,
        config.issuer.signed_v3_policy,
        config.issuer.minimum_anchor,
        read_provider_public,
        fresh_provider_incarnation()?,
        Arc::new(evidence),
    )?);
    let scope = NodeBftPrivateTransportScopeV1::new(
        vec![config.node_peer_id.clone()],
        config.operation_timeout,
    )?;
    let credentials = Arc::new(RemoteAdmissionGrantCredentialsV1::new(
        AdmissionGrantClientConfigV1 {
            endpoint: config.grant_endpoint,
            issuer_public_key: config.grant_issuer_public_key,
            target: AdmissionGrantTargetV1 {
                grant_epoch: config.grant_epoch,
                not_before_unix_ms: config.grant_not_before_unix_ms,
                expires_unix_ms: config.grant_expires_unix_ms,
            },
        },
        config.network.clone(),
        config.ingress_subject_key,
    )?);
    let ingress = Arc::new(NodeBftLibp2pAdmissionInputReaderV2::new(
        config.network.clone(),
        config.node_peer_id.clone(),
        credentials,
        scope.buffers(),
    )?);
    let owner = SharedBftSignerBackendOwnerV3::new(inputs.backend);
    let binding = inputs.binding;
    let service = Arc::new(
        BftSignerServiceV3::open_with_private_retention(
            binding.clone(),
            inputs.provider_key,
            inputs.validator_keys,
            owner.service_backend(),
            inputs.successor,
            inputs.retention.with_admission_input_reader(ingress),
        )
        .map_err(|error| error.to_string())?,
    );
    let source = Arc::new(NodeBftBackendAdmissionInputSourceV1::new(
        binding.digest().map_err(|error| error.to_string())?,
        owner.input_backend(),
        scope.buffers(),
    )?);
    let retained_input = NodeBftLibp2pAdmissionInputServerV2::new_for_provider(
        config.network.clone(),
        NodeBftAdmissionInputServerConfigV2 {
            binding,
            peers: vec![config.node_peer_id],
        },
        service.clone(),
        scope,
        source,
        gate,
    )?;
    Ok(OpenedAdmissionProviderV1 {
        service,
        _retained_input: retained_input,
        _network: config.network,
    })
}
