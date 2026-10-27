//! Bounded shipping CLI calls and exact owner cleanup for healthy recovery tests.
use crate::formal_live_runtime;
use serde_json::Value;
use std::{
    ffi::OsString,
    fs::File,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(super) fn utf8(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or("non UTF8 fixture path".into())
}
pub(super) struct OwnedShippingChild(pub(super) Child);
impl Drop for OwnedShippingChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
impl OwnedShippingChild {
    pub(super) fn kill_sigkill_and_wait(&mut self) -> Result<(), String> {
        use std::os::unix::process::ExitStatusExt;
        self.0
            .kill()
            .map_err(|error| format!("SIGKILL shipping owner: {error}"))?;
        let status = self
            .0
            .wait()
            .map_err(|error| format!("wait SIGKILL shipping owner: {error}"))?;
        if status.signal() != Some(libc::SIGKILL) {
            return Err(format!("shipping owner did not exit by SIGKILL: {status}"));
        }
        Ok(())
    }
}

pub(super) fn wait_owner(
    child: &mut Child,
    config: &Path,
    operation: &str,
    request: &str,
    digest: &str,
) -> Result<formal_live_runtime::FormalRuntimeOwnerSnapshotV1, String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(Some(owner)) =
            formal_live_runtime::query_operation_owner(config, operation, request, digest, None)
            && owner.state == "ready"
        {
            return Ok(owner);
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("shipping owner exited before readiness: {status}"));
        }
        if Instant::now() >= deadline {
            return Err("shipping owner readiness deadline".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}
/// Failure-only observation. Read the existing signed status without replacing
/// the original lookup error or changing its admission/commit deadline.
pub(super) fn original_commit_failure_status(
    config: &oasis7::world_service::client::WorldServiceClientConfig,
) -> String {
    let status = (|| {
        let address = config
            .endpoint
            .strip_prefix("http://")
            .ok_or("status endpoint must be HTTP loopback")?
            .parse::<std::net::SocketAddr>()
            .map_err(|_| "invalid status endpoint")?;
        if !address.ip().is_loopback() {
            return Err("status endpoint must be loopback".into());
        }
        super::super::availability_tests::signed_status(address, &config.trusted_service_public_key)
    })();
    match status {
        Ok(status) => format!(
            "state={} ready={} running={} ticks={} committed_position={:?} last_error={:?}",
            status.state.chars().take(64).collect::<String>(),
            status.ready,
            status.node_running,
            status.tick_count,
            status.committed_position,
            status
                .last_error
                .map(|error| error.chars().take(512).collect::<String>()),
        ),
        Err(error) => format!(
            "status_unavailable={}",
            error.chars().take(512).collect::<String>()
        ),
    }
}

pub(super) fn cli_revision(
    binary: &OsString,
    root: &Path,
    action: &str,
    store: &Path,
    id: &str,
    record: &Value,
) -> Result<Value, String> {
    let revision = record["revision"]
        .as_u64()
        .ok_or("revision missing")?
        .to_string();
    cli(
        binary,
        root,
        &[
            action,
            "--store",
            utf8(store)?,
            "--id",
            id,
            "--revision",
            &revision,
        ],
    )
}
pub(super) fn cli(binary: &OsString, root: &Path, args: &[&str]) -> Result<Value, String> {
    let output = run_cli(binary, root, args)?;
    if !output.status.success() {
        return Err(format!(
            "shipping op {args:?} failed ({}); stdout: {}; stderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("shipping CLI JSON: {e}"))
}
pub(super) fn cli_error(
    binary: &OsString,
    root: &Path,
    args: &[&str],
    expected: &str,
) -> Result<(), String> {
    let output = run_cli(binary, root, args)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The shipping tracing subscriber emits command errors on stdout; retain
    // both the nonzero exit requirement and the exact rejection-code check.
    if output.status.success() || !(stdout.contains(expected) || stderr.contains(expected)) {
        return Err(format!(
            "expected CLI rejection {expected} ({}); stdout: {stdout}; stderr: {stderr}",
            output.status
        ));
    }
    Ok(())
}
fn run_cli(binary: &OsString, root: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CALL: AtomicU64 = AtomicU64::new(0);
    let n = CALL.fetch_add(1, Ordering::Relaxed);
    let outpath = root.join(format!("op-cli-{n}.stdout"));
    let errpath = root.join(format!("op-cli-{n}.stderr"));
    let mut child = Command::new(binary)
        .arg("op")
        .args(args)
        .stdin(Stdio::null())
        .stdout(File::create(&outpath).map_err(|e| e.to_string())?)
        .stderr(File::create(&errpath).map_err(|e| e.to_string())?)
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(45);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("shipping op CLI deadline: {args:?}"));
        }
        thread::sleep(Duration::from_millis(20));
    };
    Ok(std::process::Output {
        status,
        stdout: std::fs::read(outpath).map_err(|e| e.to_string())?,
        stderr: std::fs::read(errpath).map_err(|e| e.to_string())?,
    })
}

pub(super) struct ExactOwnerCleanup {
    pub(super) config: std::path::PathBuf,
    pub(super) operation: String,
    pub(super) request_digest: String,
    pub(super) config_digest: String,
}
impl Drop for ExactOwnerCleanup {
    fn drop(&mut self) {
        if let Ok(Some(owner)) = formal_live_runtime::query_operation_owner(
            &self.config,
            &self.operation,
            &self.request_digest,
            &self.config_digest,
            None,
        ) && owner.state != "stopped"
            && owner.state != "failed"
        {
            let _ = formal_live_runtime::stop_operation_owner(
                &self.config,
                &owner.instance,
                Duration::from_secs(10),
            );
        }
    }
}

#[test]
fn original_commit_diagnostic_rejects_remote_endpoint_without_exposing_credentials() {
    let config = oasis7::world_service::client::WorldServiceClientConfig {
        endpoint: "http://192.0.2.1:1234".into(),
        trusted_service_public_key: "private-test-service-identity".into(),
        expected_world: oasis7_client_api::world_service::WorldIdentity {
            world_id: "diagnostic-world".into(),
            chain_id: "diagnostic-chain".into(),
        },
        scope_id: "owner".into(),
        read_private_key_hex: "private-test-credential".into(),
        timeout: Duration::from_secs(1),
        max_response_bytes: 1024,
    };
    assert_eq!(
        original_commit_failure_status(&config),
        "status_unavailable=status endpoint must be loopback"
    );
}
