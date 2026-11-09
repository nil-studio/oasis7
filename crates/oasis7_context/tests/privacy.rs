mod support;
use std::process::{Command, Stdio};
#[test]
fn diagnostics_do_not_echo_private_input_and_human_display_escapes_terminal_controls() {
    let h = support::Harness::new(env!("CARGO_BIN_EXE_oasis7_context"), &[]);
    h.init();
    let candidate = h.candidate(
        "machine-test",
        "PRIVATE_BODY_MARKER\x1b]52;c;fixture\x07\u{009b}31m\n",
    );
    h.put("machine-test", &candidate, None);
    let human = h.invoke(
        &["get", "machine-test", "--root", h.root.to_str().unwrap()],
        None,
    );
    assert!(human.status.success());
    assert!(!human.stdout.contains(&0x1b));
    assert!(!human.stdout.contains(&7));
    assert!(!String::from_utf8_lossy(&human.stdout).contains('\u{009b}'));
    assert!(String::from_utf8_lossy(&human.stdout).contains("PRIVATE_BODY_MARKER"));
    std::fs::write(
        &candidate,
        "---\n{\"invalid\":\"PRIVATE_ERROR_MARKER\"}\n---\n",
    )
    .unwrap();
    let invalid = h.invoke(
        &[
            "--root",
            h.root.to_str().unwrap(),
            "put",
            "machine-test",
            "--file",
            candidate.to_str().unwrap(),
            "--if-absent",
            "--json",
        ],
        None,
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&invalid.stdout).contains("PRIVATE_ERROR_MARKER"));
    assert!(!String::from_utf8_lossy(&invalid.stderr).contains("PRIVATE_ERROR_MARKER"));
    let error: serde_json::Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(error["operation"], "put");
}

#[test]
#[allow(unsafe_code)] // This fixture owns both descriptors returned by openpty exactly once.
fn raw_markdown_refuses_terminal_stdout() {
    use std::os::fd::{FromRawFd, OwnedFd};
    let h = support::Harness::new(env!("CARGO_BIN_EXE_oasis7_context"), &[]);
    h.init();
    let candidate = h.candidate("machine-test", "\x1b]52;c;fixture\x07\n");
    h.put("machine-test", &candidate, None);
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: openpty initializes the two descriptor outputs; optional pointers are null.
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        },
        0
    );
    // SAFETY: successful openpty transferred two distinct live descriptors to this fixture.
    let _master = unsafe { OwnedFd::from_raw_fd(master) };
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    let output = Command::new(&h.binary)
        .args([
            "get",
            "machine-test",
            "--root",
            h.root.to_str().unwrap(),
            "--format",
            "markdown",
        ])
        .env("XDG_CONFIG_HOME", h.temp.path().join("config"))
        .stdout(Stdio::from(slave))
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("RAW_OUTPUT_REQUIRES_PIPE"));
}

#[test]
fn interactive_passphrase_is_hidden_and_echo_is_restored() {
    let h = support::Harness::new(env!("CARGO_BIN_EXE_oasis7_context"), &[]);
    h.init();
    let target = h.temp.path().join("interactive.enc");
    let script = r#"
import os, pty, fcntl, termios, subprocess, select, signal, time, sys
master, slave = pty.openpty()
original = termios.tcgetattr(slave)
def child_terminal():
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
# Keep the controlling session alive until restoration is checked. Polling from
# the driver can miss a fast exit; macOS revokes the terminal on session exit.
supervisor = '''
import os, subprocess, sys, termios
result = subprocess.run(sys.argv[2:], timeout=25)
assert termios.tcgetattr(0)[3] == int(sys.argv[1]), 'terminal echo was not restored'
os.write(0, b'Terminal restoration checked. ')
assert sys.stdin.buffer.readline() == b'\\n'
sys.exit(result.returncode)
'''
child = subprocess.Popen([sys.executable, '-c', supervisor, str(original[3]), sys.argv[1], 'snapshot', 'export', '--root', sys.argv[2], '--output', sys.argv[3], '--json'], stdin=slave, stdout=subprocess.PIPE, stderr=subprocess.PIPE, preexec_fn=child_terminal)
def prompt(expected, hidden=True):
    seen = b''
    deadline = time.monotonic() + 20
    while expected not in seen:
        assert time.monotonic() < deadline, 'terminal prompt timeout'
        if select.select([master], [], [], 0.2)[0]: seen += os.read(master, 4096)
    if hidden:
        assert not (termios.tcgetattr(slave)[3] & termios.ECHO), 'secret input echoed'
    return seen
try:
    seen = prompt(b'Snapshot passphrase: ')
    os.write(master, b'fixture-tty-passphrase\n')
    seen += prompt(b'Confirm passphrase: ')
    assert b'fixture-tty-passphrase' not in seen
    os.write(master, b'fixture-tty-passphrase\n')
    # The supervisor waits for this ACK, so all binary TTY output is read before
    # session teardown can revoke the terminal or discard its final bytes.
    seen += prompt(b'Terminal restoration checked. ', hidden=False)
    os.write(master, b'\n')
    stdout, stderr = child.communicate(timeout=5)
    assert child.returncode == 0, stderr.decode('utf-8', errors='replace')
    assert b'fixture-tty-passphrase' not in stdout + stderr
    assert b'fixture-tty-passphrase' not in seen
finally:
    if child.poll() is None:
        os.killpg(child.pid, signal.SIGKILL)
        child.communicate(timeout=5)
    os.close(master); os.close(slave)
"#;
    let output = Command::new("/usr/bin/python3")
        .args([
            "-c",
            script,
            h.binary.to_str().unwrap(),
            h.root.to_str().unwrap(),
            target.to_str().unwrap(),
        ])
        .env("XDG_CONFIG_HOME", h.temp.path().join("config"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(target.exists());
}

#[test]
fn overlong_piped_passphrase_is_rejected_without_export() {
    let h = support::Harness::new(env!("CARGO_BIN_EXE_oasis7_context"), &[]);
    h.init();
    let target = h.temp.path().join("overlong.enc");
    let mut pass = vec![b'x'; 1024];
    pass.extend_from_slice(b"\r\ntrailing");
    let output = h.invoke(
        &[
            "snapshot",
            "export",
            "--root",
            h.root.to_str().unwrap(),
            "--output",
            target.to_str().unwrap(),
            "--passphrase-stdin",
            "--json",
        ],
        Some(&pass),
    );
    assert_eq!(output.status.code(), Some(8));
    assert!(!target.exists());
}
