//! Explicit, local-only preparation and activation of an externally approved
//! formal initial world. This module never creates a World or manufactures
//! identity, issuer trust, or an activation signature.
use ed25519_dalek::SigningKey;
use oasis7_distfs::controlled_authority::formal::{
    AuthorityDefinitionV1, FormalCompletionBranchV1, FormalRecordKindV1, FormalWorldIdentityV1,
    InitialActivationBodyV1, InitialActivationTrustV1, RecoveryPolicyV1, SignedInitialActivationV1,
    VerifiedAuthorityV1,
    coordinator::{FormalCoordinator, FormalCoordinatorErrorV1},
    transport::RemoteFormalEndpoint,
    verify_formal_qualified, verify_initial_activation,
};
use oasis7_runtime::{
    Journal, ObserverReadLimits, ReleaseSecurityPolicy, Snapshot, World, blake3_hex,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(not(test))]
use super::execution_bridge::formal_initial::{InitialNodeBootstrapV1, InitialWorldClosure};
#[cfg(not(test))]
use super::execution_bridge::formal_native_environment;
#[cfg(test)]
use super::execution_bridge_real_tests::real_execution_bridge::formal_initial::{
    InitialNodeBootstrapV1, InitialWorldClosure,
};
#[cfg(test)]
use super::execution_bridge_real_tests::real_execution_bridge::formal_native_environment;

const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_WORLD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_APPROVAL_BYTES: u64 = 1024 * 1024;
const MAX_PUBLICATION_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4096;
const REQUEST_DOMAIN: &str = "oasis7.formal.world_initialization.request.v1";
const PREPARE_DOMAIN: &str = "oasis7.formal.world_initialization.prepare.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WorldBoundaryModeV1 {
    Existing,
    Genesis,
}

/// All paths point to operator-prepared files. Paths are deployment details;
/// the prepared request commits their decoded content, never their names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorldInitializationConfigV1 {
    schema_version: u32,
    mode: WorldBoundaryModeV1,
    world: FormalWorldIdentityV1,
    snapshot_file: PathBuf,
    journal_file: PathBuf,
    /// Operator-provisioned issuer pin. It is not read from the approval.
    trusted_issuer_public_key: String,
    node_bootstrap: InitialNodeBootstrapV1,
    authority: AuthorityDefinitionV1,
    recovery_policy: RecoveryPolicyV1,
}

/// Stable request artifact passed from prepare to the independent authorizer
/// and then to activate. It contains no secret material or output paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedWorldInitializationV1 {
    schema_version: u32,
    domain: String,
    mode: WorldBoundaryModeV1,
    world: FormalWorldIdentityV1,
    snapshot_digest: String,
    journal_digest: String,
    trusted_issuer_public_key: String,
    node_bootstrap: InitialNodeBootstrapV1,
    activation_body: InitialActivationBodyV1,
    activation_ref: String,
    request_digest: String,
    prepare_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovalQuoteV1 {
    schema_version: u32,
    domain: String,
    world: FormalWorldIdentityV1,
    activation_ref: String,
    request_digest: String,
    initial_state_root: String,
    runtime_manifest_digest: String,
    governing_manifest_digest: String,
    execution_policy_digest: String,
    writer_public_key: String,
    primary_instance_digest: String,
    replica_instance_digest: String,
    issuer_public_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActivationCertificateInputV1 {
    certificate: SignedInitialActivationV1,
}

struct RebuiltRequest {
    prepared: PreparedWorldInitializationV1,
    closure: InitialWorldClosure,
}

/// CLI dispatch is invoked before normal node option parsing by the runtime
/// entrypoint. Root owns that one-line registration/wiring.
pub(super) fn dispatch<'a>(args: impl Iterator<Item = &'a str>) -> Option<Result<(), String>> {
    let mut args = args.peekable();
    if args.next()? != "world" || args.next()? != "init" {
        return None;
    }
    Some(dispatch_init(args))
}

