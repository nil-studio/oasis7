use super::*;

const PROBE_ENV: &str = "PRE2_CLEANUP_DEADLINE_PROBE";

#[test]
fn cleanup_deadline_process_probe() {
    let Ok(mode) = std::env::var(PROBE_ENV) else {
        return;
    };
    let deadline = CleanupDeadline::with_budget("cleanup_probe", Duration::from_millis(100));
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
        "incomplete" => drop(deadline),
        _ => panic!("unknown cleanup probe mode"),
    }
    panic!("stuck cleanup must terminate its owning process");
}

#[test]
fn cleanup_deadline_stops_stuck_join_lock_and_incomplete_cleanup() {
    let root = temp_dir("qa-cleanup-deadline");
    fs::create_dir_all(&root).unwrap();
    let name = concat!(module_path!(), "::cleanup_deadline_process_probe");
    let name = name.split_once("::").unwrap().1;
    for mode in ["join", "lock", "incomplete"] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", name]).env(PROBE_ENV, mode);
        let output = application_completion::run_child_phase(&mut command, &root, mode);
        assert_eq!(output.status.code(), Some(CLEANUP_EXIT_CODE));
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("conformance_cleanup_failed stage=cleanup_probe"));
        assert!(stderr.contains(if mode == "incomplete" {
            "reason=incomplete"
        } else {
            "reason=deadline"
        }));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn completed_cleanup_cancels_and_joins_its_watchdog() {
    let deadline = CleanupDeadline::with_budget("completed_probe", Duration::from_secs(1));
    deadline.complete();
    // A forgotten cancellation would terminate this process after its deadline.
    thread::sleep(Duration::from_millis(1100));
}
