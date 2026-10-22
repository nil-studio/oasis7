use crate::{Error, Result};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
/// Fixed argument arrays only; bounded streams are drained concurrently to avoid pipe deadlocks.
pub fn run(program: &str, args: &[&str], cwd: &Path) -> Result<Vec<u8>> {
    let budget = Duration::from_secs(10);
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    #[cfg(unix)]
    let _group = OwnedProcessGroup(child.id() as i32);
    fn drain(
        mut pipe: impl Read + Send + 'static,
    ) -> std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>> {
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| {
                let mut retained = Vec::new();
                let mut buf = [0; 8192];
                loop {
                    let n = pipe.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    if retained.len() < 8 * 1024 * 1024 {
                        let take = n.min(8 * 1024 * 1024 - retained.len());
                        retained.extend_from_slice(&buf[..take]);
                    }
                }
                Ok(retained)
            })();
            let _ = send.send(result);
        });
        receive
    }
    let out = drain(child.stdout.take().unwrap());
    let err = drain(child.stderr.take().unwrap());
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > budget {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Invalid("source timeout".into()));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = out
        .recv_timeout(budget.saturating_sub(start.elapsed()))
        .map_err(|_| Error::Invalid("source reader timeout".into()))??;
    let errors = err
        .recv_timeout(budget.saturating_sub(start.elapsed()))
        .map_err(|_| Error::Invalid("source reader timeout".into()))??;
    if !status.success() {
        let message: String = String::from_utf8_lossy(&errors).chars().take(500).collect();
        return Err(Error::Invalid(message));
    }
    if bytes.len() >= 8 * 1024 * 1024 {
        return Err(Error::Invalid("source output exceeded limit".into()));
    }
    Ok(bytes)
}

#[cfg(unix)]
struct OwnedProcessGroup(i32);
#[cfg(unix)]
impl Drop for OwnedProcessGroup {
    fn drop(&mut self) {
        // SAFETY: the positive OS child PID identifies the new process group
        // created above. A negative PID targets only that owned group. Cleanup
        // prevents inherited pipes from leaving detached readers after timeout.
        unsafe {
            libc::kill(-self.0, libc::SIGKILL);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn descendant_pipes_do_not_keep_source_reader_alive() {
        let start = Instant::now();
        let result = run("/bin/sh", &["-c", "sleep 60 & exit 0"], Path::new("/"));
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_secs(12));
    }
}
