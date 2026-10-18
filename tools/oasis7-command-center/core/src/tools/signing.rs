mod recovery;
mod submission;
use super::*;
pub use recovery::{reconcile_file_prepare, reconcile_signer_job, recover_file_prepare};
use submission::prepared_identity;
pub use submission::{signer_submit_identity, submit_file_frozen};

pub fn prepare_file(input: &FileSignInput) -> Result<FileSignPrepared> {
    let (resource, fingerprint) = file_prepare_identity(input)?;
    prepare_file_frozen(input, &resource, &fingerprint)
}

pub fn prepare_file_frozen(
    input: &FileSignInput,
    expected_resource: &str,
    expected_fingerprint: &str,
) -> Result<FileSignPrepared> {
    let (cfg, _) = binding()?;
    let context = validate_prepare_input(input, &cfg.deployment_id)?;
    // Carry the admitted identity through every reopen; never adopt replacement bytes.
    let file = Path::new(&input.file_path);
    let (hash, file_size, file_device_id, file_inode) =
        selected_file_identity(file, 1024 * 1024 * 1024)?;
    let (resource, fingerprint) = prepare_identity_from_source(
        input,
        &cfg,
        &context,
        (&hash, file_size, file_device_id, file_inode),
    )?;
    require_prepare_identity(
        &resource,
        &fingerprint,
        expected_resource,
        expected_fingerprint,
    )?;
    let source = FileSourceEvidence {
        schema_version: 1,
        file_path: input.file_path.clone(),
        file_sha256: hash.clone(),
        file_size_bytes: file_size,
        file_device_id,
        file_inode,
    };
    let first = signer_action("prepare", Some(&input.job_id))?;
    if first["status"] != "JOB_DIRECTORY_READY" {
        return Err(invalid(
            "Existing prepared job must be inspected; cannot replace its inputs",
        ));
    }
    let job = oasis7_local_signer::job::JobDirectory::open(
        &cfg,
        installation::current_uid(),
        &input.job_id,
        false,
    )
    .map_err(invalid)?;
    let context_bytes = identity::canonical_json(&context).map_err(invalid)?;
    write_private_new(&job.path().join("context.json"), &context_bytes)?;
    if !source_is_current(&source)? {
        return Err(invalid(
            "Original admitted file changed before payload preparation",
        ));
    }
    // file_cli verifies the frozen digest; source inode is checked again afterward.
    let args = vec![
        "file-payload".into(),
        "--context".into(),
        job.path().join("context.json").display().to_string(),
        "--expected-context-sha256".into(),
        identity::sha256_hex(&context_bytes),
        "--file".into(),
        input.file_path.clone(),
        "--expected-file-sha256".into(),
        hash.clone(),
        "--output".into(),
        job.path().join("payload.bin").display().to_string(),
    ];
    let payload = oasis7_local_signer::file_cli::dispatch(&args)
        .unwrap()
        .map_err(invalid)?;
    if !source_is_current(&source)? {
        return Err(invalid(
            "Original source file identity changed during payload preparation",
        ));
    }
    write_private_new(
        &job.path().join("command-center-source.json"),
        &identity::canonical_json(&source).map_err(invalid)?,
    )?;
    let input_domain = oasis7_local_signer::job::JobInput {
        schema_version: oasis7_local_signer::job::JOB_INPUT_SCHEMA.into(),
        installation_id: cfg.installation_id.clone(),
        purpose: "file_ed25519_v1".into(),
        provider_id: protocol::ExplicitNull(None),
        signer_id: input.signer_id.clone(),
        grant_id: protocol::ExplicitNull(Some(input.grant_id.clone())),
        context: context.clone(),
    };
    write_private_new(
        &job.path().join("input.json"),
        &identity::canonical_json(&input_domain).map_err(invalid)?,
    )?;
    let digest = payload["payload_sha256"]
        .as_str()
        .ok_or_else(|| invalid("payload digest absent"))?
        .to_owned();
    let operation_key = identity::operation_key(
        &cfg.installation_id,
        &cfg.deployment_id,
        &context.network_id,
        "file_ed25519_v1",
        &input.signer_id,
        &identity::parse_sha256_hex(&digest).map_err(invalid)?,
    )
    .map_err(invalid)?;
    let result = signer_action("prepare", Some(&input.job_id))?;
    let prepared = read_prepared(&job)?;
    let actual_source = read_source_evidence(&job, &prepared, true)?;
    if prepared.request.context != context
        || prepared.request.installation_id != cfg.installation_id
        || prepared.request.purpose != "file_ed25519_v1"
        || prepared.request.provider_id.0.is_some()
        || prepared.request.signer_id != input.signer_id
        || prepared.request.grant_id.0.as_deref() != Some(&input.grant_id)
        || prepared.payload
            != oasis7_signer_protocol::detached::signing_payload_from_digest(
                &context, &hash, file_size,
            )
            .map_err(invalid)?
        || identity::canonical_json(&actual_source).map_err(invalid)?
            != identity::canonical_json(&source).map_err(invalid)?
    {
        return Err(invalid(
            "Prepared request differs from original admitted intent",
        ));
    }
    let prepared_fingerprint = prepared_identity(&input.job_id, &cfg, &prepared, &source)?.1;
    Ok(FileSignPrepared {
        job_id: input.job_id.clone(),
        prepared_fingerprint,
        file_sha256: hash,
        payload_sha256: digest,
        operation_key,
        work_dir: job.path().display().to_string(),
        result,
    })
}
fn write_private_new(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut f = options.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

/// Identity for admission of submit/inspect. The request is never inferred from a job name.
pub fn signer_job_identity(job_id: &str) -> Result<(String, String)> {
    frozen_job_identity(job_id, true)
}
fn frozen_job_identity(job_id: &str, verify_source: bool) -> Result<(String, String)> {
    let (cfg, _) = binding()?;
    let job = oasis7_local_signer::job::JobDirectory::open(
        &cfg,
        installation::current_uid(),
        job_id,
        false,
    )
    .map_err(invalid)?;
    let prepared = read_prepared(&job)?;
    let source = read_source_evidence(&job, &prepared, verify_source)?;
    prepared_identity(job_id, &cfg, &prepared, &source)
}
pub fn verify_file_signature(
    file_path: &Path,
    context: serde_json::Value,
    expected_public_key: Vec<u8>,
    signature: Vec<u8>,
) -> Result<bool> {
    if !file_path.is_absolute() {
        return Err(invalid("Absolute selected file required"));
    }
    let context: protocol::SignContext = serde_json::from_value(context)?;
    let key: [u8; 32] = expected_public_key
        .try_into()
        .map_err(|_| invalid("Expected Ed25519 public key must have 32 bytes"))?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| invalid("Ed25519 signature must have 64 bytes"))?;
    let (digest, size) = file_digest(file_path)?;
    oasis7_signer_protocol::detached::verify_file_digest_signature(
        &context, &digest, size, &key, &signature,
    )
    .map_err(invalid)?;
    Ok(true)
}

