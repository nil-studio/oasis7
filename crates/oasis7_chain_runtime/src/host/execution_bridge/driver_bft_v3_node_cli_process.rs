//! Real shipping child runner; no test binary dispatch or replacement production backend.
use std::{
    fs, io,
    os::{fd::OwnedFd, unix::net::UnixStream},
    path::Path,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(super) fn run(
    config: &Path,
    actions: Option<&Path>,
    root: &Path,
    closed_stdout: bool,
) -> (ExitStatus, String) {
    let binary = std::env::var_os("OASIS7_CHAIN_RUNTIME_BIN")
        .expect("real process test requires OASIS7_CHAIN_RUNTIME_BIN");
    let mut command = Command::new(binary);
    command.args(["bft-header", "node", "--config"]).arg(config);
    if let Some(actions) = actions {
        command.arg("--actions").arg(actions);
    } else {
        command.arg("--print-binding");
    }
    let stdout = if closed_stdout {
        let (writer, reader) = UnixStream::pair().unwrap();
        drop(reader);
        Stdio::from(OwnedFd::from(writer))
    } else {
        Stdio::from(fs::File::create(root.join("process.stdout")).unwrap())
    };
    let stderr_path = root.join("process.stderr");
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(150);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("shipping CLI child did not exit within bound: {result:?}");
            }
        }
    };
    let stderr = fs::read_to_string(stderr_path).unwrap_or_else(|error: io::Error| {
        panic!("could not read shipping child diagnostics: {error}")
    });
    (status, stderr)
}
