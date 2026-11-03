use super::*;
use std::process::Command;

pub(super) struct ApplicationOutputValidationInputs<'a> {
    pub(super) fixture: &'a Fixture,
    pub(super) app_dir: &'a Path,
    pub(super) output: &'a std::process::Output,
    pub(super) metadata: Option<&'a provider_metadata::MetadataServer>,
    pub(super) mode: IsolatedApplicationMode<'a>,
    pub(super) admission_submit_baseline: usize,
}

pub(super) fn validate_application_output_and_remove_fixture(
    inputs: ApplicationOutputValidationInputs<'_>,
) {
    let ApplicationOutputValidationInputs {
        fixture,
        app_dir,
        output,
        metadata,
        mode,
        admission_submit_baseline,
    } = inputs;
    let IsolatedApplicationMode {
        admission_mode,
        hosted,
        release,
        wake,
        drift,
        service_probe,
        metadata_probe,
        ..
    } = mode;
    if hosted {
        if admission_mode == "hosted-wait" {
            application_hosted_wait::report(
                fixture,
                app_dir,
                metadata
                    .as_ref()
                    .unwrap()
                    .decision_count
                    .load(Ordering::SeqCst),
            );
        }
        if admission_mode == "repeated-turns" {
            // A partial durable artifact can fail the parent receipt oracle;
            // emit the actual child result first so it cannot hide its cause.
            println!(
                "repeated_turns_child_exit={}\nchild_stdout:\n{}\nchild_stderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            application_stream_boundaries::validate_repeated_receipts(fixture, app_dir);
        }
        if admission_mode == "fresh-admission" {
            let models = metadata
                .as_ref()
                .unwrap()
                .decision_count
                .load(Ordering::SeqCst);
            let submits = fixture
                .lookup_digests
                .lock()
                .unwrap()
                .iter()
                .filter(|entry| entry.starts_with("submit:"))
                .count()
                - admission_submit_baseline;
            println!(
                "fresh_admission_actual_counts preflight_counter_reset={} model_decisions={models} new_canonical_submits={submits}",
                app_dir.join("fresh-provider-counter-reset").exists()
            );
            if output.status.success() {
                assert!(
                    models > 0,
                    "ordinary fresh provider must actually be invoked"
                );
                assert!(submits >= 3, "fresh Reserve/Prefix/Act admissions required");
            }
        }
        if admission_mode == "missing-store" {
            let submits = fixture
                .lookup_digests
                .lock()
                .unwrap()
                .iter()
                .filter(|entry| entry.starts_with("submit:"))
                .count()
                - admission_submit_baseline;
            println!("missing_store_new_canonical_submits={submits}");
            if output.status.success() {
                assert_eq!(
                    submits, 0,
                    "missing store must not admit new canonical work"
                );
            }
        }
        assert!(
            output.status.success(),
            "hosted application failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let marker = if admission_mode.starts_with("memory-crash") {
            "PRE2_NATIVE_MEMORY_PROCESS_RECOVERY_PASSED"
        } else if admission_mode == "fresh-metadata" {
            "PRE2_FRESH_METADATA_BEFORE_ADMISSION_PASSED"
        } else if admission_mode == "fresh-paused" {
            "PRE2_FRESH_PAUSE_METADATA_NO_ADMISSION_PASSED"
        } else if admission_mode == "fresh-bad-store" {
            "PRE2_FRESH_CHECKPOINT_IO_PRECEDES_ADMISSION_PASSED"
        } else if admission_mode == "hosted-wait" {
            "PRE2_HOSTED_WAIT_RESUME_ACT_PASSED"
        } else if admission_mode == "fresh-admission" {
            "PRE2_HOSTED_FRESH_ADMISSION_CANONICAL_RECEIPT_PASSED"
        } else if admission_mode == "repeated-turns" {
            "PRE2_HOSTED_REPEATED_TURNS_PASSED"
        } else if admission_mode == "slow-consumer" {
            "PRE2_SLOW_CONSUMER_FAIRNESS_PASSED"
        } else if admission_mode == "missing-store" {
            "PRE2_SERVICE_AGENT_MISSING_STORE_BLOCKED"
        } else if admission_mode == "periodic-fairness" {
            "PRE2_HOSTED_PERIODIC_VIEW_FAIRNESS_PASSED"
        } else if service_probe {
            "PRE2_HOSTED_SERVICE_IO_RECONNECT_FAIRNESS_PASSED"
        } else if metadata_probe {
            "PRE2_HOSTED_SLOW_METADATA_SECOND_VIEWER_PASSED"
        } else {
            "PRE2_HOSTED_NATIVE_PROVIDER_CANONICAL_RECEIPT_PASSED"
        };
        assert!(
            stdout.contains(marker),
            "hosted child selected no required proof"
        );
        if matches!(admission_mode, "fresh-admission" | "fresh-metadata") {
            application_completion::verify_canonical_completion(fixture, app_dir);
        }
        if service_probe {
            application_fairness::validate_canonical_identity(fixture);
        }
        println!("{stdout}");
    } else if release {
        application_release::validate_output(fixture, output);
    } else {
        application_harness::validate_output(fixture, output, wake, drift);
        if wake && !drift {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let terminal_feedback_id = stdout
                .lines()
                .find_map(|line| line.strip_prefix("provider_terminal_feedback_id="))
                .expect("canonical terminal feedback evidence missing");
            metadata
                .as_ref()
                .expect("genuine Wait/wake requires a separate provider endpoint")
                .assert_canonical_terminal_feedback_delivery(fixture, terminal_feedback_id);
        }
    }
    fs::remove_dir_all(app_dir).unwrap();
}

pub(super) fn write_fairness_child_artifacts(
    service_probe: bool,
    admission_mode: &str,
    output: &std::process::Output,
) {
    if service_probe || admission_mode == "periodic-fairness" {
        let artifact = std::env::temp_dir().join(format!(
            "pre2-{}-child-{}-{}",
            if service_probe {
                "service-fairness"
            } else {
                "periodic-fairness"
            },
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&artifact).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
        }
        for (name, bytes) in [
            ("stdout.log", output.stdout.clone()),
            ("stderr.log", output.stderr.clone()),
            ("exit-status.txt", output.status.to_string().into_bytes()),
        ] {
            let path = artifact.join(name);
            fs::write(&path, &bytes).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            }
            println!(
                "fairness_child_artifact={} bytes={} blake3={}",
                path.display(),
                bytes.len(),
                blake3::hash(&bytes)
            );
        }
    }
}

