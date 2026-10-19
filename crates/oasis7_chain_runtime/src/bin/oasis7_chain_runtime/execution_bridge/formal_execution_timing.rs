//! Opt-in phase timings contain only execution positions and elapsed durations.
use std::time::Instant;

#[cfg(test)]
pub(crate) fn clock_ns() -> u128 {
    static ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed().as_nanos()
}
#[cfg(test)]
fn clock_field() -> String {
    format!(" mono_ns={}", clock_ns())
}
#[cfg(not(test))]
fn clock_field() -> &'static str {
    ""
}

pub(super) struct FormalExecutionTiming {
    clocks: Option<(Instant, Instant)>,
    position: u64,
    operation: &'static str,
}
impl FormalExecutionTiming {
    pub(super) fn test_only_begin(operation: &'static str, position: u64) -> Self {
        #[cfg(test)]
        {
            Self::begin(operation, position)
        }
        #[cfg(not(test))]
        {
            Self {
                clocks: None,
                position,
                operation,
            }
        }
    }
    pub(super) fn begin(operation: &'static str, position: u64) -> Self {
        let clocks = (std::env::var_os("OASIS7_FORMAL_EXECUTION_TIMING").as_deref()
            == Some(std::ffi::OsStr::new("1")))
        .then(|| {
            let now = Instant::now();
            eprintln!(
                "formal_execution_timing operation={operation} position={position} event=begin{}",
                clock_field()
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
                "formal_execution_timing operation={} position={} phase={phase} event=begin digest={}{}",
                self.operation, self.position, digest.unwrap_or("none"), clock_field(),
            );
            Instant::now()
        });
        let result = operation();
        if let Some(started) = started {
            match &result {
                Ok(_) => eprintln!(
                    "formal_execution_timing operation={} position={} phase={phase} event=end digest={} stage_ms={}{}",
                    self.operation,
                    self.position,
                    digest.unwrap_or("none"),
                    started.elapsed().as_millis(),
                    clock_field(),
                ),
                Err(error) => eprintln!(
                    "formal_execution_timing operation={} position={} phase={phase} event=error digest={} stage_ms={} reason={error}{}",
                    self.operation,
                    self.position,
                    digest.unwrap_or("none"),
                    started.elapsed().as_millis(),
                    clock_field(),
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
                "formal_execution_timing operation={} position={} phase={phase} digest={digest} objects={objects} object_bytes={object_bytes}{}",
                self.operation,
                self.position,
                clock_field(),
            );
        }
    }
    pub(super) fn mark(&mut self, phase: &'static str) {
        if let Some((start, last)) = &mut self.clocks {
            let now = Instant::now();
            eprintln!(
                "formal_execution_timing operation={} position={} phase={phase} stage_ms={} total_ms={}{}",
                self.operation,
                self.position,
                now.duration_since(*last).as_millis(),
                now.duration_since(*start).as_millis(),
                clock_field(),
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_clock_field_is_bounded_by_the_same_origin() {
        let before = clock_ns();
        let field = std::thread::spawn(clock_field).join().unwrap();
        let value: u128 = field
            .trim()
            .strip_prefix("mono_ns=")
            .unwrap()
            .parse()
            .unwrap();
        assert!(before <= value && value <= clock_ns());
    }
    #[test]
    fn disabled_observation_preserves_one_call_and_exact_error() {
        let timing = FormalExecutionTiming {
            clocks: None,
            position: 0,
            operation: "unit",
        };
        let calls = std::cell::Cell::new(0);
        let ok: Result<u8, &str> = timing.step("verify_native", None, || {
            calls.set(calls.get() + 1);
            Ok(7)
        });
        let error: Result<u8, &str> = timing.step("dual_freeze", None, || {
            calls.set(calls.get() + 1);
            Err("exact original error")
        });
        assert_eq!(ok, Ok(7));
        assert_eq!(error, Err("exact original error"));
        assert_eq!(calls.get(), 2);
    }
}
