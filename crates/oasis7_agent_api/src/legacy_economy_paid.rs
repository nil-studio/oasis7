//! DATA-only work variant of the original quote/request digest. The canonical
//! tuple, encoder and BLAKE3 prefix are identical to legacy_scheduler::digest.
use super::*;
use oasis7_wasm_abi::{CapabilityCanonicalWorkV1, stream_canonical_cbor_with_work_v1};
use std::io::{self, Write};
pub struct PaidLeaseDigestDataV1<C> {
    pub(super) text: String,
    _fee: Option<C>,
}
impl<C> PaidLeaseDigestDataV1<C> {
    pub fn as_str(&self) -> &str {
        &self.text
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
    if !source.is_metered() {
        return Ok(PaidLeaseDigestDataV1 {
            text: economy_digest(domain, &value),
            _fee: None,
        });
    }
    work(
        source,
        std::mem::size_of::<PaidLeaseDigestDataV1<A::Charge>>() + 71,
    )?;
    let fee = source
        .reserve(std::mem::size_of::<PaidLeaseDigestDataV1<A::Charge>>() + 71)
        .map_err(|_| stopped())?;
    struct Hash<'a, A> {
        source: &'a A,
        hash: blake3::Hasher,
    }
    impl<A: CapabilityCanonicalWorkV1> Write for Hash<'_, A> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            for chunk in b.chunks(8192) {
                self.source
                    .charge_work(1, chunk.len() as u64)
                    .map_err(|_| io::ErrorKind::Other)?;
                self.hash.update(chunk);
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut out = Hash {
        source,
        hash: blake3::Hasher::new(),
    };
    stream_canonical_cbor_with_work_v1(&(domain, value), source, &mut out)
        .map_err(|_| stopped())?;
    work(source, 71)?;
    let hex = out.hash.finalize().to_hex();
    let mut text = String::with_capacity(71);
    text.push_str("blake3:");
    text.push_str(hex.as_str());
    source.charge_work(0, 0).map_err(|_| stopped())?;
    Ok(PaidLeaseDigestDataV1 {
        text,
        _fee: Some(fee),
    })
}
