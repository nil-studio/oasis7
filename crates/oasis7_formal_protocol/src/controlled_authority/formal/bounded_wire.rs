//! Two-pass original serde-CBOR wire with prepaid retained output and caller work.
//! Encoding supplies content only, never publication or execution authority.
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryLimitExceeded, ReservedValue};
use std::io::{self, Write};

pub struct BoundedWireWorkV1 {
    pub max_wire_bytes: usize,
    pub remaining_steps: u64,
    pub remaining_bytes: u64,
}
#[derive(Debug)]
pub enum BoundedWireErrorV1 {
    Cancelled,
    StepLimit,
    ByteLimit,
    WireLimit,
    ChangedLength,
    Layout,
    Memory(MemoryLimitExceeded),
    Encode(serde_cbor::Error),
}
impl std::fmt::Display for BoundedWireErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bounded endpoint wire: {self:?}")
    }
}
impl std::error::Error for BoundedWireErrorV1 {}
#[derive(Clone, Copy)]
enum Stop {
    Cancelled,
    StepLimit,
    ByteLimit,
    WireLimit,
    ChangedLength,
    Layout,
}
impl Stop {
    fn error(self) -> BoundedWireErrorV1 {
        match self {
            Self::Cancelled => BoundedWireErrorV1::Cancelled,
            Self::StepLimit => BoundedWireErrorV1::StepLimit,
            Self::ByteLimit => BoundedWireErrorV1::ByteLimit,
            Self::WireLimit => BoundedWireErrorV1::WireLimit,
            Self::ChangedLength => BoundedWireErrorV1::ChangedLength,
            Self::Layout => BoundedWireErrorV1::Layout,
        }
    }
}
struct Wire<'a, C> {
    work: &'a mut BoundedWireWorkV1,
    cancelled: &'a mut C,
    output: Option<&'a mut Vec<u8>>,
    limit: usize,
    len: usize,
    first: Option<Stop>,
}
impl<C: FnMut() -> bool> Wire<'_, C> {
    fn fail(&mut self, reason: Stop) -> io::Result<usize> {
        if self.first.is_none() {
            self.first = Some(reason);
        }
        Err(io::ErrorKind::Other.into())
    }
    fn finish(&self) -> Result<(), BoundedWireErrorV1> {
        match self.first {
            Some(v) => Err(v.error()),
            None => Ok(()),
        }
    }
}
impl<C: FnMut() -> bool> Write for Wire<'_, C> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.first.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        if (self.cancelled)() {
            return self.fail(Stop::Cancelled);
        }
        let Some(steps) = self.work.remaining_steps.checked_sub(1) else {
            return self.fail(Stop::StepLimit);
        };
        self.work.remaining_steps = steps;
        let Ok(n) = u64::try_from(bytes.len()) else {
            return self.fail(Stop::Layout);
        };
        let Some(left) = self.work.remaining_bytes.checked_sub(n) else {
            self.work.remaining_bytes = 0;
            return self.fail(Stop::ByteLimit);
        };
        self.work.remaining_bytes = left;
        let Some(next) = self.len.checked_add(bytes.len()) else {
            return self.fail(Stop::Layout);
        };
        if next > self.limit {
            return self.fail(if self.output.is_some() {
                Stop::ChangedLength
            } else {
                Stop::WireLimit
            });
        }
        if let Some(output) = self.output.as_mut() {
            // Capacity was admitted before the second serializer runs; no growth allowed.
            if next > output.capacity() {
                return self.fail(Stop::Layout);
            }
            output.extend_from_slice(bytes);
        }
        self.len = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.first.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        if (self.cancelled)() {
            self.first = Some(Stop::Cancelled);
            return Err(io::ErrorKind::Other.into());
        }
        Ok(())
    }
}
/// Both passes debit the same counters, including failed writes. A serializer
/// swallowing a writer error cannot obtain bytes: the sticky first cause wins.
pub fn encode_bounded_wire_v1<T: serde::Serialize, C: FnMut() -> bool>(
    value: &T,
    memory: &MemoryBudget,
    work: &mut BoundedWireWorkV1,
    cancelled: &mut C,
) -> Result<ReservedValue<Vec<u8>>, BoundedWireErrorV1> {
    if cancelled() {
        return Err(BoundedWireErrorV1::Cancelled);
    }
    let limit = work.max_wire_bytes;
    let mut count = Wire {
        work,
        cancelled,
        output: None,
        limit,
        len: 0,
        first: None,
    };
    let result = serde_cbor::to_writer(&mut count, value);
    count.finish()?;
    result.map_err(BoundedWireErrorV1::Encode)?;
    let measured = count.len;
    if (count.cancelled)() {
        return Err(BoundedWireErrorV1::Cancelled);
    }
    let paid = measured
        .checked_add(std::mem::size_of::<Vec<u8>>())
        .ok_or(BoundedWireErrorV1::Layout)?;
    let fee = memory
        .reserve(paid, MemoryClass::Ordinary)
        .map_err(BoundedWireErrorV1::Memory)?;
    let mut bytes = Vec::with_capacity(measured);
    if bytes.capacity() > measured {
        return Err(BoundedWireErrorV1::Layout);
    }
    let mut writer = Wire {
        work: count.work,
        cancelled: count.cancelled,
        output: Some(&mut bytes),
        limit: measured,
        len: 0,
        first: None,
    };
    let result = serde_cbor::to_writer(&mut writer, value);
    writer.finish()?;
    result.map_err(BoundedWireErrorV1::Encode)?;
    if (writer.cancelled)() {
        return Err(BoundedWireErrorV1::Cancelled);
    }
    if writer.len != measured {
        return Err(BoundedWireErrorV1::ChangedLength);
    }
    drop(writer);
    if bytes.len() != measured || bytes.capacity() > measured {
        return Err(BoundedWireErrorV1::ChangedLength);
    }
    Ok(fee.retain(bytes))
}

#[cfg(test)]
#[path = "bounded_wire_tests.rs"]
mod tests;
