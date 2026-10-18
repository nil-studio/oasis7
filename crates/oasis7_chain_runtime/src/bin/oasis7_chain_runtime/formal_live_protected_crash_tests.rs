//! C03 kills the runtime process at fsynced durable protocol boundaries.
use super::*;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; native child fault acceptance"]
fn protected_endpoint_children_sigkill_runtime_durable_phases_resume_original_once() {
    let selected_phase = std::env::var("OASIS7_FORMAL_TEST_C03_PHASE").ok();
    if let Some(selected) = &selected_phase {
        assert!(
            [
                "pending_durable",
                "input_frozen",
                "execution_completed",
                "candidate_saved",
                "primary_published",
                "dual_qualified",
                "publication_saved",
                "attempt_output_unlinked",
                "read_published",
            ]
            .contains(&selected.as_str()),
            "unknown C03 phase selector"
        );
    }
    for phase in [
        "pending_durable",
        "input_frozen",
        "execution_completed",
        "candidate_saved",
        "primary_published",
        "dual_qualified",
        "publication_saved",
        "attempt_output_unlinked",
        "read_published",
    ] {
        if selected_phase
            .as_ref()
            .is_some_and(|selected| selected != phase)
        {
            continue;
        }
        eprintln!("C03 runtime SIGKILL phase={phase}");
        thread::Builder::new()
            .name(format!("formal-crash-{phase}"))
            .stack_size(32 * 1024 * 1024)
            .spawn(move || run_protected_runtime_round_trip(true, Some(phase)))
            .unwrap()
            .join()
            .unwrap()
            .unwrap();
    }
}

#[test]
#[ignore = "internal runtime SIGKILL child; parent supplies private config"]
fn protected_runtime_crash_child() {
    let config = std::env::var_os("OASIS7_FORMAL_TEST_CHILD_CONFIG").expect("child config");
    let ready = std::env::var_os("OASIS7_FORMAL_TEST_CHILD_READY").expect("child ready marker");
    thread::Builder::new()
        .name("formal-crash-runtime-child".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || -> Result<(), String> {
            let mut runtime = FormalLiveRuntime::open_with_native_history(Path::new(&config))?;
            runtime.start()?;
            oracle_support::write_private(Path::new(&ready), b"ready")?;
            loop {
                let snapshot = runtime
                    .runtime
                    .lock()
                    .map_err(|_| "child runtime lock poisoned")?
                    .snapshot();
                if let Some(error) = snapshot.last_error {
                    return Err(format!("child Node: {error}"));
                }
                thread::sleep(Duration::from_millis(20));
            }
        })
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
}

