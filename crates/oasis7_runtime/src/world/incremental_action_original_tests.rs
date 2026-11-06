use super::*;
use oasis7_formal_protocol::incremental_execution::ExecutionError;
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{
    cell::Cell,
    io::{self, Write},
};
fn work() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 1000000,
        bytes_remaining: 16 << 20,
        max_depth: 64,
        max_map_keys: 64,
        max_key_bytes: 4096,
    }
}
fn action() -> ActionEnvelope {
    ActionEnvelope {
        governance_authorization: None,
        committed_recipe_origin: None,
        id: 7,
        action: oasis7_agent_api::world_action::Action::QueryObservation {
            agent_id: "a".into(),
        },
    }
}
fn collect(
    action: &ActionEnvelope,
    memory: &MemoryBudget,
    work: &mut CanonicalCborWorkV1,
) -> (ActionOriginalReceiptV1, ReservedValue<Vec<u8>>) {
    let output = memory.reserve(16384, MemoryClass::Ordinary).unwrap();
    let mut bytes = Vec::with_capacity(4096);
    let receipt = persist_action_original_v1(
        action,
        memory,
        work,
        &mut || false,
        4096,
        &mut |stream: &mut dyn ActionOriginalStreamV1, cancelled: &mut dyn FnMut() -> bool| {
            stream.write_to(&mut bytes)?;
            bytes
                .flush()
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            assert!(!cancelled());
            Ok(())
        },
    )
    .unwrap();
    (receipt, output.retain(bytes))
}
#[test]
fn action_original_matches_actual_canonical_oracle_and_independent_golden() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(16384, MemoryClass::Ordinary).unwrap();
    let action = action();
    let mut work = work();
    let before = work.bytes_remaining;
    let (receipt, bytes) = collect(&action, &memory, &mut work);
    let golden =
        b"\xa2\x62id\x07\x66action\xa2\x64data\xa1\x68agent_id\x61a\x64type\x70QueryObservation";
    assert_eq!(bytes.as_slice(), golden);
    assert_eq!(
        bytes.as_slice(),
        oasis7_wasm_abi::encode_canonical_cbor(&action)
            .unwrap()
            .as_slice()
    );
    assert_eq!(receipt.original_id(), *blake3::hash(golden).as_bytes());
    assert_eq!(receipt.original_bytes(), golden.len() as u64);
    assert!(before - work.bytes_remaining >= 2 * receipt.original_bytes());
    let event = crate::WorldEvent {
        id: 9,
        time: u64::MAX,
        caused_by: Some(crate::CausedBy::Action(7)),
        body: crate::WorldEventBody::Domain(crate::DomainEvent::ActionAccepted {
            action_id: 7,
            action_kind: "QueryObservation".into(),
            actor_id: "中文".into(),
            eta_ticks: 0,
            notes: vec!["repeat".into(), "repeat".into()],
        }),
    };
    let mut bytes = Vec::with_capacity(4096);
    let receipt = persist_world_event_original_v1(
        &event,
        &memory,
        &mut self::work(),
        &mut || false,
        4096,
        &mut |stream: &mut dyn ActionOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
            stream.write_to(&mut bytes)
        },
    )
    .unwrap();
    assert_eq!(
        bytes,
        oasis7_wasm_abi::encode_canonical_cbor(&event).unwrap()
    );
    assert_eq!(receipt.original_id(), *blake3::hash(&bytes).as_bytes());
    let old = original::measure(&event, &memory, &mut self::work(), &mut || false, 4096).unwrap();
    assert_eq!(old.id(), receipt.original_id());
}
struct Zero;
impl Write for Zero {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Ok(0)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Overreport;
impl Write for Overreport {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(bytes.len() + 1)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn action_original_rejects_skipped_partial_suppressed_reused_failed_finish_and_cancelled_sink() {
    for mode in 0..6 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let _fixture = memory.reserve(16384, MemoryClass::Ordinary).unwrap();
        let cancelled = Cell::new(false);
        let result = persist_action_original_v1(
            &action(),
            &memory,
            &mut work(),
            &mut || cancelled.get(),
            4096,
            &mut |stream: &mut dyn ActionOriginalStreamV1, _: &mut dyn FnMut() -> bool| match mode {
                0 => Ok(()),
                1 => {
                    assert!(stream.write_to(&mut Zero).is_err());
                    Ok(())
                }
                2 => {
                    stream.write_to(&mut io::sink())?;
                    Err(BodyMutationError::Original(Box::new(io::Error::other(
                        "finish failed",
                    ))))
                }
                3 => {
                    stream.write_to(&mut io::sink())?;
                    assert!(stream.write_to(&mut io::sink()).is_err());
                    Ok(())
                }
                4 => {
                    assert!(stream.write_to(&mut Overreport).is_err());
                    Ok(())
                }
                _ => {
                    stream.write_to(&mut io::sink())?;
                    cancelled.set(true);
                    Ok(())
                }
            },
        );
        assert!(result.is_err(), "mode={mode}");
    }
}
#[test]
fn action_original_uses_shared_two_pass_budget_and_rejects_limits_before_sink() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(16384, MemoryClass::Ordinary).unwrap();
    let action = action();
    let mut measured_work = work();
    let before = measured_work.bytes_remaining;
    drop(
        original::measure_serializable(&action, &memory, &mut measured_work, &mut || false, 4096)
            .unwrap(),
    );
    let mut limited = work();
    limited.bytes_remaining = before - measured_work.bytes_remaining;
    assert!(matches!(
        persist_action_original_v1(
            &action,
            &memory,
            &mut limited,
            &mut || false,
            4096,
            &mut |stream: &mut dyn ActionOriginalStreamV1, _: &mut dyn FnMut() -> bool| stream
                .write_to(&mut io::sink())
        ),
        Err(BodyMutationError::Execution(
            ExecutionError::ResourceLimited(_)
        ))
    ));
    for cancelled in [false, true] {
        let calls = Cell::new(0);
        let result = persist_action_original_v1(
            &action,
            &memory,
            &mut work(),
            &mut || cancelled,
            0,
            &mut |_: &mut dyn ActionOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
                calls.set(calls.get() + 1);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(calls.get(), 0);
    }
    let tiny = MemoryBudget::new(4096, 0).unwrap();
    let calls = Cell::new(0);
    assert!(matches!(
        persist_action_original_v1(
            &action,
            &tiny,
            &mut work(),
            &mut || false,
            4096,
            &mut |_: &mut dyn ActionOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
                calls.set(calls.get() + 1);
                Ok(())
            }
        ),
        Err(BodyMutationError::Execution(ExecutionError::Memory(_)))
    ));
    assert_eq!(calls.get(), 0);
}
#[test]
fn action_original_large_borrowed_input_streams_in_small_operation_pool() {
    let length = 8 << 20;
    let input = MemoryBudget::new(16 << 20, 0).unwrap();
    let _source = input
        .reserve(length + (1 << 20), MemoryClass::Ordinary)
        .unwrap();
    let action = ActionEnvelope {
        governance_authorization: None,
        committed_recipe_origin: None,
        id: u64::MAX,
        action: oasis7_agent_api::world_action::Action::WorldServiceIntent {
            request: serde_json::Value::String("x".repeat(length)),
        },
    };
    let memory = MemoryBudget::new(2 << 20, 1 << 20).unwrap();
    let before = memory.used();
    let mut work = work();
    // Keys are the fixed short ActionEnvelope/Action field names. The large
    // request is a value; admit bounded key-sort scratch independently of it.
    work.max_key_bytes = 128;
    work.bytes_remaining = (length as u64) * 6 + (1 << 20);
    let mut hash = blake3::Hasher::new();
    let mut bytes = 0u64;
    struct Output<'a>(&'a mut blake3::Hasher, &'a mut u64);
    impl Write for Output<'_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.update(b);
            *self.1 += b.len() as u64;
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let receipt = persist_action_original_v1(
        &action,
        &memory,
        &mut work,
        &mut || false,
        length + 4096,
        &mut |stream: &mut dyn ActionOriginalStreamV1, cancel: &mut dyn FnMut() -> bool| {
            stream.write_to(&mut Output(&mut hash, &mut bytes))?;
            assert!(!cancel());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(receipt.original_id(), *hash.finalize().as_bytes());
    assert_eq!(receipt.original_bytes(), bytes);
    assert!(bytes >= length as u64);
    assert_eq!(memory.used(), before);
}
#[test]
fn original_private_engine_preserves_one_shot_and_between_pass_consistency() {
    #[derive(serde::Serialize)]
    struct Changing<'a> {
        #[serde(serialize_with = "change")]
        value: &'a Cell<u64>,
    }
    fn change<S: serde::Serializer>(cell: &Cell<u64>, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(cell.get())
    }
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let value = Changing {
        value: &Cell::new(1),
    };
    let mut work = work();
    let mut cancel = || false;
    let measured =
        original::measure_serializable(&value, &memory, &mut work, &mut cancel, 4096).unwrap();
    value.value.set(2);
    let mut producer = original::Producer::new(measured, &memory, &mut work, &mut cancel);
    let mut sink = |stream: &mut dyn ActionOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
        stream.write_to(&mut io::sink())
    };
    assert!(producer.persist_to(&mut sink).is_err());
    assert!(producer.check_complete().is_err());
    assert!(producer.persist_to(&mut sink).is_err());
    drop(producer);
    let source = action();
    let mut fresh_work = self::work();
    let measured =
        original::measure_serializable(&source, &memory, &mut fresh_work, &mut cancel, 4096)
            .unwrap();
    let mut producer = original::Producer::new(measured, &memory, &mut fresh_work, &mut cancel);
    producer.persist_to(&mut sink).unwrap();
    producer.check_complete().unwrap();
    assert!(producer.persist_to(&mut sink).is_err());
    assert!(producer.check_complete().is_err());
}
