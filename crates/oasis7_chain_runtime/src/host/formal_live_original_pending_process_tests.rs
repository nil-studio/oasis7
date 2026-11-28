//! One nonempty authenticated original crosses the real owner process, Unix
//! freeze control and Host cold-reader. Custody oracles and scheduling are
//! test-only; this is not shipping-binary or external protected-root evidence.
use super::*;
use crate::{
    formal_live_recovery_original_adapter::cold_read_original_for_config,
    formal_live_runtime::{self, original_pending_owner::process_boundary},
    node_lifecycle::local_io,
};
use oasis7_distfs::controlled_authority::LocalRequestIdentity;
use oasis7_formal_protocol::controlled_authority::formal::{
    RecoveryPendingHandoffV1, RecoveryPendingRequestV1,
};
use std::{
    fs::File,
    io::Write,
    net::Shutdown,
    os::unix::{net::UnixStream, process::ExitStatusExt},
    process::{Child, Command, Stdio},
};

struct OwnerChild(Child);
impl Drop for OwnerChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

pub(super) fn run() -> Result<(), String> {
    thread::Builder::new()
        .name("original-pending-owner-process".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            for phase in ["original_pending_prepared", "original_pending_frozen"] {
                run_protected_runtime_round_trip(false, Some(phase))?;
            }
            Ok(())
        })
        .map_err(|error| error.to_string())?
        .join()
        .map_err(|_| "original pending owner process fixture panicked".to_string())?
}

