//! Nonblocking bounded stdout/stderr drain; libtest worker summaries stay out
//! of the parent's CI result stream. Cancellation also handles inherited pipes.
use super::*;
use std::{
    io::{Read, Seek, SeekFrom},
    os::fd::AsRawFd,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
const LIMIT: usize = 8 * 1024 * 1024;
pub(super) struct Output {
    cancel: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    eof: Arc<AtomicUsize>,
    drains: Vec<thread::JoinHandle<()>>,
    path: PathBuf,
}
impl Output {
    pub(super) fn start(child: &mut Child, path: &Path) -> Result<Self, String> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| e.to_string())?;
        let sink = Arc::new(Mutex::new((file, 0_usize)));
        let mut output = Self {
            cancel: Arc::new(AtomicBool::new(false)),
            failed: Arc::new(AtomicBool::new(false)),
            eof: Arc::new(AtomicUsize::new(0)),
            drains: Vec::new(),
            path: path.to_owned(),
        };
        output.drain(
            child
                .stdout
                .take()
                .ok_or("cold worker stdout pipe missing")?,
            sink.clone(),
        )?;
        output.drain(
            child
                .stderr
                .take()
                .ok_or("cold worker stderr pipe missing")?,
            sink,
        )?;
        Ok(output)
    }
    fn drain<R: Read + AsRawFd + Send + 'static>(
        &mut self,
        mut pipe: R,
        sink: Arc<Mutex<(File, usize)>>,
    ) -> Result<(), String> {
        // SAFETY: this live pipe descriptor is owned by the drainer. Preserve
        // existing flags and add nonblocking cancellation, never change a file.
        let flags = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let cancel = self.cancel.clone();
        let failed = self.failed.clone();
        let eof = self.eof.clone();
        let drain = thread::Builder::new()
            .name("cold-writer-output".into())
            .spawn(move || {
                let mut buffer = [0_u8; 8192];
                while !cancel.load(Ordering::Relaxed) {
                    match pipe.read(&mut buffer) {
                        Ok(0) => {
                            eof.fetch_add(1, Ordering::Release);
                            break;
                        }
                        Ok(count) => {
                            let mut sink = sink.lock().unwrap_or_else(|e| e.into_inner());
                            let keep = count.min(LIMIT.saturating_sub(sink.1));
                            if sink.0.write_all(&buffer[..keep]).is_err() {
                                failed.store(true, Ordering::Relaxed);
                            }
                            sink.1 += keep;
                            if keep != count {
                                failed.store(true, Ordering::Relaxed);
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => {
                            failed.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        self.drains.push(drain);
        Ok(())
    }
    pub(super) fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
    /// Child success is not output success. Wait for both pipes to reach EOF
    /// and join their drainers before accepting their final byte/error state.
    pub(super) fn finish(&mut self, deadline: Instant) -> Result<(), String> {
        while self.drains.iter().any(|drain| !drain.is_finished()) {
            if Instant::now() >= deadline {
                return Err("cold worker output EOF exceeded whole-case deadline".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        if Instant::now() >= deadline {
            return Err("cold worker output completion exceeded whole-case deadline".into());
        }
        for drain in self.drains.drain(..) {
            if drain.join().is_err() {
                self.failed.store(true, Ordering::Relaxed);
            }
        }
        if Instant::now() >= deadline {
            return Err("cold worker output joined after whole-case deadline".into());
        }
        if self.failed() || self.eof.load(Ordering::Acquire) != 2 {
            return Err("cold worker output limit/I/O/thread failure or incomplete EOF".into());
        }
        Ok(())
    }

    pub(super) fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        for drain in self.drains.drain(..) {
            let _ = drain.join();
        }
    }
    pub(super) fn failure_tail(&self) {
        let result = (|| -> Result<String, std::io::Error> {
            let mut file = File::open(&self.path)?;
            let length = file.metadata()?.len();
            file.seek(SeekFrom::Start(length.saturating_sub(16384)))?;
            let mut bytes = Vec::new();
            file.take(16384).read_to_end(&mut bytes)?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        })();
        eprintln!(
            "COLD_WRITER_FAILURE_TAIL={}",
            serde_json::json!({"path":self.path,"tail":result.unwrap_or_else(|e| e.to_string())})
        );
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        self.stop();
    }
}

#[test]
fn cold_writer_output_success_waits_for_real_child_eof_and_final_limit() {
    for bytes in [8192, LIMIT + 1] {
        let root = oracle_support::PrivateRoot::new().unwrap();
        let path = root.path().join("output.log");
        // Direct executable, with no shell or inherited grandchild writers.
        let mut child = Command::new("head")
            .args(["-c", &bytes.to_string(), "/dev/zero"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut output = Output::start(&mut child, &path).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if Instant::now() >= deadline {
                super::terminate(&mut child);
                panic!("bounded output fixture child failed to exit");
            }
            thread::sleep(Duration::from_millis(10));
        }
        let result = output.finish(deadline);
        assert_eq!(result.is_ok(), bytes <= LIMIT);
        assert_eq!(
            std::fs::metadata(path).unwrap().len(),
            bytes.min(LIMIT) as u64
        );
        assert!(output.drains.is_empty());
        assert_eq!(output.eof.load(Ordering::Acquire), 2);
    }
}
