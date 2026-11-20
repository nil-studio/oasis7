//! Paid adapter over the one native descriptor codec and predicate kernel.
use super::*;
use crate::controlled_authority::formal::{
    FormalError,
    bounded_cbor_data::{
        self as cbor, BoundedCborDataErrorV1, CborDataLayoutV1, CborDataLimitsV1, PaidCborDataV1,
    },
    bounded_context::FormalBoundedContextV1,
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
fn invalid(n: &'static str) -> FormalError {
    FormalError::Invalid(n.into())
}
fn error(e: BoundedCborDataErrorV1) -> FormalError {
    match e {
        BoundedCborDataErrorV1::Common(e) => e,
        BoundedCborDataErrorV1::Memory(_) => invalid("native descriptor graph admission"),
        BoundedCborDataErrorV1::Invalid(n) | BoundedCborDataErrorV1::Limit(n) => invalid(n),
    }
}
pub(crate) enum NativeCodecRefusal {
    Shared(cbor::PaidCborRefusalV1),
    Local {
        note: String,
        _fee: MemoryReservation,
    },
}
impl NativeCodecRefusal {
    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Shared(v) => v.message(),
            Self::Local { note, .. } => note,
        }
    }
}
pub(crate) enum NativeBoundedError {
    Resource(FormalError),
    Codec(NativeCodecRefusal),
}
impl From<FormalError> for NativeBoundedError {
    fn from(e: FormalError) -> Self {
        Self::Resource(e)
    }
}
struct Paid<'a, 'c> {
    ctx: &'a mut FormalBoundedContextV1<'c>,
    names: Option<MemoryReservation>,
}
fn limits() -> CborDataLimitsV1 {
    CborDataLimitsV1 {
        max_wire_bytes: MAX_NATIVE_DESCRIPTOR_BYTES,
        max_graph_bytes: usize::MAX,
        max_depth: 128,
    }
}
impl NativeValidationOps for Paid<'_, '_> {
    type Error = NativeBoundedError;
    type Decoded = PaidCborDataV1<NativeExecutionEnvironmentV1>;
    type Encoded = PaidCborDataV1<Vec<u8>>;
    fn error(&mut self, n: &'static str) -> Self::Error {
        if let Err(e) = self.ctx.charge(1, n.len() as u64) {
            return NativeBoundedError::Resource(e);
        }
        match self.ctx.memory().reserve(
            n.len() + std::mem::size_of::<String>(),
            MemoryClass::Ordinary,
        ) {
            Ok(fee) => NativeBoundedError::Codec(NativeCodecRefusal::Local {
                note: n.to_owned(),
                _fee: fee,
            }),
            Err(_) => NativeBoundedError::Resource(invalid("native error admission")),
        }
    }
    fn work(&mut self, n: usize) -> Result<(), Self::Error> {
        self.ctx.charge(1, n as u64).map_err(Into::into)
    }
    fn before_names(&mut self, n: usize) -> Result<(), Self::Error> {
        self.work(n * std::mem::size_of::<&String>())?;
        let bytes = n
            .checked_mul(
                11 * std::mem::size_of::<&String>() + 14 * std::mem::size_of::<usize>() + 128,
            )
            .ok_or_else(|| invalid("native names layout"))?;
        self.names = Some(
            self.ctx
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(|_| invalid("native names graph admission"))?,
        );
        Ok(())
    }
    fn decode(&mut self, b: &[u8]) -> Result<Self::Decoded, Self::Error> {
        // Complete closed wire: root has platform Strings and Vec<Component>;
        // Component embeds role/name/digest/Vec<Chunk>; Chunk embeds digest/String.
        // Custom bounded_vec codecs remain the original decoder, not a new parser.
        let layout = CborDataLayoutV1 {
            root: std::mem::size_of::<NativeExecutionEnvironmentV1>(),
            sequence_element: std::mem::size_of::<NativeEnvironmentComponentV1>()
                .max(std::mem::size_of::<NativeEnvironmentChunkV1>()),
            map_pair: 0,
            set_element: 0,
            box_pointee: 0,
            default_heap_per_value: 0,
            symbols_bytes: 0,
            visits: 1,
            missing: None,
        };
        match cbor::decode_closed_cbor_data_v1(b, layout, &limits(), self.ctx).map_err(error)? {
            Ok(v) => Ok(v),
            Err(note) => Err(NativeBoundedError::Codec(NativeCodecRefusal::Shared(note))),
        }
    }
    fn encode(&mut self, v: &NativeExecutionEnvironmentV1) -> Result<Self::Encoded, Self::Error> {
        let mut caps = limits();
        caps.max_wire_bytes = usize::MAX;
        match cbor::encode_closed_cbor_data_v1(v, &caps, self.ctx).map_err(error)? {
            Ok(v) => Ok(v),
            Err(note) => Err(NativeBoundedError::Codec(NativeCodecRefusal::Shared(note))),
        }
    }
}
pub(crate) fn decode_native_environment_bounded(
    raw: &[u8],
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidCborDataV1<NativeExecutionEnvironmentV1>, NativeBoundedError> {
    NativeExecutionEnvironmentV1::decode_with(raw, &mut Paid { ctx, names: None })
}
