//! Special-file admission must finish without an external FIFO writer.
use super::load_module_artifact;
use std::{
    ffi::CString,
    fs,
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const CHILD_ROOT: &str = "OASIS7_LOCAL_PROVIDER_ARTIFACT_CHILD_ROOT";

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let directory = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-provider-special-file-{}",
            hex::encode(nonce)
        ));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn artifact_input_child() {
    let Some(root) = std::env::var_os(CHILD_ROOT) else {
        return;
    };
    let root = PathBuf::from(root);
    // Let the parent distinguish slow process startup from blocking file open.
    fs::write(root.join("entered"), b"opening-explicit-artifact-inputs").unwrap();
    let result = load_module_artifact(&root.join("provider.wasm"), &root.join("metadata.json"));
    assert!(result.is_err(), "special artifact input must be rejected");
}

fn rejects_fifo_without_writer(input: &str) {
    let scratch = Scratch::new();
    fs::write(scratch.0.join("provider.wasm"), b"ordinary-input-bytes").unwrap();
    fs::write(
        scratch.0.join("metadata.json"),
        serde_json::to_vec(&serde_json::json!({
            "source_hash": "11".repeat(32),
            "build_manifest_hash": "22".repeat(32),
        }))
        .unwrap(),
    )
    .unwrap();
    let special = scratch.0.join(input);
    fs::remove_file(&special).unwrap();
    let path = CString::new(special.as_os_str().as_bytes()).unwrap();
    // No other process opens this FIFO. A blocking open cannot finish.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let module = module_path!().split_once("::").unwrap().1;
    let exact_child = format!("{module}::artifact_input_child");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &exact_child, "--nocapture"])
        .env(CHILD_ROOT, &scratch.0)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let startup_deadline = Instant::now() + Duration::from_secs(10);
    while !scratch.0.join("entered").exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("artifact subprocess exited before input admission: {status}");
        }
        if Instant::now() >= startup_deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("artifact subprocess did not reach input admission");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let admission_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                status.success(),
                "artifact subprocess did not reject {input}"
            );
            return;
        }
        if Instant::now() >= admission_deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("artifact loader blocked on {input} FIFO without a writer");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn provider_wasm_fifo_is_rejected_without_blocking() {
    rejects_fifo_without_writer("provider.wasm");
}

#[test]
fn provider_metadata_fifo_is_rejected_without_blocking() {
    rejects_fifo_without_writer("metadata.json");
}

#[test]
fn provider_artifacts_require_regular_files_and_retain_bounded_bytes() {
    let scratch = Scratch::new();
    let wasm = scratch.0.join("provider.wasm");
    let metadata = scratch.0.join("metadata.json");
    let wasm_bytes = b"ordinary-explicit-input";
    let metadata_bytes = serde_json::to_vec(&serde_json::json!({
        "source_hash": "11".repeat(32),
        "build_manifest_hash": "22".repeat(32),
    }))
    .unwrap();
    fs::write(&wasm, wasm_bytes).unwrap();
    fs::write(&metadata, &metadata_bytes).unwrap();
    let (artifact, digest) = super::load_module_artifact_bound(&wasm, &metadata).unwrap();
    assert_eq!(artifact.wasm_bytes, wasm_bytes);
    assert_eq!(digest, oasis7_runtime::blake3_hex(&metadata_bytes));
    let alias = scratch.0.join("wasm-symlink");
    std::os::unix::fs::symlink(&wasm, &alias).unwrap();
    assert!(load_module_artifact(&alias, &metadata).is_err());
    let alias = scratch.0.join("metadata-symlink");
    std::os::unix::fs::symlink(&metadata, &alias).unwrap();
    assert!(load_module_artifact(&wasm, &alias).is_err());
    assert!(load_module_artifact(&scratch.0, &metadata).is_err());
    fs::write(&metadata, vec![b' '; 64 * 1024 + 1]).unwrap();
    assert!(load_module_artifact(&wasm, &metadata).is_err());
}
