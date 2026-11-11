use super::*;
use serde::Serialize;
use std::cell::Cell;
fn memory() -> MemoryBudget {
    MemoryBudget::new(1 << 20, 0).unwrap()
}
fn work() -> BoundedWireWorkV1 {
    BoundedWireWorkV1 {
        max_wire_bytes: 1 << 16,
        remaining_steps: 1_000_000,
        remaining_bytes: 1_000_000,
    }
}
#[test]
fn original_serde_wire_exact_allowances_and_retained_fee() {
    let value = ("wire雪", vec![1u64, 2, 3], Some(true));
    let expected = serde_cbor::to_vec(&value).unwrap();
    let memory = memory();
    let base = memory.used();
    let mut probe = work();
    let bytes = encode_bounded_wire_v1(&value, &memory, &mut probe, &mut || false).unwrap();
    assert_eq!(&**bytes, &expected);
    assert!(bytes.uses_budget(&memory));
    assert!(bytes.reserved_bytes() >= bytes.capacity() + std::mem::size_of::<Vec<u8>>());
    assert_eq!(memory.used(), base + bytes.reserved_bytes());
    drop(bytes);
    assert_eq!(memory.used(), base);
    let steps = 1_000_000 - probe.remaining_steps;
    let charged = 1_000_000 - probe.remaining_bytes;
    assert_eq!(charged, 2 * expected.len() as u64);
    let mut exact = BoundedWireWorkV1 {
        max_wire_bytes: expected.len(),
        remaining_steps: steps,
        remaining_bytes: charged,
    };
    let bytes = encode_bounded_wire_v1(&value, &memory, &mut exact, &mut || false).unwrap();
    assert_eq!(&**bytes, &expected);
    assert_eq!((exact.remaining_steps, exact.remaining_bytes), (0, 0));
    drop(bytes);
    for (s, b, max, kind) in [
        (steps - 1, charged, expected.len(), 0),
        (steps, charged - 1, expected.len(), 1),
        (steps, charged, expected.len() - 1, 2),
    ] {
        let mut limited = BoundedWireWorkV1 {
            max_wire_bytes: max,
            remaining_steps: s,
            remaining_bytes: b,
        };
        let error =
            encode_bounded_wire_v1(&value, &memory, &mut limited, &mut || false).unwrap_err();
        assert!(
            match kind {
                0 => matches!(error, BoundedWireErrorV1::StepLimit),
                1 => matches!(error, BoundedWireErrorV1::ByteLimit),
                _ => matches!(error, BoundedWireErrorV1::WireLimit),
            },
            "{error:?}"
        );
        assert_eq!(memory.used(), base);
    }
}
#[test]
fn zero_limits_and_real_cancellation_never_return_output() {
    let memory = memory();
    for kind in 0..3 {
        let mut w = work();
        if kind == 0 {
            w.remaining_steps = 0;
        }
        if kind == 1 {
            w.remaining_bytes = 0;
        }
        if kind == 2 {
            w.max_wire_bytes = 0;
        }
        let error = encode_bounded_wire_v1(&0u64, &memory, &mut w, &mut || false).unwrap_err();
        assert!(match kind {
            0 => matches!(error, BoundedWireErrorV1::StepLimit),
            1 => matches!(error, BoundedWireErrorV1::ByteLimit),
            _ => matches!(error, BoundedWireErrorV1::WireLimit),
        });
        assert_eq!(memory.used(), 0);
    }
    let mut w = work();
    let mut calls = 0;
    assert!(matches!(
        encode_bounded_wire_v1(&(1, 2, 3), &memory, &mut w, &mut || {
            calls += 1;
            calls >= 3
        }),
        Err(BoundedWireErrorV1::Cancelled)
    ));
    assert!(w.remaining_steps < 1_000_000);
    assert_eq!(memory.used(), 0);
}
struct Changing {
    calls: Cell<usize>,
    shorten: bool,
}
impl Serialize for Changing {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let n = self.calls.get();
        self.calls.set(n + 1);
        let data = if (n == 0) == self.shorten {
            "longer original data"
        } else {
            "x"
        };
        s.serialize_str(data)
    }
}
#[test]
fn changing_serializer_longer_and_shorter_second_pass_release_fee() {
    for shorten in [false, true] {
        let memory = memory();
        let value = Changing {
            calls: Cell::new(0),
            shorten,
        };
        let mut w = work();
        assert!(matches!(
            encode_bounded_wire_v1(&value, &memory, &mut w, &mut || false),
            Err(BoundedWireErrorV1::ChangedLength)
        ));
        assert_eq!(value.calls.get(), 2);
        assert_eq!(memory.used(), 0);
        assert!(w.remaining_steps < 1_000_000);
    }
}
#[test]
fn empty_write_is_charged_and_first_failure_is_sticky() {
    let mut w = work();
    w.remaining_steps = 1;
    let mut stop = || false;
    let mut writer = Wire {
        work: &mut w,
        cancelled: &mut stop,
        output: None,
        limit: 0,
        len: 0,
        first: None,
    };
    assert_eq!(writer.write(&[]).unwrap(), 0);
    assert_eq!(writer.work.remaining_steps, 0);
    assert!(writer.write(&[]).is_err());
    assert!(writer.write(b"x").is_err());
    assert!(matches!(
        writer.finish(),
        Err(BoundedWireErrorV1::StepLimit)
    ));
    assert_eq!(writer.work.remaining_bytes, 1_000_000);
}
#[test]
fn output_allocation_is_admitted_before_second_serialization() {
    let value = Changing {
        calls: Cell::new(0),
        shorten: false,
    };
    let memory = MemoryBudget::new(0, 0).unwrap();
    assert!(matches!(
        encode_bounded_wire_v1(&value, &memory, &mut work(), &mut || false),
        Err(BoundedWireErrorV1::Memory(_))
    ));
    assert_eq!(value.calls.get(), 1);
    assert_eq!(memory.used(), 0);
}
