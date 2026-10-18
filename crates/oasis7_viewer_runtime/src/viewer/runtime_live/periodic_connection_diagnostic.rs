//! Opt-in, bounded Viewer connection/read lifecycle diagnostics.
//!
//! This module records only process-local sequence numbers, fixed enums, and
//! bounded numeric timings/cursors. It deliberately owns no request or world
//! material. The sink is constructed only when the existing trace opt-in is
//! exactly `1`.

use std::io::Write;
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::Instant;

pub(super) const TRACE_ENV: &str = "OASIS7_VIEWER_PERIODIC_IO_TRACE";
const TRACE_PREFIX: &str = "OASIS7_VIEWER_CONNECTION_TRACE ";
const DETAIL_LIMIT: u64 = 512;
const ELAPSED_MAX_MS: u64 = 120_000;

pub(super) struct TraceSink {
    attempts: AtomicU64,
    next_connection: AtomicU64,
    next_job: AtomicU64,
    #[cfg(test)]
    observed: Mutex<Vec<String>>,
    #[cfg(test)]
    silent: bool,
}

impl Default for TraceSink {
    fn default() -> Self {
        Self {
            attempts: AtomicU64::new(0),
            next_connection: AtomicU64::new(1),
            next_job: AtomicU64::new(1),
            #[cfg(test)]
            observed: Mutex::new(Vec::new()),
            #[cfg(test)]
            silent: true,
        }
    }
}

impl TraceSink {
    pub(super) fn from_env() -> Option<Arc<Self>> {
        std::env::var_os(TRACE_ENV)
            .is_some_and(|value| value == "1")
            .then(|| {
                Arc::new(Self {
                    attempts: AtomicU64::new(0),
                    next_connection: AtomicU64::new(1),
                    next_job: AtomicU64::new(1),
                    #[cfg(test)]
                    observed: Mutex::new(Vec::new()),
                    #[cfg(test)]
                    silent: false,
                })
            })
    }

    #[cfg(test)]
    pub(super) fn for_test() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(super) fn connection(self: &Arc<Self>) -> ConnectionTrace {
        ConnectionTrace {
            sink: self.clone(),
            sequence: next_sequence(&self.next_connection),
            pending_job: Arc::new(AtomicU64::new(0)),
            close_reason: Arc::new(AtomicU8::new(CloseReason::RuntimeError.code())),
            close_snapshot: Arc::new(std::sync::Mutex::new(None)),
            opened_at: Instant::now(),
        }
    }

    pub(super) fn job_sequence(&self) -> u64 {
        next_sequence(&self.next_job)
    }

    fn emit(&self, record: impl FnOnce() -> String) {
        let attempt = self.attempts.fetch_add(1, Ordering::Relaxed) + 1;
        if attempt <= DETAIL_LIMIT {
            self.write_line(record());
        } else if attempt == DETAIL_LIMIT + 1 {
            self.write_line(format!(
                "{TRACE_PREFIX}schema=v3 event=truncated detail_attempts_seen={} first_suppressed=513",
                attempt
            ));
        }
    }

    fn write_line(&self, line: String) {
        #[cfg(test)]
        self.observed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(line.clone());
        #[cfg(test)]
        if self.silent {
            return;
        }
        let mut stderr = std::io::stderr().lock();
        let _ = stderr.write_all(line.as_bytes());
        let _ = stderr.write_all(b"\n");
    }

