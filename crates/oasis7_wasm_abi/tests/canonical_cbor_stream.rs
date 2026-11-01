use oasis7_wasm_abi::*;
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq, SerializeStruct},
};
use std::{
    cell::Cell,
    io::{self, Write},
    rc::Rc,
};
#[derive(Clone)]
struct Alloc {
    state: Rc<State>,
}
struct State {
    limit: usize,
    used: Cell<usize>,
    peak: Cell<usize>,
    calls: Cell<usize>,
    deny: Cell<Option<usize>>,
}
struct Charge {
    state: Rc<State>,
    bytes: usize,
}
impl Drop for Charge {
    fn drop(&mut self) {
        self.state.used.set(self.state.used.get() - self.bytes);
    }
}
impl Alloc {
    fn new(limit: usize) -> Self {
        Self {
            state: Rc::new(State {
                limit,
                used: Cell::new(0),
                peak: Cell::new(0),
                calls: Cell::new(0),
                deny: Cell::new(None),
            }),
        }
    }
}
impl CanonicalCborAllocator for Alloc {
    type Charge = Charge;
    fn reserve(&self, bytes: usize) -> Result<Charge, CanonicalCborStreamError> {
        let n = self.state.calls.get() + 1;
        self.state.calls.set(n);
        let next = self.state.used.get().checked_add(bytes).ok_or(
            CanonicalCborStreamError::ResourceLimited("test charge overflow"),
        )?;
        if next > self.state.limit || self.state.deny.get() == Some(n) {
            return Err(CanonicalCborStreamError::ResourceLimited("test allocator"));
        }
        self.state.used.set(next);
        self.state.peak.set(self.state.peak.get().max(next));
        Ok(Charge {
            state: self.state.clone(),
            bytes,
        })
    }
}
fn work() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 1_000_000,
        bytes_remaining: 16 << 20,
        max_depth: 64,
        max_map_keys: 64,
        max_key_bytes: 1024,
    }
}
fn parity<T: Serialize + ?Sized>(source: &T) {
    // Existing encoder is the independent, unchanged byte oracle.
    let oracle = encode_canonical_cbor(&source).unwrap();
    let alloc = Alloc::new(4 << 20);
    let mut context = CanonicalCborStreamContextV1::new(&alloc, work(), || false);
    let mut got = Vec::with_capacity(oracle.len());
    let result = stream_canonical_cbor_v1(source, &mut context, &mut got).unwrap();
    assert_eq!(got, oracle);
    assert_eq!(result.encoded_bytes, oracle.len() as u64);
    assert_eq!(alloc.state.used.get(), 0);
    let mut context = CanonicalCborStreamContextV1::new(&alloc, work(), || false);
    assert!(compare_canonical_cbor_v1(source, &oracle, &mut context).unwrap());
}
#[derive(Serialize)]
struct Unit;
#[derive(Serialize)]
struct New<'a>(&'a str);
#[derive(Serialize)]
struct Tuple(u64, bool);
#[derive(Serialize)]
enum E<'a> {
    Unit,
    Newtype(&'a str),
    Tuple(i64, bool),
    Struct { zz: &'a str, a: Option<u64> },
}
#[derive(Serialize)]
#[serde(tag = "kind")]
enum Internal<'a> {
    A { text: &'a str, id: u64 },
    B,
}
#[derive(Serialize)]
#[serde(tag = "kind", content = "data")]
enum Adjacent {
    A(u64, bool),
    B { enabled: bool },
}
#[derive(Serialize)]
#[serde(untagged)]
enum Untagged<'a> {
    A(&'a str),
    B(u64),
}
struct Bytes<'a>(&'a [u8]);
impl Serialize for Bytes<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(self.0)
    }
}
struct UnknownSeq;
impl Serialize for UnknownSeq {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(None)?;
        seq.serialize_element(&3)?;
        seq.serialize_element(&"next")?;
        seq.end()
    }
}
struct TemporaryFields;
impl Serialize for TemporaryFields {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut value = s.serialize_struct("TemporaryFields", 2)?;
        value.serialize_field("temporary-long", &format!("{}:{}", 1, 2))?;
        value.serialize_field("a", &vec![4u8, 5])?;
        value.end()
    }
}
struct DisplayValue;
impl std::fmt::Display for DisplayValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("first")?;
        write!(f, "/{}/λ", 42)
    }
}
impl Serialize for DisplayValue {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
#[test]
fn original_value_encoder_matches_primitives_all_serde_shapes_and_nested_maps() {
    macro_rules! same {($($x:expr),* $(,)?)=>{$(parity(&$x);)*};}
    same!(
        (),
        Unit,
        New("borrowed"),
        Tuple(u64::MAX, true),
        None::<bool>,
        Some(false),
        Some(0u64),
        Some(""),
        true,
        false,
        'λ',
        '😀',
        "",
        Bytes(&[0, 255, 1]),
        i128::MIN,
        i128::MAX,
        u128::MAX,
        -(1i128 << 64),
        u64::MAX,
        0i8,
        -1i8
    );
    for n in [
        0f64,
        -0.,
        1.,
        1.5,
        f64::MIN,
        f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        f64::from_bits(0xfff8_1234_5678_9012),
        1.0000000000002,
    ] {
        parity(&n);
    }
    for n in [f32::MIN, f32::MAX, f32::NAN, -0f32] {
        parity(&n);
    }
    same!(
        E::Unit,
        E::Newtype(""),
        E::Tuple(i64::MIN, false),
        E::Struct { zz: "v", a: None },
        Internal::A { text: "t", id: 9 },
        Internal::B,
        Adjacent::A(3, true),
        Adjacent::B { enabled: false },
        Untagged::A("x"),
        Untagged::B(7),
        UnknownSeq,
        TemporaryFields,
        DisplayValue
    );
    let nested = std::collections::BTreeMap::from([
        (
            "zzz",
            std::collections::BTreeMap::from([("long", 2), ("a", 1)]),
        ),
        ("a", std::collections::BTreeMap::new()),
    ]);
    parity(&nested);
    let complex = ciborium::Value::Map(vec![
        (
            ciborium::Value::Array(vec![ciborium::Value::from("x"), ciborium::Value::from(3)]),
            ciborium::Value::Tag(1, Box::new(ciborium::Value::from("v"))),
        ),
        (ciborium::Value::Bool(false), ciborium::Value::Null),
    ]);
    parity(&complex);
}
#[test]
fn old_tag_comparator_and_duplicate_order_remain_exact_wire() {
    let a = (1u128 << 72) - 1;
    let b = 1u128 << 72;
    let map = std::collections::BTreeMap::from([(a, "nine"), (b, "ten")]);
    let oracle = encode_canonical_cbor(&map).unwrap();
    let decoded: ciborium::Value = ciborium::de::from_reader(oracle.as_slice()).unwrap();
    let entries = decoded.as_map().unwrap();
    assert_eq!(entries[0].1, ciborium::Value::from("ten"));
    // This old Tag/Tag ordering differs from encoded-key length ordering.
    assert!(encode_canonical_cbor(&a).unwrap().len() < encode_canonical_cbor(&b).unwrap().len());
    parity(&map);
    let mut keys = vec![];
    for n in 0..16 {
        keys.push((
            ciborium::Value::from(a - n),
            ciborium::Value::from(n as u64),
        ));
        keys.push((
            ciborium::Value::from(b + n),
            ciborium::Value::from(n as u64),
        ));
    }
    keys.push((
        ciborium::Value::from("abcdefghijk"),
        ciborium::Value::from(9),
    ));
    keys.push((
        ciborium::Value::Tag(2, Box::new(ciborium::Value::Float(f64::NAN))),
        ciborium::Value::Null,
    ));
    keys.push((ciborium::Value::from("duplicate"), ciborium::Value::from(1)));
    keys.push((ciborium::Value::from("duplicate"), ciborium::Value::from(2)));
    parity(&ciborium::Value::Map(keys));
}

#[test]
fn bounded_sort_work_refuses_before_sort_and_cancel_after_comparisons_poison() {
    let source = std::collections::BTreeMap::from_iter((0..32).map(|n| (format!("key-{n:03}"), n)));
    let alloc = Alloc::new(4 << 20);
    let mut limited = work();
    limited.visits_remaining = 3000;
    let mut ctx = CanonicalCborStreamContextV1::new(&alloc, limited, || false);
    let mut out = Vec::new();
    let result = stream_canonical_cbor_v1(&source, &mut ctx, &mut out);
    assert!(matches!(
        result,
        Err(CanonicalCborStreamError::ResourceLimited("map sort visits"))
    ));
    assert!(ctx.is_poisoned());
    assert_eq!(ctx.metrics().map_comparisons, 0);
    assert!(out.is_empty());
    assert_eq!(alloc.state.used.get(), 0);
    struct CancelSort<'a> {
        alloc: &'a Alloc,
        flag: &'a Cell<bool>,
    }
    impl CanonicalCborAllocator for CancelSort<'_> {
        type Charge = Charge;
        fn reserve(&self, bytes: usize) -> Result<Charge, CanonicalCborStreamError> {
            let charge = self.alloc.reserve(bytes)?;
            if bytes == 1024 * 512 + 4096 {
                self.flag.set(true);
            }
            Ok(charge)
        }
    }
    let flag = Cell::new(false);
    let admission = CancelSort {
        alloc: &alloc,
        flag: &flag,
    };
    let mut ctx = CanonicalCborStreamContextV1::new(&admission, work(), || flag.get());
    let mut out = Vec::new();
    assert!(matches!(
        stream_canonical_cbor_v1(&source, &mut ctx, &mut out),
        Err(CanonicalCborStreamError::Cancelled)
    ));
    assert!(ctx.metrics().map_comparisons > 0);
    assert_eq!(ctx.metrics().map_sorts, 1);
    assert!(ctx.is_poisoned());
    assert!(out.is_empty());
    let remaining = ctx.work.visits_remaining;
    assert!(matches!(
        stream_canonical_cbor_v1(&source, &mut ctx, &mut out),
        Err(CanonicalCborStreamError::Poisoned)
    ));
    assert_eq!(ctx.work.visits_remaining, remaining);
    assert_eq!(alloc.state.used.get(), 0);
}
struct Output<'a> {
    mode: u8,
    attempts: usize,
    accepted: Vec<u8>,
    length: usize,
    cancel: &'a Cell<bool>,
}
impl Write for Output<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.attempts += 1;
        match self.mode {
            0 if self.attempts <= 2 => return Err(io::ErrorKind::Interrupted.into()),
            1 => return Ok(0),
            2 => return Ok(b.len() + 1),
            3 if !self.accepted.is_empty() => {
                return Err(io::Error::other("after accepted CBOR prefix"));
            }
            5 => {
                self.cancel.set(true);
                return Err(io::ErrorKind::Interrupted.into());
            }
            6 => return Err(io::ErrorKind::Interrupted.into()),
            _ => {}
        }
        let n = b.len().min(3);
        self.accepted.extend_from_slice(&b[..n]);
        if self.mode == 4 && self.accepted.len() == self.length {
            self.cancel.set(true);
        }
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn partial_interrupted_outputs_final_cancellation_and_failure_context_poison() {
    let source = E::Struct {
        zz: "λ-string",
        a: Some(u64::MAX),
    };
    let oracle = encode_canonical_cbor(&source).unwrap();
    for mode in 0..7 {
        let alloc = Alloc::new(4 << 20);
        let cancel = Cell::new(false);
        let mut quota = work();
        if mode == 6 {
            quota.visits_remaining = 200;
        }
        let mut ctx = CanonicalCborStreamContextV1::new(&alloc, quota, || cancel.get());
        let mut output = Output {
            mode,
            attempts: 0,
            accepted: Vec::with_capacity(oracle.len()),
            length: oracle.len(),
            cancel: &cancel,
        };
        let result = stream_canonical_cbor_v1(&source, &mut ctx, &mut output);
        match mode {
            0 => {
                assert_eq!(result.unwrap().encoded_bytes, oracle.len() as u64);
                assert_eq!(output.accepted, oracle);
                assert!(!ctx.is_poisoned());
            }
            1 => assert!(
                matches!(result,Err(CanonicalCborStreamError::Io(ref e)) if e.kind()==io::ErrorKind::WriteZero)
            ),
            2 | 3 => assert!(matches!(result, Err(CanonicalCborStreamError::Io(_)))),
            4 | 5 => assert!(matches!(result, Err(CanonicalCborStreamError::Cancelled))),
            6 => assert!(matches!(
                result,
                Err(CanonicalCborStreamError::ResourceLimited("visits"))
            )),
            _ => unreachable!(),
        }
        if mode == 3 {
            assert!(!output.accepted.is_empty());
        }
        if mode == 4 {
            assert_eq!(output.accepted, oracle);
        }
        if mode == 5 {
            assert_eq!(output.attempts, 1);
        }
        if mode == 6 {
            assert!(output.attempts <= 200);
        }
        assert_eq!(alloc.state.used.get(), 0);
        if mode != 0 {
            assert!(ctx.is_poisoned());
            let attempts = output.attempts;
            assert!(matches!(
                stream_canonical_cbor_v1(&source, &mut ctx, &mut output),
                Err(CanonicalCborStreamError::Poisoned)
            ));
            assert_eq!(output.attempts, attempts);
        }
    }
}

