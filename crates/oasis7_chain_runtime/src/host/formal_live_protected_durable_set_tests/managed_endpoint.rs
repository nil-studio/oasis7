use super::*;
use std::os::unix::fs::OpenOptionsExt;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
};
#[path = "startup_diagnostics.rs"]
mod startup_diagnostics;
pub(crate) struct ManagedEndpointChild {
    child: Child,
    config: PathBuf,
    stderr: PathBuf,
    lines: Receiver<String>,
    reader: Option<thread::JoinHandle<()>>,
}
impl ManagedEndpointChild {
    pub(crate) fn id(&self) -> u32 {
        self.child.id()
    }
    pub(crate) fn start(binary: &std::ffi::OsStr, config: &Path) -> Result<Self, String> {
        let stderr = config.with_extension(format!(
            "{}.startup.stderr",
            crate::node_lifecycle::local_io::random_id()?
        ));
        let stderr_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&stderr)
            .map_err(|e| e.to_string())?;
        let mut child = Command::new(binary)
            .arg("--config")
            .arg(config)
            .env("OASIS7_HEALTHY_NATIVE_TRACE", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr_file))
            .spawn()
            .map_err(|error| format!("start healthy protected endpoint: {error}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or("healthy endpoint stdout unavailable")?;
        let (sender, lines) = mpsc::channel();
        let reader = thread::Builder::new()
            .name("healthy-endpoint-readiness".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    match line {
                        Ok(line) => {
                            if sender.send(line).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self {
            child,
            config: config.to_path_buf(),
            stderr,
            lines,
            reader: Some(reader),
        })
    }
    pub(crate) fn wait_management(
        &mut self,
        expected: std::net::SocketAddr,
        scenario: ScenarioDeadline,
    ) -> Result<(), String> {
        // Management readiness follows full authenticated closure staging, not
        // just process spawn. Keep the existing scenario's absolute budget;
        // sequential P/R waits must not each start a new staging allowance.
        scenario.report_stage("waiting for authenticated successor staging")?;
        self.wait_address(
            "formal_endpoint_awaiting_management=",
            expected,
            scenario.expires_at,
        )?;
        startup_diagnostics::report(&self.stderr, self.child.id());
        Ok(())
    }
    pub(crate) fn wait_ordinary(
        &mut self,
        expected: std::net::SocketAddr,
        timeout: Duration,
    ) -> Result<(), String> {
        self.wait_address(
            "formal_endpoint_listening=",
            expected,
            Instant::now() + timeout,
        )
    }
    fn failure(&mut self, prefix: &str, seen: &[String]) -> String {
        use std::io::Read;
        let status = self.child.try_wait();
        let mut stderr = String::new();
        if let Ok(file) = std::fs::File::open(&self.stderr) {
            let _ = file.take(16 * 1024).read_to_string(&mut stderr);
        }
        format!(
            "endpoint missing {prefix}; pid={} status={status:?} config={} stderr_path={} stderr={stderr:?} output={seen:?}",
            self.child.id(),
            self.config.display(),
            self.stderr.display()
        )
    }
    fn wait_address(
        &mut self,
        prefix: &str,
        expected: std::net::SocketAddr,
        deadline: Instant,
    ) -> Result<(), String> {
        let started = Instant::now();
        let mut next_diagnostic = started + Duration::from_secs(20);
        let mut seen = Vec::new();
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .filter(|value| !value.is_zero())
                .ok_or_else(|| self.failure(prefix, &seen))?;
            if self
                .child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
            {
                return Err(self.failure(prefix, &seen));
            }
            if Instant::now() >= next_diagnostic {
                eprintln!(
                    "healthy endpoint readiness pending prefix={prefix} pid={} config={} wait_ms={} remaining_ms={}",
                    self.child.id(),
                    self.config.display(),
                    started.elapsed().as_millis(),
                    remaining.as_millis()
                );
                next_diagnostic = Instant::now() + Duration::from_secs(20);
            }
            match self
                .lines
                .recv_timeout(remaining.min(Duration::from_millis(100)))
            {
                Ok(line) if line.starts_with(prefix) => {
                    if Instant::now() >= deadline
                        || self
                            .child
                            .try_wait()
                            .map_err(|error| error.to_string())?
                            .is_some()
                    {
                        return Err(self.failure(prefix, &seen));
                    }
                    let address = line[prefix.len()..]
                        .parse::<std::net::SocketAddr>()
                        .map_err(|error| format!("parse endpoint readiness {line:?}: {error}"))?;
                    if address != expected {
                        return Err(format!(
                            "endpoint readiness address {address} differs from expected {expected}"
                        ));
                    }
                    return Ok(());
                }
                Ok(line) => seen.push(line),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(self.failure(prefix, &seen));
                }
            }
        }
    }
}
impl Drop for ManagedEndpointChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[cfg(test)]
#[path = "managed_endpoint_tests.rs"]
mod tests;
