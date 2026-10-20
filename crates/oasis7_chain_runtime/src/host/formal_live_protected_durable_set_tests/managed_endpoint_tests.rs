use super::*;
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;

struct Helper {
    directory: PathBuf,
    script: PathBuf,
}
impl Helper {
    fn new(body: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "oasis7-managed-endpoint-{}",
            crate::node_lifecycle::local_io::random_id().unwrap()
        ));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let script = directory.join("endpoint-helper.sh");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o700)
            .open(&script)
            .unwrap();
        writeln!(file, "#!/bin/sh\n{body}").unwrap();
        Self { directory, script }
    }
    fn start(&self) -> ManagedEndpointChild {
        ManagedEndpointChild::start(self.script.as_os_str(), &self.directory.join("unused.json"))
            .unwrap()
    }
}
impl Drop for Helper {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}
fn budget(duration: Duration) -> ScenarioDeadline {
    let started_at = Instant::now();
    ScenarioDeadline {
        started_at,
        expires_at: started_at + duration,
    }
}
fn address(port: u16) -> std::net::SocketAddr {
    ([127, 0, 0, 1], port).into()
}

#[test]
fn managed_endpoint_readiness_shares_absolute_budget_across_primary_and_replica() {
    let primary = Helper::new(
        "sleep 1.00; echo formal_endpoint_awaiting_management=127.0.0.1:19001; exec sleep 30",
    );
    let replica = Helper::new(
        "sleep 3.50; echo formal_endpoint_awaiting_management=127.0.0.1:19002; exec sleep 30",
    );
    let scenario = budget(Duration::from_secs(3));
    let mut primary_child = primary.start();
    let mut replica_child = replica.start();
    primary_child
        .wait_management(address(19001), scenario)
        .unwrap();
    let error = replica_child
        .wait_management(address(19002), scenario)
        .unwrap_err();
    assert!(
        error.contains("formal_endpoint_awaiting_management="),
        "{error}"
    );
    assert!(scenario.started_at.elapsed() >= Duration::from_millis(2800));
    // With a new three-second allowance after primary readiness, replica's marker
    // would arrive in time. The shared absolute expiry must reject it instead.
}

#[test]
fn managed_endpoint_readiness_progress_does_not_extend_absolute_budget() {
    let helper = Helper::new(
        "i=0; while [ \"$i\" -lt 150 ]; do echo staging-progress; i=$((i+1)); sleep 0.02; done; echo formal_endpoint_awaiting_management=127.0.0.1:19003; exec sleep 30",
    );
    let mut child = helper.start();
    // Synchronize helper startup before measuring progress against the budget.
    assert_eq!(
        child.lines.recv_timeout(Duration::from_secs(5)).unwrap(),
        "staging-progress"
    );
    let error = child
        .wait_management(address(19003), budget(Duration::from_secs(1)))
        .unwrap_err();
    assert!(
        error.contains("staging-progress"),
        "progress must have been observed: {error}"
    );
}

#[test]
fn managed_endpoint_readiness_rejects_wrong_address_without_waiting_for_expiry() {
    let helper =
        Helper::new("echo formal_endpoint_awaiting_management=127.0.0.1:19005; exec sleep 30");
    let mut child = helper.start();
    let started = Instant::now();
    let error = child
        .wait_management(address(19004), budget(Duration::from_secs(5)))
        .unwrap_err();
    assert!(error.contains("differs from expected"), "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "wrong-address rejection waited for the five-second budget"
    );
}

#[test]
fn managed_endpoint_readiness_rejects_child_exit_without_waiting_for_expiry() {
    let helper = Helper::new("echo endpoint-startup-failed >&2; exit 7");
    let mut child = helper.start();
    let started = Instant::now();
    let error = child
        .wait_management(address(19006), budget(Duration::from_secs(5)))
        .unwrap_err();
    assert!(error.contains("endpoint-startup-failed"), "{error}");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "child-exit rejection waited for the five-second budget"
    );
}
