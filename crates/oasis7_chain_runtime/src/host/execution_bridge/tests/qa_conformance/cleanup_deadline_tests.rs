use super::*;

const PROBE_ENV: &str = "PRE2_CLEANUP_DEADLINE_PROBE";

#[test]
fn cleanup_deadline_process_probe() {
    let Ok(mode) = std::env::var(PROBE_ENV) else {
        return;
    };
    if mode == "incomplete" {
        // Exercise the real Drop path without a competing deadline. The worker
        // waits for completion/disconnection, so a no-op Drop cannot exit 124.
        let (completed, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let _ = receiver.recv();
        });
        drop(CleanupDeadline {
            stage: "cleanup_probe",
            completed: Some(completed),
            worker: Some(worker),
        });
    } else {
        let _deadline = CleanupDeadline::with_budget("cleanup_probe", Duration::from_millis(100));
        match mode.as_str() {
            "join" => {
                let worker = thread::spawn(|| loop {
                    thread::park();
                });
                worker.join().unwrap();
            }
            "lock" => {
                let mutex = Mutex::new(());
                let _held = mutex.lock().unwrap();
                let _blocked = mutex.lock().unwrap();
            }
            _ => panic!("unknown cleanup probe mode"),
        }
    }
    panic!("stuck cleanup must terminate its owning process");
}

fn probe_command(mode: &str) -> std::process::Command {
    let name = concat!(module_path!(), "::cleanup_deadline_process_probe");
    let name = name.split_once("::").unwrap().1;
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", name]).env(PROBE_ENV, mode);
    command
}

#[test]
fn cleanup_deadline_stops_stuck_join_lock_and_incomplete_cleanup() {
    let root = temp_dir("qa-cleanup-deadline");
    fs::create_dir_all(&root).unwrap();
    for mode in ["join", "lock", "incomplete"] {
        let started = Instant::now();
        let output = application_completion::run_child_phase(&mut probe_command(mode), &root, mode);
        assert_eq!(output.status.code(), Some(CLEANUP_EXIT_CODE), "{mode}");
        assert!(started.elapsed() < Duration::from_secs(20), "{mode}");
        let stderr = String::from_utf8(output.stderr).unwrap();
        let reason = if mode == "incomplete" {
            "incomplete"
        } else {
            "deadline"
        };
        let expected = format!(
            "conformance_cleanup_failed stage=cleanup_probe reason={reason} exit_code=124\n"
        );
        // Termination is mandatory; the diagnostic may be absent or cut short
        // by the fallback exit. Its full observable format is tested below.
        assert!(expected.starts_with(&stderr), "{mode}: {stderr:?}");
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cleanup_diagnostic_preserves_stage_reason_and_exit_code() {
    for reason in ["deadline", "incomplete"] {
        let mut captured = Vec::new();
        write_diagnostic(&mut captured, "cleanup_probe", reason).unwrap();
        assert_eq!(
            String::from_utf8(captured).unwrap(),
            format!(
                "conformance_cleanup_failed stage=cleanup_probe reason={reason} exit_code=124\n"
            )
        );
    }
}

#[cfg(unix)]
#[test]
fn blocked_stderr_cannot_prevent_cleanup_exit() {
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::process::Stdio;

    let (mut reader, mut writer) = UnixStream::pair().unwrap();
    writer.set_nonblocking(true).unwrap();
    let mut filled = 0;
    // Single-byte writes leave no small free tail that could admit a diagnostic.
    loop {
        match writer.write(b"x") {
            Ok(0) => panic!("stderr socket closed while filling"),
            Ok(count) => filled += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("fill stderr socket: {error}"),
        }
    }
    assert!(filled > 0);
    writer.set_nonblocking(false).unwrap();
    let mut command = probe_command("lock");
    command.stdout(Stdio::null()).stderr(OwnedFd::from(writer));
    let started = Instant::now();
    let mut child = command.spawn().unwrap();
    // Command owns another writer reference; close it before checking EOF.
    drop(command);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(5) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("cleanup failed to exit while stderr was blocked");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(CLEANUP_EXIT_CODE));
    // Do not drain until the child has exited: the diagnostic must not need a
    // cooperating reader. Reaching EOF also confirms no owned writer remains.
    reader
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut captured = Vec::new();
    reader.read_to_end(&mut captured).unwrap();
    assert_eq!(captured, vec![b'x'; filled]);
}

#[test]
fn completed_cleanup_cancels_and_joins_its_watchdog() {
    let deadline = CleanupDeadline::with_budget("completed_probe", Duration::from_secs(1));
    deadline.complete();
    // A forgotten cancellation would terminate this process after its deadline.
    thread::sleep(Duration::from_millis(1100));
}