    #[cfg(test)]
    pub(super) fn observed(&self) -> Vec<String> {
        self.observed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    #[cfg(test)]
    pub(super) fn attempted(&self) -> u64 {
        self.attempts.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(super) fn emit_test_detail(&self, detail: u64) {
        self.emit(|| format!("{TRACE_PREFIX}schema=v3 event=test detail={detail}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_cap_emits_one_marker_and_then_stops() {
        let sink = TraceSink::for_test();
        for detail in 0..(DETAIL_LIMIT + 4) {
            sink.emit_test_detail(detail);
        }

        let observed = sink.observed();
        assert_eq!(sink.attempted(), DETAIL_LIMIT + 4);
        assert_eq!(observed.len(), DETAIL_LIMIT as usize + 1);
        assert!(
            observed[..DETAIL_LIMIT as usize]
                .iter()
                .all(|line| line.contains("event=test"))
        );
        assert_eq!(
            observed.last().map(String::as_str),
            Some(
                "OASIS7_VIEWER_CONNECTION_TRACE schema=v3 event=truncated detail_attempts_seen=513 first_suppressed=513"
            )
        );
    }
}

fn next_sequence(counter: &AtomicU64) -> u64 {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            Some(current.saturating_add(1))
        })
        .unwrap_or(u64::MAX)
}

#[derive(Clone)]
pub(super) struct ConnectionTrace {
    sink: Arc<TraceSink>,
    sequence: u64,
    pending_job: Arc<AtomicU64>,
    close_reason: Arc<AtomicU8>,
    close_snapshot: Arc<std::sync::Mutex<Option<ConnectionCloseSnapshot>>>,
    opened_at: Instant,
}

impl ConnectionTrace {
    pub(super) fn job_sequence(&self) -> u64 {
        self.sink.job_sequence()
    }

    pub(super) fn set_pending_job(&self, job_sequence: u64) {
        self.pending_job.store(job_sequence, Ordering::Release);
    }

    pub(super) fn clear_pending_job(&self, job_sequence: u64) {
        let _ =
            self.pending_job
                .compare_exchange(job_sequence, 0, Ordering::AcqRel, Ordering::Acquire);
    }

    pub(super) fn pending_at_close(&self) -> PendingAtClose {
        let job_sequence = self.pending_job.load(Ordering::Acquire);
        PendingAtClose {
            present: job_sequence != 0,
            job_sequence,
        }
    }

    pub(super) fn set_close_reason(&self, reason: CloseReason) {
        self.close_reason.store(reason.code(), Ordering::Release);
    }

    pub(super) fn capture_before_receiver_drop(&self) {
        let snapshot = ConnectionCloseSnapshot {
            connection_sequence: self.sequence,
            pending: self.pending_at_close(),
        };
        *self
            .close_snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(snapshot);
    }

    pub(super) fn close_snapshot(&self) -> ConnectionCloseSnapshot {
        let recorded = *self
            .close_snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        recorded.unwrap_or(ConnectionCloseSnapshot {
            connection_sequence: self.sequence,
            pending: self.pending_at_close(),
        })
    }

    pub(super) fn emit_close(&self, snapshot: ConnectionCloseSnapshot) {
        let reason = CloseReason::from_code(self.close_reason.load(Ordering::Acquire));
        let elapsed_ms = self
            .opened_at
            .elapsed()
            .as_millis()
            .min(ELAPSED_MAX_MS as u128) as u64;
        self.sink.emit(|| {
            format!(
                "{TRACE_PREFIX}schema=v3 event=connection_close connection={} close_reason={} pending_at_close_present={} pending_at_close_job={} elapsed_ms={}",
                snapshot.connection_sequence,
                reason.as_str(),
                u8::from(snapshot.pending.present),
                snapshot.pending.job_sequence,
                elapsed_ms,
            )
        });
    }

    pub(super) fn emit_job(
        &self,
        job_sequence: u64,
        stage: &'static str,
        stage_order: u8,
        status: &'static str,
        since_ms: u64,
        details: String,
    ) {
        self.sink.emit(|| {
            format!(
                "{TRACE_PREFIX}schema=v3 event=job connection={} job={} stage={} stage_order={} status={} since_ms={} {}",
                self.sequence, job_sequence, stage, stage_order, status, since_ms, details
            )
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PendingAtClose {
    pub(super) present: bool,
    pub(super) job_sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ConnectionCloseSnapshot {
    pub(super) connection_sequence: u64,
    pub(super) pending: PendingAtClose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CloseReason {
    Eof,
    ReadError,
    RequestError,
    WriteError,
    RuntimeError,
}

impl CloseReason {
    fn code(self) -> u8 {
        match self {
            Self::Eof => 1,
            Self::ReadError => 2,
            Self::RequestError => 3,
            Self::WriteError => 4,
            Self::RuntimeError => 5,
        }
    }

    fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Eof,
            2 => Self::ReadError,
            3 => Self::RequestError,
            4 => Self::WriteError,
            _ => Self::RuntimeError,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Eof => "eof",
            Self::ReadError => "read_error",
            Self::RequestError => "request_error",
            Self::WriteError => "write_error",
            Self::RuntimeError => "runtime_error",
        }
    }
}
