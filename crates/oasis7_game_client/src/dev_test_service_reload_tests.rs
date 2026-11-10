use super::*;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::{Duration, Instant},
};

struct Fixture {
    root: PathBuf,
    initial: Config,
}
impl Fixture {
    fn new() -> Self {
        let mut id = [0; 12];
        getrandom::fill(&mut id).unwrap();
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("o7-reload-{}", hex::encode(id)));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let initial = Config {
            profile: PROFILE.into(),
            socket: root.join("service.sock"),
            usage_directory: root.clone(),
            grant_id: "reload-test".into(),
            max_request_bytes: 4096,
            timeout_ms: 1000,
            max_calls: 5,
            credentials: vec![Credential {
                caller_id: "caller".into(),
                token: "x".repeat(32),
                expires_at_unix: u64::MAX,
                revoked: false,
            }],
        };
        let fixture = Self { root, initial };
        fixture.save(&fixture.initial);
        fixture
    }
    fn path(&self) -> PathBuf {
        self.root.join("config")
    }
    fn save(&self, config: &Config) {
        fs::write(self.path(), serde_json::to_vec(config).unwrap()).unwrap();
        fs::set_permissions(self.path(), fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn call(&self, started: Instant, bytes: usize) -> Response {
        let request = Request {
            profile: PROFILE.into(),
            audience: AUDIENCE.into(),
            credential: "x".repeat(32),
            method: "capabilities".into(),
        };
        dispatch_reloading(
            &self.initial,
            &self.initial,
            &|| read_private_cli_config(self.path().to_str().unwrap()),
            &request,
            started,
            &mut (started + Duration::from_millis(self.initial.timeout_ms)),
            bytes,
        )
    }
    fn used(&self) -> u64 {
        let path = self.root.join(format!(
            "{}.json",
            crate::dev_test_operations::usage_key(&self.initial.grant_id, "caller")
        ));
        serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap()["used"]
            .as_u64()
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn authoritative_reload_rejects_stale_permissions_and_preserves_usage() {
    let fixture = Fixture::new();
    assert_eq!(fixture.call(Instant::now(), 100).status, "ok");
    let mut changed = fixture.initial.clone();
    changed.max_calls = 1;
    fixture.save(&changed);
    assert_eq!(
        fixture.call(Instant::now(), 100).status,
        "call_limit_exhausted"
    );
    changed.max_calls = 5;
    changed.credentials[0].revoked = true;
    fixture.save(&changed);
    assert_eq!(fixture.call(Instant::now(), 100).status, "unauthorized");
    changed.credentials[0].revoked = false;
    changed.credentials[0].expires_at_unix = 0;
    fixture.save(&changed);
    assert_eq!(fixture.call(Instant::now(), 100).status, "unauthorized");
    changed.credentials[0].expires_at_unix = u64::MAX;
    changed.credentials[0].caller_id = "other".into();
    fixture.save(&changed);
    assert_eq!(fixture.call(Instant::now(), 100).status, "unauthorized");
    assert_eq!(fixture.used(), 1);
    fixture.save(&fixture.initial);
    assert_eq!(fixture.call(Instant::now(), 100).status, "ok");
    assert_eq!(fixture.used(), 2);
}

#[test]
fn reload_rechecks_binding_trust_size_and_original_deadline() {
    let fixture = Fixture::new();
    assert_eq!(fixture.call(Instant::now(), 100).status, "ok");
    for field in ["grant", "directory", "socket"] {
        let mut changed = fixture.initial.clone();
        match field {
            "grant" => changed.grant_id = "other".into(),
            "directory" => changed.usage_directory = fixture.root.join("elsewhere"),
            _ => changed.socket = fixture.root.join("other.sock"),
        }
        fixture.save(&changed);
        assert_eq!(
            fixture.call(Instant::now(), 100).status,
            "service_configuration_unavailable"
        );
    }
    fs::write(fixture.path(), b"invalid").unwrap();
    assert_eq!(
        fixture.call(Instant::now(), 100).status,
        "service_configuration_unavailable"
    );
    fixture.save(&fixture.initial);
    fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        fixture.call(Instant::now(), 100).status,
        "service_configuration_unavailable"
    );
    let mut changed = fixture.initial.clone();
    changed.max_request_bytes = 256;
    fixture.save(&changed);
    assert_eq!(
        fixture.call(Instant::now(), 300).status,
        "request_too_large"
    );
    changed.timeout_ms = 1;
    fixture.save(&changed);
    assert_eq!(
        fixture
            .call(Instant::now() - Duration::from_millis(10), 100)
            .status,
        "admission_deadline_exceeded"
    );
    assert_eq!(fixture.used(), 1);
}

#[test]
fn bounded_ledger_wait_does_not_admit_after_deadline() {
    use std::os::fd::AsRawFd;
    let mut fixture = Fixture::new();
    fixture.initial.timeout_ms = 30;
    fixture.save(&fixture.initial);
    assert_eq!(fixture.call(Instant::now(), 100).status, "ok");
    let path = fixture.root.join(format!(
        "{}.lock",
        crate::dev_test_operations::usage_key(&fixture.initial.grant_id, "caller")
    ));
    let lock = fs::File::open(path).unwrap();
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            sender
                .send(fixture.call(Instant::now(), 100).status)
                .unwrap()
        });
        let result = receiver.recv_timeout(Duration::from_secs(1));
        drop(lock); // Always unblock the worker before asserting a failed bounded wait.
        assert_eq!(result.unwrap(), "admission_deadline_exceeded");
    });
    assert_eq!(fixture.used(), 1);
    assert_eq!(fixture.call(Instant::now(), 100).status, "ok");
    assert_eq!(fixture.used(), 2);
}

#[test]
fn policy_refresh_runs_under_the_usage_lock_and_observes_revocation() {
    use std::os::fd::AsRawFd;
    let fixture = Fixture::new();
    assert_eq!(fixture.call(Instant::now(), 100).status, "ok");
    let path = fixture.root.join(format!(
        "{}.lock",
        crate::dev_test_operations::usage_key(&fixture.initial.grant_id, "caller")
    ));
    let lock = fs::File::open(&path).unwrap();
    assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            sender.send(()).unwrap();
            let request = Request {
                profile: PROFILE.into(),
                audience: AUDIENCE.into(),
                credential: "x".repeat(32),
                method: "capabilities".into(),
            };
            let started = Instant::now();
            dispatch_reloading(
                &fixture.initial,
                &fixture.initial,
                &|| {
                    let probe = fs::File::open(&path).unwrap();
                    assert_eq!(
                        unsafe { libc::flock(probe.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
                        -1,
                        "policy refresh must run while the authoritative ledger lock is held"
                    );
                    assert_eq!(
                        std::io::Error::last_os_error().kind(),
                        std::io::ErrorKind::WouldBlock
                    );
                    read_private_cli_config(fixture.path().to_str().unwrap())
                },
                &request,
                started,
                &mut (started + Duration::from_secs(1)),
                100,
            )
            .status
        });
        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        let mut revoked = fixture.initial.clone();
        revoked.credentials[0].revoked = true;
        fixture.save(&revoked);
        drop(lock);
        assert_eq!(worker.join().unwrap(), "unauthorized");
    });
    assert_eq!(fixture.used(), 1);
}
