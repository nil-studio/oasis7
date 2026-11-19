//! Owned admission for selected Cognition data. Layout work never qualifies an
//! input, continuation, lease, signature or committed evidence source.
use crate::{
    cognition_scheduler::{CognitionScheduler, SchedulerPaidErrorV1, SchedulerWakeV1},
    world::incremental_body::BodyMutationError,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use serde_json::Value;
use std::{
    mem::{align_of, size_of},
    ptr::NonNull,
};
const _: () = assert!(!std::mem::needs_drop::<serde_json::Number>());
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("selected Cognition owned graph").into()
}
fn add(a: usize, b: usize) -> Result<usize, BodyMutationError> {
    a.checked_add(b).ok_or_else(limited)
}
fn mul(a: usize, b: usize) -> Result<usize, BodyMutationError> {
    a.checked_mul(b).ok_or_else(limited)
}
fn tree<K, V>(n: usize) -> Result<usize, BodyMutationError> {
    let alignment = align_of::<K>()
        .max(align_of::<V>())
        .max(align_of::<Option<NonNull<()>>>())
        .max(align_of::<u16>());
    let slots = mul(add(size_of::<K>(), size_of::<V>())?, 11)?;
    // One full internal node per entry bounds leaf/internal-node allocation for
    // both cloned and newly collected BTree maps/sets; padding covers all arrays.
    let node = add(
        add(
            slots,
            size_of::<Option<NonNull<()>>>() + 2 * size_of::<u16>() + 12 * size_of::<NonNull<()>>(),
        )?,
        mul(alignment - 1, 7)?,
    )?;
    mul(node, n)
}
fn charge(
    common: &mut FormalBoundedContextV1<'_>,
    steps: u64,
    bytes: usize,
) -> Result<(), BodyMutationError> {
    common
        .charge(steps, u64::try_from(bytes).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
#[derive(Default)]
struct Shape {
    nodes: usize,
    seq: usize,
    map: usize,
    text: usize,
}
fn shape(
    value: &Value,
    depth: usize,
    max: usize,
    s: &mut Shape,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    charge(common, 1, 0)?;
    if depth > max {
        return Err(limited());
    }
    s.nodes = add(s.nodes, 1)?;
    match value {
        Value::String(text) => {
            charge(common, 1, text.len())?;
            s.text = add(s.text, text.len())?;
        }
        Value::Array(items) => {
            s.seq = add(s.seq, items.len())?;
            for item in items {
                shape(item, add(depth, 1)?, max, s, common)?;
            }
        }
        Value::Object(items) => {
            s.map = add(s.map, items.len())?;
            for (key, item) in items {
                charge(common, 1, key.len())?;
                s.text = add(s.text, key.len())?;
                shape(item, add(depth, 1)?, max, s, common)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn clone_selected_json_v1(
    value: &Value,
    max_depth: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Value>, BodyMutationError> {
    let mut s = Shape::default();
    shape(value, 0, max_depth, &mut s, common)?;
    let heap = add(
        add(
            add(
                mul(s.nodes, size_of::<Value>())?,
                mul(s.seq, size_of::<Value>())?,
            )?,
            tree::<String, Value>(s.map)?,
        )?,
        s.text,
    )?;
    charge(
        common,
        u64::try_from(s.nodes).map_err(|_| limited())?,
        s.text,
    )?;
    let fee = common
        .memory()
        .reserve(heap, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let copied = value.clone();
    charge(common, 0, 0)?;
    Ok(fee.retain(copied))
}
pub(super) struct OwnedCognitionSchedulerV1 {
    pub(super) value: CognitionScheduler,
    _heap: MemoryReservation,
}
pub(super) enum SchedulerOwnedErrorV1 {
    Body(BodyMutationError),
    Scheduler(crate::cognition_scheduler::SchedulerError),
}
/// Exact original parser/validator, with source-shaped target admission before
/// Deserialize. No JSON/World clone occurs in the parser.
pub(super) fn decode_scheduler_v1(
    value: &Value,
    max_depth: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<OwnedCognitionSchedulerV1, SchedulerOwnedErrorV1> {
    let admission = (|| {
        let mut s = Shape::default();
        shape(value, 0, max_depth, &mut s, common)?;
        let active = value
            .get("active")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let pressure = value
            .get("backpressure")
            .and_then(Value::as_object)
            .map_or(0, |v| v.len());
        let flight = value
            .get("in_flight")
            .and_then(Value::as_object)
            .map_or(0, |v| v.len());
        let priorities = value
            .get("backpressure_priority")
            .and_then(Value::as_object)
            .map_or(0, |v| v.len());
        let recovered = value
            .get("recovered_wakes")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let wakes = add(add(active, pressure)?, flight)?;
        let mut heap = add(size_of::<CognitionScheduler>(), s.text)?;
        heap = add(heap, mul(mul(active, 2)?, size_of::<SchedulerWakeV1>())?)?;
        heap = add(
            heap,
            tree::<String, SchedulerWakeV1>(add(pressure, flight)?)?,
        )?;
        heap = add(heap, tree::<String, i64>(priorities)?)?;
        heap = add(heap, tree::<String, ()>(recovered)?)?;
        heap = add(heap, tree::<&str, ()>(wakes)?)?;
        // Borrowed Value deserialization visits each input node at most through its
        // field visitor and ignored-any visitor; no Content buffering/flatten exists
        // in this closed Scheduler/Policy/Cursor/Wake type inventory.
        charge(
            common,
            u64::try_from(mul(s.nodes, 3)?).map_err(|_| limited())?,
            mul(s.text, 2)?,
        )?;
        Ok::<_, BodyMutationError>(
            common
                .memory()
                .reserve(heap, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?,
        )
    })()
    .map_err(SchedulerOwnedErrorV1::Body)?;
    let parsed =
        CognitionScheduler::from_snapshot_ref_paid_v1(value, &mut |bytes| charge(common, 1, bytes))
            .map_err(|error| match error {
                SchedulerPaidErrorV1::Scheduler(e) => SchedulerOwnedErrorV1::Scheduler(e),
                SchedulerPaidErrorV1::Work(e) => SchedulerOwnedErrorV1::Body(e),
            })?;
    charge(common, 0, 0).map_err(SchedulerOwnedErrorV1::Body)?;
    Ok(OwnedCognitionSchedulerV1 {
        value: parsed,
        _heap: admission,
    })
}

/// One expiry transition on the service-owned working set. The caller retains
/// this working set until the Current CAS and journal append both succeed;
/// errors never authorize publishing a partially transitioned continuation.
/// The returned moved wake is the original scheduler entry for the lifecycle
/// event, including an in-flight lease restored after a crash.
pub(super) fn expire_selected_continuation_v1(
    continuation: &mut crate::cognition_wake::AgentContinuation,
    scheduler: &mut OwnedCognitionSchedulerV1,
    tick: u64,
    hashes: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<Option<SchedulerWakeV1>>, SchedulerOwnedErrorV1> {
    use crate::cognition_wake::{ContinuationStatusV1, ContinuationTransition};
    charge(common, 1, 0).map_err(SchedulerOwnedErrorV1::Body)?;
    if matches!(
        continuation.status,
        ContinuationStatusV1::Completed
            | ContinuationStatusV1::Cancelled
            | ContinuationStatusV1::Invalidated
            | ContinuationStatusV1::Expired
            | ContinuationStatusV1::Rejected
    ) || continuation
        .valid_until_tick
        .is_none_or(|until| tick <= until)
    {
        return Ok(None);
    }
    // Transition validation is the same authoritative lifecycle algorithm used
    // by the legacy wrapper; this producer supplies neither status nor evidence.
    ContinuationTransition::apply_at_tick_state_v1(
        continuation,
        ContinuationStatusV1::Expired,
        tick,
    )
    .map_err(|e| SchedulerOwnedErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
    let digest = if continuation.schema_version
        == oasis7_world_state::cognition_wake::IWS_CONTINUATION_SCHEMA_V2
    {
        charge(common, 32768, 256 << 10).map_err(SchedulerOwnedErrorV1::Body)?;
        let _phase = common
            .memory()
            .reserve(256 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)
            .map_err(BodyMutationError::from)
            .map_err(SchedulerOwnedErrorV1::Body)?;
        oasis7_world_state::cognition_wake::validate_iws_continuation_fields_v2(continuation)
            .map_err(|e| SchedulerOwnedErrorV1::Body(BodyMutationError::Original(Box::new(e))))?;
        super::incremental_normal_cognition_expiry::values::digest(
            oasis7_world_state::cognition_wake::IWS_CONTINUATION_STATUS_DOMAIN_V2,
            &oasis7_world_state::cognition_wake::iws_continuation_status_digest_input_v2(
                continuation,
            ),
            hashes,
            common,
        )
        .map_err(SchedulerOwnedErrorV1::Body)?
    } else {
        super::incremental_normal_cognition_expiry::values::digest(
            "oasis7.cognition.continuation-status.v1",
            &continuation.status_digest_input_v1(),
            hashes,
            common,
        )
        .map_err(SchedulerOwnedErrorV1::Body)?
    };
    charge(common, 1, digest.len()).map_err(SchedulerOwnedErrorV1::Body)?;
    // The service-owned working graph's actual selected-shape admission also
    // covers these fixed-size lifecycle replacements until its final CAS.
    continuation.continuation_status_digest = Some(digest.to_string());
    let wake = scheduler
        .value
        .deactivate_wake_paid_v1(&continuation.wake_id, &mut |bytes| charge(common, 1, bytes))
        .map_err(|e| match e {
            SchedulerPaidErrorV1::Work(e) => SchedulerOwnedErrorV1::Body(e),
            SchedulerPaidErrorV1::Scheduler(e) => SchedulerOwnedErrorV1::Scheduler(e),
        })?;
    Ok(Some(wake))
}

/// Stream the exact original journal-head tuple with the same ABI canonical
/// primitive and key comparator. Both codec work and hash work debit Common;
/// no encoded journal Vec or owned CBOR value tree is constructed.
pub(super) fn journal_head_hash_paid_v1(
    head_seq: u64,
    events: &Value,
    common: &mut FormalBoundedContextV1<'_>,
    work: oasis7_wasm_abi::CanonicalCborWorkV1,
) -> Result<[u8; 32], BodyMutationError> {
    use oasis7_wasm_abi::{
        CanonicalCborAllocator, CanonicalCborStreamContextV1, CanonicalCborStreamError as Error,
        stream_canonical_cbor_v1,
    };
    use std::{
        cell::RefCell,
        io::{self, Write},
    };
    struct Meter<'a, 'b> {
        common: RefCell<&'a mut FormalBoundedContextV1<'b>>,
        memory: oasis7_proto::memory_budget::MemoryBudget,
        first: RefCell<Option<BodyMutationError>>,
    }
    impl CanonicalCborAllocator for Meter<'_, '_> {
        type Charge = MemoryReservation;
        fn reserve(&self, bytes: usize) -> Result<Self::Charge, Error> {
            if self.first.borrow().is_some() {
                return Err(Error::Poisoned);
            }
            self.memory
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(|error| {
                    *self.first.borrow_mut() = Some(ValueTreeError::Memory(error).into());
                    Error::ResourceLimited("journal codec memory")
                })
        }
        fn charge_work(&self, steps: u64, bytes: u64) -> Result<(), Error> {
            if self.first.borrow().is_some() {
                return Err(Error::Poisoned);
            }
            self.common
                .borrow_mut()
                .charge(steps, bytes)
                .map_err(|error| {
                    *self.first.borrow_mut() = Some(BodyMutationError::Original(Box::new(error)));
                    Error::ResourceLimited("journal Common work")
                })
        }
    }
    struct Hash<'a, 'b, 'c> {
        meter: &'a Meter<'b, 'c>,
        hash: blake3::Hasher,
    }
    impl Write for Hash<'_, '_, '_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.meter
                .charge_work(1, bytes.len() as u64)
                .map_err(|_| io::Error::from(io::ErrorKind::Other))?;
            self.hash.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[derive(serde::Serialize)]
    struct Head<'a> {
        head_seq: u64,
        events: &'a Value,
    }
    let memory = common.memory().clone();
    let meter = Meter {
        common: RefCell::new(common),
        memory,
        first: RefCell::new(None),
    };
    let mut context = CanonicalCborStreamContextV1::new(&meter, work, || false);
    let mut hash = Hash {
        meter: &meter,
        hash: blake3::Hasher::new(),
    };
    let encoded = stream_canonical_cbor_v1(
        &(
            "oasis7.cognition.journal-head.v1",
            Head { head_seq, events },
        ),
        &mut context,
        &mut hash,
    );
    // IO/Serde carry only a codec sentinel. Preserve the actual caller failure,
    // including typed cancellation, and never debit again after that failure.
    if let Some(error) = meter.first.borrow_mut().take() {
        return Err(error);
    }
    encoded.map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    let final_charge = meter.charge_work(1, 32);
    if let Some(error) = meter.first.borrow_mut().take() {
        return Err(error);
    }
    final_charge.map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    Ok(*hash.hash.finalize().as_bytes())
}

#[cfg(test)]
mod journal_meter_tests {
    use super::*;
    use oasis7_formal_protocol::controlled_authority::formal::{
        FormalError, bounded_context::FormalBoundedWorkV1,
    };
    #[test]
    fn journal_codec_preserves_common_first_error_without_retrying_meter() {
        let memory = oasis7_proto::memory_budget::MemoryBudget::new(1 << 20, 0).unwrap();
        for (steps, cancel, expected) in [
            (100, true, "bounded archive read cancelled"),
            (0, false, "bounded archive read steps"),
        ] {
            let work = FormalBoundedWorkV1::new(steps, 1 << 20).unwrap();
            let mut calls = 0;
            let mut stop = || {
                calls += 1;
                cancel
            };
            let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
            let error = journal_head_hash_paid_v1(
                1,
                &serde_json::json!([]),
                &mut common,
                oasis7_wasm_abi::CanonicalCborWorkV1 {
                    visits_remaining: 1000,
                    bytes_remaining: 1 << 20,
                    max_depth: 16,
                    max_map_keys: 16,
                    max_key_bytes: 1024,
                },
            )
            .unwrap_err();
            let BodyMutationError::Original(error) = error else {
                panic!("lost original meter error")
            };
            assert!(
                matches!(error.downcast_ref::<FormalError>(), Some(FormalError::Invalid(message)) if message == expected)
            );
            drop(common);
            drop(stop);
            assert_eq!(calls, 1);
            assert_eq!(work.remaining_steps(), steps);
            assert_eq!(memory.used(), 0);
        }
    }
}
