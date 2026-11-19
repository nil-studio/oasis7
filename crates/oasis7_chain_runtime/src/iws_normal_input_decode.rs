//! Paid DATA decoding of one original consensus payload. The fixed caller must
//! first classify replicated bootstrap inputs using the original Node grammar.
//! Nothing here applies input or creates a normal execution permission.
pub(crate) use cbor::{
    BoundedCborDataErrorV1 as NormalInputDecodeErrorV1,
    CborDataLimitsV1 as NormalInputDecodeLimitsV1,
};
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadEnvelope, ConsensusPayloadDecodeOpsV1, DecodedConsensusPayloadV1,
    decode_consensus_action_payload_envelope_with,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::{
    self as cbor, PaidCborDataV1 as PaidDecodeValueV1,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, WorkIdentityV1,
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use oasis7_world_state::incremental_pending_action_family::PendingActionOwnedLayoutV1;
use serde::de::DeserializeOwned;
use std::{borrow::Borrow, fmt};
fn limited() -> NormalInputDecodeErrorV1 {
    NormalInputDecodeErrorV1::Limit("normal payload layout/declared hint")
}
/// Keeps the genuine decoded graph or original codec refusal and all still-live
/// error-string charges. No public constructor, Clone or detached DTO getter.
pub(crate) struct PaidNormalPayloadV1 {
    result: Result<PaidDecodeValueV1<ConsensusActionPayloadEnvelope>, String>,
    core: WorkIdentityV1,
    metadata_refusal: bool,
    _kernel: MemoryReservation,
    _errors: [Option<MemoryReservation>; 5],
}
impl PaidNormalPayloadV1 {
    pub(crate) fn is_metadata_refusal(&self) -> bool {
        self.metadata_refusal
    }
    pub(crate) fn envelope(&self) -> Result<&ConsensusActionPayloadEnvelope, &str> {
        self.result
            .as_ref()
            .map(|v| v.borrow())
            .map_err(String::as_str)
    }
    pub(crate) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), NormalInputDecodeErrorV1> {
        common.consume_steps(1)?;
        if self.core != common.work().identity()
            || !self._kernel.uses_budget(common.memory())
            || self
                ._errors
                .iter()
                .flatten()
                .any(|fee| !fee.uses_budget(common.memory()))
        {
            return Err(NormalInputDecodeErrorV1::Invalid(
                "normal payload owner context",
            ));
        }
        if let Ok(owner) = &self.result {
            owner.validate_context(common)?
        }
        Ok(())
    }
    /// DATA consumption only. The fixed callback must produce independently
    /// admitted outputs rather than return or stash the decoded graph. All input
    /// fees remain live through its success, error return and unwinding.
    pub(crate) fn with_envelope_data<T>(
        self,
        common: &mut FormalBoundedContextV1<'_>,
        consume: impl FnOnce(ConsensusActionPayloadEnvelope, &mut FormalBoundedContextV1<'_>) -> T,
    ) -> Result<T, NormalPayloadConsumeErrorV1> {
        if let Err(error) = self.validate_context(common) {
            return Err(NormalPayloadConsumeErrorV1::Context {
                error,
                source: self,
            });
        }
        if self.result.is_err() {
            return Err(NormalPayloadConsumeErrorV1::Codec(self));
        }
        let Self {
            result,
            core: _,
            metadata_refusal: _,
            _kernel,
            _errors,
        } = self;
        let Ok(owner) = result else {
            unreachable!("checked owned result above")
        };
        let result = owner
            .with_data(common, consume)
            .map_err(|error| NormalPayloadConsumeErrorV1::Resource(error))?;
        drop((_kernel, _errors));
        Ok(result)
    }
}
pub(crate) enum NormalPayloadConsumeErrorV1 {
    Resource(NormalInputDecodeErrorV1),
    Context {
        error: NormalInputDecodeErrorV1,
        source: PaidNormalPayloadV1,
    },
    Codec(PaidNormalPayloadV1),
}
impl fmt::Debug for NormalPayloadConsumeErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource(error) => error.fmt(f),
            Self::Context { error, .. } => f.debug_tuple("Context").field(error).finish(),
            Self::Codec(_) => f.write_str("Codec(original paid refusal)"),
        }
    }
}

