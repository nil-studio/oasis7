//! Test-only Linux sampling independent of World/Writer/Node locks. Proc paths
//! are limited to this test process and its explicitly owned P/R children.
pub(super) struct Sampler {
    #[cfg(target_os = "linux")]
    stop: Option<std::sync::mpsc::Sender<()>>,
}

impl Sampler {
    pub(super) fn start(endpoint_pids: [u32; 2]) -> Self {
        #[cfg(target_os = "linux")]
        {
            if std::env::var("OASIS7_FORMAL_EXECUTION_TIMING").as_deref() != Ok("1") {
                return Self { stop: None };
            }
            let (stop, receive) = std::sync::mpsc::channel();
            let pids = [std::process::id(), endpoint_pids[0], endpoint_pids[1]];
            let result = std::thread::Builder::new()
                .name("formal-proc-sampler".into())
                .spawn(move || linux::run(pids, receive));
            if let Err(error) = result {
                eprintln!("formal_proc_sampler event=spawn_failed error={error:?}");
                return Self { stop: None };
            }
            // Dropping JoinHandle detaches; shutdown wakes recv_timeout rather
            // than waiting for the next 20s interval or joining the test thread.
            Self { stop: Some(stop) }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = endpoint_pids;
            Self {}
        }
    }
}
impl Drop for Sampler {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        collections::BTreeMap,
        fs::File,
        io::Read,
        path::Path,
        time::{Duration, Instant},
    };
    const THREADS: usize = 64;
    const STATUS: usize = 16 * 1024;
    const SMALL: usize = 4 * 1024;

    pub(super) fn run(pids: [u32; 3], stop: std::sync::mpsc::Receiver<()>) {
        let started = Instant::now();
        let mut previous = BTreeMap::new();
        loop {
            if stop.try_recv().is_ok() {
                return;
            }
            for (role, pid) in ["test", "primary", "replica"].into_iter().zip(pids) {
                sample(role, pid, started.elapsed(), &mut previous);
            }
            match stop.recv_timeout(Duration::from_secs(20)) {
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                _ => return,
            }
        }
    }
    fn bounded(path: &Path, limit: usize) -> (Option<String>, bool, Option<String>) {
        let result = (|| {
            let mut bytes = Vec::new();
            File::open(path)?
                .take((limit + 1) as u64)
                .read_to_end(&mut bytes)?;
            let truncated = bytes.len() > limit;
            bytes.truncate(limit);
            Ok::<_, std::io::Error>((String::from_utf8_lossy(&bytes).into_owned(), truncated))
        })();
        match result {
            Ok((text, truncated)) => (Some(text), truncated, None),
            Err(error) => (None, false, Some(error.kind().to_string())),
        }
    }
    fn fields(text: &str) -> serde_json::Value {
        let keep = ["Name", "State", "Threads", "VmRSS", "VmHWM"];
        let mut values = serde_json::Map::new();
        for line in text.lines() {
            if let Some((key, value)) = line.split_once(':')
                && keep.contains(&key)
            {
                values.insert(key.into(), value.trim().into());
            }
        }
        values.into()
    }
    fn sample(role: &str, pid: u32, elapsed: Duration, previous: &mut BTreeMap<(u32, u32), u64>) {
        let root = std::path::PathBuf::from(format!("/proc/{pid}"));
        let (status, status_truncated, status_error) = bounded(&root.join("status"), STATUS);
        let mut ids = Vec::new();
        let mut scan_truncated = false;
        let task_error = match std::fs::read_dir(root.join("task")) {
            Ok(entries) => {
                for entry in entries.take(4097) {
                    if ids.len() == 4096 {
                        scan_truncated = true;
                        break;
                    }
                    if let Ok(entry) = entry
                        && let Ok(tid) = entry.file_name().to_string_lossy().parse::<u32>()
                    {
                        ids.push(tid);
                    }
                }
                None
            }
            Err(error) => Some(error.kind().to_string()),
        };
        ids.sort_unstable();
        let observed_threads = ids.len();
        // Include both initial and newly spawned workers when a process has
        // many pool threads. Emit at most 64 records per known process/sample.
        if ids.len() > THREADS {
            ids.drain(THREADS / 2..ids.len() - THREADS / 2);
        }
        previous.retain(|(owner, tid), _| *owner != pid || ids.contains(tid));
        eprintln!(
            "formal_proc_sampler {}",
            serde_json::json!({
                "event":"process", "role":role,"pid":pid,"elapsed_ms":elapsed.as_millis(),
                "status":status.as_deref().map(fields), "status_truncated":status_truncated,
                "status_error":status_error,"task_error":task_error,
                "observed_threads":observed_threads,"emitted_threads":ids.len(),
                "threads_truncated":scan_truncated || observed_threads>THREADS,
            })
        );
        for tid in ids {
            let task = root.join("task").join(tid.to_string());
            let (stat, stat_truncated, stat_error) = bounded(&task.join("stat"), SMALL);
            let (wchan, wchan_truncated, wchan_error) = bounded(&task.join("wchan"), SMALL);
            // /proc stat's comm can contain whitespace and parentheses; only
            // fields after the final ')' have fixed numeric column positions.
            let parsed = stat.as_deref().and_then(|text| {
                let close = text.rfind(')')?;
                let open = text.find('(')?;
                let tail: Vec<_> = text[close + 1..].split_whitespace().collect();
                let user = tail.get(11)?.parse::<u64>().ok()?;
                let system = tail.get(12)?.parse::<u64>().ok()?;
                let cpu = user.saturating_add(system);
                let delta = previous
                    .insert((pid, tid), cpu)
                    .map(|old| cpu.saturating_sub(old));
                Some(
                    serde_json::json!({"name":&text[open+1..close],"state":tail.first(),
                    "utime_ticks":user,"stime_ticks":system,"cpu_delta_ticks":delta,
                    "rss_pages":tail.get(21),"processor":tail.get(36)}),
                )
            });
            eprintln!(
                "formal_proc_sampler {}",
                serde_json::json!({
                    "event":"thread","role":role,"pid":pid,"tid":tid,
                    "elapsed_ms":elapsed.as_millis(),"stat":parsed,
                    "stat_truncated":stat_truncated,"stat_error":stat_error,
                    "wchan":wchan.as_deref().map(str::trim),"wchan_truncated":wchan_truncated,
                    "wchan_error":wchan_error,
                })
            );
        }
    }
}