fn dispatch_init<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let action = args
        .next()
        .ok_or("world init requires prepare, quote, or activate")?;
    let mut values = std::collections::BTreeMap::<String, String>::new();
    while let Some(flag) = args.next() {
        if !flag.starts_with("--") {
            return Err(format!("unexpected world init argument: {flag}"));
        }
        if values.contains_key(flag) {
            return Err(format!("duplicate world init argument: {flag}"));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        if value.starts_with("--") {
            return Err(format!("missing value for {flag}"));
        }
        values.insert(flag.to_owned(), value.to_owned());
    }
    match action {
        "prepare" => {
            exact_flags(&values, &["--config", "--request"])?;
            let config: WorldInitializationConfigV1 = serde_json::from_slice(
                &read_private_owned_bounded(path_arg(&values, "--config")?, MAX_CONFIG_BYTES)?,
            )
            .map_err(|error| format!("world initialization config: {error}"))?;
            if config.schema_version != 2 {
                return Err("new world initialization requires config schema 2 with the original native environment; legacy schema 1 is only for previously prepared requests".into());
            }
            prepare(
                path_arg(&values, "--config")?,
                path_arg(&values, "--request")?,
            )
        }
        "quote" => {
            exact_flags(&values, &["--config", "--request", "--certificate"])?;
            let quote = quote_approval(
                path_arg(&values, "--config")?,
                path_arg(&values, "--request")?,
                path_arg(&values, "--certificate")?,
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&quote).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        "activate" => {
            exact_flags(
                &values,
                &[
                    "--config",
                    "--request",
                    "--certificate",
                    "--writer-key",
                    "--primary",
                    "--replica",
                    "--timeout-ms",
                    "--output",
                ],
            )?;
            activate(
                path_arg(&values, "--config")?,
                path_arg(&values, "--request")?,
                path_arg(&values, "--certificate")?,
                path_arg(&values, "--writer-key")?,
                parse_socket(&values, "--primary")?,
                parse_socket(&values, "--replica")?,
                parse_timeout(&values)?,
                path_arg(&values, "--output")?,
            )
        }
        _ => Err(format!("unknown world init action: {action}")),
    }
}

fn exact_flags(
    values: &std::collections::BTreeMap<String, String>,
    required: &[&str],
) -> Result<(), String> {
    if values.len() != required.len() || required.iter().any(|key| !values.contains_key(*key)) {
        return Err(format!(
            "world init requires exactly: {}",
            required.join(" ")
        ));
    }
    Ok(())
}

fn path_arg<'a>(
    values: &'a std::collections::BTreeMap<String, String>,
    name: &str,
) -> Result<&'a Path, String> {
    values
        .get(name)
        .map(Path::new)
        .ok_or_else(|| format!("required world init argument {name}"))
}

fn parse_socket(
    values: &std::collections::BTreeMap<String, String>,
    name: &str,
) -> Result<SocketAddr, String> {
    let address: SocketAddr = values
        .get(name)
        .ok_or_else(|| format!("required world init argument {name}"))?
        .parse()
        .map_err(|error| format!("invalid {name} socket address: {error}"))?;
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err(format!("{name} must be a nonzero loopback endpoint"));
    }
    Ok(address)
}

fn parse_timeout(values: &std::collections::BTreeMap<String, String>) -> Result<Duration, String> {
    let milliseconds: u64 = values
        .get("--timeout-ms")
        .ok_or("required world init argument --timeout-ms")?
        .parse()
        .map_err(|error| format!("invalid --timeout-ms: {error}"))?;
    if !(1..=60_000).contains(&milliseconds) {
        return Err("--timeout-ms must be in 1..=60000".into());
    }
    Ok(Duration::from_millis(milliseconds))
}

pub(super) fn prepare(config_path: &Path, request_path: &Path) -> Result<(), String> {
    let rebuilt = rebuild(config_path)?;
    let bytes = serde_cbor::to_vec(&rebuilt.prepared).map_err(|e| e.to_string())?;
    write_private_idempotent(request_path, &bytes, MAX_PUBLICATION_BYTES)?;
    println!(
        "prepared formal world initialization request {}",
        rebuilt.prepared.request_digest
    );
    Ok(())
}

