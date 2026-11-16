//! Original service signing image with actual continuing canonical work. This
//! module chooses the codec/domain; callbacks can supply only work/allocation.
use super::*;
use oasis7_wasm_abi::{
    CanonicalCborStreamError, CapabilityCanonicalWorkV1, stream_canonical_cbor_with_work_v1,
};
use std::{
    io::{self, Write},
    mem::size_of,
};
pub struct PaidServiceSigningBytesV1<C> {
    pub(in crate::world_service) bytes: Vec<u8>,
    pub(in crate::world_service) fee: Option<C>,
}
impl<C> PaidServiceSigningBytesV1<C> {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
pub struct PaidServiceProtocolRefusalV1<C> {
    pub(crate) message: String,
    pub(crate) fee: Option<C>,
}
impl<C> PaidServiceProtocolRefusalV1<C> {
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl<C> std::fmt::Debug for PaidServiceProtocolRefusalV1<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PaidServiceProtocolRefusalV1")
            .field(&self.message)
            .finish()
    }
}
#[derive(Debug)]
pub enum ServiceAuthorityWorkErrorV1<C> {
    Work(CanonicalCborStreamError),
    Protocol(PaidServiceProtocolRefusalV1<C>),
}
impl<C> std::fmt::Display for ServiceAuthorityWorkErrorV1<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Work(e) => e.fmt(f),
            Self::Protocol(e) => e.message.fmt(f),
        }
    }
}
impl<C: std::fmt::Debug> std::error::Error for ServiceAuthorityWorkErrorV1<C> {}
pub(crate) fn refusal<A: CapabilityCanonicalWorkV1>(
    source: &A,
    message: &str,
) -> Result<PaidServiceProtocolRefusalV1<A::Charge>, ServiceAuthorityWorkErrorV1<A::Charge>> {
    source
        .charge_work(1, message.len() as u64)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    let fee = if source.is_metered() {
        Some(
            source
                .reserve(
                    size_of::<ServiceAuthorityWorkErrorV1<A::Charge>>()
                        .max(size_of::<PaidServiceProtocolRefusalV1<A::Charge>>())
                        + message.len(),
                )
                .map_err(ServiceAuthorityWorkErrorV1::Work)?,
        )
    } else {
        None
    };
    Ok(PaidServiceProtocolRefusalV1 {
        message: message.to_owned(),
        fee,
    })
}
pub(crate) fn legacy_error(error: ServiceAuthorityWorkErrorV1<()>) -> String {
    match error {
        ServiceAuthorityWorkErrorV1::Protocol(error) => {
            debug_assert!(error.fee.is_none());
            error.message
        }
        ServiceAuthorityWorkErrorV1::Work(error) => error.to_string(),
    }
}
struct Count {
    n: usize,
}
impl Write for Count {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.n = self
            .n
            .checked_add(b.len())
            .ok_or(io::ErrorKind::OutOfMemory)?;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Output<'a, A> {
    source: &'a A,
    bytes: Vec<u8>,
    bound: usize,
    first: Option<CanonicalCborStreamError>,
}
impl<A: CapabilityCanonicalWorkV1> Write for Output<'_, A> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.first.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        for part in b.chunks(8192) {
            if let Err(error) = self.source.charge_work(1, part.len() as u64) {
                self.first = Some(error);
                return Err(io::ErrorKind::Other.into());
            }
            let Some(n) = self
                .bytes
                .len()
                .checked_add(part.len())
                .filter(|n| *n <= self.bound)
            else {
                self.first = Some(CanonicalCborStreamError::Invalid(
                    "service signing image length changed",
                ));
                return Err(io::ErrorKind::InvalidData.into());
            };
            self.bytes.extend_from_slice(part);
            debug_assert_eq!(self.bytes.len(), n);
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub fn signing_bytes_with_work_v1<T: Serialize, A: CapabilityCanonicalWorkV1>(
    domain: &str,
    value: &T,
    source: &A,
) -> Result<PaidServiceSigningBytesV1<A::Charge>, ServiceAuthorityWorkErrorV1<A::Charge>> {
    let image = ("oasis7.world-service.v1", domain, value);
    if !source.is_metered() {
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&image).map_err(|error| {
            ServiceAuthorityWorkErrorV1::Protocol(PaidServiceProtocolRefusalV1 {
                message: error.to_string(),
                fee: None,
            })
        })?;
        return Ok(PaidServiceSigningBytesV1 { bytes, fee: None });
    }
    let mut count = Count { n: 0 };
    stream_canonical_cbor_with_work_v1(&image, source, &mut count)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    let n = size_of::<PaidServiceSigningBytesV1<A::Charge>>()
        .checked_add(count.n)
        .ok_or(ServiceAuthorityWorkErrorV1::Work(
            CanonicalCborStreamError::ResourceLimited("service signing bytes layout"),
        ))?;
    source
        .charge_work(1, n as u64)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    let fee = source
        .reserve(n)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    let bytes = Vec::with_capacity(count.n);
    if bytes.capacity() != count.n {
        return Err(ServiceAuthorityWorkErrorV1::Work(
            CanonicalCborStreamError::ResourceLimited("service signing bytes exact capacity"),
        ));
    }
    let mut out = Output {
        source,
        bytes,
        bound: count.n,
        first: None,
    };
    let result = stream_canonical_cbor_with_work_v1(&image, source, &mut out);
    if let Some(error) = out.first {
        return Err(ServiceAuthorityWorkErrorV1::Work(error));
    }
    result.map_err(ServiceAuthorityWorkErrorV1::Work)?;
    source
        .charge_work(0, 0)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    if out.bytes.len() != count.n {
        return Err(ServiceAuthorityWorkErrorV1::Work(
            CanonicalCborStreamError::Invalid("service signing image length changed"),
        ));
    }
    Ok(PaidServiceSigningBytesV1 {
        bytes: out.bytes,
        fee: Some(fee),
    })
}
pub struct PaidServiceRequestDigestV1<C> {
    pub(in crate::world_service) text: String,
    pub(in crate::world_service) fee: Option<C>,
}
impl<C> PaidServiceRequestDigestV1<C> {
    pub fn as_str(&self) -> &str {
        &self.text
    }
}
pub fn request_digest_with_work_v1<T: Serialize, A: CapabilityCanonicalWorkV1>(
    domain: &str,
    value: &T,
    source: &A,
) -> Result<PaidServiceRequestDigestV1<A::Charge>, ServiceAuthorityWorkErrorV1<A::Charge>> {
    let image = signing_bytes_with_work_v1(domain, value, source)?;
    source
        .charge_work(1, 0)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    let mut hash = blake3::Hasher::new();
    for part in image.bytes.chunks(8192) {
        source
            .charge_work(1 + (part.len() as u64).div_ceil(64), part.len() as u64)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        hash.update(part);
    }
    source
        .charge_work(1, 32)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    let digest = hash.finalize();
    let n = size_of::<PaidServiceRequestDigestV1<A::Charge>>() + 71;
    source
        .charge_work(1, n as u64)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    let fee = if source.is_metered() {
        Some(
            source
                .reserve(n)
                .map_err(ServiceAuthorityWorkErrorV1::Work)?,
        )
    } else {
        None
    };
    let mut text = String::with_capacity(71);
    if text.capacity() != 71 {
        return Err(ServiceAuthorityWorkErrorV1::Work(
            CanonicalCborStreamError::ResourceLimited("service digest exact capacity"),
        ));
    }
    text.push_str("blake3:");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest.as_bytes() {
        source
            .charge_work(1, 2)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        text.push(HEX[(byte >> 4) as usize] as char);
        text.push(HEX[(byte & 15) as usize] as char)
    }
    source
        .charge_work(0, 0)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    Ok(PaidServiceRequestDigestV1 { text, fee })
}

