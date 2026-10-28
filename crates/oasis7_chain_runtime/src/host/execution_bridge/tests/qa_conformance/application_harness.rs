//! Parent-side sandbox result and canonical receipt evidence, separate from application execution.
use super::*;

#[path = "application_harness_phases.rs"]
mod phases;
use phases::*;
pub(super) fn validate_output(
    fixture: &Fixture,
    output: &std::process::Output,
    wake: bool,
    drift: bool,
) {
    if !output.status.success() {
        eprintln!(
            "fixture_failure_stages={}",
            fixture.stage_diagnostics.snapshot()
        );
        application_wake::report_canonical_resume_failure(fixture);
    }
    assert!(
        output.status.success(),
        "sandbox application failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let marker = if drift {
        "PRE2_APPLICATION_RESOURCE_DRIFT_WAKE_REJECTED"
    } else {
        application_wake::required_child_marker(wake)
    };
    assert!(
        stdout.contains(marker),
        "child test filter ran no required proof"
    );
    if !drift {
        let feedback_id = stdout
            .lines()
            .find_map(|line| {
                line.split_once("provider_terminal_feedback_id=")
                    .map(|(_, id)| id.trim())
            })
            .expect("provider feedback evidence missing");
        let world = fixture.driver.lock().unwrap().execution_world.clone();
        let feedback = world
            .runtime_feedback_outbox()
            .unwrap()
            .into_iter()
            .find(|record| record.feedback_id == feedback_id)
            .expect("child terminal feedback must exist in canonical outbox");
        let receipt_id = feedback.payload["runtime_receipt_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .expect("canonical provider receipt identity missing");
        let lineage = world.read_runtime_receipt_lineage(receipt_id).unwrap();
        world.verify_runtime_receipt_lineage(&lineage).unwrap();
        assert_eq!(lineage.feedback_id, feedback_id);
        if wake {
            validate_resume_receipt(fixture, &stdout);
        }
    } else {
        application_wake::report_canonical_resume_failure(fixture);
        let world = fixture.driver.lock().unwrap().execution_world.clone();
        let results = world.capability_revocation_state();
        assert!(results.world_service_results.values().any(|value| {
            value["rejected"]
                .as_str()
                .is_some_and(|reason| reason.contains("cognition_context_mismatch"))
        }));
        assert!(
            !world.cognition_in_flight_wakes().unwrap().is_empty(),
            "rejected Resume consumed canonical wake"
        );
    }
    println!("{stdout}");
}

fn validate_resume_receipt(fixture: &Fixture, stdout: &str) {
    let original = stdout
        .lines()
        .find_map(|line| line.strip_prefix("provider_original_continuation_id="))
        .expect("original continuation evidence missing");
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let handoff=world.capability_revocation_state().world_service_results.values()
        .filter_map(|value|serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok())
        .filter(|result|result.rejected.is_none() && matches!(&result.request.signed_payload,WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation,SchedulerOperationV1::ResumeWake {..})))
        .filter_map(|result|serde_json::from_value::<oasis7_runtime::CognitionWakeHandoffResultV1>(result.receipt).ok())
        .find(|result|result.continuation.continuation_id==original).expect("actual Resume receipt association missing");
    assert_eq!(
        handoff.continuation.status,
        oasis7_runtime::ContinuationStatusV1::Consumed
    );
    let next = handoff
        .replanned_continuation
        .expect("actual Resume successor receipt missing");
    assert_ne!(next.continuation_id, original);
    assert_eq!(next.status, oasis7_runtime::ContinuationStatusV1::Scheduled);
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let mut request = fixture.view(None);
    request.scope_id = "agent:agent-a".into();
    let current = client.read_view(request).unwrap();
    assert!(
        current
            .projection()
            .continuations
            .iter()
            .any(|entry| entry.continuation_id == next.continuation_id
                && entry.status == next.status)
    );
    println!(
        "PRE2_CANONICAL_RESUME_RECEIPT_SUCCESSOR_ASSOCIATION_PASSED original_consumed=true successor_scheduled=true"
    );
}

pub(super) fn run_isolated_application(
    wake: bool,
    drift: bool,
    release: bool,
    hosted: bool,
    metadata_probe: bool,
    service_probe: bool,
) {
    run_isolated_application_mode(
        wake,
        drift,
        release,
        hosted,
        metadata_probe,
        service_probe,
        "",
    );
}