fn quote_approval(
    config_path: &Path,
    request_path: &Path,
    approval_path: &Path,
) -> Result<ApprovalQuoteV1, String> {
    let rebuilt = rebuild(config_path)?;
    verify_saved_request(request_path, &rebuilt.prepared)?;
    let certificate = load_certificate(approval_path)?;
    let trust = activation_trust(&rebuilt.prepared)?;
    let authority = verify_initial_activation(&certificate, &trust)
        .map_err(|error| format!("external activation approval: {error}"))?;
    require_exact_approval(&rebuilt.prepared, &certificate, &authority)?;
    let quote = approval_quote(&rebuilt.prepared, &trust)?;
    Ok(quote)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn activate(
    config_path: &Path,
    request_path: &Path,
    approval_path: &Path,
    writer_key_path: &Path,
    primary_address: SocketAddr,
    replica_address: SocketAddr,
    timeout: Duration,
    output_path: &Path,
) -> Result<(), String> {
    if primary_address == replica_address
        || !primary_address.ip().is_loopback()
        || !replica_address.ip().is_loopback()
        || primary_address.port() == 0
        || replica_address.port() == 0
        || timeout.is_zero()
        || timeout > Duration::from_secs(60)
    {
        return Err(
            "formal activation requires distinct loopback endpoints and timeout in (0,60s]".into(),
        );
    }
    let rebuilt = rebuild(config_path)?;
    verify_saved_request(request_path, &rebuilt.prepared)?;
    let certificate = load_certificate(approval_path)?;
    let trust = activation_trust(&rebuilt.prepared)?;
    let writer_key = read_existing_signing_key(writer_key_path)?;
    let (authority, candidate) = rebuilt
        .closure
        .authorize(&certificate, &trust, &writer_key)
        .map_err(|error| format!("authorize exact staged world closure: {error}"))?;
    if candidate.candidate.record.request_digest != rebuilt.prepared.request_digest
        || candidate.candidate.record.kind != FormalRecordKindV1::InitialActivation
        || candidate.candidate.record.position != 1
        || candidate.candidate.record.before_state_root != rebuilt.prepared.snapshot_digest
        || candidate.candidate.record.after_state_root != rebuilt.prepared.snapshot_digest
    {
        return Err("authorized initial candidate differs from saved original request".into());
    }
    let genesis = authority
        .definition()
        .world
        .genesis_head()
        .map_err(|error| error.to_string())?;
    let primary = RemoteFormalEndpoint::new(
        primary_address,
        timeout,
        authority.clone(),
        oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole::Primary,
        writer_key.clone(),
        genesis.clone(),
    )
    .map_err(|error| format!("formal primary endpoint: {error}"))?;
    let replica = RemoteFormalEndpoint::new(
        replica_address,
        timeout,
        authority.clone(),
        oasis7_distfs::controlled_authority::replicated_protocol::EndpointRole::Replica,
        writer_key,
        genesis,
    )
    .map_err(|error| format!("formal replica endpoint: {error}"))?;
    let mut coordinator = FormalCoordinator::new(
        primary,
        replica,
        authority.clone(),
        authority.clone(),
        None,
        None,
    )
    .map_err(|error| format!("formal activation coordinator: {error}"))?;
    if let Some(original) = formal_native_environment::descriptor(&candidate.material)? {
        let (primary, replica) = coordinator.endpoints_mut();
        let staged = formal_native_environment::stage_current(primary, replica)?;
        if staged != original {
            return Err("Unavailable: current native environment differs from the externally approved initial closure".into());
        }
    }
    let proof = coordinator.submit(candidate.clone()).map_err(|error| match error {
        FormalCoordinatorErrorV1::OutcomeUnknown { request_digest, cause } => format!(
            "OutcomeUnknown for original formal activation {request_digest}: {cause}; retry with the same request and approval"
        ),
        FormalCoordinatorErrorV1::Rejected(cause) => {
            format!("formal activation rejected for original request: {cause}")
        }
    })?;
    verify_formal_qualified(&proof, &authority, None, None)
        .map_err(|error| format!("formal activation proof: {error}"))?;
    if proof.decision.record != candidate.candidate.record
        || proof.decision.writer_signature_hex != candidate.candidate.writer_signature_hex
        || proof.decision.branch != FormalCompletionBranchV1::Normal
        || proof.decision.frozen.is_some()
    {
        return Err(
            "qualified formal result differs from the original activation candidate".into(),
        );
    }
    let publication = super::formal_live_config::ApprovedInitialPublicationV1 {
        schema_version: 1,
        candidate,
        proof,
    };
    let bytes = serde_cbor::to_vec(&publication).map_err(|e| e.to_string())?;
    write_private_idempotent(output_path, &bytes, MAX_PUBLICATION_BYTES)?;
    println!("formal initial activation qualified and saved for the exact request");
    Ok(())
}

fn rebuild(config_path: &Path) -> Result<RebuiltRequest, String> {
    #[cfg(not(unix))]
    {
        let _ = config_path;
        return Err("formal world initialization requires Unix private-file guarantees".into());
    }
    #[cfg(unix)]
    {
        let config_path = absolute_path(config_path)?;
        let config_bytes = read_private_owned_bounded(&config_path, MAX_CONFIG_BYTES)?;
        let config: WorldInitializationConfigV1 = serde_json::from_slice(&config_bytes)
            .map_err(|error| format!("world initialization config: {error}"))?;
        if !matches!(config.schema_version, 1 | 2) || config.authority.profile_version != 2 {
            return Err(
                "world initialization requires schema 1 or 2 and formal profile version 2".into(),
            );
        }
        validate_issuer_public_key(&config.trusted_issuer_public_key)?;
        config.world.validate().map_err(|error| error.to_string())?;
        if config.authority.world != config.world
            || config.authority.epoch != 1
            || config.authority.valid_from_position != 1
            || config.authority.predecessor_authority_id.is_some()
        {
            return Err(
                "world initialization identity must match the exact initial authority".into(),
            );
        }
        config
            .authority
            .verify_policy(&config.recovery_policy)
            .map_err(|error| error.to_string())?;
        let directory = config_path
            .parent()
            .ok_or("world initialization config parent unavailable")?;
        let snapshot_path = resolve_path(directory, &config.snapshot_file, "World snapshot")?;
        let journal_path = resolve_path(directory, &config.journal_file, "World journal")?;
        let snapshot_bytes = read_private_owned_bounded(&snapshot_path, MAX_WORLD_BYTES)?;
        let journal_bytes = read_private_owned_bounded(&journal_path, MAX_WORLD_BYTES)?;
        World::validate_offline_cbor_budget(&snapshot_bytes, ObserverReadLimits::default())
            .map_err(|error| format!("formal World snapshot budget: {error:?}"))?;
        World::validate_offline_cbor_budget(&journal_bytes, ObserverReadLimits::default())
            .map_err(|error| format!("formal World journal budget: {error:?}"))?;
        let snapshot: Snapshot = serde_cbor::from_slice(&snapshot_bytes)
            .map_err(|error| format!("formal World snapshot CBOR: {error}"))?;
        let journal: Journal = serde_cbor::from_slice(&journal_bytes)
            .map_err(|error| format!("formal World journal CBOR: {error}"))?;
        let world = oasis7::runtime_host::world::from_snapshot(snapshot, journal)
            .map_err(|error| format!("restore explicit formal World boundary: {error:?}"))?;
        let release = ReleaseSecurityPolicy::production_hardened();
        let mut closure = match config.mode {
            WorldBoundaryModeV1::Existing => {
                InitialWorldClosure::from_existing_world_with_node_bootstrap(
                    &world,
                    &config.world,
                    &release,
                    config.node_bootstrap.clone(),
                )
            }
            WorldBoundaryModeV1::Genesis => {
                InitialWorldClosure::from_genesis_world_with_node_bootstrap(
                    &world,
                    &config.world,
                    &release,
                    config.node_bootstrap.clone(),
                )
            }
        }
        .map_err(|error| format!("prepare explicit initial World closure: {error}"))?;
        if config.schema_version == 2 {
            let native = super::native_environment_capture::capture(|_, _| Ok(()))?;
            closure = closure.with_native_environment(&native)?;
        }
        let activation_body = closure
            .activation_body(config.authority, config.recovery_policy)
            .map_err(|error| format!("formal v2 activation body: {error}"))?;
        let snapshot_digest = activation_body.initial_state_root.clone();
        let journal_digest =
            blake3_hex(&serde_cbor::to_vec(world.journal()).map_err(|error| error.to_string())?);
        let activation_ref = activation_body
            .activation_ref()
            .map_err(|error| error.to_string())?;
        let request_digest = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.initial_request.v1",
                &config.world,
                &activation_ref,
            ))
            .map_err(|error| error.to_string())?,
        );
        let mut prepared = PreparedWorldInitializationV1 {
            schema_version: 1,
            domain: REQUEST_DOMAIN.into(),
            mode: config.mode,
            world: config.world,
            snapshot_digest,
            journal_digest,
            trusted_issuer_public_key: config.trusted_issuer_public_key,
            node_bootstrap: config.node_bootstrap,
            activation_body,
            activation_ref,
            request_digest,
            prepare_digest: String::new(),
        };
        prepared.prepare_digest = compute_prepare_digest(&prepared)?;
        Ok(RebuiltRequest { prepared, closure })
    }
}

