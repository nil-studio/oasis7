//! Process/pipe component tests only, never Native/H/Account authority evidence.
use super::*;
#[test]
#[cfg(unix)]
fn native_model_local_client_stalled_stdin_is_deadline_bounded() {
    let child = Command::new("/bin/sleep")
        .arg("10")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    // Exceed any ordinary pipe capacity deterministically; production rejects
    // >32KiB before spawning. This tests cancellation, not a larger wire cap.
    let result = wait_client(
        child,
        vec![0; 2 * 1024 * 1024],
        now().unwrap() + 100,
        Duration::from_millis(100),
        started,
    );
    assert!(result.is_err());
    assert!(started.elapsed() >= Duration::from_millis(75));
    assert!(started.elapsed() < Duration::from_secs(2));
}
#[test]
#[cfg(unix)]
fn native_model_local_client_normal_pipe_completes() {
    let child = Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    assert_eq!(
        wait_client(
            child,
            b"{}".to_vec(),
            now().unwrap() + 2000,
            Duration::from_secs(2),
            started
        )
        .unwrap(),
        b"{}"
    );
}
