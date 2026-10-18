//! Acceptance-run limits and durable progress; these are not product wire limits.
use super::*;
use std::{fs::File, io::Write, os::unix::fs::OpenOptionsExt, time::Duration};

pub(super) struct Progress {
    pub(super) max_logical_bytes: u64,
    pub(super) step_bytes: u64,
    pub(super) interval: u64,
    requested: u64,
    completed: u64,
    started: Instant,
    timeout: Duration,
    output: Option<File>,
}
fn bounded(name: &str, value: &str, max: u64) -> u64 {
    let value: u64 = value.parse().unwrap_or_else(|_| panic!("invalid {name}"));
    assert!((1..=max).contains(&value), "{name} must be in 1..={max}");
    value
}
fn setting(name: &str, default: u64, max: u64) -> u64 {
    bounded(
        name,
        &std::env::var(name).unwrap_or_else(|_| default.to_string()),
        max,
    )
}
impl Progress {
    pub(super) fn from_env(requested: u64) -> Self {
        let output = std::env::var_os("OASIS7_STORAGE_PROGRESS_PATH").map(|path| {
            // The caller owns this output path. Never overwrite a previous run or
            // follow a final-component symlink; the CI runner supplies a fresh path.
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
                .expect("create fresh capacity progress output")
        });
        let mut progress = Self {
            max_logical_bytes: setting(
                "OASIS7_STORAGE_MAX_LOGICAL_BYTES",
                4 * 1024 * 1024 * 1024,
                1024 * 1024 * 1024 * 1024,
            ),
            step_bytes: setting(
                "OASIS7_STORAGE_STEP_BYTES",
                64 * 1024 * 1024,
                1024 * 1024 * 1024,
            ),
            interval: setting("OASIS7_STORAGE_PROGRESS_INTERVAL", 100, 1000),
            requested,
            completed: 0,
            started: Instant::now(),
            timeout: Duration::from_secs(setting("OASIS7_STORAGE_MAX_SECONDS", 18_000, 21_600)),
            output,
        };
        progress.emit(serde_json::json!({"event":"started","status":"running",
            "requested_heights":requested,"max_logical_bytes_per_domain":progress.max_logical_bytes,
            "shadow_step_bytes":progress.step_bytes,"max_seconds":progress.timeout.as_secs(),
            "scope":"object namespace only; excludes native executable/environment archive; same-host P/R are not independent fault domains"}));
        progress
    }
    fn emit(&mut self, value: serde_json::Value) {
        let line = serde_json::to_string(&value).unwrap();
        eprintln!("STORAGE_CAPACITY_PROGRESS={line}");
        if let Some(file) = &mut self.output {
            writeln!(file, "{line}").expect("write capacity progress");
            file.sync_data().expect("persist capacity progress");
        }
    }
    pub(super) fn check_time(&self) {
        assert!(
            self.started.elapsed() < self.timeout,
            "capacity acceptance time budget exceeded; partial run is not PASS"
        );
    }
    pub(super) fn sample(&mut self, height: u64, corpus: &Corpus) {
        self.completed = height;
        self.emit(serde_json::json!({"event":"growth","status":"running",
            "executed_heights":height,"requested_heights":self.requested,
            "elapsed_ns":self.started.elapsed().as_nanos(),"domains":corpus.inventories()}));
        self.check_time();
    }
    pub(super) fn partial(&mut self) {
        self.emit(serde_json::json!({"event":"finished","status":"partial",
            "last_reported_height":self.completed,"requested_heights":self.requested,
            "elapsed_ns":self.started.elapsed().as_nanos(),
            "reason":"fixture failed or acceptance budget exhausted; see test error; not PASS"}));
    }
    pub(super) fn complete(&mut self, report: &serde_json::Value) {
        self.check_time();
        self.emit(serde_json::json!({"event":"finished","status":"passed","report":report}));
    }
}
#[test]
fn capacity_acceptance_limits_reject_zero_malformed_and_excessive_budgets() {
    for value in ["0", "1025", "-1", "invalid"] {
        assert!(std::panic::catch_unwind(|| bounded("test budget", value, 1024)).is_err());
    }
    assert_eq!(bounded("test budget", "1", 1024), 1);
    assert_eq!(bounded("test budget", "1024", 1024), 1024);
}

#[test]
fn capacity_progress_persists_partial_without_a_success_report() {
    let scratch = Scratch::new();
    let path = scratch.path().join("progress.jsonl");
    let mut progress = Progress {
        max_logical_bytes: 8,
        step_bytes: 8,
        interval: 100,
        requested: 1000,
        completed: 0,
        started: Instant::now(),
        timeout: Duration::from_secs(1),
        output: Some(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap(),
        ),
    };
    let corpus = Corpus::new(scratch.path(), 8);
    progress.sample(0, &corpus);
    progress.partial();
    let lines: Vec<serde_json::Value> = fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["event"], "growth");
    assert_eq!(lines[0]["domains"][0]["logical_bytes"], 0);
    assert_eq!(lines[1]["status"], "partial");
    assert!(lines.iter().all(|line| line["status"] != "passed"));
}