struct RuntimeChild(Child);
impl Drop for RuntimeChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wait_marker(child: &mut RuntimeChild, marker: &Path, expected: &[u8]) -> Result<(), String> {
    // The attempt retire phase is reached by genuine idle-tick maintenance:
    // full native/environment and sparse-tree verification spans bounded pages.
    let seconds = if expected == b"attempt_output_unlinked" {
        180
    } else {
        30
    };
    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        if marker.exists() {
            let bytes = crate::controlled_history_cli::read_bounded(marker, 1024)?;
            if bytes != expected {
                return Err("runtime child phase marker mismatch".into());
            }
            return Ok(());
        }
        if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("runtime fault child exited before phase: {status}"));
        }
        if Instant::now() >= deadline {
            return Err("runtime fault child phase deadline".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

pub(super) fn run_crash_recovery(
    phase: &str,
    config: &Path,
    public_config: WorldServiceClientConfig,
    owner_config: WorldServiceClientConfig,
    world: &WorldIdentity,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    initial_emits: usize,
) -> Result<(), String> {
    // C03 previously had no shared scenario deadline. Use the healthy fixture's
    // existing five-minute scenario budget once, including both exact reopens
    // and retries of the same pending stop owner. The phase/RPC bounds stay put.
    let started_at = Instant::now();
    let scenario = ScenarioDeadline {
        started_at,
        expires_at: started_at + Duration::from_secs(5 * 60),
    };
    let root = config.parent().ok_or("crash fixture root missing")?;
    let marker = root.join("runtime-crash-phase");
    let target_request = root.join("crash-original-request.cbor");
    oracle_support::write_private(
        &target_request,
        &serde_cbor::to_vec(request).map_err(|e| e.to_string())?,
    )?;
    let ready = root.join("runtime-crash-ready");
    let error_log = root.join("runtime-crash-child.stderr");
    oracle_support::write_private(&error_log, b"")?;
    let stderr = std::fs::OpenOptions::new()
        .write(true)
        .open(&error_log)
        .map_err(|e| e.to_string())?;
    let mut child = RuntimeChild(
        Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args([
                "protected_runtime_crash_child",
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("OASIS7_FORMAL_TEST_CHILD_CONFIG", config)
            .env("OASIS7_FORMAL_TEST_CHILD_READY", &ready)
            .env("OASIS7_FORMAL_TEST_CRASH_PHASE", phase)
            .env("OASIS7_FORMAL_TEST_CRASH_REQUEST", &target_request)
            .env("OASIS7_FORMAL_TEST_CRASH_MARKER", &marker)
            .stdout(Stdio::null())
            .stderr(stderr)
            .spawn()
            .map_err(|e| e.to_string())?,
    );
    let observed = (|| {
        wait_marker(&mut child, &ready, b"ready")?;
        let client =
            RemoteWorldServiceClient::new(public_config.clone()).map_err(|e| e.to_string())?;
        let before = read_current(&client, world)?;
        if before.version().commit.position < 1 {
            return Err("crash fixture missing qualified initial head".into());
        }
        let admission = client.submit(request.clone()).map_err(|e| e.to_string())?;
        assert_volatile_admission(admission, &request.correlation)?;
        wait_marker(&mut child, &marker, phase.as_bytes())
    })();
    if let Err(error) = observed {
        let diagnostics =
            crate::controlled_history_cli::read_bounded(&error_log, 16 * 1024).unwrap_or_default();
        return Err(format!(
            "C03 {phase}: {error}; child: {}",
            String::from_utf8_lossy(&diagnostics)
        ));
    }
    child.0.kill().map_err(|e| e.to_string())?;
    let status = child.0.wait().map_err(|e| e.to_string())?;
    if status.signal() != Some(9) {
        return Err(format!("C03 did not observe SIGKILL: {status}"));
    }
    drop(child);
    scenario.report_stage("runtime SIGKILL observed")?;

    let config_value: Value =
        serde_json::from_slice(&std::fs::read(config).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let pending_file = PathBuf::from(
        config_value["writer_directory"]
            .as_str()
            .ok_or("test writer directory missing")?,
    )
    .join("pending.cbor");
    let (original_block_hash, target_position) = if phase == "attempt_output_unlinked" {
        crate::execution_bridge_real_tests::real_execution_bridge::formal_writer::retirement_test_target(
            pending_file.parent().ok_or("writer root missing")?,
            &marker.with_extension("retirement-target.cbor"),
        )?
    } else {
        (crate::execution_bridge_real_tests::real_execution_bridge::formal_writer::pending_test_block_hash(&pending_file)?,
         crate::execution_bridge_real_tests::real_execution_bridge::formal_writer::pending_test_position(&pending_file)?)
    };
    if phase == "primary_published" {
        // Force the retained genesis path to use the endpoint's durable
        // logical artifact CAS, not an old local Published C0 byte cache.
        let initial_cache = pending_file
            .parent()
            .ok_or("writer root missing")?
            .join("published")
            .join("00000000000000000001.cbor");
        match std::fs::remove_file(&initial_cache) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("remove test-only initial material cache: {error}")),
        }
        if initial_cache.exists() {
            return Err("test initial cache removal failed".into());
        }
    }
    let mut reference = None;
    let mut qualified_record = None;
    let mut exact_effect = None;
    for reopen in 0..2 {
        scenario.report_stage("exact crash recovery reopen started")?;
        crate::execution_bridge_real_tests::real_execution_bridge::formal_crash_test_boundary::reset_execution_attempts(&original_block_hash);
        let mut runtime = FormalLiveRuntime::open_with_native_history(config)?;
        runtime.start()?;
        wait_for_published_position(&runtime, target_position)?;
        let public =
            RemoteWorldServiceClient::new(public_config.clone()).map_err(|e| e.to_string())?;
        let view = read_until_position(&public, world, target_position)?;
        let cut = runtime.published.current()?;
        let attempts = crate::execution_bridge_real_tests::real_execution_bridge::formal_crash_test_boundary::execution_attempts();
        let expected_attempts = if reopen == 0
            && matches!(
                phase,
                "pending_durable" | "input_frozen" | "execution_completed"
            ) {
            1
        } else {
            0
        };
        if attempts != expected_attempts {
            return Err(format!(
                "C03 {phase} reopen {reopen}: actual execution attempts {attempts}, expected {expected_attempts}"
            ));
        }
        assert_live_descends_from_view(&runtime, &cut, &view)?;
        let owner =
            RemoteWorldServiceClient::new(owner_config.clone()).map_err(|e| e.to_string())?;
        let original_commit = match lookup_original_commit(&owner, request) {
            Ok(commit) => commit,
            Err(error) => {
                let result_key = correlation::key_digest(&request.correlation.key)
                    .map_err(|error| error.to_string())?;
                let retained = retained_result_from_hot_or_cold(&cut.world, &result_key)
                    .map(|_| "present".to_owned())
                    .unwrap_or_else(|error| format!("unavailable: {error}"));
                let local = runtime.history.business_result(
                    &view.version().commit,
                    &request.correlation,
                    &owner_config.scope_id,
                    &request.correlation.key.verified_subject,
                );
                let diagnostic = match local {
                    Ok(Some(_)) => "verified proof present".to_owned(),
                    Ok(None) => "verified tree proves absence".to_owned(),
                    Err(error) => error,
                };
                return Err(format!(
                    "C03 {phase} reopen {reopen}: {error}; retained result {retained}; local result: {diagnostic}"
                ));
            }
        };
        if original_commit.position != target_position {
            return Err(format!(
                "C03 original result changed its qualified decision identity: expected position {target_position}, got {}",
                original_commit.position
            ));
        }
        let exact_original = runtime.history.fixed_publication(&original_commit)?;
        if wasm_emit_count(exact_original.world()) != initial_emits + 1 {
            return Err("C03 original Wasmtime effect did not persist exactly once".into());
        }
        let effect = first_agent_test_effect::first_agent_effect(exact_original.world(), request)?;
        if let Some(original) = &exact_effect {
            if original != &effect {
                return Err("C03 original agent/owner/resources/result changed".into());
            }
        } else {
            exact_effect = Some(effect);
        }
        let record_id = original_commit.execution_block_hash.clone();
        if reopen == 0 {
            reference = Some(original_commit);
            qualified_record = Some(record_id);
        } else if reference.as_ref() != Some(&original_commit)
            || qualified_record.as_ref() != Some(&record_id)
        {
            return Err("C03 repeat recovery changed original immutable identity".into());
        }
        let before = runtime.published.current()?;
        let emissions = wasm_emit_count(&before.world);
        let next = before
            .anchor
            .head()
            .record()
            .position
            .checked_add(1)
            .ok_or("C03 duplicate position overflow")?;
        let admission = owner.submit(request.clone()).map_err(|e| e.to_string())?;
        assert_volatile_admission(admission, &request.correlation)?;
        wait_for_published_position(&runtime, next)?;
        let after = runtime.published.current()?;
        if wasm_emit_count(&after.world) != emissions || after.world.state().agents.len() != 1 {
            return Err("C03 original retry produced a second Wasmtime/agent effect".into());
        }
        let repeated = lookup_original_commit(&owner, request)?;
        let old = runtime.history.fixed_publication(&repeated)?;
        if reference.as_ref() != Some(&repeated)
            || Some(first_agent_test_effect::first_agent_effect(
                old.world(),
                request,
            )?) != exact_effect
        {
            return Err("C03 original retry changed CommitRef/agent/owner/resources/result".into());
        }
        eprintln!(
            "C03 phase={phase} reopen={reopen} exact identity/effect/retry assertions passed; stopping same runtime owner"
        );
        super::durable_set_tests::stop_healthy_runtime(&mut runtime, scenario)?;
        scenario.report_stage("exact crash recovery reopen stopped")?;
    }
    eprintln!(
        "C03 phase={phase} SIGKILL verified; original qualified once; two exact reopen reads"
    );
    Ok(())
}
