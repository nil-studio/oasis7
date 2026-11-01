//! Local CLI routing proof only; no socket, different-UID or grant is fabricated.
#![cfg(unix)]
use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn explicit_custody_call_missing_binding_never_falls_back_to_executor() {
    let socket = std::env::temp_dir().join(format!(
        "oasis7-missing-custody-{}.sock",
        std::process::id()
    ));
    assert!(!socket.exists());
    let mut child = Command::new(env!("CARGO_BIN_EXE_oasis7_game_client"))
        .arg("custody-control-call")
        .arg("--socket")
        .arg(socket)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(br#"{"api_version":1,"resource_id":"fixed","request_id":"stable","method":"tools_list"}"#).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap().trim(),
        "custody Host unavailable; operator action required"
    );
}
