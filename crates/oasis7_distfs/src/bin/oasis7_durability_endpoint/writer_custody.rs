//! A separately permissioned, executable writer service for the controlled-file
//! crash model. Runtime configuration carries a client credential, not this key.
use super::*;
use oasis7_distfs::controlled_authority::formal::{
    self, fencing::ControlledFileWriterProvider, issuer::IssuerAnchorV1,
    writer_signing::WriterCustodyServerV1,
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum StorageMode {
    Initialize,
    OpenExisting,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriterConfig {
    schema_version: u32,
    storage_mode: StorageMode,
    initial_trust: formal::InitialActivationTrustV1,
    initial_certificate: formal::SignedInitialActivationV1,
    #[serde(default)]
    recovery_permission: Option<formal::RecoveryPermissionBootstrapV1>,
    #[serde(default)]
    minimum_issuer_anchor: Option<IssuerAnchorV1>,
    data_dir: PathBuf,
    writer_key_file: PathBuf,
    normal_credential_public_key: String,
    recovery_credential_public_key: String,
    minimum_revision: u64,
    listen: std::net::SocketAddr,
    timeout_ms: u64,
}

pub(super) fn run(bytes: &[u8], config_parent: &Path) -> Result<(), String> {
    let mut config: WriterConfig = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if config.schema_version != 1
        || !config.listen.ip().is_loopback()
        || !(1..=60_000).contains(&config.timeout_ms)
    {
        return Err("invalid writer custody service configuration".into());
    }
    if config.normal_credential_public_key == config.initial_trust.issuer_public_key
        || config.recovery_credential_public_key == config.initial_trust.issuer_public_key
    {
        return Err(
            "writer custody client credentials must be independent of the initial issuer".into(),
        );
    }
    let initial =
        formal::verify_initial_activation(&config.initial_certificate, &config.initial_trust)
            .map_err(|e| e.to_string())?;
    let (authority, recovery) =
        match (&config.recovery_permission, &config.minimum_issuer_anchor) {
            (None, None) => (initial, None),
            (Some(permission), Some(minimum)) => {
                let verified = permission.verify_at_minimum(
                    &config.initial_trust,
                    &config.initial_certificate,
                    minimum,
                )?;
                (
                    verified.recovery().successor().clone(),
                    Some(verified.recovery().clone()),
                )
            }
            _ => return Err(
                "writer recovery requires both original permission and independent issuer floor"
                    .into(),
            ),
        };
    for path in [&mut config.data_dir, &mut config.writer_key_file] {
        if !path.is_absolute() {
            *path = config_parent.join(&*path);
        }
    }
    let writer_key = key(&config.writer_key_file)?;
    if config.data_dir.exists() {
        private_data_dir(&config.data_dir)?;
    }
    let provider = match config.storage_mode {
        StorageMode::Initialize => ControlledFileWriterProvider::initialize(
            &config.data_dir,
            authority,
            writer_key,
            config.normal_credential_public_key,
            config.recovery_credential_public_key,
            recovery,
        ),
        StorageMode::OpenExisting => ControlledFileWriterProvider::open_existing(
            &config.data_dir,
            authority,
            writer_key,
            config.normal_credential_public_key,
            config.recovery_credential_public_key,
            recovery,
        ),
    }
    .map_err(|e| e.to_string())?;
    provider
        .require_minimum_revision(config.minimum_revision)
        .map_err(|e| e.to_string())?;
    // Publish only after exact authority, exclusive custody, original ledger,
    // and externally configured revision floor have all been established.
    let listener = std::net::TcpListener::bind(config.listen).map_err(|e| e.to_string())?;
    let mut server = WriterCustodyServerV1::new(provider, Duration::from_millis(config.timeout_ms))
        .map_err(|e| e.to_string())?;
    println!(
        "writer_custody_listening={}",
        listener.local_addr().map_err(|e| e.to_string())?
    );
    loop {
        if server.serve_one(&listener).is_err() {
            eprintln!("writer custody RPC rejected or incomplete; reconcile original operation");
        }
    }
}