pub(super) fn exercise(
    phase: &str,
    root: &Path,
    config: &Path,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
    client_config: WorldServiceClientConfig,
) -> Result<(), String> {
    let phase = phase
        .strip_prefix("original_pending_")
        .ok_or("invalid process-test phase")?;
    // Compute the independent pin from the signed request BEFORE the owner is
    // launched. Never adopt the child's candidate pin as the expected root.
    let bytes = serde_json::to_vec(original).map_err(|error| error.to_string())?;
    let key = &original.correlation.key;
    let expected = RecoveryPendingHandoffV1::from_requests(vec![RecoveryPendingRequestV1 {
        identity: LocalRequestIdentity {
            verified_subject: key.verified_subject.clone(),
            operation_domain: key.operation_domain.clone(),
            nonce_scope: key.nonce_scope.clone(),
            request_id: key.request_id_or_nonce.clone(),
        },
        signed_request: bytes,
    }])
    .map_err(|error| error.to_string())?;
    let independent_root = expected.root().map_err(|error| error.to_string())?;
    let marker = root.join("original-pending-boundary.json");
    let operation = "original-pending-process-owner";
    let request_digest = local_io::digest(&(operation, config))?;
    let config_digest = local_io::sha256(config)?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let listing = Command::new(&executable)
        .arg("--list")
        .output()
        .map_err(|e| e.to_string())?;
    if !listing.status.success() {
        return Err("owner worker listing failed".into());
    }
    let listing = String::from_utf8(listing.stdout).map_err(|e| e.to_string())?;
    let workers: Vec<_> = listing
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .filter(|name| name.ends_with("::protected_handoff_owner_worker"))
        .collect();
    if workers.len() != 1 {
        return Err("exact owner worker is not uniquely registered".into());
    }
    let mut child = OwnerChild(
        Command::new(executable)
            .args(["--ignored", "--exact", workers[0], "--nocapture"])
            .env("OASIS7_FORMAL_OWNER_TEST_CONFIG", config)
            .env("OASIS7_FORMAL_OWNER_TEST_OPERATION", operation)
            .env("OASIS7_FORMAL_OWNER_TEST_REQUEST_DIGEST", &request_digest)
            .env("OASIS7_FORMAL_OWNER_TEST_CONFIG_DIGEST", &config_digest)
            .env_remove("OASIS7_FORMAL_OWNER_TEST_READ_TRUST_DIGEST")
            .env(process_boundary::PHASE, phase)
            .env(process_boundary::MARKER, &marker)
            .stdin(Stdio::null())
            .stdout(File::create(root.join("original-owner.stdout")).map_err(|e| e.to_string())?)
            .stderr(File::create(root.join("original-owner.stderr")).map_err(|e| e.to_string())?)
            .spawn()
            .map_err(|error| error.to_string())?,
    );
    let deadline = Instant::now() + Duration::from_secs(45);
    let owner = loop {
        if let Ok(Some(owner)) = formal_live_runtime::query_operation_owner(
            config,
            operation,
            &request_digest,
            &config_digest,
            None,
        ) && owner.state == "ready"
        {
            break owner;
        }
        pending(&mut child, deadline)?;
    };
    let paused = wait_marker(&marker.with_extension("node.json"), &mut child, deadline)?;
    if paused["point"] != "before_resume" || paused["action_count"] != 0 {
        return Err(format!(
            "owner did not pause at a real empty continuation: {paused}"
        ));
    }
    if owner.instance.process_id != child.0.id() {
        return Err("owner receipt does not name the retained child".into());
    }
    let client = RemoteWorldServiceClient::new(client_config).map_err(|e| e.to_string())?;
    assert_volatile_admission(
        client.submit(original.clone()).map_err(|e| e.to_string())?,
        &original.correlation,
    )?;
    // Even with the correct independently retained root, an active producer
    // without the real freeze must never become a readable Frozen original.
    if cold_read_original_for_config(
        config,
        &owner.instance,
        &owner.instance.authority_id,
        &independent_root,
        1,
    )
    .is_ok()
    {
        return Err("active original was cold-readable before freeze".into());
    }
    let (_, owner_record, socket) = formal_live_runtime::owner::paths(config, operation)?;
    let mut stream = UnixStream::connect(socket).map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let frame = serde_json::to_vec(&serde_json::json!({
        "api_version": "oasis7.formal-runtime-owner/v1", "action": "freeze_original_pending",
        "operation_id": operation, "request_digest": request_digest,
        "config_digest": config_digest, "incarnation": owner.instance.incarnation,
        "timeout_ms": 30000,
    }))
    .map_err(|error| error.to_string())?;
    stream
        .write_all(&(frame.len() as u32).to_be_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(&frame).map_err(|e| e.to_string())?;
    // Lose the entire reply at the real transport, without a fake owner server.
    stream.shutdown(Shutdown::Both).map_err(|e| e.to_string())?;
    drop(stream);
    let reached = wait_marker(&marker, &mut child, deadline)?;
    if reached["phase"] != phase
        || reached["pin"]["root"] != independent_root
        || reached["pin"]["count"] != 1
        || reached["node_running"] != (phase == "prepared")
    {
        return Err(format!(
            "real freeze boundary differs from independent original: {reached}"
        ));
    }
    child.0.kill().map_err(|e| e.to_string())?;
    let status = child.0.wait().map_err(|e| e.to_string())?;
    if status.signal() != Some(libc::SIGKILL) {
        return Err(format!("owner was not SIGKILLed: {status}"));
    }
    let read = cold_read_original_for_config(
        config,
        &owner.instance,
        &owner.instance.authority_id,
        &independent_root,
        1,
    );
    if phase == "prepared" {
        if read.is_ok() {
            return Err("prepared-only killed owner authorized a Frozen read".into());
        }
    } else {
        if read?.pending().handoff() != &expected {
            return Err(
                "Host cold reader changed the original signed request bytes or identity".into(),
            );
        }
        let mut wrong_owner = owner.instance.clone();
        let different = if wrong_owner.incarnation.starts_with('0') {
            "1"
        } else {
            "0"
        };
        wrong_owner.incarnation.replace_range(..1, different);
        for (instance, root, count) in [
            (&owner.instance, "0".repeat(64), 1),
            (&owner.instance, independent_root.clone(), 2),
            (&wrong_owner, independent_root.clone(), 1),
        ] {
            if cold_read_original_for_config(
                config,
                instance,
                &owner.instance.authority_id,
                &root,
                count,
            )
            .is_ok()
            {
                return Err("cold read accepted a different independent pin/owner".into());
            }
        }
    }
    // A prepared or Frozen original must not silently become a fresh accepting
    // owner on retry. Use the actual launcher, then retain query-only evidence.
    let before_retry = std::fs::read(&owner_record).map_err(|error| error.to_string())?;
    let retry = formal_live_runtime::launch_operation_owner(
        config,
        operation,
        &request_digest,
        &config_digest,
        Duration::from_secs(5),
    );
    let refusal = retry.expect_err("owner retry must refuse unresolved original admission");
    if !refusal.contains("original pending owner has unresolved prior freeze") {
        return Err(format!(
            "owner retry failed for a different reason: {refusal}"
        ));
    }
    if std::fs::read(owner_record).map_err(|error| error.to_string())? != before_retry {
        return Err("refused restart replaced the exact original owner receipt".into());
    }
    eprintln!(
        "original_pending_process phase={phase} count=1 signed_bytes=exact reply=lost exit=SIGKILL host_read={}",
        if phase == "frozen" {
            "verified"
        } else {
            "refused"
        }
    );
    Ok(())
}

fn pending(child: &mut OwnerChild, deadline: Instant) -> Result<(), String> {
    if let Some(status) = child.0.try_wait().map_err(|e| e.to_string())? {
        return Err(format!(
            "original owner exited before requested boundary: {status}"
        ));
    }
    if Instant::now() >= deadline {
        return Err("original owner process scenario deadline".into());
    }
    thread::sleep(Duration::from_millis(20));
    Ok(())
}

fn wait_marker(path: &Path, child: &mut OwnerChild, deadline: Instant) -> Result<Value, String> {
    while !path.exists() {
        pending(child, deadline)?;
    }
    local_io::read(path)
}
