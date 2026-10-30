//! Numeric-only, bounded evidence for the actual shared-stream budget fixture.
//! Disabled in production; no formatting or I/O occurs in a measured phase.
//!
//! Rows are [stage, process-local thread, start_us, end_us, a, b]. The clock
//! starts before either stream. Sort by start_us (nested spans finish out of order).
//! 1=request loop before periodic, 2=pump-start check, 3=socket read,
//! 5=handler including lock wait, 6=request outbox, 7=periodic tail,
//! 10=Viewer mutex wait, 20=pump iteration including wait, 21=pump mutex wait,
//! 22=pump apply, 23=pump prepare, 24=hosted prepare, 25=fresh prepare,
//! 26=metadata prepare, 30=periodic poll, 31=catch-up including wait,
//! 32=catch-up snapshot projection/encoding, 33=periodic apply,
//! 34=periodic response encoding, 35=periodic remote query,
//! 40=HelloV2 preparation/handling, 41=RequestSnapshot preparation/handling,
//! 42=other request preparation/handling. Stages 23..26 use a=thread CPU delta
//! in microseconds, b=1 if known / 0 if unknown. Linux test_tier_required only;
//! other targets, failed reads and backwards CPU samples are unknown, not zero CPU.
//! Wall minus CPU includes blocking and descheduling; it does not identify either.
//! Other ordinary phase a/b are zero.
//! 200 maps a serving thread to role a=0 offender / a=1 observer (b=0).
//! The fixture appends 100/101 sample rows after measurement: thread=0,
//! a=sample index; b=useful_us for 100, b=within_budget|complete<<1|pressure<<2
//! for 101. These are every actual sample, never a selected successful subset.
//! The ring retains the latest 4096 phases. dropped counts overwritten phases
//! and failed diagnostic try_lock acquisitions; nonzero means incomplete evidence.
//! Spans include scheduling pauses; overlap alone cannot prove a lock holder.
//! H1 needs an observer lock-wait interval plus a matching pump/periodic holder;
//! H2 needs the observer's preceding periodic interval before its next socket read.
#[cfg(any(test, feature = "test_tier_required"))]
mod enabled {
    use std::cell::Cell;
    use std::sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    };
    use std::time::Instant;
    const CAPACITY: usize = 4096;
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static DROPPED: AtomicU64 = AtomicU64::new(0);
    static NEXT_THREAD: AtomicU64 = AtomicU64::new(1);
    static CLOCK: OnceLock<Instant> = OnceLock::new();
    static ROWS: Mutex<Rows> = Mutex::new(Rows {
        values: [[0; 6]; CAPACITY],
        len: 0,
        next: 0,
    });
    thread_local! { static THREAD: Cell<u64> = const { Cell::new(0) }; }
    struct Rows {
        values: [[u64; 6]; CAPACITY],
        len: usize,
        next: usize,
    }
    pub(super) fn now() -> u64 {
        CLOCK
            .get_or_init(Instant::now)
            .elapsed()
            .as_micros()
            .min(u64::MAX as u128) as u64
    }
    pub(super) fn record(kind: u64, start: u64, a: u64, b: u64) {
        if !ENABLED.load(Ordering::Relaxed) {
            return;
        }
        let end = now();
        let thread = THREAD.with(|id| {
            if id.get() == 0 {
                id.set(NEXT_THREAD.fetch_add(1, Ordering::Relaxed));
            }
            id.get()
        });
        // Never wait for a diagnostic mutex while holding the Viewer mutex.
        if let Ok(mut rows) = ROWS.try_lock() {
            let index = rows.next;
            rows.values[index] = [kind, thread, start, end, a, b];
            rows.next = (index + 1) % CAPACITY;
            if rows.len == CAPACITY {
                DROPPED.fetch_add(1, Ordering::Relaxed);
            } else {
                rows.len += 1;
            }
            return;
        }
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
    fn thread_cpu() -> Option<u64> {
        #[cfg(all(target_os = "linux", feature = "test_tier_required"))]
        {
            let mut time = libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            // SAFETY: The clock constant is valid on Linux and time points to a
            // writable initialized timespec. This queries only the calling thread.
            if unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) } != 0 {
                return None;
            }
            let seconds = u64::try_from(time.tv_sec).ok()?;
            let nanos = u64::try_from(time.tv_nsec).ok()?;
            if nanos >= 1_000_000_000 {
                return None;
            }
            seconds.checked_mul(1_000_000_000)?.checked_add(nanos)
        }
        #[cfg(not(all(target_os = "linux", feature = "test_tier_required")))]
        None
    }
    fn cpu_delta(start: Option<u64>, end: Option<u64>) -> (u64, u64) {
        match start
            .zip(end)
            .and_then(|(start, end)| end.checked_sub(start))
        {
            Some(nanos) => (nanos / 1000, 1),
            None => (0, 0),
        }
    }
    // A thread CPU pair must never be completed on a different thread.
    pub(crate) struct Phase(
        Option<(u64, u64, Option<u64>)>,
        std::marker::PhantomData<std::rc::Rc<()>>,
    );
    fn phase_with_clock(kind: u64, enabled: bool, clock: impl FnOnce() -> Option<u64>) -> Phase {
        Phase(
            enabled.then(|| {
                let start = now();
                let cpu = if (23..=26).contains(&kind) {
                    clock()
                } else {
                    None
                };
                (kind, start, cpu)
            }),
            std::marker::PhantomData,
        )
    }
    pub(crate) fn phase(kind: u64) -> Phase {
        phase_with_clock(kind, ENABLED.load(Ordering::Relaxed), thread_cpu)
    }
    impl Drop for Phase {
        fn drop(&mut self) {
            if let Some((kind, start, cpu)) = self.0 {
                let (a, b) = if cpu.is_some() {
                    cpu_delta(cpu, thread_cpu())
                } else {
                    (0, 0)
                };
                record(kind, start, a, b);
            }
        }
    }
    #[cfg(test)]
    mod cpu_tests {
        use super::*;
        #[test]
        fn cpu_unknown_and_disabled_paths_remain_explicit() {
            assert_eq!(cpu_delta(Some(1000), Some(4000)), (3, 1));
            assert_eq!(cpu_delta(Some(4000), Some(1000)), (0, 0));
            assert_eq!(cpu_delta(None, Some(4000)), (0, 0));
            assert_eq!(cpu_delta(Some(1000), None), (0, 0));
            assert!(
                phase_with_clock(23, false, || panic!("disabled clock read"))
                    .0
                    .is_none()
            );
            assert_eq!(
                phase_with_clock(10, true, || panic!("unpaired clock read"))
                    .0
                    .unwrap()
                    .2,
                None
            );
        }
    }
    impl super::super::ViewerRuntimeLiveServer {
        /// Opt in only in the isolated slow-consumer child, before either stream starts.
        pub fn test_begin_stream_budget_diagnostics() {
            let _ = CLOCK.get_or_init(Instant::now);
            let mut rows = ROWS.lock().unwrap();
            rows.len = 0;
            rows.next = 0;
            DROPPED.store(0, Ordering::Relaxed);
            ENABLED.store(true, Ordering::Relaxed);
        }
        pub fn test_record_stream_budget_connection(observer: bool) {
            record(200, now(), u64::from(observer), 0);
        }
        pub fn test_stream_budget_timestamp() -> u64 {
            now()
        }
        /// Call after both serving workers finish, outside the measured budget and all guards.
        pub fn test_take_stream_budget_diagnostics() -> serde_json::Value {
            ENABLED.store(false, Ordering::Relaxed);
            let rows = ROWS.lock().unwrap();
            serde_json::json!({"rows": rows.values[..rows.len], "dropped": DROPPED.load(Ordering::Relaxed)})
        }
    }
}
#[cfg(any(test, feature = "test_tier_required"))]
pub(super) use enabled::phase;
#[cfg(not(any(test, feature = "test_tier_required")))]
pub(super) struct Phase;
#[cfg(not(any(test, feature = "test_tier_required")))]
#[inline]
pub(super) fn phase(_: u64) -> Phase {
    Phase
}

