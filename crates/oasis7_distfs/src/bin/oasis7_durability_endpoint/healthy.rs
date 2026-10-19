//! Healthy endpoint bootstrap replays signed public qualifications from the
//! independently provisioned initial certificate. JSON definitions, file names
//! and status flags supply no authority. Ordinary Ready is never an install reply.
use super::*;
use formal::endpoint::{DurableSetCandidateV1, SignedFormalArchiveTransferV1};
use formal::transport::{
    DurableSetCandidateServerV1, RemoteFormalEndpoint, SignedArchiveReadGrantV1,
};
use serde::Deserialize;
use std::net::{SocketAddr, TcpListener};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HealthyBootstrap {
    pub schema_version: u32,
    pub lineage_directory: PathBuf,
    pub sealed: formal::DurableSetSealedEvidenceV1,
    pub management_listen: SocketAddr,
    pub source_address: Option<SocketAddr>,
    pub source_role: Option<EndpointRole>,
    pub archive_read_grant: Option<SignedArchiveReadGrantV1>,
    /// One complete copy/readback operation budget; not a lifetime record limit.
    pub operation_byte_budget: u64,
}
struct Verified {
    sealed: Arc<formal::VerifiedSealedDurableSetTransitionV1>,
    old: formal::VerifiedAuthorityV1,
    boundary: formal::VerifiedFormalCommitV1,
}
impl HealthyBootstrap {
    fn verify(
        &mut self,
        initial: &formal::VerifiedAuthorityV1,
        initial_qualified: &formal::FormalQualifiedCommitProofV1,
        config_parent: &Path,
    ) -> Result<Verified, String> {
        if self.schema_version != 1
            || !self.management_listen.ip().is_loopback()
            || self.operation_byte_budget == 0
        {
            return Err("invalid healthy endpoint bootstrap".into());
        }
        if !self.lineage_directory.is_absolute() {
            self.lineage_directory = config_parent.join(&self.lineage_directory);
        }
        private_data_dir(&self.lineage_directory)?;
        let mut page_number = 0u64;
        let directory = self.lineage_directory.clone();
        let pages = std::iter::from_fn(move || {
            let result = (|| {
                let path = directory.join(format!("{page_number:020}.cbor"));
                private_file(&path).map_err(formal::FormalError::Io)?;
                let bytes = read(&path, 64 * 1024 * 1024).map_err(formal::FormalError::Io)?;
                let page = formal::HealthyEndpointLineagePageV1::decode(&bytes)?;
                page_number = page_number.checked_add(1).ok_or_else(|| {
                    formal::FormalError::Invalid("healthy page number overflow".into())
                })?;
                Ok(page)
            })();
            Some(result)
        });
        let sealed = formal::verify_healthy_endpoint_bootstrap(
            initial,
            initial_qualified,
            &self.sealed,
            pages,
        )
        .map_err(|e| e.to_string())?;
        let old = sealed.transition().predecessor_authority().clone();
        let boundary = sealed.transition().boundary().clone();
        Ok(Verified {
            sealed: Arc::new(sealed),
            old,
            boundary,
        })
    }
}
#[cfg(unix)]
fn private_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    // SAFETY: geteuid only returns process ownership identity.
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err("healthy lineage page must be owned private regular file".into());
    }
    Ok(())
}
#[cfg(not(unix))]
fn private_file(_: &Path) -> Result<(), String> {
    Err("healthy bootstrap requires Unix private storage".into())
}