fn file_digest(path: &Path) -> Result<(String, u64)> {
    selected_file_digest(path, 1024 * 1024 * 1024)
}

pub fn selected_file_digest(path: &Path, max_bytes: u64) -> Result<(String, u64)> {
    let (digest, size, _, _) = selected_file_identity(path, max_bytes)?;
    Ok((digest, size))
}
fn selected_file_identity(path: &Path, max_bytes: u64) -> Result<(String, u64, u64, u64)> {
    if !path.is_absolute() {
        return Err(invalid("Absolute selected file required"));
    }
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    let mut file = options.open(path)?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() > max_bytes {
        return Err(invalid(
            "Selected file must be regular and within the allowed size",
        ));
    }
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > before.len() {
            return Err(invalid("Selected file changed while reading"));
        }
        hash.update(&buffer[..n]);
    }
    let after = file.metadata()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.nlink() != 1
            || (
                before.dev(),
                before.ino(),
                before.len(),
                before.mtime(),
                before.mtime_nsec(),
                before.ctime(),
                before.ctime_nsec(),
            ) != (
                after.dev(),
                after.ino(),
                after.len(),
                after.mtime(),
                after.mtime_nsec(),
                after.ctime(),
                after.ctime_nsec(),
            )
        {
            return Err(invalid("Selected file identity changed"));
        }
    }
    if size != before.len() || before.modified()? != after.modified()? {
        return Err(invalid("Selected file changed while reading"));
    }
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        (before.dev(), before.ino())
    };
    #[cfg(not(unix))]
    let identity = (0, 0);
    Ok((
        format!("{:x}", hash.finalize()),
        size,
        identity.0,
        identity.1,
    ))
}

pub(super) fn compatible_info(value: &serde_json::Value) -> bool {
    value["schema_version"] == 1
        && value["tool_id"] == "oasis7_local_signer"
        && value["capabilities"].as_array().is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| entry["id"] == "file_ed25519_v1" && entry["supported"] == true)
        })
        && value["protocol_schemas"]
            .as_array()
            .is_some_and(|schemas| schemas.iter().any(|schema| schema == protocol::IPC_SCHEMA))
}

pub fn file_prepare_identity(input: &FileSignInput) -> Result<(String, String)> {
    let (cfg, _) = binding()?;
    let context = validate_prepare_input(input, &cfg.deployment_id)?;
    let (digest, size, device_id, inode) =
        selected_file_identity(Path::new(&input.file_path), 1024 * 1024 * 1024)?;
    prepare_identity_from_source(input, &cfg, &context, (&digest, size, device_id, inode))
}
fn prepare_identity_from_source(
    input: &FileSignInput,
    cfg: &oasis7_signer_protocol::types::InstallationConfig,
    context: &protocol::SignContext,
    source: (&str, u64, u64, u64),
) -> Result<(String, String)> {
    let (digest, size, device_id, inode) = source;
    let context_bytes = identity::canonical_json(&context).map_err(invalid)?;
    let fingerprint = identity::sha256_hex(
        &identity::canonical_json(&(
            cfg.installation_id.clone(),
            &cfg.deployment_id,
            &cfg.release_id,
            input,
            digest,
            size,
            device_id,
            inode,
            identity::sha256_hex(&context_bytes),
        ))
        .map_err(invalid)?,
    );
    Ok((
        format!("signer:{}:{}", cfg.installation_id, input.job_id),
        fingerprint,
    ))
}
fn require_prepare_identity(
    resource: &str,
    fingerprint: &str,
    expected_resource: &str,
    expected_fingerprint: &str,
) -> Result<()> {
    if resource != expected_resource || fingerprint != expected_fingerprint {
        return Err(invalid("Original admitted preparation identity changed"));
    }
    Ok(())
}

