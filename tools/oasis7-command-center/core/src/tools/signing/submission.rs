use super::*;

pub(super) fn prepared_identity(
    job_id: &str,
    cfg: &oasis7_signer_protocol::types::InstallationConfig,
    prepared: &oasis7_local_signer::job::PreparedJob,
    source: &FileSourceEvidence,
) -> Result<(String, String)> {
    if prepared.request.installation_id != cfg.installation_id
        || prepared.request.context.deployment_id != cfg.deployment_id
    {
        return Err(invalid("Prepared request belongs to another installation"));
    }
    let fingerprint = identity::sha256_hex(
        &identity::canonical_json(&(
            cfg.installation_id.clone(),
            &cfg.deployment_id,
            &cfg.release_id,
            &prepared.request,
            identity::sha256_hex(&prepared.payload),
            source,
        ))
        .map_err(invalid)?,
    );
    Ok((
        format!("signer:{}:{}", cfg.installation_id, job_id),
        fingerprint,
    ))
}

/// Read-only capability validation must precede durable submit admission.
pub fn signer_submit_identity(job_id: &str) -> Result<(String, String)> {
    let (_, executable) = binding()?;
    let info = caller_submit_info(&executable)?;
    with_submit_capability(&info, || signer_job_identity(job_id))
}

fn require_caller_submit_capability(executable: &Path) -> Result<()> {
    with_submit_capability(&caller_submit_info(executable)?, || Ok(()))
}

fn caller_submit_info(executable: &Path) -> Result<serde_json::Value> {
    let binary = executable
        .to_str()
        .ok_or_else(|| invalid("Caller path must be UTF-8"))?;
    Ok(serde_json::from_slice(&crate::process::run(
        binary,
        &["info", "--json"],
        Path::new("/"),
    )?)?)
}

fn with_submit_capability<T>(
    info: &serde_json::Value,
    work: impl FnOnce() -> Result<T>,
) -> Result<T> {
    require_frozen_submit_capability(info)?;
    work()
}

fn require_frozen_submit_capability(info: &serde_json::Value) -> Result<()> {
    if !compatible_info(info)
        || !info["capabilities"].as_array().is_some_and(|capabilities| {
            capabilities
                .iter()
                .any(|value| value["id"] == "frozen_submit_v1" && value["supported"] == true)
        })
    {
        return Err(invalid(
            "Installed caller lacks frozen_submit_v1; upgrade the caller before submitting",
        ));
    }
    Ok(())
}

pub fn submit_file_frozen(job_id: &str, expected_fingerprint: &str) -> Result<serde_json::Value> {
    let (cfg, executable) = binding()?;
    let job = oasis7_local_signer::job::JobDirectory::open(
        &cfg,
        installation::current_uid(),
        job_id,
        false,
    )
    .map_err(invalid)?;
    let prepared = read_prepared(&job)?;
    let source = read_source_evidence(&job, &prepared, true)?;
    if prepared_identity(job_id, &cfg, &prepared, &source)?.1 != expected_fingerprint {
        return Err(invalid("Original previewed signing request changed"));
    }
    let expected_request = prepared.submission_identity(&cfg).map_err(invalid)?;
    let binary = executable
        .to_str()
        .ok_or_else(|| invalid("Caller path must be UTF-8"))?;
    // Recheck adjacent to execution; a changed caller after admission stays unknown.
    require_caller_submit_capability(&executable)?;
    Ok(serde_json::from_slice(&crate::process::run(
        binary,
        &[
            "submit",
            "--job-id",
            job_id,
            "--expected-request-sha256",
            &expected_request,
        ],
        Path::new("/"),
    )?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_caller_preflight_never_admits_and_supported_caller_can_retry() {
        let jobs = jobs::ToolJobs::open(":memory:").unwrap();
        let mut info = serde_json::json!({"schema_version":1,"tool_id":"oasis7_local_signer",
            "protocol_schemas":[protocol::IPC_SCHEMA],
            "capabilities":[{"id":"file_ed25519_v1","supported":true}]});
        assert!(compatible_info(&info));
        let mut attempted = false;
        let rejected = with_submit_capability(&info, || {
            attempted = true;
            jobs.admit("original", "frozen", "signer:installation:job", "submit")
        });
        assert!(rejected.is_err());
        assert!(!attempted);
        assert!(jobs.list().unwrap().is_empty());
        assert!(!jobs.quit_pending().unwrap());
        info["capabilities"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id":"frozen_submit_v1","supported":true}));
        with_submit_capability(&info, || {
            jobs.admit("original", "frozen", "signer:installation:job", "submit")
        })
        .unwrap();
        assert_eq!(jobs.list().unwrap()[0].state, "running");
        // A post-admission capability change never changes the existing reservation to failed.
        info["capabilities"].as_array_mut().unwrap().pop();
        assert!(require_frozen_submit_capability(&info).is_err());
        assert_eq!(jobs.list().unwrap()[0].state, "running");
    }
}