fn verify_saved_request(
    path: &Path,
    expected: &PreparedWorldInitializationV1,
) -> Result<(), String> {
    let bytes = read_private_owned_bounded(path, MAX_PUBLICATION_BYTES)?;
    let saved: PreparedWorldInitializationV1 = serde_cbor::from_slice(&bytes)
        .map_err(|error| format!("prepared formal initialization request: {error}"))?;
    validate_prepared(&saved)?;
    let expected_bytes = serde_cbor::to_vec(expected).map_err(|error| error.to_string())?;
    if bytes != expected_bytes {
        return Err("staged formal World/identity/Node boundary changed since prepare".into());
    }
    Ok(())
}

fn validate_prepared(prepared: &PreparedWorldInitializationV1) -> Result<(), String> {
    if prepared.schema_version != 1
        || prepared.domain != REQUEST_DOMAIN
        || prepared.world != prepared.activation_body.authority.world
        || prepared.activation_body.authority.profile_version != 2
        || prepared.activation_body.initial_state_root != prepared.snapshot_digest
        || prepared.node_bootstrap.execution_state_root != prepared.snapshot_digest
        || prepared.activation_body.authority.epoch != 1
        || prepared.activation_body.authority.valid_from_position != 1
    {
        return Err("prepared formal initialization schema/boundary mismatch".into());
    }
    if prepared
        .activation_body
        .activation_ref()
        .map_err(|error| error.to_string())?
        != prepared.activation_ref
        || compute_request_digest(&prepared.world, &prepared.activation_ref)?
            != prepared.request_digest
        || compute_prepare_digest(prepared)? != prepared.prepare_digest
    {
        return Err("prepared formal initialization identity digest mismatch".into());
    }
    Ok(())
}

