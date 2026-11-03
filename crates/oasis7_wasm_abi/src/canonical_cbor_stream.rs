//! Bounded streaming companion to the existing Value-based canonical encoder.
//! Input must be a stable borrowed Serde snapshot: map nodes are replayed by
//! original ordinal, without retaining references passed to serialize_field.
//! Arbitrary side-effecting Serialize implementations are not a snapshot API.
//!
//! Maps retain only bounded, pre-admitted canonical KEY bytes and key models.
//! The exact existing CanonicalValue comparator (including Tag/Tag behavior)
//! and stable sorting are retained; this is not a new RFC wire profile.
//! Selecting values safely replays their parent by ordinal: O(keys²) callbacks,
//! charged to finite visits. Arrays and Display strings use count/replay passes.
//! Large Text/bytes VALUES are streamed from their original borrowed slices;
//! no owned Value or encoded output Vec is created for those values.
use serde::Serialize;
use std::{
    fmt,
    io::{self, Write},
};
#[path = "canonical_cbor_stream_encode.rs"]
mod encode;
#[path = "canonical_cbor_stream_key_model.rs"]
mod key_model;
#[path = "canonical_cbor_stream_keys.rs"]
mod keys;
#[path = "canonical_cbor_stream_replay.rs"]
mod replay;

#[derive(Debug)]
pub enum CanonicalCborStreamError {
    ResourceLimited(&'static str),
    Invalid(&'static str),
    Cancelled,
    Poisoned,
    Io(io::Error),
}
impl fmt::Display for CanonicalCborStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ResourceLimited(s) => write!(f, "canonical CBOR allowance: {s}"),
            Self::Invalid(s) => write!(f, "canonical CBOR: {s}"),
            Self::Cancelled => f.write_str("canonical CBOR cancelled"),
            Self::Poisoned => f.write_str("canonical CBOR context poisoned"),
            Self::Io(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for CanonicalCborStreamError {}
impl serde::ser::Error for CanonicalCborStreamError {
    // Do not allocate an unadmitted error String from arbitrary Serialize code.
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self::Invalid("Serialize rejected snapshot")
    }
}
type Error = CanonicalCborStreamError;
/// Implementations must reserve before returning and release only when Charge
/// drops. The ABI deliberately has no dependency on a native/proto allocator.
pub trait CanonicalCborAllocator {
    type Charge;
    fn reserve(&self, bytes: usize) -> Result<Self::Charge, CanonicalCborStreamError>;
}
/// Work includes key discovery/replay and every output attempt, including
/// Interrupted retries. bytes_remaining covers output plus discovered/selected
/// key encodings. Comparator re-encoding is bounded separately by max_key_bytes,
/// its pre-admitted scratch, and the prepaid quadratic comparison allowance.
pub struct CanonicalCborWorkV1 {
    pub visits_remaining: u64,
    pub bytes_remaining: u64,
    pub max_depth: usize,
    pub max_map_keys: usize,
    pub max_key_bytes: usize,
}
pub struct CanonicalCborStreamContextV1<'a, A: CanonicalCborAllocator, C: FnMut() -> bool> {
    pub allocator: &'a A,
    pub work: CanonicalCborWorkV1,
    pub cancelled: C,
    poisoned: bool,
    metrics: CanonicalCborStreamMetricsV1,
}
impl<'a, A: CanonicalCborAllocator, C: FnMut() -> bool> CanonicalCborStreamContextV1<'a, A, C> {
    pub fn new(allocator: &'a A, work: CanonicalCborWorkV1, cancelled: C) -> Self {
        Self {
            allocator,
            work,
            cancelled,
            poisoned: false,
            metrics: CanonicalCborStreamMetricsV1::default(),
        }
    }
    pub fn metrics(&self) -> CanonicalCborStreamMetricsV1 {
        self.metrics
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CanonicalCborStreamMetricsV1 {
    pub map_sorts: u64,
    pub map_comparisons: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalCborStreamSummaryV1 {
    pub encoded_bytes: u64,
}
struct Counter<'a> {
    out: &'a mut dyn Write,
    bytes: u64,
}
impl Write for Counter<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let n = self.out.write(b)?;
        if n > b.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "writer overreported count",
            ));
        }
        self.bytes = self
            .bytes
            .checked_add(n as u64)
            .ok_or(io::ErrorKind::InvalidData)?;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Output remains provisional until success; a buffered destination still needs
