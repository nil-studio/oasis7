use super::*;
use std::io::{self, Write};
struct Buffer<'a, 'c, 'm, C> {
    bytes: Vec<u8>,
    max: usize,
    remaining: &'a mut u64,
    common: &'c mut FormalBoundedContextV1<'m>,
    cancelled: &'a mut C,
    failure: Option<DeliveryDataErrorV1>,
}
impl<C: FnMut() -> bool> Write for Buffer<'_, '_, '_, C> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.max)
        {
            self.failure = Some(limited());
            return Err(io::Error::other("delivery original bound"));
        }
        for part in bytes.chunks(16 * 1024) {
            let result: Result<(), DeliveryDataErrorV1> = (|| {
                if (self.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                *self.remaining = self
                    .remaining
                    .checked_sub(part.len() as u64)
                    .ok_or_else(limited)?;
                self.common.charge(1, part.len() as u64)?;
                Ok(())
            })();
            if let Err(error) = result {
                self.failure = Some(error);
                return Err(io::Error::other("delivery original stopped"));
            }
            self.bytes.extend_from_slice(part);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn read<O: OriginalObjectStreamReader, C: FnMut() -> bool>(
    id: &[u8; 32],
    exact: Option<u64>,
    originals: &mut O,
    work: &mut ProductValidationDeliveryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    cancelled: &mut C,
) -> Result<(ReservedValue<DecodedWorldEventV1>, u64), DeliverySourceErrorV1<O::Error>> {
    let data = (|| {
        common.consume_steps(1)?;
        if !originals.allocation_budget().same_pool(common.memory())
            || originals.allocation_class() != MemoryClass::Ordinary
        {
            return Err(invalid("delivery original pool/class"));
        }
        work.originals_remaining = work
            .originals_remaining
            .checked_sub(1)
            .ok_or_else(limited)?;
        let max = match exact {
            Some(n) => usize::try_from(n)
                .ok()
                .filter(|n| *n <= work.decode.max_original_bytes)
                .ok_or_else(limited)?,
            None => work.decode.max_original_bytes,
        };
        let fee = common.memory().reserve(
            max.checked_add(size_of::<Vec<u8>>() + 4096)
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(max).map_err(|_| limited())?;
        if bytes.capacity() > max {
            return Err(limited());
        }
        Ok((fee, bytes, max))
    })()
    .map_err(DeliverySourceErrorV1::Data)?;
    let (fee, bytes, max) = data;
    let memory = common.memory().clone();
    let mut buffer = Buffer {
        bytes,
        max,
        remaining: &mut work.original_bytes_remaining,
        common,
        cancelled,
        failure: None,
    };
    let result = originals.stream_original(id, max as u64, &memory, &mut buffer);
    if let Some(error) = buffer.failure.take() {
        return Err(error.into());
    }
    let actual = result.map_err(DeliverySourceErrorV1::Original)?;
    let Buffer {
        bytes,
        common,
        cancelled,
        ..
    } = buffer;
    if actual != bytes.len() as u64 || exact.is_some_and(|n| n != actual) {
        return Err(invalid("delivery original exact length").into());
    }
    let mut hash = blake3::Hasher::new();
    for part in bytes.chunks(16 * 1024) {
        common
            .charge(1, part.len() as u64)
            .map_err(DeliveryDataErrorV1::from)?;
        hash.update(part);
    }
    if hash.finalize().as_bytes() != id {
        return Err(invalid("delivery original digest").into());
    }
    // Decode all original owned layout/canonical lanes under the same exclusive
    // Common child, including returned errors. No independent work is created.
    let memory = common.memory().clone();
    let (_, meter, stop) = common.parts();
    let child = meter.begin_child().map_err(DeliveryDataErrorV1::from)?;
    let steps = child.remaining_steps() / 2;
    let read = child.remaining_read_bytes() / 2;
    work.decode.structure.values_remaining = work.decode.structure.values_remaining.min(steps);
    work.decode.canonical.visits_remaining = work.decode.canonical.visits_remaining.min(steps);
    work.decode.structure.bytes_remaining = work.decode.structure.bytes_remaining.min(read);
    work.decode.canonical.bytes_remaining = work.decode.canonical.bytes_remaining.min(read);
    let before = [
        work.decode.structure.values_remaining,
        work.decode.canonical.visits_remaining,
        work.decode.structure.bytes_remaining,
        work.decode.canonical.bytes_remaining,
    ];
    let decoded =
        oasis7_world_state::incremental_world_event_decode::decode_world_event_original_v1(
            &bytes,
            &memory,
            MemoryClass::Ordinary,
            &mut work.decode,
            || cancelled() || stop(),
        );
    let after = [
        work.decode.structure.values_remaining,
        work.decode.canonical.visits_remaining,
        work.decode.structure.bytes_remaining,
        work.decode.canonical.bytes_remaining,
    ];
    let debit = |mut range: std::ops::Range<usize>| {
        range.try_fold(0u64, |n, i| n.checked_add(before[i].checked_sub(after[i])?))
    };
    let steps = debit(0..2).ok_or_else(limited)?;
    let read = debit(2..4).ok_or_else(limited)?;
    let paid = child.consume(steps, read);
    child.finish();
    let decoded = decoded.map_err(DeliveryDataErrorV1::from)?;
    paid.map_err(DeliveryDataErrorV1::from)?;
    common.consume_steps(0).map_err(DeliveryDataErrorV1::from)?;
    drop(bytes);
    drop(fee);
    Ok((decoded, actual))
}