struct PaidFixedV1<const N: usize, C> {
    bytes: [u8; N],
    _fee: Option<C>,
}
fn decode_fixed_with_work_v1<const N: usize, A: CapabilityCanonicalWorkV1>(
    value: &str,
    source: &A,
) -> Result<PaidFixedV1<N, A::Charge>, ServiceAuthorityWorkErrorV1<A::Charge>> {
    let n = (value.len() / 2)
        .checked_add(size_of::<PaidFixedV1<N, A::Charge>>() + 1024)
        .ok_or(ServiceAuthorityWorkErrorV1::Work(
            CanonicalCborStreamError::ResourceLimited("service signature decode layout"),
        ))?;
    for part in value.as_bytes().chunks(8192) {
        source
            .charge_work(1 + part.len() as u64, part.len() as u64)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    }
    let fee = if source.is_metered() {
        Some(
            source
                .reserve(n)
                .map_err(ServiceAuthorityWorkErrorV1::Work)?,
        )
    } else {
        None
    };
    let bytes = match hex::decode(value) {
        Ok(v) => v,
        Err(error) => {
            return Err(ServiceAuthorityWorkErrorV1::Protocol(
                PaidServiceProtocolRefusalV1 {
                    message: error.to_string(),
                    fee,
                },
            ));
        }
    };
    let bytes = match bytes.try_into() {
        Ok(v) => v,
        Err(_) => {
            return Err(ServiceAuthorityWorkErrorV1::Protocol(
                PaidServiceProtocolRefusalV1 {
                    message: "invalid key or signature length".to_owned(),
                    fee,
                },
            ));
        }
    };
    Ok(PaidFixedV1 { bytes, _fee: fee })
}
pub(crate) fn verify_bytes_with_work_v1<A: CapabilityCanonicalWorkV1>(
    key: &str,
    signature: &str,
    bytes: &[u8],
    source: &A,
) -> Result<(), ServiceAuthorityWorkErrorV1<A::Charge>> {
    let public = decode_fixed_with_work_v1::<32, _>(key, source)?;
    source
        .charge_work(1, 32)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    let fee = if source.is_metered() {
        Some(
            source
                .reserve(size_of::<PaidServiceProtocolRefusalV1<A::Charge>>() + 1024)
                .map_err(ServiceAuthorityWorkErrorV1::Work)?,
        )
    } else {
        None
    };
    let key = match VerifyingKey::from_bytes(&public.bytes) {
        Ok(v) => v,
        Err(error) => {
            return Err(ServiceAuthorityWorkErrorV1::Protocol(
                PaidServiceProtocolRefusalV1 {
                    message: error.to_string(),
                    fee,
                },
            ));
        }
    };
    let signature = decode_fixed_with_work_v1::<64, _>(signature, source)?;
    source
        .charge_work(
            1 + (bytes.len() as u64).div_ceil(128),
            bytes.len() as u64 + 96,
        )
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    match key.verify(bytes, &Signature::from_bytes(&signature.bytes)) {
        Ok(()) => Ok(()),
        Err(error) => Err(ServiceAuthorityWorkErrorV1::Protocol(
            PaidServiceProtocolRefusalV1 {
                message: error.to_string(),
                fee,
            },
        )),
    }
}
pub fn verify_read_request_with_work_v1<T: Serialize, A: CapabilityCanonicalWorkV1>(
    domain: &str,
    request: &SignedReadRequest<T>,
    source: &A,
) -> Result<(), ServiceAuthorityWorkErrorV1<A::Charge>> {
    let image = signing_bytes_with_work_v1(domain, &request.request, source)?;
    verify_bytes_with_work_v1(
        &request.subject_public_key,
        &request.signature_hex,
        image.bytes(),
        source,
    )
}
