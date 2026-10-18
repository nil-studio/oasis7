use super::*;

/// Reconcile only the original submitted identity. Missing/reserved observations do
/// not prove no eventual effect and deliberately retain the unknown reservation.
pub fn reconcile_signer_job(
    jobs: &jobs::ToolJobs,
    job_id: &str,
    expected_fingerprint: Option<&str>,
) -> Result<serde_json::Value> {
    let (cfg, _) = binding()?;
    let directory = oasis7_local_signer::job::JobDirectory::open(
        &cfg,
        installation::current_uid(),
        job_id,
        false,
    )
    .map_err(invalid)?;
    let prepared = read_prepared(&directory)?;
    let source = read_source_evidence(&directory, &prepared, false)?;
    let (resource, fingerprint) = prepared_identity(job_id, &cfg, &prepared, &source)?;
    require_expected_inspection(&fingerprint, expected_fingerprint)?;
    let result = signer_action("inspect", Some(job_id))?;
    let response: protocol::IpcResponse = serde_json::from_value(result.clone())?;
    if let protocol::IpcResponse::Inspect(response) = response {
        if response.request_id != prepared.request.request_id {
            return Err(invalid("Inspection changed original request identity"));
        }
        if response.status == "committed" && response.error_code.0.is_none() {
            let expected_operation = identity::operation_key(
                &cfg.installation_id,
                &cfg.deployment_id,
                &prepared.request.context.network_id,
                &prepared.request.purpose,
                &prepared.request.signer_id,
                &identity::sha256(&prepared.payload),
            )
            .map_err(invalid)?;
            if response.operation_key.0.as_deref() != Some(&expected_operation) {
                return Err(invalid(
                    "Committed operation key differs from original envelope",
                ));
            }
            if response.payload_sha256.0.as_deref()
                != Some(&identity::sha256_hex(&prepared.payload))
                || response.audit_digest.0.is_none()
            {
                return Err(invalid(
                    "Committed observation does not bind original payload/audit",
                ));
            }
            finish_committed_submissions(jobs, &resource, &fingerprint)?;
        }
    } else {
        return Err(invalid("Expected original request inspection"));
    }
    Ok(result)
}

fn require_expected_inspection(actual: &str, expected: Option<&str>) -> Result<()> {
    if expected.is_some_and(|value| value != actual) {
        return Err(invalid(
            "Inspected Job differs from original prepared preview",
        ));
    }
    Ok(())
}

fn finish_committed_submissions(
    jobs: &jobs::ToolJobs,
    resource: &str,
    fingerprint: &str,
) -> Result<()> {
    let matching: Vec<_> = jobs
        .list()?
        .into_iter()
        .filter(|job| {
            jobs::same_reserved_resource(&job.resource, resource)
                && job.kind == "submit"
                && job.state == "unknown"
        })
        .collect();
    // Check every matched immutable record before changing any of their states.
    if matching.iter().any(|job| job.fingerprint != fingerprint) {
        return Err(invalid(
            "Original submitted fingerprint differs; retain unknown",
        ));
    }
    for job in matching {
        jobs.finish(
            &job.id,
            "succeeded",
            Some("Original request committed; verified inspect receipt"),
        )?;
    }
    Ok(())
}

pub fn recover_file_prepare(
    jobs: &jobs::ToolJobs,
    operation_id: &str,
) -> Result<FilePrepareRecovery> {
    let input = jobs.file_prepare_input(operation_id)?.ok_or_else(|| {
        invalid("Original preparation input unavailable; legacy operation cannot be recovered")
    })?;
    let prepared = reconcile_file_prepare(jobs, &input, operation_id)?;
    Ok(FilePrepareRecovery {
        operation_id: operation_id.into(),
        input,
        prepared,
    })
}

