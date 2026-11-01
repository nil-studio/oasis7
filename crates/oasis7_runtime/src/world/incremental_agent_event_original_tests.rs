use super::*;
use crate::world::incremental_body_event::original;
use std::io::Write;

#[test]
fn borrowed_original_large_reason_fits_small_operation_pool() {
    for length in [32 << 20, 64 << 20] {
        // Caller-owned immutable input is admitted separately; the operation
        // cannot reserve an encoded copy of this input in its 8MiB pool.
        let input = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
        let _input = input
            .reserve(length + (2 << 20), MemoryClass::Ordinary)
            .unwrap();
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let (world, _, _) = fixture(&memory);
        let mut event = event(&world, 1);
        let WorldEventBody::Domain(DomainEvent::BodyAttributesUpdated { reason, .. }) =
            &mut event.body
        else {
            unreachable!()
        };
        *reason = "r".repeat(length);
        let baseline = memory.used();
        let mut work = original_work();
        work.bytes_remaining = (length as u64) * 3 + (1 << 20);
        let starting = work.bytes_remaining;
        let mut cancel = || false;
        let measured =
            original::measure(&event, &memory, &mut work, &mut cancel, length + 4096).unwrap();
        let expected = measured.id();
        let mut producer = original::Producer::new(measured, &memory, &mut work, &mut cancel);
        let mut actual = blake3::Hasher::new();
        let mut bytes = 0u64;
        struct Output<'a>(&'a mut blake3::Hasher, &'a mut u64);
        impl Write for Output<'_> {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.update(b);
                *self.1 += b.len() as u64;
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        producer
            .persist_to(&mut |source: &mut dyn AgentEventOriginalStreamV1,
                              cancelled: &mut dyn FnMut() -> bool| {
                let mut out = Output(&mut actual, &mut bytes);
                source.write_to(&mut out)?;
                out.flush().unwrap();
                assert!(!cancelled());
                Ok(())
            })
            .unwrap();
        producer.check_complete().unwrap();
        drop(producer);
        assert_eq!(*actual.finalize().as_bytes(), expected);
        assert!(bytes >= length as u64 && bytes <= (length + 4096) as u64);
        assert!(
            starting - work.bytes_remaining >= 2 * bytes,
            "both passes must consume one work"
        );
        assert_eq!(memory.used(), baseline, "all operation scratch is released");
    }
}

struct Partial;
impl Write for Partial {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Ok(0)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn callback_skip_partial_finish_failure_or_suppressed_error_cannot_append_fact() {
    for mode in 0..7 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (world, mut parent, mut state) = fixture(&memory);
        let mut values = Values::default();
        seed(
            &world,
            &mut parent,
            &mut state,
            &mut values,
            &memory,
            EventSequenceV1 { next: 1, era: 0 },
            MailboxFrontierV1 { head: 0, tail: 0 },
        );
        let source = values.clone();
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &state, &memory, limits()).unwrap();
        txn.write(Ns::Business, b"successful-outer-prefix", Some([9; 32]))
            .unwrap();
        let result = world.contribute_iws_agent_event(
            &event(&world, 1),
            &mut txn,
            &mut AgentEventContext {
                values: &source,
                mutation: BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
                persist_original: |producer: &mut dyn AgentEventOriginalV1| {
                    if mode == 0 {
                        return Ok(());
                    } // skip producer entirely
                    let outcome = producer.persist_to(
                        &mut |stream: &mut dyn AgentEventOriginalStreamV1,
                              _: &mut dyn FnMut() -> bool| {
                            if mode == 1 {
                                return Ok(());
                            } // sink skipped the stream
                            if mode == 2 {
                                return stream.write_to(&mut Partial);
                            }
                            stream.write_to(&mut std::io::sink())?;
                            if mode == 3 {
                                return Err(BodyMutationError::Original(Box::new(
                                    std::io::Error::from(std::io::ErrorKind::WriteZero),
                                )));
                            }
                            if mode == 4 {
                                let _ = stream.write_to(&mut std::io::sink());
                            }
                            if mode == 5 {
                                return Err(ValueTreeError::Cancelled.into());
                            }
                            Ok(())
                        },
                    );
                    if mode == 6 {
                        outcome?;
                        // A second ignored persistence failure invalidates the
                        // previously private completion state as well.
                        let _ = collect_original(producer, &memory);
                    }
                    // Suppressing a sink failure must not manufacture completion.
                    Ok(())
                },
                max_event_bytes: 1 << 20,
                original_work: original_work(),
            },
        );
        assert!(result.is_err(), "mode={mode}");
        let guard = txn.guard();
        assert!(
            matches!(txn.seal(&guard, &live, 0), Err(ExecutionError::Aborted)),
            "mode={mode}"
        );
    }
}

#[test]
fn second_stream_pass_cannot_reset_the_measurement_work_allowance() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (world, mut parent, mut state) = fixture(&memory);
    let event = event(&world, 1);
    let mut calibration = original_work();
    let before = calibration.bytes_remaining;
    let measured =
        original::measure(&event, &memory, &mut calibration, &mut || false, 4096).unwrap();
    drop(measured);
    let one_pass = before - calibration.bytes_remaining;
    let mut work = original_work();
    work.bytes_remaining = one_pass;
    let mut values = Values::default();
    seed(
        &world,
        &mut parent,
        &mut state,
        &mut values,
        &memory,
        EventSequenceV1 { next: 1, era: 0 },
        MailboxFrontierV1 { head: 0, tail: 0 },
    );
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &state, &memory, limits()).unwrap();
    txn.write(Ns::Business, b"successful-prefix", Some([9; 32]))
        .unwrap();
    let result = world.contribute_iws_agent_event(
        &event,
        &mut txn,
        &mut AgentEventContext {
            values: &source,
            mutation: BodyMutationContext {
                sink: &mut values,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
            persist_original: |producer: &mut dyn AgentEventOriginalV1| {
                collect_original(producer, &memory).map(|_| ())
            },
            max_event_bytes: 4096,
            original_work: work,
        },
    );
    assert!(matches!(
        result,
        Err(BodyMutationError::Execution(
            ExecutionError::ResourceLimited(_)
        ))
    ));
    let guard = txn.guard();
    assert!(matches!(
        txn.seal(&guard, &live, 0),
        Err(ExecutionError::Aborted)
    ));
}