pub(super) fn run_memory_crash_mode(
    fixture: &Fixture,
    app_dir: &Path,
    command: &mut Command,
    output: &mut std::process::Output,
    admission_mode: &str,
) -> bool {
    if admission_mode.starts_with("memory-crash") {
        println!("{}", String::from_utf8_lossy(&output.stdout));
        assert_eq!(
            output.status.code(),
            Some(73),
            "memory crash did not reach real persisted Settle boundary: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(app_dir.join("world-settle-started").exists());
        let checkpoint = fs::read(app_dir.join("native-memory-lineage.json")).unwrap();
        fs::write(
            app_dir.join("world-settle-crash-exit73-confirmed"),
            b"actual child exit73",
        )
        .unwrap();
        fs::write(app_dir.join("world-settle-release"), b"release").unwrap();
        let settlement: SubmitIntentRequest<WorldServicePayloadV1> =
            serde_json::from_slice(&fs::read(app_dir.join("world-settle-started")).unwrap())
                .unwrap();
        // committed() requires the signed original identity and rejects canonical rejection.
        let _settled_commit = fixture.committed(&settlement);
        if admission_mode == "memory-crash-tamper" {
            application_memory_tamper::verify_processes(fixture, &checkpoint, command, app_dir);
            fs::remove_dir_all(app_dir).unwrap();
            return true;
        }
        if admission_mode == "memory-crash-gate" {
            application_memory_authority::verify_actual_checkpoint_gate(fixture, &checkpoint);
            fs::remove_dir_all(app_dir).unwrap();
            return true;
        }
        *output = command
            .env("PRE2_APP_ADMISSION", "memory-recover")
            .output()
            .unwrap();
        application_fairness::validate_canonical_identity(fixture);
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let revision = stdout
                .lines()
                .find_map(|line| line.strip_prefix("native_memory_revision_blake3="))
                .expect("recovered memory revision witness missing")
                .to_string();
            println!("{stdout}");
            *output = command
                .env("PRE2_APP_ADMISSION", "memory-repeat")
                .env("PRE2_EXPECT_MEMORY_REVISION", revision)
                .output()
                .unwrap();
            application_fairness::validate_canonical_identity(fixture);
        }
    }
    false
}