#[test]
fn explicit_small_bigint_tags_remain_tag_models_for_original_key_order() {
    let key = ciborium::Value::Tag(2, Box::new(ciborium::Value::Bytes(vec![1])));
    let map = ciborium::Value::Map(vec![
        (key, ciborium::Value::Null),
        (ciborium::Value::from(2), ciborium::Value::Null),
    ]);
    parity(&map);
    let outside_i128 = ciborium::Value::Tag(3, Box::new(ciborium::Value::Bytes(vec![255; 16])));
    parity(&ciborium::Value::Map(vec![(
        outside_i128,
        ciborium::Value::Null,
    )]));
}

struct UnknownMap;
impl Serialize for UnknownMap {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("long-key", &2)?;
        map.serialize_entry("a", &1)?;
        map.end()
    }
}
#[test]
fn all_allocator_admissions_and_finite_limits_fail_closed_without_leaks() {
    parity(&UnknownMap);
    let source = std::collections::BTreeMap::from([("long-key", vec![1u64, 2]), ("a", vec![3, 4])]);
    let alloc = Alloc::new(4 << 20);
    let mut ctx = CanonicalCborStreamContextV1::new(&alloc, work(), || false);
    stream_canonical_cbor_v1(&source, &mut ctx, &mut io::sink()).unwrap();
    let admissions = alloc.state.calls.get();
    assert!(admissions > 10);
    for denied in 1..=admissions {
        let alloc = Alloc::new(4 << 20);
        alloc.state.deny.set(Some(denied));
        let mut ctx = CanonicalCborStreamContextV1::new(&alloc, work(), || false);
        assert!(
            matches!(
                stream_canonical_cbor_v1(&source, &mut ctx, &mut io::sink()),
                Err(CanonicalCborStreamError::ResourceLimited("test allocator"))
            ),
            "admission{denied}"
        );
        assert!(ctx.is_poisoned());
        assert_eq!(alloc.state.used.get(), 0);
        assert!(matches!(
            stream_canonical_cbor_v1(&source, &mut ctx, &mut io::sink()),
            Err(CanonicalCborStreamError::Poisoned)
        ));
    }
    for mode in 0..6 {
        let alloc = Alloc::new(if mode == 5 { 0 } else { 4 << 20 });
        let mut limits = work();
        match mode {
            0 => limits.visits_remaining = 0,
            1 => limits.bytes_remaining = 0,
            2 => limits.max_depth = 1,
            3 => limits.max_map_keys = 0,
            4 => limits.max_key_bytes = 2,
            _ => {}
        }
        let mut ctx = CanonicalCborStreamContextV1::new(&alloc, limits, || false);
        assert!(
            matches!(
                stream_canonical_cbor_v1(&source, &mut ctx, &mut io::sink()),
                Err(CanonicalCborStreamError::ResourceLimited(_))
            ),
            "limit mode{mode}"
        );
        assert!(ctx.is_poisoned());
        assert_eq!(alloc.state.used.get(), 0);
    }
    let alloc = Alloc::new(4 << 20);
    let mut ctx = CanonicalCborStreamContextV1::new(&alloc, work(), || false);
    let before = ctx.work.visits_remaining;
    let original = encode_canonical_cbor(&source).unwrap();
    assert!(compare_canonical_cbor_v1(&source, &original, &mut ctx).unwrap());
    let after = ctx.work.visits_remaining;
    assert!(after < before);
    let mut wrong = original;
    wrong.push(0);
    assert!(!compare_canonical_cbor_v1(&source, &wrong, &mut ctx).unwrap());
    assert!(ctx.work.visits_remaining < after);
    assert!(!ctx.is_poisoned());
    ctx.work.visits_remaining = 0;
    assert!(matches!(
        stream_canonical_cbor_v1(&source, &mut ctx, &mut io::sink()),
        Err(CanonicalCborStreamError::ResourceLimited("visits"))
    ));
    assert!(ctx.is_poisoned());
}
fn header(major: u8, n: usize) -> ([u8; 9], usize) {
    let mut b = [0; 9];
    if n < 24 {
        b[0] = (major << 5) | n as u8;
        (b, 1)
    } else if n <= 255 {
        b[0] = (major << 5) | 24;
        b[1] = n as u8;
        (b, 2)
    } else if n <= 65535 {
        b[0] = (major << 5) | 25;
        b[1..3].copy_from_slice(&(n as u16).to_be_bytes());
        (b, 3)
    } else {
        b[0] = (major << 5) | 26;
        b[1..5].copy_from_slice(&(n as u32).to_be_bytes());
        (b, 5)
    }
}
struct Expected<'a> {
    segments: [&'a [u8]; 6],
    segment: usize,
    offset: usize,
    bytes: u64,
    max_write: usize,
}
impl Write for Expected<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.max_write = self.max_write.max(bytes.len());
        let mut remaining = bytes;
        while !remaining.is_empty() {
            while self.segment < self.segments.len()
                && self.offset == self.segments[self.segment].len()
            {
                self.segment += 1;
                self.offset = 0;
            }
            let expected = self
                .segments
                .get(self.segment)
                .ok_or(io::ErrorKind::InvalidData)?;
            let n = remaining.len().min(expected.len() - self.offset);
            if remaining[..n] != expected[self.offset..self.offset + n] {
                return Err(io::Error::other(
                    "large canonical stream differs from literal byte oracle",
                ));
            }
            remaining = &remaining[n..];
            self.offset += n;
        }
        self.bytes += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Serialize)]
