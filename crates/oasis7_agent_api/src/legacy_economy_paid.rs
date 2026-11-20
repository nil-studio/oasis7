//! DATA-only work variant of the original quote/request digest. The canonical
//! tuple, encoder and BLAKE3 prefix are identical to legacy_scheduler::digest.
use super::*;
use oasis7_domain_types::canonical_digest::{
    PaidCanonicalBlake3DataV1, canonical_blake3_data_with_work_v1,
};
use oasis7_wasm_abi::CapabilityCanonicalWorkV1;
use std::io::{self, Write};
pub struct PaidLeaseDigestDataV1<C> {
    digest: PaidCanonicalBlake3DataV1<C>,
}
impl<C> PaidLeaseDigestDataV1<C> {
    pub fn as_str(&self) -> &str {
        self.digest.as_str()
    }
}
fn stopped() -> CognitionEconomyError {
    CognitionEconomyError::InvalidState("bounded_cognition_economy_work")
}
pub(super) fn work<A: CapabilityCanonicalWorkV1>(
    source: &A,
    n: usize,
) -> Result<(), CognitionEconomyError> {
    source.charge_work(1, n as u64).map_err(|_| stopped())
}
pub(super) fn phase<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    value: &T,
    source: &A,
) -> Result<Option<A::Charge>, CognitionEconomyError> {
    if !source.is_metered() {
        return Ok(None);
    }
    struct Count<'a, A> {
        source: &'a A,
        n: usize,
    }
    impl<A: CapabilityCanonicalWorkV1> Write for Count<'_, A> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.source
                .charge_work(1, b.len() as u64)
                .map_err(|_| io::ErrorKind::Other)?;
            self.n = self.n.checked_add(b.len()).ok_or(io::ErrorKind::Other)?;
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count { source, n: 0 };
    serde_json::to_writer(&mut count, value).map_err(|_| stopped())?;
    let n = count
        .n
        .checked_mul(1024)
        .and_then(|n| n.checked_add(32768))
        .ok_or_else(stopped)?;
    source
        .charge_work(count.n as u64, n as u64)
        .map_err(|_| stopped())?;
    source.reserve(n).map(Some).map_err(|_| stopped())
}
pub(super) fn digest<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    domain: &str,
    value: &T,
    source: &A,
) -> Result<PaidLeaseDigestDataV1<A::Charge>, CognitionEconomyError> {
    let digest =
        canonical_blake3_data_with_work_v1(&(domain, value), source).map_err(|_| stopped())?;
    source.charge_work(0, 0).map_err(|_| stopped())?;
    Ok(PaidLeaseDigestDataV1 { digest })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metering_test_support::Meter;

    #[test]
    fn lease_digest_keeps_original_preimage_and_typed_stream_hook_before_error_mapping() {
        let payload = ("payer", "中文\0quote", 17u64);
        let complete = Meter::new(usize::MAX, false);
        let output = digest(COGNITION_ECONOMY_QUOTE_DOMAIN, &payload, &complete).unwrap();
        assert_eq!(
            output.as_str(),
            economy_digest(COGNITION_ECONOMY_QUOTE_DOMAIN, &payload)
        );
        let mut saw_hook = false;
        for cancel in [false, true] {
            for fail_at in 1..=complete.calls.get() {
                let source = Meter::new(fail_at, cancel);
                let error = digest(COGNITION_ECONOMY_QUOTE_DOMAIN, &payload, &source)
                    .err()
                    .expect("injected failure");
                assert_eq!(
                    error,
                    CognitionEconomyError::InvalidState("bounded_cognition_economy_work")
                );
                source.assert_first_cause();
                saw_hook |= !source.recorded.borrow().is_empty();
            }
        }
        assert!(
            saw_hook,
            "the canonical hash sink must preserve its caller's hook"
        );
    }
}
