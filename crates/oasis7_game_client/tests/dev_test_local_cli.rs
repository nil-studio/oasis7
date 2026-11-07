#![cfg(unix)]

use std::{
    fs::{self, File},
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const LEGACY_USAGE: &str =
    "dev-test-serve --profile <private-config> | dev-test-call --socket <approved-socket>";

struct Invocation {
    root: PathBuf,
    child: Option<Child>,
}

impl Drop for Invocation {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child
            && !matches!(child.try_wait(), Ok(Some(_)))
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn rejects(args: &[&str], input: &[u8], expected: &str) {
    let mut id = [0; 12];
    getrandom::fill(&mut id).unwrap();
    let root = std::env::temp_dir().join(format!("o7-local-cli-{}", hex::encode(id)));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    let mut invocation = Invocation { root, child: None };
    let stdin = invocation.root.join("stdin");
    let stdout = invocation.root.join("stdout");
    let stderr = invocation.root.join("stderr");
    fs::write(&stdin, input).unwrap();
    // Finite files avoid pipe backpressure; the owner kills and reaps a broken
    // dispatcher even if an assertion unwinds before the command exits.
    invocation.child = Some(
        Command::new(env!("CARGO_BIN_EXE_oasis7_game_client"))
            .args(args)
            .stdin(Stdio::from(File::open(stdin).unwrap()))
            .stdout(Stdio::from(File::create(&stdout).unwrap()))
            .stderr(Stdio::from(File::create(&stderr).unwrap()))
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = invocation.child.as_mut().unwrap().try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "local CLI exceeded its deadline");
        thread::sleep(Duration::from_millis(10));
    };
    assert!(!status.success());
    assert!(fs::read(stdout).unwrap().is_empty());
    assert_eq!(fs::read_to_string(stderr).unwrap().trim(), expected);
}

#[test]
fn local_command_dispatch_preserves_legacy_errors_and_exact_names() {
    rejects(&["dev-test-local-call"], b"", LEGACY_USAGE);
    for command in [
        "dev-test-local-call",
        "dev-test-local-call-extra",
        "dev-test-local-serve-extra",
        "dev-test-unknown",
    ] {
        rejects(
            &[command, "--unknown", "unused"],
            &[b'x'; 65537],
            "unsupported_service_command",
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        rejects(
            &["dev-test-local-serve"],
            b"",
            "local peer authenticated service requires Linux",
        );
        rejects(
            &["dev-test-local-call", "--socket", "unused"],
            b"invalid",
            "unsupported_service_command",
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn local_serve_validates_arguments_uid_and_isolation_before_paths() {
    let usage = "dev-test-local-serve --socket PATH --profile PRIVATE_CONFIG --caller-uid UID --caller-id ID";
    rejects(&["dev-test-local-serve"], b"", usage);
    let mut args = [
        "dev-test-local-serve",
        "--socket",
        "unused",
        "--profile",
        "unused",
        "--caller-uid",
        "invalid",
        "--caller-id",
        "model",
    ];
    rejects(&args, b"", "invalid caller uid");
    // SAFETY: geteuid has no preconditions and only reads this process's UID.
    let uid = unsafe { libc::geteuid() }.to_string();
    args[6] = &uid;
    rejects(&args, b"", "distinct model uid required");
    args[5] = "--unknown";
    rejects(&args, b"", usage);
}

#[cfg(target_os = "linux")]
#[test]
fn local_call_keeps_bounded_closed_input_before_transport() {
    let args = ["dev-test-local-call", "--socket", "relative-unused-path"];
    for input in [b"".as_slice(), b"{", b"{\"unexpected\":true}"] {
        rejects(&args, input, "invalid local request");
    }
    let mut boundary = br#"{"profile":"dev-test-accounts-v1","audience":"oasis7-dev-test-tools","method":"capabilities"}"#.to_vec();
    boundary.resize(65536, b' ');
    rejects(&args, &boundary, "absolute local socket required");
    boundary.push(b' ');
    rejects(&args, &boundary, "local request too large");
    rejects(
        &args,
        br#"{"profile":"dev-test-accounts-v1","audience":"oasis7-dev-test-tools","method":"capabilities","credential":"synthetic-untrusted-token"}"#,
        "invalid local request",
    );
}
