//! Independently provisioned grant issuer process. No managed role, witness,
//! provider lease or private material is initialized by this command.
use super::endpoint::AdmissionGrantIssuerV1;
use crate::formal_live_service::full_sync_read_gate::{
    FullSyncV3ReadGate, fresh_provider_incarnation,
    remote_evidence::RemoteFullSyncReadEvidenceSourceV3,
};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_distfs::controlled_authority::formal::{
    InitialActivationTrustV1,
    full_sync_issuer::{
        FullSyncIssuerAnchorV3, FullSyncReadRoleV2, SignedFullSyncIssuerPolicyV2,
        SignedFullSyncIssuerPolicyV3, VerifiedFullSyncIssuerPolicyV2,
        VerifiedFullSyncIssuerPolicyV3,
    },
};
use oasis7_distfs::protected_bft_v3::{BftSignerErrorV3, BftSigningKeyHandleV3};
use serde::Deserialize;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema_version: u32,
    initial_trust: InitialActivationTrustV1,
    signed_v2_policy: SignedFullSyncIssuerPolicyV2,
    signed_v3_policy: SignedFullSyncIssuerPolicyV3,
    minimum_anchor: FullSyncIssuerAnchorV3,
    evidence_endpoint: String,
    read_provider_signing_key_path: PathBuf,
    issuer_signing_key_path: PathBuf,
    bind: SocketAddr,
}
struct ProvisionedIssuerKey(SigningKey);
impl BftSigningKeyHandleV3 for ProvisionedIssuerKey {
    fn public_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        Ok(self.0.sign(bytes).to_bytes())
    }
}
pub(crate) fn run<'a>(
    mut args: impl Iterator<Item = &'a str>,
    shutdown: Option<Arc<AtomicBool>>,
) -> Result<(), String> {
    if args.next() != Some("--config") {
        return Err("admission-grant-issuer requires --config ABS".into());
    }
    let path = PathBuf::from(args.next().ok_or("admission issuer config path missing")?);
    if !path.is_absolute() || args.next().is_some() {
        return Err("admission issuer requires one absolute config path".into());
    }
    crate::controlled_live_config::require_private_owned(&path, false)?;
    let bytes = crate::controlled_history_cli::read_bounded(&path, 2 * 1024 * 1024)?;
    let config: Config = crate::formal_live_service::parse_strict(&bytes)?;
    if config.schema_version != 1
        || !config.bind.ip().is_loopback()
        || config.bind.port() == 0
        || !config.read_provider_signing_key_path.is_absolute()
        || !config.issuer_signing_key_path.is_absolute()
    {
        return Err("admission issuer explicit configuration rejected".into());
    }
    let verified_v2 =
        VerifiedFullSyncIssuerPolicyV2::verify(&config.signed_v2_policy, &config.initial_trust)
            .map_err(|_| "admission issuer V2 policy rejected")?;
    let verified_v3 = VerifiedFullSyncIssuerPolicyV3::verify(
        &config.signed_v3_policy,
        &config.initial_trust,
        &verified_v2,
    )
    .map_err(|_| "admission issuer V3 policy rejected")?;
    if !verified_v3
        .body()
        .allowed_roles
        .contains(&FullSyncReadRoleV2::BftOriginalAdmissionInputV1)
    {
        return Err("admission issuer requires explicit OriginalAdmissionInput role policy".into());
    }
    let issuer = ProvisionedIssuerKey(crate::controlled_live_config::read_existing_signing_key(
        &config.issuer_signing_key_path,
    )?);
    let provider = crate::controlled_live_config::read_existing_signing_key(
        &config.read_provider_signing_key_path,
    )?;
    let provider_public = hex::encode(provider.verifying_key().to_bytes());
    if issuer.public_key_hex() != config.initial_trust.issuer_public_key
        || issuer.public_key_hex() == provider_public
    {
        return Err("admission issuer requires distinct pinned issuer and provider keys".into());
    }
    let evidence = RemoteFullSyncReadEvidenceSourceV3::new(&config.evidence_endpoint)?
        .with_provider_lease_requests(verified_v2, verified_v3, provider)?;
    let gate = Arc::new(FullSyncV3ReadGate::new(
        config.initial_trust,
        config.signed_v2_policy,
        config.signed_v3_policy,
        config.minimum_anchor,
        provider_public,
        fresh_provider_incarnation()?,
        Arc::new(evidence),
    )?);
    let issuer = Arc::new(AdmissionGrantIssuerV1::new(gate, Arc::new(issuer)));
    let mut server = issuer.serve_loopback(config.bind)?;
    let shutdown = match shutdown {
        Some(shutdown) => shutdown,
        None => {
            let shutdown = Arc::new(AtomicBool::new(false));
            let signal = shutdown.clone();
            ctrlc::set_handler(move || signal.store(true, Ordering::Release))
                .map_err(|_| "admission issuer shutdown handler unavailable")?;
            shutdown
        }
    };
    while !shutdown.load(Ordering::Acquire) {
        if server.is_finished() {
            return server.stop().and(Err(
                "admission issuer stopped before requested shutdown".into()
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    server.stop()
}
