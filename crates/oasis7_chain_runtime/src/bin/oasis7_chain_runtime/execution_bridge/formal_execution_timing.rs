//! Opt-in phase timings contain only execution positions and elapsed durations.
use std::time::Instant;

pub(super) struct FormalExecutionTiming {
    clocks: Option<(Instant, Instant)>,
    position: u64,
    operation: &'static str,
}
impl FormalExecutionTiming {
    pub(super) fn begin(operation: &'static str, position: u64) -> Self {
        let clocks = (std::env::var_os("OASIS7_FORMAL_EXECUTION_TIMING").as_deref()
            == Some(std::ffi::OsStr::new("1")))
        .then(|| {
            let now = Instant::now();
            eprintln!(
                "formal_execution_timing operation={operation} position={position} event=begin"
            );
            (now, now)
        });
        Self {
            clocks,
            position,
            operation,
        }
    }
    /// Observe an existing operation without reading or serializing its payload.
    pub(super) fn step<T, E: std::fmt::Display>(
        &self,
        phase: &'static str,
        digest: Option<&str>,
        operation: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let started = self.clocks.as_ref().map(|_| {
            eprintln!(
                "formal_execution_timing operation={} position={} phase={phase} event=begin digest={}",
                self.operation, self.position, digest.unwrap_or("none"),
            );
            Instant::now()
        });
        let result = operation();
        if let Some(started) = started {
            match &result {
                Ok(_) => eprintln!(
                    "formal_execution_timing operation={} position={} phase={phase} event=end digest={} stage_ms={}",
                    self.operation,
                    self.position,
                    digest.unwrap_or("none"),
                    started.elapsed().as_millis(),
                ),
                Err(error) => eprintln!(
                    "formal_execution_timing operation={} position={} phase={phase} event=error digest={} stage_ms={} reason={error}",
                    self.operation,
                    self.position,
                    digest.unwrap_or("none"),
                    started.elapsed().as_millis(),
                ),
            }
        }
        result
    }
    pub(super) fn material(
        &self,
        phase: &'static str,
        digest: &str,
        inventory: impl FnOnce() -> (usize, usize),
    ) {
        if self.clocks.is_some() {
            let (objects, object_bytes) = inventory();
            eprintln!(
                "formal_execution_timing operation={} position={} phase={phase} digest={digest} objects={objects} object_bytes={object_bytes}",
                self.operation, self.position,
            );
        }
    }
    pub(super) fn mark(&mut self, phase: &'static str) {
        if let Some((start, last)) = &mut self.clocks {
            let now = Instant::now();
            eprintln!(
                "formal_execution_timing operation={} position={} phase={phase} stage_ms={} total_ms={}",
                self.operation,
                self.position,
                now.duration_since(*last).as_millis(),
                now.duration_since(*start).as_millis(),
            );
            *last = now;
        }
    }
}
impl Drop for FormalExecutionTiming {
    fn drop(&mut self) {
        self.mark("end");
    }
}