/// its own finish. Hash writers can consume this stream without a CBOR Vec.
/// The source Serialize/Display implementation and output sink remain responsible
/// for their own allocations; derived borrowed snapshots need no extra value copy.
/// Any failure permanently poisons this context; allowances are never reset.
pub fn stream_canonical_cbor_v1<
    T: Serialize + ?Sized,
    A: CanonicalCborAllocator,
    C: FnMut() -> bool,
>(
    value: &T,
    context: &mut CanonicalCborStreamContextV1<'_, A, C>,
    output: &mut dyn Write,
) -> Result<CanonicalCborStreamSummaryV1, Error> {
    if context.poisoned {
        return Err(Error::Poisoned);
    }
    context.poisoned = true;
    let mut out = Counter {
        out: output,
        bytes: 0,
    };
    let mut engine = Engine {
        allocator: context.allocator,
        work: &mut context.work,
        cancelled: &mut context.cancelled,
        depth: 0,
        metrics: &mut context.metrics,
    };
    engine.encode(value, &mut out)?;
    engine.step()?;
    context.poisoned = false;
    Ok(CanonicalCborStreamSummaryV1 {
        encoded_bytes: out.bytes,
    })
}
/// Compare a known original CBOR slice without allocating encoded output.
pub fn compare_canonical_cbor_v1<
    T: Serialize + ?Sized,
    A: CanonicalCborAllocator,
    C: FnMut() -> bool,