fn value_layout() -> PendingActionOwnedLayoutV1 {
    let mut l = PendingActionOwnedLayoutV1::scalar::<serde_cbor::Value>();
    l.sequence_element = std::mem::size_of::<serde_cbor::Value>();
    l.map_pair = std::mem::size_of::<(serde_cbor::Value, serde_cbor::Value)>();
    l.box_pointee = std::mem::size_of::<serde_cbor::Value>();
    l
}
struct PaidOps<'a, 'ctx> {
    common: &'a mut FormalBoundedContextV1<'ctx>,
    limits: &'a NormalInputDecodeLimitsV1,
    layout: PendingActionOwnedLayoutV1,
    raw_len: usize,
    errors: [Option<MemoryReservation>; 5],
}
impl PaidOps<'_, '_> {
    fn decode<T: DeserializeOwned>(
        &mut self,
        raw: &[u8],
        layout: PendingActionOwnedLayoutV1,
    ) -> Result<Result<PaidDecodeValueV1<T>, String>, NormalInputDecodeErrorV1> {
        match cbor::decode_closed_cbor_data_v1(raw, layout.into(), self.limits, self.common)? {
            Ok(v) => Ok(Ok(v)),
            Err(refusal) => {
                let note = self.error(format_args!("{}", refusal.message()))?;
                Ok(Err(note))
            }
        }
    }
    fn error(&mut self, args: fmt::Arguments<'_>) -> Result<String, NormalInputDecodeErrorV1> {
        struct Count(usize);
        impl fmt::Write for Count {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                self.0 = self.0.checked_add(s.len()).ok_or(fmt::Error)?;
                Ok(())
            }
        }
        let mut count = Count(0);
        fmt::write(&mut count, args).map_err(|_| limited())?;
        self.common
            .charge(1, u64::try_from(count.0).map_err(|_| limited())?)?;
        let bytes = count
            .0
            .checked_add(std::mem::size_of::<String>())
            .ok_or_else(limited)?;
        let fee = self.common.memory().reserve(bytes, MemoryClass::Ordinary)?;
        let mut output = String::with_capacity(count.0);
        fmt::write(&mut output, args).map_err(|_| limited())?;
        if output.capacity() > count.0 {
            return Err(limited());
        }
        let slot = self.errors.iter_mut().find(|x| x.is_none()).ok_or(
            NormalInputDecodeErrorV1::Invalid("payload error carrier count"),
        )?;
        *slot = Some(fee);
        Ok(output)
    }
    fn encode(
        &mut self,
        value: &ConsensusActionPayloadEnvelope,
    ) -> Result<Result<PaidDecodeValueV1<Vec<u8>>, String>, NormalInputDecodeErrorV1> {
        match cbor::encode_closed_cbor_data_v1(value, self.limits, self.common)? {
            Ok(v) => {
                self.common.charge(
                    1,
                    u64::try_from(
                        v.as_ref()
                            .len()
                            .checked_add(self.raw_len)
                            .ok_or_else(limited)?,
                    )
                    .map_err(|_| limited())?,
                )?;
                Ok(Ok(v))
            }
            Err(refusal) => {
                let note = self.error(format_args!(
                    "encode consensus action payload envelope failed: {}",
                    refusal.message()
                ))?;
                Ok(Err(note))
            }
        }
    }
}
struct RuntimeEnvelopeMove;
impl cbor::CborDataMoveWrapV1<oasis7_runtime::Action> for RuntimeEnvelopeMove {
    type Output = ConsensusActionPayloadEnvelope;
    fn wrap_data(value: oasis7_runtime::Action) -> Self::Output {
        ConsensusActionPayloadEnvelope::from_runtime_action(value)
    }
}
impl ConsensusPayloadDecodeOpsV1 for PaidOps<'_, '_> {
    type Error = NormalInputDecodeErrorV1;
    type Value = PaidDecodeValueV1<serde_cbor::Value>;
    type Envelope = PaidDecodeValueV1<ConsensusActionPayloadEnvelope>;
    type RuntimeAction = PaidDecodeValueV1<oasis7_runtime::Action>;
    type Encoded = PaidDecodeValueV1<Vec<u8>>;
    fn decode_value(&mut self, raw: &[u8]) -> Result<Result<Self::Value, String>, Self::Error> {
        self.decode(raw, value_layout())
    }
    fn decode_envelope(
        &mut self,
        raw: &[u8],
    ) -> Result<Result<Self::Envelope, String>, Self::Error> {
        self.decode(raw, self.layout)
    }
    fn decode_runtime(
        &mut self,
        raw: &[u8],
    ) -> Result<Result<Self::RuntimeAction, String>, Self::Error> {
        self.decode(raw, self.layout)
    }
    fn encode_envelope(
        &mut self,
        value: &ConsensusActionPayloadEnvelope,
    ) -> Result<Result<Self::Encoded, String>, Self::Error> {
        self.encode(value)
    }
}
/// Decode only after actual Node bootstrap classification. Caller keeps this
/// paid owner through use of every borrowed Action/auth/origin field.
pub(crate) fn decode_normal_payload_data_v1(
    raw: &[u8],
    limits: &NormalInputDecodeLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidNormalPayloadV1, NormalInputDecodeErrorV1> {
    common.consume_steps(1)?;
    if raw.len() > limits.max_wire_bytes {
        return Err(limited());
    }
    let layout = oasis7::consensus_action_payload::consensus_payload_owned_layout_v1(&mut || {
        common.consume_steps(1)
    })?;
    if layout.missing.is_some() {
        return Err(NormalInputDecodeErrorV1::Invalid(
            "normal payload closed layout missing",
        ));
    }
    // This covers the grammar's bounded key probes and final formatted refusal;
    // failed full decoder graphs are dropped separately after each attempt.
    let kernel_bytes = raw
        .len()
        .checked_mul(32)
        .and_then(|n| {
            layout
                .symbols_bytes
                .checked_mul(8)
                .and_then(|m| n.checked_add(m))
        })
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(limited)?;
    common.charge(1, u64::try_from(kernel_bytes).map_err(|_| limited())?)?;
    let kernel = common
        .memory()
        .reserve(kernel_bytes, MemoryClass::Ordinary)?;
    let mut ops = PaidOps {
        common,
        limits,
        layout,
        raw_len: raw.len(),
        errors: std::array::from_fn(|_| None),
    };
    let decoded = decode_consensus_action_payload_envelope_with(raw, &mut ops)?;
    let decoded = match decoded {
        Err(e) => Err(e),
        Ok(DecodedConsensusPayloadV1::Envelope(v)) => Ok(v),
        Ok(DecodedConsensusPayloadV1::LegacyRuntime(v)) => {
            Ok(v.map_move::<RuntimeEnvelopeMove>(ops.common)?)
        }
    };
    let mut metadata_refusal = false;
    let result = match decoded {
        Err(error) => Err(error),
        Ok(owner) => {
            // All required governance strings came from this genuine decoded raw
            // source. Charge the original pure loops before traversing any signature.
            let visits = match &Borrow::<ConsensusActionPayloadEnvelope>::borrow(&owner).auth {
                Some(
                    oasis7::consensus_action_payload::ConsensusActionAuthEnvelope::GovernanceAction(
                        proof,
                    ),
                ) => proof
                    .signatures
                    .len()
                    .checked_mul(8)
                    .and_then(|n| n.checked_add(64))
                    .ok_or_else(limited)?,
                _ => 8,
            };
            let bytes = ops.raw_len.checked_mul(8).ok_or_else(limited)?;
            ops.common.charge(
                u64::try_from(visits).map_err(|_| limited())?,
                u64::try_from(bytes).map_err(|_| limited())?,
            )?;
            match crate::committed_input_metadata::validate_committed_payload_metadata_v1(
                owner.borrow(),
            ) {
                Ok(()) => Ok(owner),
                Err(error) => {
                    metadata_refusal = true;
                    Err(error)
                }
            }
        }
    };
    ops.common.consume_steps(0)?;
    let core = ops.common.work().identity();
    Ok(PaidNormalPayloadV1 {
        result,
        core,
        metadata_refusal,
        _kernel: kernel,
        _errors: ops.errors,
    })
}
#[cfg(test)]
#[path = "iws_normal_input_decode_tests.rs"]
mod tests;

#[path = "iws_normal_input_decode_replicated.rs"]
mod replicated;
pub(crate) use replicated::{PaidNodeReplicatedInputV1, decode_paid_replicated_input_codec_v1};
