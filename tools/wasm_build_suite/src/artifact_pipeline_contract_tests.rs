//! Exercise the scripts that select and hydrate the builder's Runtime artifacts.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const SCOPE: &str = include_str!("../../../scripts/plan-wasm-determinism-scope.sh");
const SCOPE_TEST: &str = include_str!("../../../scripts/plan-wasm-determinism-scope.test.sh");
const SYNC_M1: &str = include_str!("../../../scripts/sync-m1-builtin-wasm-artifacts.sh");
const SYNC_TEST: &str = include_str!("../../../scripts/sync-m1-builtin-wasm-artifacts.test.sh");
const SYNC_M4: &str = include_str!("../../../scripts/sync-m4-builtin-wasm-artifacts.sh");
const SYNC_M5: &str = include_str!("../../../scripts/sync-m5-builtin-wasm-artifacts.sh");

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assert_compiled_input(relative: &str, compiled: &str) {
    assert_eq!(
        fs::read_to_string(repo_root().join(relative)).expect("read script input"),
        compiled,
        "executed script must equal the compiled test input: {relative}"
    );
}

fn assert_success(output: Output) -> String {
    assert!(
        output.status.success(),
        "script failed: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("script stdout UTF-8")
}

#[test]
fn artifact_scope_executes_fail_closed_and_owner_path_contracts() {
    assert_compiled_input("scripts/plan-wasm-determinism-scope.sh", SCOPE);
    assert_compiled_input("scripts/plan-wasm-determinism-scope.test.sh", SCOPE_TEST);
    let stdout = assert_success(
        Command::new("bash")
            .arg(repo_root().join("scripts/plan-wasm-determinism-scope.test.sh"))
            .current_dir(repo_root())
            .output()
            .expect("execute actual scope regression suite"),
    );
    assert!(stdout.contains("plan-wasm-determinism-scope.test: OK"));
}

#[test]
fn artifact_sync_executes_cli_no_write_and_distfs_layout_contracts() {
    assert_compiled_input("scripts/sync-m1-builtin-wasm-artifacts.sh", SYNC_M1);
    assert_compiled_input("scripts/sync-m1-builtin-wasm-artifacts.test.sh", SYNC_TEST);
    assert_success(
        Command::new("bash")
            .arg(repo_root().join("scripts/sync-m1-builtin-wasm-artifacts.test.sh"))
            .current_dir(repo_root())
            .output()
            .expect("execute actual sync CLI fixture suite"),
    );
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "oasis7-artifact-wrapper-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("unix epoch")
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("scripts")).expect("create wrapper fixture");
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn artifact_sync_wrappers_forward_exact_runtime_owner_paths_and_caller_arguments() {
    for (module, source) in [("m4", SYNC_M4), ("m5", SYNC_M5)] {
        let relative = format!("scripts/sync-{module}-builtin-wasm-artifacts.sh");
        assert_compiled_input(&relative, source);
        let scratch = Scratch::new();
        let wrapper = scratch.0.join(&relative);
        fs::write(&wrapper, source).expect("write actual wrapper");
        let delegate = scratch.0.join("scripts/sync-m1-builtin-wasm-artifacts.sh");
        fs::write(
            &delegate,
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$@\"\n",
        )
        .expect("write argument capture delegate");
        fs::set_permissions(&delegate, fs::Permissions::from_mode(0o755))
            .expect("make capture delegate executable");
        let output_dir = scratch.0.join("output with spaces");
        let stdout = assert_success(
            Command::new("bash")
                .arg(&wrapper)
                .arg("--check")
                .arg("--out-dir")
                .arg(&output_dir)
                .output()
                .expect("execute actual wrapper"),
        );
        let artifacts = scratch.0.join("crates/oasis7_runtime/src/world/artifacts");
        let expected = vec![
            "--module-ids-path".to_string(),
            artifacts
                .join(format!("{module}_builtin_module_ids.txt"))
                .display()
                .to_string(),
            "--hash-path".to_string(),
            artifacts
                .join(format!("{module}_builtin_modules.sha256"))
                .display()
                .to_string(),
            "--check".to_string(),
            "--out-dir".to_string(),
            output_dir.display().to_string(),
        ];
        assert_eq!(stdout.lines().collect::<Vec<_>>(), expected);
        let help = assert_success(
            Command::new("bash")
                .arg(&wrapper)
                .arg("--help")
                .output()
                .expect("wrapper help"),
        );
        assert!(help.contains(&format!(
            "crates/oasis7_runtime/src/world/artifacts/{module}_builtin_module_ids.txt"
        )));
        assert!(help.contains(&format!(
            "crates/oasis7_runtime/src/world/artifacts/{module}_builtin_modules.sha256"
        )));
        assert!(
            !output_dir.exists(),
            "wrapper contract must not perform build writes"
        );
    }
}
