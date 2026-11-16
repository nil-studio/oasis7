//! Actual continuing processing/IO and two-pass JSON output, no inventory rules.
use super::*;
use oasis7_formal_protocol::incremental_value::ValueNodeReader;
use std::io::{self, Write};
pub(super) struct Processing<'a> {
    pub(super) work: RefCell<&'a mut BusinessDiffProcessingWorkV1>,
    pub(super) cancel: &'a dyn Fn() -> bool,
}
impl Processing<'_> {
    pub(super) fn spend(&self, n: usize) -> Result<(), ValueTreeError> {
        if (self.cancel)() {
            return Err(ValueTreeError::Cancelled);
        }
        let n = u64::try_from(n).map_err(|_| limit())?;
        let steps = n.checked_add(1).ok_or_else(limit)?;
        let mut w = self.work.borrow_mut();
        let old = (w.steps_remaining, w.bytes_remaining);
        w.steps_remaining = w.steps_remaining.saturating_sub(steps);
        w.bytes_remaining = w.bytes_remaining.saturating_sub(n);
        if old.0 < steps || old.1 < n {
            Err(limit())
        } else {
            Ok(())
        }
    }
}
pub(super) struct PaidSource<'a, 'b, R> {
    pub(super) source: &'a R,
    pub(super) p: &'a Processing<'b>,
}
impl<R: ValueNodeReader> ValueNodeReader for PaidSource<'_, '_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.p.spend(0)?;
        let node = self.source.read_node(id, max, memory, class)?;
        let n = node
            .len()
            .checked_mul(16)
            .and_then(|n| n.checked_add(32 + 256))
            .ok_or_else(limit)?;
        self.p.spend(n)?;
        Ok(node)
    }
}

struct Output<'a, 'b> {
    pub(super) p: &'a Processing<'b>,
    bytes: Option<Vec<u8>>,
    n: usize,
    max: usize,
    first: Option<ValueTreeError>,
}
impl Write for Output<'_, '_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        for chunk in b.chunks(8192) {
            if self.first.is_some() {
                return Err(io::ErrorKind::Other.into());
            }
            if let Err(e) = self.p.spend(chunk.len()) {
                self.first = Some(e);
                return Err(io::ErrorKind::Other.into());
            }
            let Some(n) = self.n.checked_add(chunk.len()).filter(|n| *n <= self.max) else {
                self.first = Some(limit());
                return Err(io::ErrorKind::Other.into());
            };
            if let Some(out) = &mut self.bytes {
                out.extend_from_slice(chunk)
            }
            self.n = n
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn encode(
    memory: &MemoryBudget,
    p: &Processing<'_>,
    mut writer: impl FnMut(&mut dyn Write) -> Result<(), ValueTreeError>,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let mut count = Output {
        p,
        bytes: None,
        n: 0,
        max: usize::MAX,
        first: None,
    };
    let result = writer(&mut count);
    if let Some(e) = count.first {
        return Err(e);
    }
    result?;
    let n = count.n;
    let fee = memory.reserve(
        n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
        MemoryClass::Ordinary,
    )?;
    p.spend(n)?;
    let bytes = Vec::with_capacity(n);
    if bytes.capacity() != n {
        return Err(limit());
    }
    let mut out = Output {
        p,
        bytes: Some(bytes),
        n: 0,
        max: n,
        first: None,
    };
    let result = writer(&mut out);
    if let Some(e) = out.first {
        return Err(e);
    }
    result?;
    if out.n != n {
        return Err(bad());
    }
    Ok(fee.retain(out.bytes.take().ok_or_else(bad)?))
}
pub(super) fn json<T: serde::Serialize>(
    value: &T,
    memory: &MemoryBudget,
    p: &Processing<'_>,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    encode(memory, p, |out| {
        serde_json::to_writer(out, value).map_err(|_| bad())
    })
}