#[expect(
    clippy::too_many_arguments,
    reason = "independent origin, provider credential, role, storage and ports must each bind the same staged operation"
)]
pub(super) fn run(
    mut bootstrap: HealthyBootstrap,
    initial: formal::VerifiedAuthorityV1,
    initial_qualified: formal::FormalQualifiedCommitProofV1,
    config_parent: &Path,
    role: EndpointRole,
    data_dir: &Path,
    authentication: ed25519_dalek::SigningKey,
    oracle_address: SocketAddr,
    listen: SocketAddr,
    timeout: Duration,
) -> Result<(), String> {
    let preparation_started = std::time::Instant::now();
    if bootstrap.management_listen == listen {
        return Err("healthy management and ordinary ports must be distinct".into());
    }
    let verified = bootstrap.verify(&initial, &initial_qualified, config_parent)?;
    report_bootstrap_timing("lineage", preparation_started);
    let public = hex::encode(authentication.verifying_key().to_bytes());
    let successor = &verified.sealed.transition().body().successor;
    let old = verified.old.definition();
    if [
        old.writer_public_key.as_str(),
        old.primary.instance.endpoint_public_key.as_str(),
        old.replica.instance.endpoint_public_key.as_str(),
        successor.writer_public_key.as_str(),
        successor.primary.instance.endpoint_public_key.as_str(),
        successor.replica.instance.endpoint_public_key.as_str(),
        verified.old.policy().fencing_provider_public_key.as_str(),
        verified.old.policy().recovery_issuer_public_key.as_str(),
        verified.old.policy().issuer_anchor_public_key.as_str(),
    ]
    .contains(&public.as_str())
    {
        return Err("FENCING_UNPROVEN: healthy owner credential must be independent".into());
    }
    private_data_dir(data_dir)?;
    let provider: Arc<dyn formal::fencing::MonotonicEndpointProvider> = Arc::new(
        RemoteMonotonicEndpointProvider::new(
            oracle_address,
            timeout,
            authentication.clone(),
            TrustedFencingPolicyV1 {
                provider_public_key: verified.old.policy().fencing_provider_public_key.clone(),
                authorized_resource_bindings: verified
                    .old
                    .policy()
                    .authorized_fencing_resource_bindings
                    .clone(),
            },
        )
        .map_err(|e| e.to_string())?,
    );
    let (mut server, active) =
        if DurableSetCandidateV1::has_staged_manifest(data_dir, &verified.sealed)
            .map_err(|e| e.to_string())?
        {
            DurableSetCandidateServerV1::reopen(
                data_dir,
                verified.sealed.clone(),
                role,
                timeout,
                provider.clone(),
                bootstrap.operation_byte_budget,
            )
            .map_err(|e| e.to_string())?
        } else {
            let candidate = if role == EndpointRole::Primary {
                if bootstrap.source_address.is_some()
                    || bootstrap.source_role.is_some()
                    || bootstrap.archive_read_grant.is_some()
                {
                    return Err(
                        "healthy primary rebind uses its own exact old sealed storage".into(),
                    );
                }
                FileFormalEndpoint::open_protected(
                    data_dir,
                    FormalEndpointAuthorizationV1 {
                        authority: verified.old.clone(),
                        original: verified.old.clone(),
                        recovery: None,
                        parent: Some(verified.boundary.clone()),
                    },
                    role,
                    provider.clone(),
                    None,
                )
                .and_then(|old| {
                    old.into_durable_set_primary_candidate(
                        verified.sealed.clone(),
                        bootstrap.operation_byte_budget,
                    )
                })
            } else {
                let source_role = bootstrap
                    .source_role
                    .ok_or("healthy replica requires independent TCP source role")?;
                let address = bootstrap
                    .source_address
                    .ok_or("healthy replica requires TCP archive source")?;
                let grant = bootstrap
                    .archive_read_grant
                    .ok_or("healthy replica requires writer-signed scoped archive grant")?;
                if grant.body.transition_digest != verified.sealed.transition().digest()
                    || grant.body.sealed_cut != verified.boundary.head()
                    || grant.body.next_epoch_floor != successor.epoch
                {
                    return Err("healthy archive grant exact transition/cut/successor floor".into());
                }
                let target = successor
                    .replica
                    .instance
                    .digest()
                    .map_err(|e| e.to_string())?;
                let mut remote = RemoteFormalEndpoint::new_archive_reader(
                    address,
                    timeout,
                    verified.old.clone(),
                    source_role,
                    authentication,
                    grant,
                    &target,
                )
                .map_err(|e| e.to_string())?;
                let manifest_started = std::time::Instant::now();
                let manifest: SignedFormalArchiveTransferV1 =
                    remote.archive_manifest().map_err(|e| e.to_string())?;
                report_bootstrap_timing("archive_manifest", manifest_started);
                DurableSetCandidateV1::stage_replica(
                    data_dir,
                    verified.sealed.clone(),
                    manifest,
                    remote.into_archive_source(),
                    provider.clone(),
                    bootstrap.operation_byte_budget,
                )
            }
            .map_err(|e| e.to_string())?;
            let server =
                DurableSetCandidateServerV1::new(candidate, verified.sealed.clone(), role, timeout)
                    .map_err(|e| e.to_string())?;
            (server, None)
        };
    let management = TcpListener::bind(bootstrap.management_listen).map_err(|e| e.to_string())?;
    if let Some(endpoint) = active {
        retain_management(server, management)?;
        return serve_ready(endpoint, listen, timeout);
    }
    report_bootstrap_timing("management_ready", preparation_started);
    println!(
        "formal_endpoint_awaiting_management={}",
        management.local_addr().map_err(|e| e.to_string())?
    );
    loop {
        match server.serve_one_recovering(
            &management,
            data_dir,
            provider.clone(),
            bootstrap.operation_byte_budget,
        ) {
            Ok(Some(endpoint)) => {
                retain_management(server, management)?;
                return serve_ready(endpoint, listen, timeout);
            }
            Ok(None) => {}
            Err(error) => report_management_error(&error),
        }
    }
}
fn report_bootstrap_timing(phase: &'static str, started: std::time::Instant) {
    if std::env::var_os("OASIS7_HEALTHY_NATIVE_TRACE").is_some_and(|value| value == "1") {
        eprintln!(
            "healthy-native-bootstrap phase={phase} elapsed_us={}",
            started.elapsed().as_micros(),
        );
    }
}
fn report_management_error(error: &impl std::fmt::Display) {
    if std::env::var_os("OASIS7_HEALTHY_NATIVE_TRACE").is_some() {
        eprintln!("healthy management diagnostic: {error}");
    }
    eprintln!("healthy management rejected or outcome unknown; reconcile original transition");
}
fn retain_management(
    mut server: DurableSetCandidateServerV1,
    listener: TcpListener,
) -> Result<(), String> {
    std::thread::Builder::new()
        .name("formal-healthy-management".into())
        .spawn(move || {
            loop {
                match server.serve_one(&listener) {
                    Ok(None) => {}
                    Ok(Some(_)) => {
                        eprintln!(
                            "healthy management unexpected second owner; process must reopen"
                        );
                        std::process::exit(1);
                    }
                    Err(error) => report_management_error(&error),
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn serve_ready(
    endpoint: FileFormalEndpoint,
    listen: SocketAddr,
    timeout: Duration,
) -> Result<(), String> {
    let listener = TcpListener::bind(listen).map_err(|e| e.to_string())?;
    let mut server = FormalEndpointServer::new(endpoint, timeout).map_err(|e| e.to_string())?;
    println!(
        "formal_endpoint_listening={}",
        listener.local_addr().map_err(|e| e.to_string())?
    );
    loop {
        if let Err(error) = server.serve_one(&listener) {
            if std::env::var_os("OASIS7_HEALTHY_NATIVE_TRACE").is_some() {
                eprintln!("healthy endpoint RPC diagnostic: {error}");
            }
            eprintln!("formal endpoint RPC rejected or incomplete; reconcile original request");
        }
    }
}