fn compute_request_digest(
    world: &FormalWorldIdentityV1,
    activation_ref: &str,
) -> Result<String, String> {
    let bytes = serde_cbor::to_vec(&("oasis7.formal.initial_request.v1", world, activation_ref))
        .map_err(|error| error.to_string())?;
    Ok(blake3_hex(&bytes))
}

fn compute_prepare_digest(prepared: &PreparedWorldInitializationV1) -> Result<String, String> {
    let body = (
        PREPARE_DOMAIN,
        prepared.schema_version,
        prepared.domain.as_str(),
        prepared.mode,
        &prepared.world,
        prepared.snapshot_digest.as_str(),
        prepared.journal_digest.as_str(),
        prepared.trusted_issuer_public_key.as_str(),
        &prepared.node_bootstrap,
        &prepared.activation_body,
        prepared.activation_ref.as_str(),
        prepared.request_digest.as_str(),
    );
    let bytes = serde_cbor::to_vec(&body).map_err(|error| error.to_string())?;
    Ok(blake3_hex(&bytes))
}

fn load_certificate(path: &Path) -> Result<SignedInitialActivationV1, String> {
    let bytes = read_private_owned_bounded(path, MAX_APPROVAL_BYTES)?;
    let input: ActivationCertificateInputV1 = serde_json::from_slice(&bytes)
        .map_err(|error| format!("external signed activation certificate: {error}"))?;
    Ok(input.certificate)
}