pub fn reconcile_file_prepare(
    jobs: &jobs::ToolJobs,
    input: &FileSignInput,
    operation_id: &str,
) -> Result<FileSignPrepared> {
    let (resource, fingerprint) = file_prepare_identity(input)?;
    let operation = jobs
        .list()?
        .into_iter()
        .find(|job| job.id == operation_id)
        .ok_or_else(|| invalid("Original preparation operation is absent"))?;
    validate_prepare_operation(&operation, &resource, &fingerprint)?;
    let (cfg, _) = binding()?;
    let job = oasis7_local_signer::job::JobDirectory::open(
        &cfg,
        installation::current_uid(),
        &input.job_id,
        false,
    )
    .map_err(invalid)?;
    let prepared = read_prepared(&job)?;
    let source_evidence = read_source_evidence(&job, &prepared, true)?;
    if source_evidence.file_path != input.file_path {
        return Err(invalid(
            "Original selected file path differs from preparation evidence",
        ));
    }
    let context: protocol::SignContext = serde_json::from_value(input.context.clone())?;
    let (digest, size) = file_digest(Path::new(&input.file_path))?;
    if prepared.request.installation_id != cfg.installation_id
        || prepared.request.purpose != "file_ed25519_v1"
        || prepared.request.provider_id.0.is_some()
        || prepared.request.context.deployment_id != cfg.deployment_id
        || prepared.request.context != context
        || prepared.request.signer_id != input.signer_id
        || prepared.request.grant_id.0.as_deref() != Some(&input.grant_id)
        || prepared.payload
            != oasis7_signer_protocol::detached::signing_payload_from_digest(
                &context, &digest, size,
            )
            .map_err(invalid)?
    {
        return Err(invalid("Prepared files do not match original intent"));
    }
    // Fingerprint the exact snapshot whose intent was verified above; reopening
    // here could bind a replacement request to the original input.
    let prepared_fingerprint =
        prepared_identity(&input.job_id, &cfg, &prepared, &source_evidence)?.1;
    let payload_sha256 = identity::sha256_hex(&prepared.payload);
    let operation_key = identity::operation_key(
        &cfg.installation_id,
        &cfg.deployment_id,
        &context.network_id,
        &prepared.request.purpose,
        &input.signer_id,
        &identity::sha256(&prepared.payload),
    )
    .map_err(invalid)?;
    let result = FileSignPrepared {
        job_id: input.job_id.clone(),
        prepared_fingerprint,
        file_sha256: digest,
        payload_sha256,
        operation_key,
        work_dir: job.path().display().to_string(),
        result: serde_json::json!({"status":"PREPARED","job_id":input.job_id,"request_id":prepared.request.request_id,"signing_enabled":false}),
    };
    if operation.state == "unknown" {
        jobs.finish(
            operation_id,
            "succeeded",
            Some("Original prepared request files verified"),
        )?;
    }
    Ok(result)
}

fn validate_prepare_operation(
    operation: &jobs::ToolJob,
    resource: &str,
    fingerprint: &str,
) -> Result<()> {
    if operation.resource != resource
        || operation.kind != "file_prepare"
        || operation.fingerprint != fingerprint
        || !matches!(operation.state.as_str(), "unknown" | "succeeded")
    {
        return Err(invalid(
            "Original preparation operation identity or state differs",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspection_reset_receipt_requires_original_preview_identity() {
        assert!(require_expected_inspection("original", Some("original")).is_ok());
        assert!(require_expected_inspection("replacement", Some("original")).is_err());
        assert!(require_expected_inspection("original", None).is_ok());
    }
    #[test]
    fn verified_receipt_resolves_original_ascii_alias_only_with_exact_fingerprint() {
        let jobs = jobs::ToolJobs::open(":memory:").unwrap();
        jobs.admit("original", "frozen", "signer:install:JobOne", "submit")
            .unwrap();
        jobs.finish("original", "unknown", None).unwrap();
        assert!(
            finish_committed_submissions(&jobs, "signer:install:jobone", "replacement").is_err()
        );
        assert_eq!(jobs.list().unwrap()[0].state, "unknown");
        finish_committed_submissions(&jobs, "signer:install:jobone", "frozen").unwrap();
        assert_eq!(jobs.list().unwrap()[0].state, "succeeded");
        assert!(!jobs.quit_pending().unwrap());
        jobs.admit(
            "next-explicit",
            "next-frozen",
            "signer:install:jobone",
            "submit",
        )
        .unwrap();

        let ordinary = jobs::ToolJobs::open(":memory:").unwrap();
        ordinary
            .admit("ordinary", "frozen", "context:Library", "submit")
            .unwrap();
        ordinary.finish("ordinary", "unknown", None).unwrap();
        finish_committed_submissions(&ordinary, "context:library", "frozen").unwrap();
        assert_eq!(ordinary.list().unwrap()[0].state, "unknown");
        assert!(!jobs::same_reserved_resource(
            "signer:install:JobÉ",
            "signer:install:jobé"
        ));
    }
    #[test]
    fn recovery_requires_exact_original_operation_and_quiescent_state() {
        let jobs = jobs::ToolJobs::open(":memory:").unwrap();
        let (mut operation, _) = jobs
            .admit_once("original", "frozen", "signer:install:job", "file_prepare")
            .unwrap();
        assert!(validate_prepare_operation(&operation, "signer:install:job", "frozen").is_err());
        operation.state = "unknown".into();
        assert!(validate_prepare_operation(&operation, "signer:install:job", "frozen").is_ok());
        assert!(validate_prepare_operation(&operation, "signer:install:other", "frozen").is_err());
        assert!(validate_prepare_operation(&operation, "signer:install:job", "changed").is_err());
        operation.state = "failed".into();
        assert!(validate_prepare_operation(&operation, "signer:install:job", "frozen").is_err());
        operation.state = "succeeded".into();
        assert!(validate_prepare_operation(&operation, "signer:install:job", "frozen").is_ok());
        operation.kind = "submit".into();
        assert!(validate_prepare_operation(&operation, "signer:install:job", "frozen").is_err());
    }
}
