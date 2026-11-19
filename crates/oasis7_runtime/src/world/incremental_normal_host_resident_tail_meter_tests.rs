//! The exclusive Common loan is tested independently of qualification. These
//! scalar readers cannot construct a parent, directory or execution result.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use oasis7_proto::memory_budget::MemoryBudget;
use std::cell::Cell;

struct ReaderLanes {
    steps: u64,
    bytes: u64,
    entries: usize,
    malformed: bool,
}
impl ReaderLanes {
    fn new() -> Self {
        Self {
            steps: 10_000,
            bytes: 200_000,
            entries: 10_000,
            malformed: false,
        }
    }
    fn read_attempt(&mut self) {
        self.steps -= 3;
        self.bytes -= 7;
        self.entries -= 2;
    }
}
impl Lanes for ReaderLanes {
    fn visit_lanes(&mut self, visit: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
        if self.malformed {
            return;
        }
        visit(FactoryPhaseMeterLaneV1::Steps(&mut self.steps));
        visit(FactoryPhaseMeterLaneV1::Bytes(&mut self.bytes));
        visit(FactoryPhaseMeterLaneV1::Entries(&mut self.entries));
    }
}

#[test]
fn common_pre_admission_refuses_read_on_low_budget_or_cancellation() {
    for (steps, bytes, cancelled) in [
        (0, 100_000, false),
        (1_000, 0, false),
        (1_000, 100_000, true),
    ] {
        let memory = MemoryBudget::new(1 << 20, 64 << 10).unwrap();
        let work = FormalBoundedWorkV1::new(steps, bytes).unwrap();
        let mut stop = || cancelled;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut reader = ReaderLanes::new();
        let called = Cell::new(false);
        let result = run_lanes(&mut reader, &mut common, |_, _| {
            called.set(true);
            Ok(())
        });
        assert!(result.is_err());
        assert!(!called.get());
        assert_eq!(
            (reader.steps, reader.bytes, reader.entries),
            (10_000, 200_000, 10_000)
        );
        assert!(!work.is_child_active());
    }
}

#[test]
fn successful_and_failed_reads_keep_actual_debits_without_refunding_common() {
    for fail in [false, true] {
        let memory = MemoryBudget::new(1 << 20, 64 << 10).unwrap();
        let work = FormalBoundedWorkV1::new(1_000, 100_000).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut reader = ReaderLanes::new();
        let result = run_lanes(&mut reader, &mut common, |reader, _| {
            assert!(work.is_child_active());
            assert!(reader.steps <= 999 / 2);
            assert!(reader.entries <= 999 / 2);
            reader.read_attempt();
            if fail {
                Err(BodyMutationError::Invalid("reader original failure"))
            } else {
                Ok(())
            }
        });
        assert_eq!(result.is_err(), fail);
        if fail {
            assert!(matches!(
                result,
                Err(BodyMutationError::Invalid("reader original failure"))
            ));
        }
        assert_eq!(
            (reader.steps, reader.bytes, reader.entries),
            (9_997, 199_993, 9_998)
        );
        assert_eq!(work.remaining_steps(), 994);
        assert_eq!(
            work.remaining_read_bytes(),
            100_000 - 3 * std::mem::size_of::<Snapshot>() as u64 - 7
        );
        assert!(!work.is_child_active());
        // Reusing the same accounts consumes the next attempt, not a refill.
        run_lanes(&mut reader, &mut common, |reader, _| {
            reader.read_attempt();
            Ok(())
        })
        .unwrap();
        assert_eq!(
            (reader.steps, reader.bytes, reader.entries),
            (9_994, 199_986, 9_996)
        );
        assert_eq!(work.remaining_steps(), 988);
    }
}

#[test]
fn cancellation_after_a_read_keeps_the_attempt_debit() {
    let memory = MemoryBudget::new(1 << 20, 64 << 10).unwrap();
    let work = FormalBoundedWorkV1::new(1_000, 100_000).unwrap();
    let cancelled = Cell::new(false);
    let mut stop = || cancelled.get();
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let mut reader = ReaderLanes::new();
    let result = run_lanes(&mut reader, &mut common, |reader, stop| {
        reader.read_attempt();
        cancelled.set(true);
        assert!(stop());
        Err::<(), _>(ValueTreeError::Cancelled.into())
    });
    assert!(matches!(
        result,
        Err(BodyMutationError::Value(ValueTreeError::Cancelled))
    ));
    assert_eq!(
        (reader.steps, reader.bytes, reader.entries),
        (9_997, 199_993, 9_998)
    );
    assert_eq!(work.remaining_steps(), 994);
    assert_eq!(
        work.remaining_read_bytes(),
        100_000 - 3 * std::mem::size_of::<Snapshot>() as u64 - 7
    );
    assert!(!work.is_child_active());
}

#[test]
fn unaccounted_reader_keeps_its_original_error_and_burns_common() {
    let memory = MemoryBudget::new(1 << 20, 64 << 10).unwrap();
    let work = FormalBoundedWorkV1::new(1_000, 100_000).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let mut reader = ReaderLanes::new();
    let result = run_lanes(&mut reader, &mut common, |reader, _| {
        reader.read_attempt();
        reader.malformed = true;
        Err::<(), _>(BodyMutationError::Invalid("first reader error"))
    });
    assert!(matches!(
        result,
        Err(BodyMutationError::Invalid("first reader error"))
    ));
    assert_eq!(
        (work.remaining_steps(), work.remaining_read_bytes()),
        (0, 0)
    );
    assert!(!work.is_child_active());
}

#[test]
fn panic_keeps_its_original_payload_even_when_the_final_snapshot_fails() {
    for malformed in [false, true] {
        let memory = MemoryBudget::new(1 << 20, 64 << 10).unwrap();
        let work = FormalBoundedWorkV1::new(1_000, 100_000).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut reader = ReaderLanes::new();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<(), BodyMutationError> =
                run_lanes(&mut reader, &mut common, |reader, _| {
                    reader.read_attempt();
                    reader.malformed = malformed;
                    std::panic::panic_any("first producer panic");
                });
        }));
        let panic = outcome.expect_err("the original panic must escape");
        assert_eq!(panic.downcast_ref::<&str>(), Some(&"first producer panic"));
        assert_eq!(
            (work.remaining_steps(), work.remaining_read_bytes()),
            (0, 0)
        );
        assert!(!work.is_child_active());
    }
}
