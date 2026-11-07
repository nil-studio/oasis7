//! Real owned-child probes for the launcher, independent of the world fixture.
use super::*;

const PROBE_ENV: &str = "PRE2_COMPLETION_LAUNCHER_PROBE";
const ROOT_ENV: &str = "PRE2_COMPLETION_LAUNCHER_ROOT";
const LABEL_ENV: &str = "PRE2_COMPLETION_LAUNCHER_LABEL";
const PROBE_BUDGET: Duration = Duration::from_secs(5);

// Ordinary no-op unless selected by the exact-name subprocess below. Keep the
// existing ignored-test inventory unchanged, as other libtest child probes do.
#[test]
fn isolated_launcher_child() {
    let Ok(mode) = std::env::var(PROBE_ENV) else {
        return;
    };
    let root = std::path::PathBuf::from(std::env::var_os(ROOT_ENV).unwrap());
    let label = std::env::var(LABEL_ENV).unwrap();
    println!("launcher_probe_stdout={label}");
    eprintln!("launcher_probe_stderr={label}");
    std::io::stdout().flush().unwrap();
    std::io::stderr().flush().unwrap();
    fs::write(
        root.join(format!("{label}.ready")),
        std::process::id().to_string(),
    )
    .unwrap();
    match mode.as_str() {
        "hang" => loop {
            thread::park();
        },
        "exit73" => std::process::exit(73),
        "success" => (),
        _ => panic!("unknown launcher probe mode"),
    }
}

fn probe_command(root: &std::path::Path, mode: &str, label: &str) -> Command {
    let name = concat!(module_path!(), "::isolated_launcher_child");
    let name = name.split_once("::").unwrap().1;
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", name, "--nocapture"])
        .env(PROBE_ENV, mode)
        .env(ROOT_ENV, root)
        .env(LABEL_ENV, label);
    command
}

fn assert_probe_started(root: &std::path::Path, label: &str, output: &Output) {
    let pid = fs::read_to_string(root.join(format!("{label}.ready")))
        .expect("the real child must reach the injected phase before the deadline");
    assert!(pid.parse::<u32>().unwrap() > 0);
    assert!(String::from_utf8_lossy(&output.stdout).contains("running 1 test"));
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains(&format!("launcher_probe_stdout={label}"))
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains(&format!("launcher_probe_stderr={label}"))
    );
}

fn assert_killed(output: &Output) {
    assert!(!output.status.success());
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(output.status.signal(), Some(libc::SIGKILL));
    }
}

fn captured_log(root: &std::path::Path, phase: &str, stream: &str) -> Vec<u8> {
    let directory = std::env::var_os("PRE2_CONFORMANCE_EVIDENCE_DIR")
        .map(|directory| std::path::PathBuf::from(directory).join(root.file_name().unwrap()))
        .unwrap_or_else(|| root.to_path_buf());
    fs::read(directory.join(format!("completion-{phase}.{stream}.log"))).unwrap()
}

#[test]
fn hanging_initial_child_is_killed_and_reaped_with_captured_output() {
    let root = temp_dir("qa-launcher-hanging-initial");
    fs::create_dir_all(&root).unwrap();
    let mut command = probe_command(&root, "hang", "initial");
    let started = Instant::now();
    let output = run_child_with_budget(
        &mut command,
        &root,
        "wait-rejection-initial",
        PROBE_BUDGET,
    );
    assert_probe_started(&root, "initial", &output);
    assert_killed(&output);
    assert!(started.elapsed() < Duration::from_secs(20));
    assert_eq!(
        captured_log(&root, "wait-rejection-initial", "stdout"),
        output.stdout
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn hanging_recovery_child_is_bounded_after_exit73() {
    let root = temp_dir("qa-launcher-hanging-recovery");
    fs::create_dir_all(&root).unwrap();
    let mut command = probe_command(&root, "exit73", "crash");
    let first = run_child_with_budget(
        &mut command,
        &root,
        "wait-rejection-initial",
        PROBE_BUDGET,
    );
    assert_probe_started(&root, "crash", &first);
    assert_eq!(first.status.code(), Some(73));
    let first_stdout = captured_log(&root, "wait-rejection-initial", "stdout");
    let first_stderr = captured_log(&root, "wait-rejection-initial", "stderr");
    command.env(PROBE_ENV, "hang").env(LABEL_ENV, "recovery");
    let started = Instant::now();
    let recovered = run_child_with_budget(
        &mut command,
        &root,
        "wait-rejection-recovery",
        PROBE_BUDGET,
    );
    assert_probe_started(&root, "recovery", &recovered);
    assert_killed(&recovered);
    assert!(started.elapsed() < Duration::from_secs(20));
    assert_eq!(
        captured_log(&root, "wait-rejection-initial", "stdout"),
        first_stdout
    );
    assert_eq!(
        captured_log(&root, "wait-rejection-initial", "stderr"),
        first_stderr
    );
    assert_eq!(
        captured_log(&root, "wait-rejection-recovery", "stdout"),
        recovered.stdout
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn same_directory_phases_preserve_logs_and_reusable_command_capture() {
    let root = temp_dir("qa-launcher-two-phases");
    fs::create_dir_all(&root).unwrap();
    let mut command = probe_command(&root, "exit73", "crash");
    let first = run_child_phase(&mut command, &root, "wait-rejection-initial");
    assert_probe_started(&root, "crash", &first);
    assert_eq!(first.status.code(), Some(73));
    command.env(PROBE_ENV, "success").env(LABEL_ENV, "recovery");
    let recovered = run_child_phase(&mut command, &root, "wait-rejection-recovery");
    assert_probe_started(&root, "recovery", &recovered);
    assert!(recovered.status.success());
    let recovery_stdout = captured_log(&root, "wait-rejection-recovery", "stdout");
    let recovery_stderr = captured_log(&root, "wait-rejection-recovery", "stderr");
    assert_eq!(recovery_stdout, recovered.stdout);
    assert_eq!(recovery_stderr, recovered.stderr);

    // output() must not retain either phase's file descriptors and return
    // empty buffers, nor may it append this third run to the recovery logs.
    command.env(LABEL_ENV, "plain-output");
    let plain = command.output().unwrap();
    assert_probe_started(&root, "plain-output", &plain);
    assert!(plain.status.success());
    assert_eq!(
        captured_log(&root, "wait-rejection-initial", "stdout"),
        first.stdout
    );
    assert_eq!(
        captured_log(&root, "wait-rejection-initial", "stderr"),
        first.stderr
    );
    assert_eq!(
        captured_log(&root, "wait-rejection-recovery", "stdout"),
        recovery_stdout
    );
    assert_eq!(
        captured_log(&root, "wait-rejection-recovery", "stderr"),
        recovery_stderr
    );
    fs::remove_dir_all(root).unwrap();
}
