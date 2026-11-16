//! Metered generic CBOR DATA. No execution or source qualification is created.
use super::{
    FormalError,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryLimitExceeded, MemoryReservation};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    borrow::Borrow,
    fmt,
    io::{self, Write},
};
#[path = "bounded_cbor_data_shape.rs"]
mod shape;
pub use shape::{CborDataShapeV1, probe_cbor_data_shape_v1};
#[derive(Clone, Copy, Debug)]
pub struct CborDataLayoutV1 {
    pub root: usize,
    pub sequence_element: usize,
    pub map_pair: usize,
    pub set_element: usize,
    pub box_pointee: usize,
    pub default_heap_per_value: usize,
    pub symbols_bytes: usize,
    pub visits: usize,
    pub missing: Option<&'static str>,
}
#[derive(Debug)]
pub enum BoundedCborDataErrorV1 {
    Common(FormalError),
    Memory(MemoryLimitExceeded),
    Invalid(&'static str),
    Limit(&'static str),
}
impl std::fmt::Display for BoundedCborDataErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Common(e) => e.fmt(f),
            Self::Memory(e) => e.fmt(f),
            Self::Invalid(label) => write!(f, "invalid normal payload: {label}"),
            Self::Limit(label) => write!(f, "normal payload resource limit: {label}"),
        }
    }
}
impl std::error::Error for BoundedCborDataErrorV1 {}
impl From<FormalError> for BoundedCborDataErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Common(e)
    }
}
impl From<MemoryLimitExceeded> for BoundedCborDataErrorV1 {
    fn from(e: MemoryLimitExceeded) -> Self {
        Self::Memory(e)
    }
}
fn limited() -> BoundedCborDataErrorV1 {
    BoundedCborDataErrorV1::Limit("normal payload layout/declared hint")
}
/// Explicit caps, not a refillable budget. All actual work uses the same Common.
pub struct CborDataLimitsV1 {
    pub max_wire_bytes: usize,
    pub max_graph_bytes: usize,
    pub max_depth: usize,
}
pub struct PaidCborDataV1<T> {
    value: T,
    core: WorkIdentityV1,
    _fee: MemoryReservation,
}
impl<T> Borrow<T> for PaidCborDataV1<T> {
    fn borrow(&self) -> &T {
        &self.value
    }
}
impl AsRef<[u8]> for PaidCborDataV1<Vec<u8>> {
    fn as_ref(&self) -> &[u8] {
        &self.value
    }
}
/// Implementors may only move fields into a closed wrapper, without cloning or allocating children.
pub trait CborDataMoveWrapV1<T> {
    type Output;
    fn wrap_data(value: T) -> Self::Output;
}
impl<T> PaidCborDataV1<T> {
    pub fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BoundedCborDataErrorV1> {
        common.consume_steps(1)?;
        if self.core != common.work().identity() || !self._fee.uses_budget(common.memory()) {
            return Err(BoundedCborDataErrorV1::Invalid("CBOR data context"));
        }
        Ok(())
    }
    pub fn with_data<R>(
        self,
        common: &mut FormalBoundedContextV1<'_>,
        consume: impl FnOnce(T, &mut FormalBoundedContextV1<'_>) -> R,
    ) -> Result<R, BoundedCborDataErrorV1> {
        self.validate_context(common)?;
        let Self {
            value,
            core: _,
            _fee,
        } = self;
        let result = consume(value, common);
        drop(_fee);
        Ok(result)
    }
    pub fn map_move<W: CborDataMoveWrapV1<T>>(
        self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<PaidCborDataV1<W::Output>, BoundedCborDataErrorV1> {
        self.validate_context(common)?;
        let extra = std::mem::size_of::<W::Output>().saturating_sub(std::mem::size_of::<T>());
        common.charge(1, extra as u64)?;
        let mut fee = common.memory().reserve(extra, MemoryClass::Ordinary)?;
        let Self {
            value,
            core,
            mut _fee,
        } = self;
        _fee.merge(&mut fee)
            .map_err(BoundedCborDataErrorV1::Invalid)?;
        Ok(PaidCborDataV1 {
            value: W::wrap_data(value),
            core,
            _fee,
        })
    }
}
pub struct PaidCborRefusalV1 {
    data: PaidCborDataV1<String>,
}
impl PaidCborRefusalV1 {
    pub fn message(&self) -> &str {
        self.data.value.as_str()
    }
    pub fn validate_context(
        &self,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BoundedCborDataErrorV1> {
        self.data.validate_context(c)
    }
}
fn graph(
    shape: CborDataShapeV1,
    l: CborDataLayoutV1,
    root: usize,
) -> Result<usize, BoundedCborDataErrorV1> {
    let content = std::mem::size_of::<serde::__private229::de::Content<'static>>();
    let seq = l
        .sequence_element
        .max(content)
        .checked_mul(4)
        .and_then(|n| l.set_element.checked_mul(16).and_then(|m| n.checked_add(m)))
        .and_then(|n| n.checked_add(256))
        .ok_or_else(limited)?;
    let pair = l
        .map_pair
        .max(2 * content)
        .checked_mul(16)
        .and_then(|n| n.checked_add(256 + 8 * content))
        .ok_or_else(limited)?;
    let token = l
        .box_pointee
        .checked_add(content)
        .and_then(|n| n.checked_add(std::mem::size_of::<serde_json::Value>() + 128))
        .ok_or_else(limited)?;
    let bytes = shape
        .sequences
        .checked_mul(seq as u64)
        .and_then(|n| {
            shape
                .maps
                .checked_mul(pair as u64)
                .and_then(|m| n.checked_add(m))
        })
        .and_then(|n| {
            shape
                .values
                .checked_mul(token as u64)
                .and_then(|v| n.checked_add(v))
        })
        .and_then(|n| shape.strings.checked_mul(16).and_then(|v| n.checked_add(v)))
        .and_then(|n| {
            shape
                .values
                .checked_add(1)
                .and_then(|v| v.checked_mul(l.default_heap_per_value as u64))
                .and_then(|v| n.checked_add(v))
        })
        .and_then(|n| usize::try_from(n).ok())
        .and_then(|n| n.checked_add(root.max(l.root)))
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(limited)?;
    Ok(bytes)
}
struct Engine<'a, 'ctx> {
    common: &'a mut FormalBoundedContextV1<'ctx>,
    limits: &'a CborDataLimitsV1,
    raw_len: usize,
}
pub fn decode_closed_cbor_data_v1<T: DeserializeOwned>(
    raw: &[u8],
    layout: CborDataLayoutV1,
    limits: &CborDataLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Result<PaidCborDataV1<T>, PaidCborRefusalV1>, BoundedCborDataErrorV1> {
    common.consume_steps(1)?;
    if layout.missing.is_some() {
        return Err(BoundedCborDataErrorV1::Invalid(
            "closed CBOR layout missing",
        ));
    }
    if raw.len() > limits.max_wire_bytes {
        return Err(limited());
    }
    Engine {
        common,
        limits,
        raw_len: raw.len(),
    }
    .decode(raw, layout)
}
pub fn encode_closed_cbor_data_v1<T: Serialize>(
    value: &T,
    limits: &CborDataLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Result<PaidCborDataV1<Vec<u8>>, PaidCborRefusalV1>, BoundedCborDataErrorV1> {
    Engine {
        common,
        limits,
        raw_len: 0,
    }
    .encode(value)
}
impl Engine<'_, '_> {
    fn decode<T: DeserializeOwned>(
        &mut self,
        raw: &[u8],
        layout: CborDataLayoutV1,
    ) -> Result<Result<PaidCborDataV1<T>, PaidCborRefusalV1>, BoundedCborDataErrorV1> {
        let shape = probe_cbor_data_shape_v1(raw, self.limits.max_depth, self.common)?;
        let output = graph(shape, layout, std::mem::size_of::<T>())?;
        let scratch = output
            .checked_mul(shape.depth.checked_add(3).ok_or_else(limited)?)
            .ok_or_else(limited)?;
        if output > self.limits.max_graph_bytes || scratch > self.limits.max_graph_bytes {
            return Err(limited());
        }
        let stages = u64::try_from(shape.depth.checked_add(3).ok_or_else(limited)?)
            .map_err(|_| limited())?;
        let steps = shape
            .values
            .checked_mul(stages)
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(limited)?;
        let comparisons = shape
            .strings
            .checked_mul(u64::from(64 - shape.maps.saturating_add(1).leading_zeros()) + 8)
            .and_then(|n| n.checked_mul(stages))
            .ok_or_else(limited)?;
        self.common.charge(
            steps,
            u64::try_from(output)
                .map_err(|_| limited())?
                .checked_add(comparisons)
                .ok_or_else(limited)?,
        )?;
        let fee = self
            .common
            .memory()
            .reserve(output, MemoryClass::Ordinary)?;
        let phase = self
            .common
            .memory()
            .reserve(scratch, MemoryClass::Ordinary)?;
        // The exact .13 decoder remains authoritative for all legal/invalid forms.
        let result = serde_cbor::from_slice::<T>(raw);
        self.common.consume_steps(0)?;
        let result = match result {
            Ok(value) => Ok(PaidCborDataV1 {
                value,
                core: self.common.work().identity(),
                _fee: fee,
            }),
            Err(error) => Err(self.error(format_args!("{error}"))?),
        };
        drop(phase);
        Ok(result)
    }
    fn error(
        &mut self,
        args: fmt::Arguments<'_>,
    ) -> Result<PaidCborRefusalV1, BoundedCborDataErrorV1> {
        struct Count(usize);
        impl fmt::Write for Count {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0 = self.0.checked_add(s.len()).ok_or(fmt::Error)?;
                Ok(())
            }
        }
        let mut count = Count(0);
        fmt::write(&mut count, args).map_err(|_| limited())?;
        self.common
            .charge(1, u64::try_from(count.0).map_err(|_| limited())?)?;
        let bytes = count
            .0
            .checked_add(std::mem::size_of::<String>())
            .ok_or_else(limited)?;
        let fee = self.common.memory().reserve(bytes, MemoryClass::Ordinary)?;
        let mut output = String::with_capacity(count.0);
        fmt::write(&mut output, args).map_err(|_| limited())?;
        if output.capacity() > count.0 {
            return Err(limited());
        }
        Ok(PaidCborRefusalV1 {
            data: PaidCborDataV1 {
                value: output,
                core: self.common.work().identity(),
                _fee: fee,
            },
        })
    }
    fn encode<T: Serialize>(
        &mut self,
        value: &T,
    ) -> Result<Result<PaidCborDataV1<Vec<u8>>, PaidCborRefusalV1>, BoundedCborDataErrorV1> {
        let mut count = MeteredWrite {
            common: self.common,
            output: None,
            length: 0,
            limit: self.limits.max_wire_bytes,
            error: None,
        };
        let counted = serde_cbor::to_writer(&mut count, value);
        let n = count.length;
        if let Some(error) = count.error.take() {
            return Err(error);
        }
        count.common.consume_steps(0)?;
        if let Err(error) = counted {
            return Ok(Err(self.error(format_args!("{error}"))?));
        }
        self.common.charge(
            1,
            u64::try_from(n.checked_add(self.raw_len).ok_or_else(limited)?)
                .map_err(|_| limited())?,
        )?;
        let fee = self.common.memory().reserve(
            n.checked_add(std::mem::size_of::<Vec<u8>>())
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )?;
        let mut bytes = Vec::with_capacity(n);
        if bytes.capacity() != n {
            return Err(limited());
        }
        let mut output = MeteredWrite {
            common: self.common,
            output: Some(&mut bytes),
            length: 0,
            limit: n,
            error: None,
        };
        let encoded = serde_cbor::to_writer(&mut output, value);
        if let Some(error) = output.error.take() {
            return Err(error);
        }
        output.common.consume_steps(0)?;
        if let Err(error) = encoded {
            return Ok(Err(self.error(format_args!("{error}"))?));
        }
        if output.length != n {
            return Err(BoundedCborDataErrorV1::Invalid(
                "payload encoder length changed",
            ));
        }
        Ok(Ok(PaidCborDataV1 {
            value: bytes,
            core: self.common.work().identity(),
            _fee: fee,
        }))
    }
}
struct MeteredWrite<'a, 'ctx, 'v> {
    common: &'a mut FormalBoundedContextV1<'ctx>,
    output: Option<&'v mut Vec<u8>>,
    length: usize,
    limit: usize,
    error: Option<BoundedCborDataErrorV1>,
}
impl Write for MeteredWrite<'_, '_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.error.is_some() {
            return Err(io::Error::other("normal payload writer stopped"));
        }
        let run = (|| {
            self.common
                .charge(1, u64::try_from(bytes.len()).map_err(|_| limited())?)?;
            let end = self
                .length
                .checked_add(bytes.len())
                .filter(|n| *n <= self.limit)
                .ok_or_else(limited)?;
            if let Some(output) = self.output.as_mut() {
                output.extend_from_slice(bytes)
            }
            self.length = end;
            Ok::<_, BoundedCborDataErrorV1>(bytes.len())
        })();
        match run {
            Ok(n) => Ok(n),
            Err(error) => {
                self.error = Some(error);
                Err(io::Error::other("normal payload writer stopped"))
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.error.is_some() {
            Err(io::Error::other("normal payload writer stopped"))
        } else {
            Ok(())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::super::bounded_context::FormalBoundedWorkV1;
    use super::*;
    use oasis7_proto::memory_budget::MemoryBudget;
    fn layout() -> CborDataLayoutV1 {
        CborDataLayoutV1 {
            root: std::mem::size_of::<String>(),
            sequence_element: 0,
            map_pair: 0,
            set_element: 0,
            box_pointee: 0,
            default_heap_per_value: 0,
            symbols_bytes: 0,
            visits: 1,
            missing: None,
        }
    }
    fn limits() -> CborDataLimitsV1 {
        CborDataLimitsV1 {
            max_wire_bytes: 4096,
            max_graph_bytes: 1 << 20,
            max_depth: 128,
        }
    }
    #[test]
    fn consuming_move_keeps_same_pool_fee_until_callback_finishes() {
        struct Wrap;
        impl CborDataMoveWrapV1<String> for Wrap {
            type Output = Option<String>;
            fn wrap_data(v: String) -> Self::Output {
                Some(v)
            }
        }
        let memory = MemoryBudget::new(8 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(100_000, 1 << 24).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let owner =
            decode_closed_cbor_data_v1::<String>(&[0x61, b'x'], layout(), &limits(), &mut common)
                .unwrap()
                .ok()
                .unwrap();
        let owner = owner.map_move::<Wrap>(&mut common).unwrap();
        let result = owner
            .with_data(&mut common, |v, _| {
                assert!(memory.used() > 0);
                assert_eq!(v.as_deref(), Some("x"));
                Err::<(), _>("consumer error")
            })
            .unwrap();
        assert_eq!(result, Err("consumer error"));
        assert_eq!(memory.used(), 0);
    }
    #[test]
    fn codec_refusal_is_paid_and_missing_layout_never_runs_deserializer() {
        let memory = MemoryBudget::new(8 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(100_000, 1 << 24).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let result =
            decode_closed_cbor_data_v1::<String>(&[0xff], layout(), &limits(), &mut common)
                .unwrap();
        let refusal = result.err().unwrap();
        assert!(!refusal.message().is_empty());
        assert!(memory.used() > 0);
        refusal.validate_context(&mut common).unwrap();
        drop(refusal);
        assert_eq!(memory.used(), 0);
        let mut missing = layout();
        missing.missing = Some("closed child");
        assert!(matches!(
            decode_closed_cbor_data_v1::<String>(&[0x61, b'x'], missing, &limits(), &mut common),
            Err(BoundedCborDataErrorV1::Invalid(_))
        ));
        assert_eq!(memory.used(), 0);
    }
}
/// Resource admission for a shallow move into a separately paid DATA carrier.
/// The descriptor must cover that carrier's complete closed output graph.
/// This reservation grants no right to execute or qualify its eventual value.
pub fn reserve_closed_cbor_output_v1(
    raw: &[u8],
    layout: CborDataLayoutV1,
    limits: &CborDataLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, BoundedCborDataErrorV1> {
    if layout.missing.is_some() {
        return Err(BoundedCborDataErrorV1::Invalid(
            "closed CBOR layout missing",
        ));
    }
    if raw.len() > limits.max_wire_bytes {
        return Err(limited());
    }
    let shape = probe_cbor_data_shape_v1(raw, limits.max_depth, common)?;
    let bytes = graph(shape, layout, layout.root)?;
    if bytes > limits.max_graph_bytes {
        return Err(limited());
    }
    common.charge(1, u64::try_from(bytes).map_err(|_| limited())?)?;
    Ok(common.memory().reserve(bytes, MemoryClass::Ordinary)?)
}
/// Consume the genuine JSON Value through serde_json's original from_value
/// visitor. The caller retains the source Value's admission until this call
/// returns. This neither changes its accepted codec nor creates authority.
pub fn decode_closed_json_value_data_v1<T: DeserializeOwned>(
    value: serde_json::Value,
    layout: CborDataLayoutV1,
    limits: &CborDataLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Result<PaidCborDataV1<T>, PaidCborRefusalV1>, BoundedCborDataErrorV1> {
    fn visit(
        v: &serde_json::Value,
        d: usize,
        max: usize,
        s: &mut CborDataShapeV1,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BoundedCborDataErrorV1> {
        if d > max.min(128) {
            return Err(limited());
        }
        c.consume_steps(1)?;
        s.values = s.values.checked_add(1).ok_or_else(limited)?;
        s.depth = s.depth.max(d);
        match v {
            serde_json::Value::String(v) => {
                c.charge(0, v.len() as u64)?;
                s.strings = s.strings.checked_add(v.len() as u64).ok_or_else(limited)?
            }
            serde_json::Value::Array(v) => {
                s.sequences = s
                    .sequences
                    .checked_add(v.len() as u64)
                    .ok_or_else(limited)?;
                for v in v {
                    visit(v, d + 1, max, s, c)?
                }
            }
            serde_json::Value::Object(v) => {
                s.maps = s.maps.checked_add(v.len() as u64).ok_or_else(limited)?;
                for (k, v) in v {
                    c.charge(1, k.len() as u64)?;
                    s.strings = s.strings.checked_add(k.len() as u64).ok_or_else(limited)?;
                    visit(v, d + 1, max, s, c)?
                }
            }
            _ => {}
        }
        Ok(())
    }
    if layout.missing.is_some() {
        return Err(BoundedCborDataErrorV1::Invalid(
            "closed JSON layout missing",
        ));
    }
    let mut shape = CborDataShapeV1::default();
    visit(&value, 0, limits.max_depth, &mut shape, common)?;
    let bytes = graph(shape, layout, std::mem::size_of::<T>())?;
    let scratch = bytes
        .checked_mul(shape.depth.checked_add(3).ok_or_else(limited)?)
        .ok_or_else(limited)?;
    if bytes > limits.max_graph_bytes || scratch > limits.max_graph_bytes {
        return Err(limited());
    }
    common.charge(
        shape.values.checked_mul(8).ok_or_else(limited)?,
        bytes as u64,
    )?;
    let fee = common.memory().reserve(bytes, MemoryClass::Ordinary)?;
    let phase = common.memory().reserve(scratch, MemoryClass::Ordinary)?;
    let result = serde_json::from_value::<T>(value);
    common.consume_steps(0)?;
    let result = match result {
        Ok(value) => Ok(PaidCborDataV1 {
            value,
            core: common.work().identity(),
            _fee: fee,
        }),
        Err(error) => Err(Engine {
            common,
            limits,
            raw_len: 0,
        }
        .error(format_args!("{error}"))?),
    };
    drop(phase);
    Ok(result)
}