fn activation_trust(
    prepared: &PreparedWorldInitializationV1,
) -> Result<InitialActivationTrustV1, String> {
    validate_issuer_public_key(&prepared.trusted_issuer_public_key)?;
    Ok(InitialActivationTrustV1 {
        issuer_public_key: prepared.trusted_issuer_public_key.clone(),
        world: prepared.world.clone(),
        initial_state_root: prepared.activation_body.initial_state_root.clone(),
        governing_manifest_digest: prepared.activation_body.governing_manifest_digest.clone(),
        recovery_policy_digest: prepared
            .activation_body
            .recovery_policy
            .digest()
            .map_err(|error| error.to_string())?,
    })
}

fn validate_issuer_public_key(encoded: &str) -> Result<(), String> {
    let bytes = hex::decode(encoded)
        .map_err(|_| "trusted activation issuer public key must be lowercase 32-byte hex")?;
    if bytes.len() != 32 || hex::encode(&bytes) != encoded {
        return Err("trusted activation issuer public key must be lowercase 32-byte hex".into());
    }
    let key_bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "trusted activation issuer public key must be 32 bytes")?;
    ed25519_dalek::VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| "trusted activation issuer public key is invalid")?;
    Ok(())
}

fn require_exact_approval(
    prepared: &PreparedWorldInitializationV1,
    certificate: &SignedInitialActivationV1,
    authority: &VerifiedAuthorityV1,
) -> Result<(), String> {
    validate_prepared(prepared)?;
    if certificate.body != prepared.activation_body
        || authority.definition() != &prepared.activation_body.authority
        || authority.policy() != &prepared.activation_body.recovery_policy
    {
        return Err("external activation approval differs from the exact prepared request".into());
    }
    Ok(())
}

fn approval_quote(
    prepared: &PreparedWorldInitializationV1,
    trust: &InitialActivationTrustV1,
) -> Result<ApprovalQuoteV1, String> {
    if trust != &activation_trust(prepared)? {
        return Err("activation trust is not the operator-pinned exact prepared trust".into());
    }
    Ok(ApprovalQuoteV1 {
        schema_version: 1,
        domain: "oasis7.formal.world_initialization.quote.v1".into(),
        world: prepared.world.clone(),
        activation_ref: prepared.activation_ref.clone(),
        request_digest: prepared.request_digest.clone(),
        initial_state_root: prepared.activation_body.initial_state_root.clone(),
        runtime_manifest_digest: prepared.activation_body.runtime_manifest_digest.clone(),
        governing_manifest_digest: prepared.activation_body.governing_manifest_digest.clone(),
        execution_policy_digest: prepared.activation_body.execution_policy_digest.clone(),
        writer_public_key: prepared.activation_body.authority.writer_public_key.clone(),
        primary_instance_digest: prepared
            .activation_body
            .authority
            .primary
            .instance
            .digest()
            .map_err(|error| error.to_string())?,
        replica_instance_digest: prepared
            .activation_body
            .authority
            .replica
            .instance
            .digest()
            .map_err(|error| error.to_string())?,
        issuer_public_key: trust.issuer_public_key.clone(),
    })
}

