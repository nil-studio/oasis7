//! Canonical CBOR/BLAKE3 digest data with the caller's continuing work and allocation budget.
//! The digest kernel owns the encoder and hash algorithm; callers only supply data and work.
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamError as Error, CapabilityCanonicalWorkV1,
    stream_canonical_cbor_with_work_v1,
};
use serde::Serialize;
use std::io::{self, Write};

pub struct PaidCanonicalBlake3DataV1<C> {
    text: String,
    _fee: Option<C>,
}
impl<C> PaidCanonicalBlake3DataV1<C> {
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

/// Hash the original canonical payload without copying it into an encoded buffer when metered.
pub fn canonical_blake3_data_with_work_v1<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    value: &T,
    source: &A,
) -> Result<PaidCanonicalBlake3DataV1<A::Charge>, Error> {
    if !source.is_metered() {
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&value)
            .map_err(|_| Error::Invalid("cognition digest canonical payload"))?;
        return Ok(PaidCanonicalBlake3DataV1 {
            text: format!("blake3:{}", blake3::hash(&bytes)),
            _fee: None,
        });
    }
    let n = std::mem::size_of::<PaidCanonicalBlake3DataV1<A::Charge>>() + 71;
    source.charge_work(1, n as u64)?;
    let fee = source.reserve(n)?;

    struct Hash<'a, A> {
        source: &'a A,
        hash: blake3::Hasher,
        error: Option<Error>,
    }
    impl<A: CapabilityCanonicalWorkV1> Write for Hash<'_, A> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.error.is_some() {
                return Err(io::ErrorKind::Other.into());
            }
            for chunk in bytes.chunks(8192) {
                if let Err(error) = self.source.charge_work(1, chunk.len() as u64) {
                    self.error = Some(error);
                    return Err(io::ErrorKind::Other.into());
                }
                self.hash.update(chunk);
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    // Delay the stream-error hook until a writer failure has recovered its typed cause.
    // The allocator and codec loan remain the original caller's, with no fresh allowance.
    struct DeferredError<'a, A>(&'a A);
    impl<A: CapabilityCanonicalWorkV1> CanonicalCborAllocator for DeferredError<'_, A> {
        type Charge = A::Charge;
        fn reserve(&self, n: usize) -> Result<Self::Charge, Error> {
            self.0.reserve(n)
        }
        fn charge_work(&self, steps: u64, bytes: u64) -> Result<(), Error> {
            self.0.charge_work(steps, bytes)
        }
    }
    impl<A: CapabilityCanonicalWorkV1> CapabilityCanonicalWorkV1 for DeferredError<'_, A> {
        type Loan<'a>
            = A::Loan<'a>
        where
            Self: 'a;
        fn is_metered(&self) -> bool {
            self.0.is_metered()
        }
        fn loan_canonical_work(&self) -> Self::Loan<'_> {
            self.0.loan_canonical_work()
        }
    }
    let mut output = Hash {
        source,
        hash: blake3::Hasher::new(),
        error: None,
    };
    let result = stream_canonical_cbor_with_work_v1(value, &DeferredError(source), &mut output);
    if let Some(error) = output.error {
        return Err(source.record_stream_error(error));
    }
    result.map_err(|error| source.record_stream_error(error))?;
    source.charge_work(1, 71)?;
    let mut text = String::with_capacity(71);
    if text.capacity() > 71 {
        return Err(Error::ResourceLimited("cognition digest string capacity"));
    }
    text.push_str("blake3:");
    text.push_str(output.hash.finalize().to_hex().as_str());
    Ok(PaidCanonicalBlake3DataV1 {
        text,
        _fee: Some(fee),
    })
}

#[cfg(test)]
#[path = "canonical_digest_tests.rs"]
mod tests;
