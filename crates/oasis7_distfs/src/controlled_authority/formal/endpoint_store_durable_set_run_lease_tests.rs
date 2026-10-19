use super::*;
use std::time::{Duration, Instant};
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "o7-healthy-run-{}-{}",
            std::process::id(),
            fencing::fresh_challenge().unwrap()
        ));
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700).create(&path).unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn healthy_scratch_creation_guard_protects_run_before_lease_exists() {
    let root = Root::new();
    let staging = root.0.join(".formal-durable-set-staging");
    directory(&staging).unwrap();
    let creation = parent_guard(&staging).unwrap();
    let interrupted = staging.join(format!("run-{}", "ac".repeat(32)));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&interrupted)
        .unwrap();
    assert!(matches!(
        RunLease::start(&root.0, false),
        Err(FormalError::Conflict(_))
    ));
    assert!(
        interrupted.exists(),
        "an in-progress creator cannot be declared orphaned"
    );
    drop(creation);
    let next = RunLease::start(&root.0, false).unwrap();
    assert!(
        !interrupted.exists(),
        "an empty interrupted creation is reclaimable only after its guard releases"
    );
    assert!(next.path.exists());
}
#[test]
fn healthy_scratch_scan_budget_preserves_unprobed_runs() {
    let root = Root::new();
    let staging = root.0.join(".formal-durable-set-staging");
    directory(&staging).unwrap();
    for number in 0..=MAX_SCAN_ENTRIES {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(staging.join(format!("run-{number:064x}")))
            .unwrap();
    }
    assert!(matches!(
        RunLease::start(&root.0, false),
        Err(FormalError::Conflict(_))
    ));
    let remaining = fs::read_dir(&staging)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("run-")
        })
        .count();
    assert_eq!(
        remaining, MAX_SCAN_ENTRIES,
        "one bounded call reclaims at most one confirmed orphan"
    );
}
#[test]
fn healthy_scratch_process_exit_releases_orphan_lease() {
    const CHILD_ROOT: &str = "OASIS7_HEALTHY_SCRATCH_CRASH_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let run = RunLease::start(Path::new(&root), false).unwrap();
        fs::write(
            Path::new(&root).join("orphan-name"),
            run.path.file_name().unwrap().as_encoded_bytes(),
        )
        .unwrap();
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(run.path.join("object-test"))
            .unwrap();
        // Abrupt process exit bypasses normal Drop, but the kernel releases the
        // lease. No PID/mtime/age assertion is used to authorize reclamation.
        std::process::exit(0);
    }
    let root = Root::new();
    let name = format!(
        "{}::healthy_scratch_process_exit_releases_orphan_lease",
        module_path!().split_once("::").unwrap().1
    );
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--test-threads=1"])
        .env(CHILD_ROOT, &root.0)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("scratch crash child exceeded bounded deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(status.success());
    let orphan = root
        .0
        .join(".formal-durable-set-staging")
        .join(fs::read_to_string(root.0.join("orphan-name")).unwrap());
    assert!(orphan.join(LEASE).exists());
    let next = RunLease::start(&root.0, false).unwrap();
    assert!(!orphan.exists());
    assert!(next.path.exists());
}
#[test]
fn healthy_scratch_rejects_foreign_entries_and_symlink_leases() {
    use std::os::unix::fs::symlink;
    let root = Root::new();
    let run = RunLease::start(&root.0, false).unwrap();
    let staging = run.staging.clone();
    let foreign = staging.join("not-a-run");
    fs::write(&foreign, []).unwrap();
    assert!(RunLease::start(&root.0, false).is_err());
    fs::remove_file(foreign).unwrap();
    let malformed = staging.join("run-not-a-digest");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&malformed)
        .unwrap();
    assert!(RunLease::start(&root.0, false).is_err());
    assert!(malformed.exists());
    fs::remove_dir(malformed).unwrap();
    let malicious = staging.join(format!("run-{}", "ab".repeat(32)));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&malicious)
        .unwrap();
    let outside = root.0.join("outside");
    fs::write(&outside, b"must remain unchanged").unwrap();
    symlink(&outside, malicious.join(LEASE)).unwrap();
    assert!(RunLease::start(&root.0, false).is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"must remain unchanged");
    assert!(run.path.exists());
}