>(
    value: &T,
    expected: &[u8],
    context: &mut CanonicalCborStreamContextV1<'_, A, C>,
) -> Result<bool, Error> {
    struct Compare<'a> {
        expected: &'a [u8],
        offset: usize,
        equal: bool,
    }
    impl Write for Compare<'_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self
                .expected
                .get(self.offset..self.offset.saturating_add(b.len()))
                != Some(b)
            {
                self.equal = false;
            }
            self.offset = self
                .offset
                .checked_add(b.len())
                .ok_or(io::ErrorKind::InvalidData)?;
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut out = Compare {
        expected,
        offset: 0,
        equal: true,
    };
    stream_canonical_cbor_v1(value, context, &mut out)?;
    Ok(out.equal && out.offset == expected.len())
}
struct Engine<'a, A: CanonicalCborAllocator, C: FnMut() -> bool> {
    allocator: &'a A,
    work: &'a mut CanonicalCborWorkV1,
    cancelled: &'a mut C,
    depth: usize,
    metrics: &'a mut CanonicalCborStreamMetricsV1,
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> Engine<'_, A, C> {
    fn step(&mut self) -> Result<(), Error> {
        if (self.cancelled)() {
            return Err(Error::Cancelled);
        }
        self.work.visits_remaining = self
            .work
            .visits_remaining
            .checked_sub(1)
            .ok_or(Error::ResourceLimited("visits"))?;
        Ok(())
    }
    fn emit(&mut self, out: &mut dyn Write, bytes: &[u8]) -> Result<(), Error> {
        self.step()?;
        self.work.bytes_remaining = self
            .work
            .bytes_remaining
            .checked_sub(bytes.len() as u64)
            .ok_or(Error::ResourceLimited("encoded bytes"))?;
        let mut remaining = bytes;
        while !remaining.is_empty() {
            self.step()?;
            let chunk = &remaining[..remaining.len().min(16 * 1024)];
            match out.write(chunk) {
                Ok(0) => return Err(Error::Io(io::ErrorKind::WriteZero.into())),
                Ok(n) if n <= chunk.len() => remaining = &remaining[n..],
                Ok(_) => return Err(Error::Invalid("writer count")),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(Error::Io(e)),
            }
        }
        self.step()
    }
    fn head(&mut self, out: &mut dyn Write, major: u8, n: u64) -> Result<(), Error> {
        let mut b = [0u8; 9];
        let len = if n < 24 {
            b[0] = (major << 5) | n as u8;
            1
        } else if n <= u8::MAX as u64 {
            b[0] = (major << 5) | 24;
            b[1] = n as u8;
            2
        } else if n <= u16::MAX as u64 {
            b[0] = (major << 5) | 25;
            b[1..3].copy_from_slice(&(n as u16).to_be_bytes());
            3
        } else if n <= u32::MAX as u64 {
            b[0] = (major << 5) | 26;
            b[1..5].copy_from_slice(&(n as u32).to_be_bytes());
            5
        } else {
            b[0] = (major << 5) | 27;
            b[1..9].copy_from_slice(&n.to_be_bytes());
            9
        };
        self.emit(out, &b[..len])
    }
    fn text(&mut self, out: &mut dyn Write, value: &str) -> Result<(), Error> {
        self.head(out, 3, value.len() as u64)?;
        self.emit(out, value.as_bytes())
    }
    fn encode<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
        out: &mut dyn Write,
    ) -> Result<(), Error> {
        self.step()?;
        if self.depth >= self.work.max_depth {
            return Err(Error::ResourceLimited("depth"));
        }
        let _frame = self.allocator.reserve(512)?;
        self.depth += 1;
        let result = value.serialize(encode::Node {
            engine: self,
            out,
            root: value,
        });
        self.depth -= 1;
        result
    }
    fn display<T: fmt::Display + ?Sized>(
        &mut self,
        value: &T,
        out: &mut dyn Write,
    ) -> Result<(), Error> {
        struct Count<'a, 'b, A: CanonicalCborAllocator, C: FnMut() -> bool> {
            engine: &'b mut Engine<'a, A, C>,
            bytes: u64,
            error: Option<Error>,
        }
        impl<A: CanonicalCborAllocator, C: FnMut() -> bool> fmt::Write for Count<'_, '_, A, C> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                let result = self.engine.step().and_then(|()| {
                    self.bytes = self
                        .bytes
                        .checked_add(s.len() as u64)
                        .ok_or(Error::ResourceLimited("display bytes"))?;
                    Ok(())
                });
                if let Err(e) = result {
                    self.error = Some(e);
                    return Err(fmt::Error);
                }
                Ok(())
            }
        }
        let mut count = Count {
            engine: self,
            bytes: 0,
            error: None,
        };
        let first = fmt::write(&mut count, format_args!("{value}"));
        if let Some(e) = count.error {
            return Err(e);
        }
        first.map_err(|_| Error::Invalid("Display rejected snapshot"))?;
        let expected = count.bytes;
        self.head(out, 3, expected)?;
        struct Emit<'a, 'b, A: CanonicalCborAllocator, C: FnMut() -> bool> {
            engine: &'b mut Engine<'a, A, C>,
            out: &'b mut dyn Write,
            bytes: u64,
            error: Option<Error>,
        }
        impl<A: CanonicalCborAllocator, C: FnMut() -> bool> fmt::Write for Emit<'_, '_, A, C> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                let result = self.engine.emit(self.out, s.as_bytes());
                if let Err(e) = result {
                    self.error = Some(e);
                    return Err(fmt::Error);
                }
                self.bytes = self.bytes.checked_add(s.len() as u64).ok_or(fmt::Error)?;
                Ok(())
            }
        }
        let mut emit = Emit {
            engine: self,
            out,
            bytes: 0,
            error: None,
        };
        let result = fmt::write(&mut emit, format_args!("{value}"));
        if let Some(e) = emit.error {
            return Err(e);
        }
        result.map_err(|_| Error::Invalid("Display rejected snapshot"))?;
        if emit.bytes != expected {
            return Err(Error::Invalid("Display changed during replay"));
        }
        self.step()
    }
    fn primitive<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
        out: &mut dyn Write,
    ) -> Result<(), Error> {
        struct Forward<'a, 'b, A: CanonicalCborAllocator, C: FnMut() -> bool> {
            engine: &'b mut Engine<'a, A, C>,
            out: &'b mut dyn Write,
            error: Option<Error>,
        }
        impl<A: CanonicalCborAllocator, C: FnMut() -> bool> Write for Forward<'_, '_, A, C> {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                match self.engine.emit(self.out, b) {
                    Ok(()) => Ok(b.len()),
                    Err(e) => {
                        self.error = Some(e);
                        Err(io::ErrorKind::Other.into())
                    }
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut writer = Forward {
            engine: self,
            out,
            error: None,
        };
        let result = ciborium::ser::into_writer(value, &mut writer);
        if let Some(e) = writer.error {
            return Err(e);
        }
        result.map_err(|_| Error::Invalid("primitive encoding"))
    }
}