struct Large<'a> {
    payload: &'a str,
    binary: Bytes<'a>,
}
#[test]
fn text_and_bytes_32_and_64mib_stream_with_fixed_half_mib_operation_pool() {
    let mut baseline = None;
    for length in [64, 1 << 20, 32 << 20, 64 << 20] {
        let original = "x".repeat(length);
        let value = Large {
            payload: &original,
            binary: Bytes(original.as_bytes()),
        };
        let (binary, n) = header(2, length);
        let (text, m) = header(3, length);
        let mut output = Expected {
            segments: [
                b"\xa2\x66binary",
                &binary[..n],
                original.as_bytes(),
                b"\x67payload",
                &text[..m],
                original.as_bytes(),
            ],
            segment: 0,
            offset: 0,
            bytes: 0,
            max_write: 0,
        };
        let alloc = Alloc::new(512 << 10);
        let mut quota = work();
        quota.bytes_remaining = (length as u64) * 4 + 65536;
        quota.max_map_keys = 8;
        quota.max_key_bytes = 256;
        quota.max_depth = 16;
        let mut ctx = CanonicalCborStreamContextV1::new(&alloc, quota, || false);
        let summary = stream_canonical_cbor_v1(&value, &mut ctx, &mut output).unwrap();
        assert_eq!(summary.encoded_bytes, (length * 2 + 16 + n + m) as u64);
        assert_eq!(output.bytes, summary.encoded_bytes);
        assert!(output.max_write <= 16 << 10);
        assert_eq!(alloc.state.used.get(), 0);
        let peak = alloc.state.peak.get();
        assert!(peak < 512 << 10);
        if let Some(before) = baseline {
            assert_eq!(peak, before);
        } else {
            baseline = Some(peak);
        }
        eprintln!(
            "canonical borrowed Text+bytes each={length} output={} helper_peak_reserved={peak} max_write={} comparisons={}",
            summary.encoded_bytes,
            output.max_write,
            ctx.metrics().map_comparisons
        );
    }
}