#[cfg(not(any(test, feature = "test_tier_required")))]
impl Drop for Phase {
    fn drop(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::super::ViewerRuntimeLiveServer;
    use super::*;
    #[test]
    fn numeric_ring_is_bounded_and_disabled_after_snapshot() {
        const CHILD: &str = "OASIS7_STREAM_BUDGET_RING_PROBE";
        if std::env::var_os(CHILD).is_none() {
            // The full Viewer test binary runs shared streams concurrently.
            // Never enable the process-global recorder in that parent process.
            let name = concat!(
                module_path!(),
                "::numeric_ring_is_bounded_and_disabled_after_snapshot"
            );
            let name = name.split_once("::").unwrap().1;
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated ring probe failed: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                String::from_utf8_lossy(&output.stdout).contains("running 1 test"),
                "exact ring probe must actually execute"
            );
            return;
        }
        ViewerRuntimeLiveServer::test_begin_stream_budget_diagnostics();
        for _ in 0..4104 {
            drop(phase(1));
        }
        let evidence = ViewerRuntimeLiveServer::test_take_stream_budget_diagnostics();
        assert_eq!(evidence["rows"].as_array().unwrap().len(), 4096);
        assert_eq!(evidence["dropped"], 8);
        drop(phase(1));
        assert_eq!(
            ViewerRuntimeLiveServer::test_take_stream_budget_diagnostics(),
            evidence
        );
        ViewerRuntimeLiveServer::test_begin_stream_budget_diagnostics();
        {
            let _parent = phase(23);
            for kind in 24..=26 {
                drop(phase(kind));
            }
        }
        let paired = ViewerRuntimeLiveServer::test_take_stream_budget_diagnostics();
        let rows = paired["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 4);
        let parent = rows.iter().find(|row| row[0] == 23).unwrap();
        for kind in 24..=26 {
            let child = rows.iter().find(|row| row[0] == kind).unwrap();
            assert_eq!(child[1], parent[1], "CPU spans remain on the same thread");
            assert!(child[2].as_u64().unwrap() >= parent[2].as_u64().unwrap());
            assert!(child[3].as_u64().unwrap() <= parent[3].as_u64().unwrap());
            assert!(child[5].as_u64().unwrap() <= 1);
            if child[5] == 0 {
                assert_eq!(child[4], 0);
            }
            #[cfg(not(all(target_os = "linux", feature = "test_tier_required")))]
            assert_eq!(child[5], 0, "unsupported CPU clocks are explicitly unknown");
        }
        assert_eq!(paired["dropped"], 0);
    }
}
