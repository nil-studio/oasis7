//! Strict formal branch. The local credential authenticates the oracle client;
//! endpoint receipt keys stay in the independently provisioned protected signer.
use super::*;
#[path = "healthy.rs"]
mod healthy_bootstrap;
#[path = "recovery.rs"]
mod recovery_bootstrap;
use oasis7_distfs::controlled_authority::formal::{
    self,
    endpoint::{FileFormalEndpoint, FormalEndpointAuthorizationV1},
    fencing::{RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1},
    transport::FormalEndpointServer,
};
use std::sync::Arc;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FormalConfig {
    schema_version: u32,
    initial_trust: formal::InitialActivationTrustV1,
    initial_certificate: formal::SignedInitialActivationV1,
    initial_qualified: Option<formal::FormalQualifiedCommitProofV1>,
    recovery_bootstrap: Option<recovery_bootstrap::RecoveryBootstrap>,
    #[serde(default)]
    healthy_bootstrap: Option<healthy_bootstrap::HealthyBootstrap>,
    role: EndpointRole,
    data_dir: PathBuf,
    oracle_authentication_key_file: PathBuf,
    oracle_address: std::net::SocketAddr,
    listen: std::net::SocketAddr,
    timeout_ms: u64,
}
pub(super) fn run(bytes: &[u8], config_parent: &Path) -> Result<(), String> {
    let envelope: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if envelope.get("signing_key_file").is_some() || envelope.get("writer_key_file").is_some() {
        return Err(
            "FENCING_UNPROVEN: formal endpoint cannot load exportable receipt/writer keys".into(),
        );
    }
    if envelope.get("oracle_address").is_none()
        || envelope.get("oracle_authentication_key_file").is_none()
    {
        return Err("FENCING_UNPROVEN: external protected monotonic provider is required".into());
    }
    let mut config: FormalConfig = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if config.schema_version != 2
        || !config.listen.ip().is_loopback()
        || !(1..=60_000).contains(&config.timeout_ms)
    {
        return Err("invalid formal endpoint configuration".into());
    }
    for path in [
        &mut config.data_dir,
        &mut config.oracle_authentication_key_file,
    ] {
        if !path.is_absolute() {
            *path = config_parent.join(&*path);
        }
    }
    let initial_authority =
        formal::verify_initial_activation(&config.initial_certificate, &config.initial_trust)
            .map_err(|e| e.to_string())?;
    if let Some(bootstrap) = config.healthy_bootstrap {
        if config.recovery_bootstrap.is_some() {
            return Err("healthy bootstrap cannot mix recovery authorization".into());
        }
        let initial_qualified = config
            .initial_qualified
            .ok_or("healthy bootstrap requires independent initial qualified proof")?;
        let authentication = key(&config.oracle_authentication_key_file)?;
        let authentication_public = hex::encode(authentication.verifying_key().to_bytes());
        if authentication_public == config.initial_trust.issuer_public_key {
            return Err("FENCING_UNPROVEN: bootstrap issuer cannot be archive credential".into());
        }
        return healthy_bootstrap::run(
            bootstrap,
            initial_authority,
            initial_qualified,
            config_parent,
            config.role,
            &config.data_dir,
            authentication,
            config.oracle_address,
            config.listen,
            Duration::from_millis(config.timeout_ms),
        );
    }
    let (authorization, archive) = if let Some(mut bootstrap) = config.recovery_bootstrap {
        if config.initial_qualified.is_some() {
            return Err("recovery bootstrap cannot mix separate initial parent".into());
        }
        let verified = bootstrap.verify(
            &config.initial_trust,
            &config.initial_certificate,
            config_parent,
        )?;
        (verified.authorization, Some(verified.archive))
    } else {
        let parent = config
            .initial_qualified
            .as_ref()
            .map(|proof| formal::verify_formal_qualified(proof, &initial_authority, None, None))
            .transpose()
            .map_err(|e| e.to_string())?;
        (
            FormalEndpointAuthorizationV1 {
                original: initial_authority.clone(),
                authority: initial_authority,
                recovery: None,
                parent,
            },
            None,
        )
    };
    let authority = &authorization.authority;
    // Key separation is checked BEFORE any oracle/storage operation. Config has
    // no writer or endpoint private-key field, including aliases.
    let authentication = key(&config.oracle_authentication_key_file)?;
    let public = hex::encode(authentication.verifying_key().to_bytes());
    if [
        authority.definition().writer_public_key.as_str(),
        authority
            .definition()
            .primary
            .instance
            .endpoint_public_key
            .as_str(),
        authority
            .definition()
            .replica
            .instance
            .endpoint_public_key
            .as_str(),
        authorization
            .original
            .definition()
            .writer_public_key
            .as_str(),
        authorization
            .original
            .definition()
            .primary
            .instance
            .endpoint_public_key
            .as_str(),
        authorization
            .original
            .definition()
            .replica
            .instance
            .endpoint_public_key
            .as_str(),
        authority.policy().fencing_provider_public_key.as_str(),
        authority.policy().recovery_issuer_public_key.as_str(),
        authority.policy().issuer_anchor_public_key.as_str(),
        config.initial_trust.issuer_public_key.as_str(),
    ]
    .contains(&public.as_str())
        || authorization.recovery.as_ref().is_some_and(|recovery| {
            let old = recovery.predecessor().definition();
            [
                old.writer_public_key.as_str(),
                old.primary.instance.endpoint_public_key.as_str(),
                old.replica.instance.endpoint_public_key.as_str(),
            ]
            .contains(&public.as_str())
        })
    {
        return Err("FENCING_UNPROVEN: oracle credential must be an independent key".into());
    }
    private_data_dir(&config.data_dir)?;
    let timeout = Duration::from_millis(config.timeout_ms);
    let provider = RemoteMonotonicEndpointProvider::new(
        config.oracle_address,
        timeout,
        authentication,
        TrustedFencingPolicyV1 {
            provider_public_key: authority.policy().fencing_provider_public_key.clone(),
            authorized_resource_bindings: authority
                .policy()
                .authorized_fencing_resource_bindings
                .clone(),
        },
    )
    .map_err(|e| e.to_string())?;
    let endpoint = FileFormalEndpoint::open_protected(
        &config.data_dir,
        authorization,
        config.role,
        Arc::new(provider),
        archive.as_ref(),
    )
    .map_err(|e| match e {
        formal::FormalError::FencingUnproven => "FENCING_UNPROVEN".into(),
        other => format!(
            "protected endpoint open role={:?} target={:?}: {other}",
            config.role, config.data_dir
        ),
    })?;
    // A port is published only after strict custody/floor and native restore.
    let listener = std::net::TcpListener::bind(config.listen).map_err(|e| e.to_string())?;
    let mut server = FormalEndpointServer::new(endpoint, timeout).map_err(|e| e.to_string())?;
    println!(
        "formal_endpoint_listening={}",
        listener.local_addr().map_err(|e| e.to_string())?
    );
    loop {
        if server.serve_one(&listener).is_err() {
            eprintln!("formal endpoint RPC rejected or incomplete; reconcile original request");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formal_configuration_missing_protected_provider_or_local_role_key_fails_before_io() {
        let missing = run(br#"{"schema_version":2}"#, Path::new("/nonexistent"));
        assert!(missing.unwrap_err().contains("FENCING_UNPROVEN"));
        let local = run(
            br#"{"schema_version":2,"signing_key_file":"/nonexistent/key"}"#,
            Path::new("/nonexistent"),
        );
        assert!(local.unwrap_err().contains("cannot load exportable"));
        let writer = run(
            br#"{"schema_version":2,"writer_key_file":"/nonexistent/key"}"#,
            Path::new("/nonexistent"),
        );
        assert!(writer.unwrap_err().contains("cannot load exportable"));
    }
}
