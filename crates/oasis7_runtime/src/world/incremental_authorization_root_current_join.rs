//! Fixed byte equivalence for untrusted hint DATA versus original source DTOs.
//! A successful comparison grants no Current or accepted capability by itself.
use super::*;
struct Compare<'a> {
    expected: &'a [u8],
    at: usize,
}
impl Write for Compare<'_> {
    fn write(&mut self, part: &[u8]) -> io::Result<usize> {
        let end = self
            .at
            .checked_add(part.len())
            .ok_or(io::ErrorKind::InvalidData)?;
        if self.expected.get(self.at..end) != Some(part) {
            return Err(io::Error::other(
                "authorization source canonical bytes mismatch",
            ));
        }
        self.at = end;
        Ok(part.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn compare_original<T: serde::Serialize>(
    cached: &ValueRef,
    actual: &T,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<(), AuthorizationRootCacheErrorV1> {
    // The authoritative DTO uses exactly the maintained original canonical
    // encoder. Holding this paid byte owner closes the compare's peak lifetime.
    let source_scratch = limits
        .max_depth
        .checked_add(1)
        .and_then(|n| {
            n.checked_mul(std::mem::size_of::<String>() + 40 + 2 * std::mem::align_of::<String>())
        })
        .ok_or_else(limited)?;
    let _source_scratch = common.memory().reserve(source_scratch, class)?;
    let actual = wire::encode(actual, limits, common, class)?;
    let mut compare = Compare {
        expected: &actual,
        at: 0,
    };
    cbor::write(
        cached,
        &mut compare,
        common,
        limits.max_depth,
        limits.max_wire_bytes,
    )?;
    if compare.at != actual.len() {
        return Err(invalid("authorization canonical source length"));
    }
    common.consume_steps(0)?;
    Ok(())
}
pub(super) fn compare_hint_capability(
    cached: &ValueRef,
    hint: &decode::HintCapabilityRecordV1,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<(), AuthorizationRootCacheErrorV1> {
    match hint {
        decode::HintCapabilityRecordV1::Nonce(v) => {
            compare_original(cached, v, limits, common, class)
        }
        decode::HintCapabilityRecordV1::Budget(v) => {
            compare_original(cached, v, limits, common, class)
        }
        decode::HintCapabilityRecordV1::Link(v) => {
            compare_original(cached, v, limits, common, class)
        }
    }
}