pub(super) fn verify_missing_runner_refusal(
    fixture: &Fixture,
    app_dir: &Path,
    output: &std::process::Output,
    metadata: &provider_metadata::MetadataServer,
) {
    metadata.assert_canonical_feedback_delivery(fixture, false);
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "actual missing-runner refusal failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("PRE2_PRIVATE_MEMORY_ACK_MISSING_RUNNER_REFUSED")
    );
    application_feedback_ack_recovery::assert_initial_economic_requests(fixture, 0, 0);
    let refusal: serde_json::Value = serde_json::from_slice(
        &fs::read(app_dir.join("feedback-ack-missing-runner-refused")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        refusal["actual_error"],
        "feedback consumption native runner missing"
    );
    let canonical = fixture.driver.lock().unwrap().execution_world.clone();
    assert!(
        canonical
            .runtime_feedback_outbox()
            .unwrap()
            .iter()
            .any(|r| r.state == "pending")
    );
    let disk: serde_json::Value =
        serde_json::from_slice(&fs::read(app_dir.join("native-memory-lineage.json")).unwrap())
            .unwrap();
    assert!(disk["provider_service_pending"]["agent-a"]["feedback_ack"].is_null());
    assert!(
        disk["provider_memory_store"]["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fs::remove_dir_all(app_dir).unwrap();
}

pub(super) fn retain_hosted_wait_artifact(
    fixture: &Fixture,
    app_dir: &Path,
    output: &std::process::Output,
) {
    let artifact = std::env::temp_dir().join(format!(
        "pre2-hosted-wait-child-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&artifact).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(artifact.join("stdout.log"), &output.stdout).unwrap();
    fs::write(artifact.join("stderr.log"), &output.stderr).unwrap();
    fs::write(artifact.join("exit-status.txt"), output.status.to_string()).unwrap();
    if let Ok(bytes) = fs::read(app_dir.join("hosted-wait-private-lineage.json")) {
        let private_artifact = artifact.join("private-lineage.json");
        fs::write(&private_artifact, &bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&private_artifact, fs::Permissions::from_mode(0o600)).unwrap();
        }
        println!(
            "hosted_wait_private_checkpoint_retained=true bytes={} blake3={}",
            bytes.len(),
            blake3::hash(&bytes)
        );
    }

    // Preserve canonical objects before assertions or Node teardown. The artifact
    // is private; neither signed payloads nor raw rejection reasons are logged.
    let canonical_results = fixture
        .driver
        .lock()
        .unwrap()
        .execution_world
        .capability_revocation_state()
        .world_service_results
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let canonical_bytes = serde_json::to_vec(&canonical_results).unwrap();
    let canonical_file = artifact.join("canonical-results.json");
    fs::write(&canonical_file, &canonical_bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&canonical_file, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let origin_digest = fs::read(app_dir.join("hosted-wait-origin.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|origin| origin["request_digest"].as_str().map(str::to_owned));
    let exact_admits = canonical_results
        .iter()
        .filter(|value| {
            serde_json::from_value::<wire::CanonicalIntentResultV1>((*value).clone())
                .ok()
                .is_some_and(|result| {
                    matches!(
                        &result.request.signed_payload,
                        WorldServicePayloadV1::Scheduler(signed)
                            if matches!(
                                &signed.request.operation,
                                SchedulerOperationV1::AdmitContinuation(proposal)
                                    if origin_digest.as_deref()
                                        == Some(proposal.origin_request_digest.as_str())
                            )
                    )
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    let exact_bytes = serde_json::to_vec(&exact_admits).unwrap();
    let exact_file = artifact.join("exact-admit-results.json");
    fs::write(&exact_file, &exact_bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&exact_file, fs::Permissions::from_mode(0o600)).unwrap();
    }
    println!(
        "hosted_wait_true_exact_admits_retained count={} bytes={} blake3={}",
        exact_admits.len(),
        exact_bytes.len(),
        blake3::hash(&exact_bytes)
    );
    println!(
        "hosted_wait_true_canonical_results_retained count={} bytes={} blake3={}",
        canonical_results.len(),
        canonical_bytes.len(),
        blake3::hash(&canonical_bytes)
    );
    println!(
        "hosted_wait_child_output_artifact={} exit={} stdout_blake3={} stderr_blake3={}",
        artifact.display(),
        output.status,
        blake3::hash(&output.stdout),
        blake3::hash(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));
}