#[derive(Clone, Copy)]
struct IsolatedApplicationMode<'a> {
    wake: bool,
    drift: bool,
    release: bool,
    hosted: bool,
    metadata_probe: bool,
    service_probe: bool,
    admission_mode: &'a str,
    provider_closure: bool,
}

pub(super) fn run_isolated_application_mode(
    wake: bool,
    drift: bool,
    release: bool,
    hosted: bool,
    metadata_probe: bool,
    service_probe: bool,
    admission_mode: &str,
) {
    run_isolated_application_mode_with_options(IsolatedApplicationMode {
        wake,
        drift,
        release,
        hosted,
        metadata_probe,
        service_probe,
        admission_mode,
        provider_closure: false,
    });
}

pub(super) fn run_isolated_application_provider_closure() {
    run_isolated_application_mode_with_options(IsolatedApplicationMode {
        wake: false,
        drift: false,
        release: false,
        hosted: false,
        metadata_probe: false,
        service_probe: false,
        admission_mode: "",
        provider_closure: true,
    });
}

fn run_isolated_application_mode_with_options(mode: IsolatedApplicationMode<'_>) {
    let IsolatedApplicationMode {
        wake,
        drift,
        release,
        hosted,
        metadata_probe,
        service_probe,
        admission_mode,
        provider_closure,
    } = mode;
    let resume_rejection_fs = admission_mode == "resume-rejection-write-failure";
    let resume_rejection_crash = admission_mode == "resume-rejection-crash";
    let resume_rejected = matches!(
        admission_mode,
        "resume-rejected" | "resume-rejection-write-failure" | "resume-rejection-crash"
    );
    let rejection_fs = admission_mode == "wait-rejection-write-failure";
    let rejection_crash = admission_mode == "wait-rejection-crash";
    let wait_rejected = matches!(
        admission_mode,
        "wait-admit-rejected" | "wait-rejection-write-failure" | "wait-rejection-crash"
    );
    let resume_crash = admission_mode == "hosted-wait-resume-crash";
    let fs_handoff = admission_mode == "hosted-resume-write-failure";
    let fs_wait = admission_mode == "hosted-wait-write-failure";
    let capture_crash = matches!(
        admission_mode,
        "wait-capture-crash" | "wait-capture-nonempty"
    );
    let admit_crash = admission_mode == "wait-admit-crash";
    let final_budget_fs = admission_mode == "hosted-final-budget-write-failure";
    let final_budget_crash = admission_mode == "hosted-final-budget-crash";
    let final_budget = matches!(
        admission_mode,
        "hosted-final-budget" | "hosted-final-budget-write-failure" | "hosted-final-budget-crash"
    );
    let ordinary_wait = final_budget
        || resume_rejected
        || admission_mode == "hosted-wait"
        || resume_crash
        || fs_handoff
        || fs_wait
        || admit_crash
        || capture_crash
        || wait_rejected;
    let fixture = application_fixture(wake, drift, ordinary_wait);
    let app_dir = temp_dir("qa-world-service-application");
    fs::create_dir_all(&app_dir).unwrap();
    if matches!(
        admission_mode,
        "memory-ack-before" | "memory-ack-after" | "memory-ack-write-failure"
    ) {
        fs::write(
            app_dir.join("world-feedback-ack-single-submit-proof"),
            b"await actual canonical commit before releasing original response",
        )
        .unwrap();
    }
    if admission_mode.starts_with("memory-ack-") {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&app_dir, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    // Controlled Submit persistence runs after its HTTP response. Prepare the
    // bounded listener before any child constructor/Reserve/Prefix, rather
    // than waiting for a child that needs those requests to write readiness.
    // Fault gates still require their separate arm/release markers. CAS
    // publication, the driver commit lock and worker teardown are unchanged.
    fixture.concurrent_dispatch.store(true, Ordering::SeqCst);
    *fixture.world_gate.root.lock().unwrap() = Some(app_dir.clone());
    fs::write(app_dir.join("world-concurrent-ready"), b"ready").unwrap();
    if matches!(admission_mode, "memory-ack-before" | "memory-ack-after") {
        *fixture.world_gate.root.lock().unwrap() = Some(app_dir.clone());
        let kind = if admission_mode == "memory-ack-before" {
            "before"
        } else {
            "after"
        };
        fs::write(
            app_dir.join(format!("world-feedback-ack-{kind}-arm")),
            b"arm",
        )
        .unwrap();
    }
    if admission_mode.starts_with("memory-crash") {
        *fixture.world_gate.root.lock().unwrap() = Some(app_dir.clone());
        fs::write(app_dir.join("world-settle-arm"), b"arm").unwrap();
    }
    let registration = fixture.delegation();
    fixture.client.submit(registration.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        2,
        Some(registration.clone()),
    );
    fixture.committed(&registration);
    fixture.lose_next_release.store(release, Ordering::SeqCst);
    let _clock = application_wake::start_parent_clock(&fixture, wake && !drift);
    let original = cognition_request(&fixture);
    fixture
        .controlled_submit_commit
        .store(true, Ordering::SeqCst);
    let admission_submit_baseline = fixture
        .lookup_digests
        .lock()
        .unwrap()
        .iter()
        .filter(|entry| entry.starts_with("submit:"))
        .count();
    let executable = std::env::current_exe().unwrap();
    // The application receives connection configuration and signed request only.
    // The forbidden path is supplied solely to prove OS denial, never as client configuration.
    let denied = fs::canonicalize(&fixture.root).unwrap();
    let profile = format!(
        "(version 1)(allow default)(deny file-read* file-write* (subpath \"{}\"))",
        denied.display()
    );
    let config = fixture.client.config();
    println!(
        "application_artifact_blake3={} mechanism={}",
        blake3::hash(&fs::read(&executable).unwrap()),
        application_sandbox::mechanism()
    );
    println!(
        "application_config_identity={} sandbox_profile_blake3={} isolated_cwd=true",
        serde_json::json!({"endpoint":config.endpoint,"trusted_service_public_key":config.trusted_service_public_key,"world":config.expected_world,"scope":config.scope_id,"agent_scope":"agent:agent-a","owner_public_key":sign_read_request("owner",(),&config.read_private_key_hex).unwrap().subject_public_key,"agent_delegate_public_key":sign_read_request("delegate",(),&hex::encode([8u8;32])).unwrap().subject_public_key,"delegation_generation":1,"decision_source":"provider_backed","provider_backend":"provider_local_mock","execution_lane":"headless_agent"}),
        application_sandbox::profile_digest(&executable, &app_dir, &profile)
    );
    let boundary_mode = matches!(
        admission_mode,
        "fresh-metadata" | "fresh-paused" | "fresh-bad-store"
    );
    let metadata_held = matches!(admission_mode, "fresh-metadata" | "fresh-paused");
    if admission_mode == "repeated-turns" {
        fs::write(
            app_dir.join("provider-repeated-turns"),
            b"actual ordinary cadence",
        )
        .unwrap();
    }
    if metadata_held {
        fs::write(
            app_dir.join("provider-gates-deferred"),
            b"preflight separate",
        )
        .unwrap();
    }
    if final_budget {
        fs::write(
            app_dir.join("provider-hosted-final-budget"),
            b"Wait twice without reset",
        )
        .unwrap();
    }
    if ordinary_wait {
        fs::write(
            app_dir.join("provider-hosted-wait"),
            b"ordinary Wait then resumed Act",
        )
        .unwrap();
    }
    let observation = ordinary_wait.then(|| {
        if resume_rejected {
            application_hosted_resume_rejection::observation(&fixture, app_dir.clone())
        } else if rejection_crash {
            application_hosted_wait_rejection_recovery::observation(&fixture, app_dir.clone())
        } else if wait_rejected {
            application_hosted_wait_rejection::observation(&fixture, app_dir.clone())
        } else if admit_crash {
            application_hosted_wait_crash::observation(&fixture, app_dir.clone())
        } else {
            application_hosted_wait::observation(&fixture, app_dir.clone())
        }
    });
    let metadata = (hosted || provider_closure || wake).then(|| {
        provider_metadata::MetadataServer::start(
            (metadata_probe || metadata_held).then(|| app_dir.clone()),
            app_dir.clone(),
            observation,
        )
    });
    let hosted_wait_clock = (ordinary_wait && !final_budget)
        .then(|| hosted_wait_clock::start(&fixture, app_dir.clone()));
    let final_budget_clock = final_budget
        .then(|| application_hosted_final_budget::start_clock(&fixture, app_dir.clone()));
    let provider_url = metadata
        .as_ref()
        .map(|server| server.endpoint.as_str())
        .unwrap_or(config.endpoint.as_str());
    let mut command = build_isolated_application_child_command(
        mode,
        &executable,
        &app_dir,
        &profile,
        provider_url,
        config,
        &original,
        &denied,
    );
    let observer_stop = Arc::new(AtomicBool::new(false));
    let pending_observer = (admission_mode == "pending-cadence")
        .then(|| application_hosted_unpublished_cadence::observe_parent(&fixture, &app_dir));
    let observer = boundary_mode.then(|| {
        application_admission_boundaries::observe_parent(
            &fixture,
            &app_dir,
            metadata.as_ref().unwrap().decision_count.clone(),
            admission_submit_baseline,
            observer_stop.clone(),
        )
    });
    // Keep private phase diagnostics scoped to the isolated Hosted probes whose
    // terminal and repeated-receipt assertions need the original child failure.
    if hosted
        && (metadata_probe
            || matches!(
                admission_mode,
                "fresh-admission" | "fresh-metadata" | "fresh-paused" | "repeated-turns"
            ))
    {
        command
            .env("PRE2_RESUME_REJECTION_RECOVERY_TRACE", "1")
            .env("PRE2_WORLD_COHERENCE_TRACE", "1");
    }
    if admission_mode == "slow-consumer" {
        command.env(
            "RUST_LOG",
            "oasis7_viewer_runtime::viewer::stream_stage=debug",
        );
    }
    if resume_crash || admit_crash || capture_crash || wait_rejected || resume_rejected {
        fs::write(
            app_dir.join("world-concurrent-ready"),
            b"ordinary Resume crash connections",
        )
        .unwrap();
    }
    let fs_worker = fs_handoff.then(|| {
        command.env("PRE2_RESUME_HANDOFF_FS_ROOT", &app_dir);
        application_hosted_resume_write_failure::start(app_dir.clone())
    });
    let wait_fs_worker = fs_wait.then(|| {
        command.env("PRE2_WAIT_CLEANUP_FS_ROOT", &app_dir);
        application_hosted_wait_write_failure::start(app_dir.clone())
    });
    let capture_worker = capture_crash.then(|| {
        command.env("PRE2_WAIT_CAPTURE_FS_ROOT", &app_dir);
        application_hosted_wait_capture_crash::start(app_dir.clone())
    });
    let admit_selector =
        admit_crash.then(|| application_hosted_wait_crash::start_selector(&fixture));
    let resume_selector =
        resume_crash.then(|| application_hosted_wait_recovery::start_selector(&fixture));
    let rejection_worker =
        wait_rejected.then(|| application_hosted_wait_rejection::start(&fixture, app_dir.clone()));
    let rejection_fs_worker = rejection_fs.then(|| {
        command.env("PRE2_WAIT_REJECTION_FS_ROOT", &app_dir);
        application_hosted_wait_rejection_write_failure::start(app_dir.clone())
    });
    let rejection_selector = rejection_crash
        .then(|| application_hosted_wait_rejection_recovery::start_selector(&fixture));
    let resume_rejection_worker = resume_rejected.then(|| {
        fs::write(app_dir.join("resume-rejected-arm"), b"one original Resume").unwrap();
        application_hosted_resume_rejection::start(&fixture, app_dir.clone())
    });
    let resume_rejection_fs_worker = resume_rejection_fs.then(|| {
        command.env("PRE2_RESUME_REJECTION_FS_ROOT", &app_dir);
        application_hosted_resume_rejection_write_failure::start(app_dir.clone())
    });
    if resume_rejection_crash {
        command.env("PRE2_RESUME_REJECTION_FS_ROOT", &app_dir);
    }
    let final_budget_fs_worker = final_budget_fs.then(|| {
        command.env("PRE2_FINAL_BUDGET_FS_ROOT", &app_dir);
        application_hosted_final_budget_write_failure::start(app_dir.clone())
    });
    if final_budget_crash {
        command.env("PRE2_FINAL_BUDGET_FS_ROOT", &app_dir);
    }
    let ack_fs = (admission_mode == "memory-ack-write-failure")
        .then(|| application_feedback_ack_write_failure::start(app_dir.clone(), &fixture));
    let ack_external_models_before = metadata
        .as_ref()
        .map(|m| m.decision_count.load(Ordering::SeqCst));
    let mut output = run_application_child_and_report(
        &mut command,
        &fixture,
        &app_dir,
        admission_mode,
        hosted,
        metadata_probe,
    );
    if admission_mode.starts_with("memory-ack-") {
        assert_eq!(
            metadata
                .as_ref()
                .map(|m| m.decision_count.load(Ordering::SeqCst)),
            ack_external_models_before,
            "actual external provider model cannot be called by original scripted native ACK fixture"
        );
    }
    if let Some(worker) = ack_fs {
        application_feedback_ack_write_failure::finish(&fixture, &app_dir, &output, worker);
        application_feedback_ack_recovery::assert_canonical_ack(&fixture, &app_dir);
        metadata
            .as_ref()
            .unwrap()
            .assert_canonical_feedback_delivery(&fixture, true);
        fs::remove_dir_all(app_dir).unwrap();
        return;
    }
    if admission_mode == "memory-ack-missing-runner" {
        verify_missing_runner_refusal(&fixture, &app_dir, &output, metadata.as_ref().unwrap());
        return;
    }
    if let Some(worker) = resume_rejection_worker {
        if resume_rejection_crash {
            application_hosted_resume_rejection_recovery::recover_parent(
                &fixture,
                &app_dir,
                &mut command,
                &output,
                worker,
            );
            drop(hosted_wait_clock);
            fs::remove_dir_all(app_dir).unwrap();
            return;
        }
        if let Some(fs_worker) = resume_rejection_fs_worker {
            application_hosted_resume_rejection_write_failure::finish(
                &fixture, &app_dir, &output, fs_worker,
            );
        }
        application_hosted_resume_rejection::finish(&fixture, &app_dir, &output, worker);
        drop(hosted_wait_clock);
        fs::remove_dir_all(app_dir).unwrap();
        return;
    }
    if let Some(worker) = rejection_worker {
        if let Some(selector) = rejection_selector {
            application_hosted_wait_rejection_recovery::recover_parent(
                &fixture,
                &app_dir,
                &mut command,
                &output,
                worker,
                selector,
            );
            drop(hosted_wait_clock);
            fs::remove_dir_all(app_dir).unwrap();
            return;
        }
        if let Some(fs_worker) = rejection_fs_worker {
            application_hosted_wait_rejection::secure_artifact(&fixture, &app_dir, &output, "fs");
            application_hosted_wait_rejection_write_failure::finish(
                &fixture, &app_dir, &output, fs_worker,
            );
        }
        application_hosted_wait_rejection::finish(&fixture, &app_dir, &output, worker);
        drop(hosted_wait_clock);
        fs::remove_dir_all(app_dir).unwrap();
        return;
    }
    if let Some(worker) = capture_worker {
        application_hosted_wait_capture_crash::recover_parent(
            &fixture,
            &app_dir,
            &mut command,
            &output,
            worker,
            &metadata.as_ref().unwrap().decision_count,
            admission_mode == "wait-capture-nonempty",
        );
        drop(hosted_wait_clock);
        fs::remove_dir_all(app_dir).unwrap();
        return;
    }
    if let Some(selector) = admit_selector {
        application_hosted_wait_crash::recover_parent(
            &fixture,
            &app_dir,
            &mut command,
            &output,
            selector,
            &metadata.as_ref().unwrap().decision_count,
        );
        drop(hosted_wait_clock);
        fs::remove_dir_all(app_dir).unwrap();
        return;
    }
    if let Some(worker) = wait_fs_worker {
        application_hosted_wait_write_failure::finish(
            &fixture,
            &app_dir,
            &output,
            worker,
            metadata
                .as_ref()
                .unwrap()
                .decision_count
                .load(Ordering::SeqCst),
        );
        drop(hosted_wait_clock);
        fs::remove_dir_all(app_dir).unwrap();
        return;
    }
    if let Some(worker) = fs_worker {
        application_hosted_resume_write_failure::finish(
            &fixture,
            &app_dir,
            &output,
            worker,
            metadata
                .as_ref()
                .unwrap()
                .decision_count
                .load(Ordering::SeqCst),
        );
        drop(hosted_wait_clock);
        fs::remove_dir_all(app_dir).unwrap();
        return;
    }
    if let Some(selector) = resume_selector {
        application_hosted_wait_recovery::recover_parent(
            &fixture,
            &app_dir,
            &mut command,
            &output,
            selector,
            &metadata.as_ref().unwrap().decision_count,
        );
        drop(hosted_wait_clock);
        fs::remove_dir_all(app_dir).unwrap();
        return;
    }
    write_fairness_child_artifacts(service_probe, admission_mode, &output);
    if final_budget {
        if final_budget_crash {
            output = application_hosted_final_budget_recovery::recover_parent(
                &fixture,
                &app_dir,
                &mut command,
                &output,
            );
        }
        if let Some(worker) = final_budget_fs_worker {
            application_hosted_final_budget_write_failure::finish(
                &fixture, &app_dir, &output, worker,
            );
        }
        drop(final_budget_clock);
        application_hosted_final_budget::report(
            &fixture,
            &app_dir,
            &output,
            metadata
                .as_ref()
                .unwrap()
                .decision_count
                .load(Ordering::SeqCst),
        );
        fs::remove_dir_all(app_dir).unwrap();
        return;
    }
    if admission_mode == "hosted-wait" {
        retain_hosted_wait_artifact(&fixture, &app_dir, &output);
    }
    drop(hosted_wait_clock);
    if let Some(observer) = pending_observer {
        observer.join().unwrap();
    }
    observer_stop.store(true, Ordering::SeqCst);
    if let Some(observer) = observer {
        observer.join().unwrap();
    }
    if provider_closure {
        metadata
            .as_ref()
            .expect("provider closure requires its own provider endpoint")
            .assert_canonical_feedback_delivery(&fixture, true);
    }
    if matches!(admission_mode, "memory-ack-before" | "memory-ack-after") {
        application_feedback_ack_recovery::verify(
            &fixture,
            &mut command,
            &app_dir,
            &output,
            admission_mode,
        );
        metadata
            .as_ref()
            .unwrap()
            .assert_canonical_feedback_delivery(&fixture, true);
        assert_eq!(
            metadata
                .as_ref()
                .map(|m| m.decision_count.load(Ordering::SeqCst)),
            ack_external_models_before,
            "ACK restart cannot invoke actual external model"
        );
        fs::remove_dir_all(app_dir).unwrap();
        return;
    }
    if run_memory_crash_mode(
        &fixture,
        &app_dir,
        &mut command,
        &mut output,
        admission_mode,
    ) {
        return;
    }
    validate_application_output_and_remove_fixture(
        &fixture,
        &app_dir,
        &output,
        metadata.as_ref(),
        admission_mode,
        admission_submit_baseline,
        hosted,
        release,
        wake,
        drift,
        service_probe,
        metadata_probe,
    );
}

fn run_application_child_and_report(
    command: &mut std::process::Command,
    fixture: &Fixture,
    app_dir: &std::path::Path,
    admission_mode: &str,
    hosted: bool,
    metadata_probe: bool,
) -> std::process::Output {
    let output = if matches!(
        admission_mode,
        "fresh-admission"
            | "fresh-metadata"
            | "repeated-turns"
            | "hosted-wait"
            | "hosted-wait-write-failure"
            | "hosted-resume-write-failure"
    ) {
        application_completion::run_child(command, app_dir)
    } else {
        command.output().unwrap()
    };
    if admission_mode == "slow-consumer" {
        application_stream_budget_diagnostics::relay(&output);
    }
    if !output.status.success() {
        eprintln!(
            "fixture_failure_stages={}",
            fixture.stage_diagnostics.snapshot()
        );
        if admission_mode == "pending-cadence" {
            // Report the actual child failure before the observer's join can
            // replace it with a secondary readiness or sampling assertion.
            eprintln!(
                "pending_cadence_child_exit={} ready={} gated={} sampled={} released={}\nchild_stdout:\n{}\nchild_stderr:\n{}",
                output.status,
                app_dir.join("pending-cadence-ready").exists(),
                app_dir.join("pending-cadence-gated").exists(),
                app_dir.join("pending-cadence-sampled").exists(),
                app_dir.join("pending-cadence-released").exists(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    if hosted
        && (metadata_probe
            || matches!(
                admission_mode,
                "fresh-admission" | "fresh-metadata" | "fresh-paused" | "repeated-turns"
            ))
    {
        println!(
            "hosted_probe_child_exit={}\nchild_stderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    output
}

fn build_isolated_application_child_command(
    mode: IsolatedApplicationMode<'_>,
    executable: &std::path::Path,
    app_dir: &std::path::Path,
    profile: &str,
    provider_url: &str,
    config: &oasis7_game_client::remote_world_service::WorldServiceClientConfig,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
    denied: &std::path::Path,
) -> std::process::Command {
    let mut command = application_sandbox::command(executable, app_dir, profile);
    command
        .args([
            "--ignored",
            "--exact",
            "execution_bridge_real_tests::real_execution_bridge::tests::qa_conformance::application_process_probe",
            "--nocapture",
        ])
        .current_dir(app_dir)
        .env("OASIS7_AGENT_DECISION_SOURCE", "provider_backed")
        .env("OASIS7_AGENT_PROVIDER_BACKEND", "provider_local_mock")
        .env("OASIS7_AGENT_PROVIDER_CONTRACT", "worldsim_provider_v1")
        .env("OASIS7_AGENT_PROVIDER_TRANSPORT", "loopback_http")
        .env("OASIS7_AGENT_PROVIDER_PROFILE", "oasis7_p0_low_freq_npc")
        .env("OASIS7_AGENT_EXECUTION_LANE", "headless_agent")
        .env("OASIS7_AGENT_PROVIDER_URL", provider_url)
        .env_remove("OASIS7_AGENT_PROVIDER_AUTH_TOKEN")
        .env("PRE2_APP_WAKE", if mode.wake { "1" } else { "0" })
        .env("PRE2_APP_WAKE_DRIFT", if mode.drift { "1" } else { "0" })
        .env(
            "PRE2_APP_FAIRNESS",
            if mode.service_probe { "1" } else { "0" },
        )
        .env(
            "PRE2_APP_PERIODIC_FAIRNESS",
            if mode.admission_mode == "periodic-fairness" {
                "1"
            } else {
                "0"
            },
        )
        .env("PRE2_APP_ADMISSION", mode.admission_mode)
        .env(
            "PRE2_APP_METADATA",
            if mode.metadata_probe { "1" } else { "0" },
        )
        .env("PRE2_METADATA_DIR", app_dir)
        .env("PRE2_APP_HOSTED", if mode.hosted { "1" } else { "0" })
        .env("PRE2_APP_RELEASE", if mode.release { "1" } else { "0" })
        .env("PRE2_APP_ENDPOINT", &config.endpoint)
        .env("PRE2_APP_TRUST", &config.trusted_service_public_key)
        .env(
            "PRE2_APP_WORLD",
            serde_json::to_string(&config.expected_world).unwrap(),
        )
        .env("PRE2_APP_OWNER", &config.read_private_key_hex)
        .env(
            "PRE2_APP_REQUEST",
            serde_json::to_string(original).unwrap(),
        )
        .env("PRE2_DENIED_NODE_ROOT", denied);
    command
}

fn application_fixture(wake: bool, drift: bool, ordinary_wait: bool) -> Fixture {
    // Ordinary and final-budget Wait clocks enforce the same non-time-state
    // invariant as the standalone wake clock. Keep automatic crises out of
    // their initial canonical fixture, without changing the deliberate drift case.
    Fixture::with_clock_options(
        true,
        wake || ordinary_wait,
        (wake && !drift) || ordinary_wait,
    )
}

#[test]
fn ordinary_wait_clock_preserves_gameplay_across_automatic_crisis_tick() {
    let fixture = application_fixture(false, false, true);
    let mut driver = fixture.driver.lock().unwrap();
    let mut before = serde_json::to_value(driver.execution_world.state()).unwrap();
    before["time"] = serde_json::json!(0);
    for _ in 0..16 {
        commit_request(&mut driver, 0, None);
    }
    let mut after = serde_json::to_value(driver.execution_world.state()).unwrap();
    after["time"] = serde_json::json!(0);
    assert_eq!(
        before, after,
        "ordinary Wait fixture clock must preserve gameplay"
    );
}