fn absolute_path(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(path)
    };
    validate_path(&absolute)?;
    Ok(absolute)
}

fn resolve_path(base: &Path, path: &Path, description: &str) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() {
        return Err(format!("{description} path is empty"));
    }
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    validate_path(&resolved)?;
    Ok(resolved)
}

fn validate_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().to_string_lossy().len() > MAX_PATH_BYTES {
        return Err("world initialization path exceeds length bound".into());
    }
    Ok(())
}

fn read_existing_signing_key(path: &Path) -> Result<SigningKey, String> {
    let bytes = read_private_owned_bounded(path, 128)?;
    let encoded =
        std::str::from_utf8(&bytes).map_err(|_| "invalid existing writer key encoding")?;
    let decoded =
        hex::decode(encoded.trim()).map_err(|_| "invalid existing writer key encoding")?;
    let secret: [u8; 32] = decoded
        .try_into()
        .map_err(|_| "invalid existing writer key length")?;
    Ok(SigningKey::from_bytes(&secret))
}

fn read_private_owned_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    #[cfg(not(unix))]
    {
        let _ = (path, maximum);
        Err("formal world initialization private input requires Unix".into())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let path = absolute_path(path)?;
        for component in path.ancestors() {
            let metadata = fs::symlink_metadata(component).map_err(|error| error.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err("formal world initialization private input symlink refused".into());
            }
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&path)
            .map_err(|error| error.to_string())?;
        let before = file.metadata().map_err(|error| error.to_string())?;
        // SAFETY: geteuid takes no arguments and reads only the process identity.
        let uid = unsafe { libc::geteuid() };
        if !before.is_file()
            || before.uid() != uid
            || before.mode() & 0o777 != 0o600
            || before.len() > maximum
        {
            return Err("formal world initialization requires a private owned regular file within its size bound".into());
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(maximum.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        let after = file.metadata().map_err(|error| error.to_string())?;
        if bytes.len() as u64 > maximum
            || before.dev() != after.dev()
            || before.ino() != after.ino()
            || before.uid() != after.uid()
            || before.mode() != after.mode()
            || before.len() != after.len()
            || before.mtime() != after.mtime()
            || before.mtime_nsec() != after.mtime_nsec()
            || before.ctime() != after.ctime()
            || before.ctime_nsec() != after.ctime_nsec()
        {
            return Err("formal world initialization input changed while being read".into());
        }
        Ok(bytes)
    }
}

fn write_private_idempotent(path: &Path, bytes: &[u8], maximum: u64) -> Result<(), String> {
    #[cfg(not(unix))]
    {
        let _ = (path, bytes, maximum);
        return Err("formal world initialization private output requires Unix".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        if bytes.len() as u64 > maximum {
            return Err("formal world initialization output exceeds size budget".into());
        }
        let output = absolute_path(path)?;
        let parent = output
            .parent()
            .ok_or("formal initialization output directory unavailable")?;
        super::controlled_live_config::require_private_owned(parent, true)?;
        if output.exists() {
            let old = read_private_owned_bounded(&output, maximum)?;
            return if old == bytes {
                Ok(())
            } else {
                Err("formal initialization output already exists with different contents".into())
            };
        }
        let nonce = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos()
        );
        let temporary = parent.join(format!(".world-init-{nonce}.tmp"));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        let write_result = (|| {
            file.write_all(bytes).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            match fs::hard_link(&temporary, &output) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let old = read_private_owned_bounded(&output, maximum)?;
                    if old != bytes {
                        return Err(
                            "formal initialization output raced with different contents".into()
                        );
                    }
                }
                Err(error) => return Err(error.to_string()),
            }
            FileSync::sync_directory(parent)?;
            Ok(())
        })();
        let _ = fs::remove_file(&temporary);
        write_result
    }
}

#[cfg(unix)]
struct FileSync;
#[cfg(unix)]
impl FileSync {
    fn sync_directory(path: &Path) -> Result<(), String> {
        std::fs::File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
#[path = "world_initialization_tests.rs"]
mod tests;