/// Resolve the real reservation before validating other user input. `None` is
/// only for an invalid Job identifier; a missing/untrusted binding is an error.
pub fn file_prepare_resource(job_id: &str) -> Result<Option<String>> {
    if protocol::validate_id(job_id).is_err() {
        return Ok(None);
    }
    let (cfg, _) = binding()?;
    Ok(Some(format!("signer:{}:{}", cfg.installation_id, job_id)))
}

// This function and selected_file_identity are read-only. Keep all ordinary
// correctable input validation ahead of the first caller prepare invocation.
fn validate_prepare_input(
    input: &FileSignInput,
    deployment_id: &str,
) -> Result<protocol::SignContext> {
    let context: protocol::SignContext = serde_json::from_value(input.context.clone())?;
    oasis7_signer_protocol::detached::signing_payload(&context, b"").map_err(invalid)?;
    if context.deployment_id != deployment_id {
        return Err(invalid("deployment does not match installation"));
    }
    protocol::validate_id(&input.job_id).map_err(invalid)?;
    protocol::validate_id(&input.signer_id).map_err(invalid)?;
    protocol::validate_id(&input.grant_id).map_err(invalid)?;
    if !Path::new(&input.file_path).is_absolute() {
        return Err(invalid("absolute selected file required"));
    }
    Ok(context)
}

fn read_prepared(
    job: &oasis7_local_signer::job::JobDirectory,
) -> Result<oasis7_local_signer::job::PreparedJob> {
    let uid = installation::current_uid();
    let request = job.read_request().map_err(invalid)?;
    let payload = oasis7_local_signer::read_candidate_bytes(
        &job.path().join("payload.bin"),
        uid,
        protocol::MAX_ROLLBACK_PAYLOAD_BYTES,
    )
    .map_err(invalid)?;
    let hash = oasis7_local_signer::read_candidate_bytes(
        &job.path().join("request.payload.sha256"),
        uid,
        64,
    )
    .map_err(invalid)?;
    let input: oasis7_local_signer::job::JobInput = serde_json::from_slice(
        &oasis7_local_signer::read_candidate_bytes(
            &job.path().join("input.json"),
            uid,
            protocol::MAX_METADATA_BYTES,
        )
        .map_err(invalid)?,
    )?;
    if hash != identity::sha256_hex(&payload).as_bytes()
        || !oasis7_local_signer::job::job_request_matches_input(&input, &request)
    {
        return Err(invalid("Prepared request input or payload drift"));
    }
    oasis7_signer_protocol::detached::validate_signing_payload(&payload, &request.context)
        .map_err(invalid)?;
    Ok(oasis7_local_signer::job::PreparedJob { request, payload })
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSourceEvidence {
    schema_version: u32,
    file_path: String,
    file_sha256: String,
    file_size_bytes: u64,
    file_device_id: u64,
    file_inode: u64,
}
fn read_source_evidence(
    job: &oasis7_local_signer::job::JobDirectory,
    prepared: &oasis7_local_signer::job::PreparedJob,
    verify_source: bool,
) -> Result<FileSourceEvidence> {
    let bytes = oasis7_local_signer::read_candidate_bytes(
        &job.path().join("command-center-source.json"),
        installation::current_uid(),
        16384,
    )
    .map_err(invalid)?;
    let source: FileSourceEvidence = serde_json::from_slice(&bytes)?;
    if source.schema_version != 1
        || source.file_path.len() > 4096
        || !Path::new(&source.file_path).is_absolute()
    {
        return Err(invalid("Original source evidence is invalid"));
    }
    let envelope = oasis7_signer_protocol::detached::validate_signing_payload(
        &prepared.payload,
        &prepared.request.context,
    )
    .map_err(invalid)?;
    if source.file_sha256 != envelope.file_sha256
        || source.file_size_bytes != envelope.file_size_bytes
    {
        return Err(invalid(
            "Original selected-file evidence differs from frozen envelope",
        ));
    }
    if verify_source && !source_is_current(&source)? {
        return Err(invalid(
            "Original selected file changed after preparation; prepare a new request with explicit authorization",
        ));
    }
    Ok(source)
}

fn source_is_current(source: &FileSourceEvidence) -> Result<bool> {
    Ok(
        selected_file_identity(Path::new(&source.file_path), 1024 * 1024 * 1024)?
            == (
                source.file_sha256.clone(),
                source.file_size_bytes,
                source.file_device_id,
                source.file_inode,
            ),
    )
}

#[cfg(all(test, unix))]
mod file_tests;
