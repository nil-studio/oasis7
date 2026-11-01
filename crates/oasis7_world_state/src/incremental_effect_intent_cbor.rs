//! Borrowed original EffectIntent CBOR wire, not a new canonical encoding.
//! The caller owns the source DTO and output sink allocations. This concrete
//! projection never invokes Serialize on params containers or the whole Intent.
use crate::{EffectIntent, EffectOrigin};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use serde::Serialize;
use serde_json::Value;
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectIntentCborBudgetV1 {
    /// Structural encoding nodes plus every nonempty sink write attempt,
    /// including Interrupted and partial writes.
    pub nodes_remaining: u64,
    pub bytes_remaining: u64,
    pub max_json_depth: usize,
}
pub struct EffectIntentCborContextV1<'a, C> {
    memory: &'a MemoryBudget,
    class: MemoryClass,
    budget: EffectIntentCborBudgetV1,
    cancelled: C,
    poisoned: bool,
}
impl<'a, C: FnMut() -> bool> EffectIntentCborContextV1<'a, C> {
    pub fn new(
        memory: &'a MemoryBudget,
        class: MemoryClass,
        budget: EffectIntentCborBudgetV1,
        cancelled: C,
    ) -> Self {
        Self {
            memory,
            class,
            budget,
            cancelled,
            poisoned: false,
        }
    }
    pub fn remaining(&self) -> EffectIntentCborBudgetV1 {
        self.budget
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    fn check(&mut self) -> Result<(), ValueTreeError> {
        if self.poisoned {
            return Err(ValueTreeError::Invalid("effect CBOR context poisoned"));
        }
        if (self.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        Ok(())
    }
    fn node(&mut self) -> Result<(), ValueTreeError> {
        self.check()?;
        self.budget.nodes_remaining = self
            .budget
            .nodes_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("effect CBOR nodes"))?;
        Ok(())
    }
}
struct Output<'a, 'm, C, W> {
    context: &'a mut EffectIntentCborContextV1<'m, C>,
    sink: &'a mut W,
    bytes: u64,
    error: Option<ValueTreeError>,
}
impl<C: FnMut() -> bool, W: Write> Output<'_, '_, C, W> {
    fn fail(&mut self, error: ValueTreeError) -> std::io::Error {
        if self.error.is_none() {
            self.error = Some(error);
        }
        self.context.poisoned = true;
        std::io::Error::other("effect CBOR output failed")
    }
    fn recover(&mut self) -> ValueTreeError {
        self.error
            .take()
            .unwrap_or(ValueTreeError::Invalid("effect original CBOR serializer"))
    }
    // Only closed primitives, serde_json Number, and the shallow original
    // EffectOrigin enum enter this function. Never a JSON collection or Intent.
    fn atom<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), ValueTreeError> {
        self.context.node()?;
        if serde_cbor::to_writer(&mut *self, &value).is_err() {
            return Err(self.recover());
        }
        self.context.check()
    }
    fn header(&mut self, major: u8, len: usize) -> Result<(), ValueTreeError> {
        self.context.node()?;
        let len = u64::try_from(len)
            .map_err(|_| ValueTreeError::ResourceLimited("effect CBOR length"))?;
        let mut bytes = [0u8; 9];
        let count = if len < 24 {
            bytes[0] = (major << 5) | len as u8;
            1
        } else if len <= u8::MAX as u64 {
            bytes[0] = (major << 5) | 24;
            bytes[1] = len as u8;
            2
        } else if len <= u16::MAX as u64 {
            bytes[0] = (major << 5) | 25;
            bytes[1..3].copy_from_slice(&(len as u16).to_be_bytes());
            3
        } else if len <= u32::MAX as u64 {
            bytes[0] = (major << 5) | 26;
            bytes[1..5].copy_from_slice(&(len as u32).to_be_bytes());
            5
        } else {
            bytes[0] = (major << 5) | 27;
            bytes[1..9].copy_from_slice(&len.to_be_bytes());
            9
        };
        if self.write_all(&bytes[..count]).is_err() {
            return Err(self.recover());
        }
        self.context.check()
    }
}
impl<C: FnMut() -> bool, W: Write> Write for Output<'_, '_, C, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if let Err(e) = self.context.check() {
            return Err(self.fail(e));
        }
        if bytes.is_empty() {
            return Ok(0);
        }
        if let Err(error) = self.context.node() {
            return Err(self.fail(error));
        }
        let part = &bytes[..bytes.len().min(16 << 10)];
        if self.context.budget.bytes_remaining < part.len() as u64 {
            return Err(self.fail(ValueTreeError::ResourceLimited("effect CBOR bytes")));
        }
        let result = self.sink.write(part);
        let count = match result {
            Ok(0) => {
                return Err(self.fail(std::io::Error::from(std::io::ErrorKind::WriteZero).into()));
            }
            Ok(n) if n <= part.len() => n,
            Ok(_) => return Err(self.fail(ValueTreeError::Invalid("effect CBOR sink count"))),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                if let Err(e) = self.context.check() {
                    return Err(self.fail(e));
                }
                return Err(e);
            }
            Err(e) => return Err(self.fail(e.into())),
        };
        self.context.budget.bytes_remaining -= count as u64;
        self.bytes = self
            .bytes
            .checked_add(count as u64)
            .ok_or_else(|| self.fail(ValueTreeError::ResourceLimited("effect CBOR total")))?;
        if let Err(e) = self.context.check() {
            return Err(self.fail(e));
        }
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        // Encoding makes no durable-sink promise and never flushes on behalf
        // of the caller. Even an explicit flush observes cancellation.
        self.context.check().map_err(|e| self.fail(e))
    }
}
enum Frame<'a> {
    Array(std::slice::Iter<'a, Value>),
    Object(serde_json::map::Iter<'a>),
}
fn json<C: FnMut() -> bool, W: Write>(
    value: &Value,
    output: &mut Output<'_, '_, C, W>,
) -> Result<(), ValueTreeError> {
    let capacity = output.context.budget.max_json_depth.checked_add(1).ok_or(
        ValueTreeError::ResourceLimited("effect JSON depth admission"),
    )?;
    let bytes = capacity
        .checked_mul(std::mem::size_of::<Frame<'_>>())
        .ok_or(ValueTreeError::ResourceLimited(
            "effect JSON frame admission",
        ))?;
    let _frames = output.context.memory.reserve(bytes, output.context.class)?;
    let mut stack = Vec::with_capacity(capacity);
    let mut next = Some(value);
    loop {
        output.context.check()?;
        if let Some(value) = next.take() {
            match value {
                Value::Null => output.atom(&())?,
                Value::Bool(v) => output.atom(v)?,
                Value::Number(v) => output.atom(v)?,
                Value::String(v) => output.atom(v)?,
                Value::Array(values) => {
                    if stack.len() >= output.context.budget.max_json_depth {
                        return Err(ValueTreeError::ResourceLimited("effect JSON depth"));
                    }
                    output.header(4, values.len())?;
                    stack.push(Frame::Array(values.iter()));
                }
                Value::Object(values) => {
                    if stack.len() >= output.context.budget.max_json_depth {
                        return Err(ValueTreeError::ResourceLimited("effect JSON depth"));
                    }
                    output.header(5, values.len())?;
                    stack.push(Frame::Object(values.iter()));
                }
            }
        }
        let Some(frame) = stack.last_mut() else {
            break;
        };
        next = match frame {
            Frame::Array(values) => values.next(),
            Frame::Object(values) => match values.next() {
                Some((key, value)) => {
                    output.atom(key)?;
                    Some(value)
                }
                None => None,
            },
        };
        if next.is_none() {
            stack.pop();
        }
    }
    output.context.check()
}
/// Exact original tuple/domain and original derived field order. JSON traversal
/// is iterative, preserves source map iteration order, and has no whole Value/Vec
/// copy or replay. Partial output remains provisional on every error.
pub fn stream_effect_intent_payload_v1<C: FnMut() -> bool, W: Write>(
    intent: &EffectIntent,
    context: &mut EffectIntentCborContextV1<'_, C>,
    sink: &mut W,
) -> Result<u64, ValueTreeError> {
    let result = (|| {
        context.check()?;
        let _scratch = context.memory.reserve(16 << 10, context.class)?;
        let EffectIntent {
            birth_era,
            intent_id,
            kind,
            params,
            cap_ref,
            origin,
        } = intent;
        let mut output = Output {
            context,
            sink,
            bytes: 0,
            error: None,
        };
        output.header(4, 2)?;
        output.atom("oasis7.effect.intent-payload.v2")?;
        output.header(5, if birth_era.is_some() { 6 } else { 5 })?;
        if let Some(era) = birth_era {
            output.atom("birth_era")?;
            output.atom(era)?;
        }
        output.atom("intent_id")?;
        output.atom(intent_id)?;
        output.atom("kind")?;
        output.atom(kind)?;
        output.atom("params")?;
        json(params, &mut output)?;
        output.atom("cap_ref")?;
        output.atom(cap_ref)?;
        output.atom("origin")?;
        // Preserve the existing tagged/content enum serializer and its labels.
        let _: &EffectOrigin = origin;
        output.atom(origin)?;
        output.context.check()?;
        Ok(output.bytes)
    })();
    if result.is_err() {
        context.poisoned = true;
    }
    result
}
#[derive(Debug, PartialEq, Eq)]
pub struct EffectIntentPayloadDigestV1 {
    pub digest: [u8; 32],
    pub encoded_bytes: u64,
}
struct Hash(blake3::Hasher);
impl Write for Hash {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn hash_effect_intent_payload_v1<C: FnMut() -> bool>(
    intent: &EffectIntent,
    context: &mut EffectIntentCborContextV1<'_, C>,
) -> Result<ReservedValue<EffectIntentPayloadDigestV1>, ValueTreeError> {
    let result = (|| {
        context.check()?;
        let retained = context.memory.reserve(128, context.class)?;
        let _hash_scratch = context.memory.reserve(8192, context.class)?;
        let mut output = Hash(blake3::Hasher::new());
        let encoded_bytes = stream_effect_intent_payload_v1(intent, context, &mut output)?;
        let digest = *output.0.finalize().as_bytes();
        context.check()?;
        Ok(retained.retain(EffectIntentPayloadDigestV1 {
            digest,
            encoded_bytes,
        }))
    })();
    if result.is_err() {
        context.poisoned = true;
    }
    result
}
#[cfg(test)]
#[path = "incremental_effect_intent_cbor_tests.rs"]
mod tests;
