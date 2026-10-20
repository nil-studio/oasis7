#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn invoke(args: &[&str]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_oasis7_game_client"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"synthetic private CLI unlock")
        .unwrap();
    child.wait_with_output().unwrap()
}
#[test]
fn executable_initializes_and_reopens_encrypted_identity_without_network_or_secret_output() {
    let mut id = [0; 16];
    getrandom::fill(&mut id).unwrap();
    let root = Directory(
        std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("identity-cli-{}", hex::encode(id))),
    );
    std::fs::create_dir(&root.0).unwrap();
    let path = root.0.join("identity.json");
    std::fs::write(&path,serde_json::to_vec(&json!({"endpoint":"https://identity.invalid","identity_service_id":"test","environment_id":"test","audience":"https://identity.invalid","state_directory":root.0.join("vault")})).unwrap()).unwrap();
    let config = path.to_str().unwrap();
    let first = invoke(&["--identity-config", config, "identity-init"]);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let initialized: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(initialized["network_effects"], false);
    let reopened = invoke(&["--identity-config", config, "identity-public-key"]);
    assert!(reopened.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&reopened.stdout).unwrap()["public_key"],
        initialized["public_key"]
    );
    let duplicate = invoke(&["--identity-config", config, "identity-init"]);
    assert!(!duplicate.status.success());
    let status = invoke(&[
        "--identity-config",
        config,
        "identity-status",
        "--operation",
        "never-sent",
    ]);
    assert!(status.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&status.stdout).unwrap()["state"],
        "not_started"
    );
    for output in [first, reopened, duplicate, status] {
        assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic private CLI unlock"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("synthetic private CLI unlock"));
    }
}
