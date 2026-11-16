//! Runtime's private original-payload projection. The shared generic wire and
//! predicates remain unique; side packets are deferred with their real owner.
use oasis7_client_api::consensus_action_wire::{
    self as wire, ConsensusPayloadBodyViewV1, ConsensusPayloadDecodeOpsV1,
    DecodedConsensusPayloadV1, RuntimePayloadProjectionV1 as Envelope,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_cbor_data::*,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use oasis7_world_state::{
    Action,
    incremental_pending_action_family::{
        PendingActionOwnedLayoutV1, pending_action_owned_decode_layout_v1,
    },
};
use std::borrow::Borrow;

type Body = wire::ConsensusActionPayloadBody<
    wire::IgnoredSidePayloadV1,
    wire::IgnoredSidePayloadV1,
    wire::IgnoredSidePayloadV1,
    wire::IgnoredSidePayloadV1,
>;
enum Stop {
    Resource(BoundedCborDataErrorV1),
    Side(PaidCborDataV1<Envelope>),
}
impl From<BoundedCborDataErrorV1> for Stop {
    fn from(error: BoundedCborDataErrorV1) -> Self {
        Self::Resource(error)
    }
}
impl From<oasis7_formal_protocol::controlled_authority::formal::FormalError> for Stop {
    fn from(error: oasis7_formal_protocol::controlled_authority::formal::FormalError) -> Self {
        Self::Resource(error.into())
    }
}
fn limit() -> BoundedCborDataErrorV1 {
    BoundedCborDataErrorV1::Limit("normal Runtime payload resource bound")
}
fn to_client(value: PendingActionOwnedLayoutV1) -> wire::ConsensusPayloadOwnedLayoutV1 {
    let PendingActionOwnedLayoutV1 {
        root,
        sequence_element,
        map_pair,
        set_element,
        box_pointee,
        default_heap_per_value,
        symbols_bytes,
        visits,
        missing,
    } = value;
    wire::ConsensusPayloadOwnedLayoutV1 {
        root,
        sequence_element,
        map_pair,
        set_element,
        box_pointee,
        default_heap_per_value,
        symbols_bytes,
        visits,
        missing,
    }
}
fn to_formal(value: wire::ConsensusPayloadOwnedLayoutV1) -> CborDataLayoutV1 {
    let wire::ConsensusPayloadOwnedLayoutV1 {
        root,
        sequence_element,
        map_pair,
        set_element,
        box_pointee,
        default_heap_per_value,
        symbols_bytes,
        visits,
        missing,
    } = value;
    CborDataLayoutV1 {
        root,
        sequence_element,
        map_pair,
        set_element,
        box_pointee,
        default_heap_per_value,
        symbols_bytes,
        visits,
        missing,
    }
}
fn value_layout() -> CborDataLayoutV1 {
    CborDataLayoutV1 {
        root: std::mem::size_of::<serde_cbor::Value>(),
        sequence_element: std::mem::size_of::<serde_cbor::Value>(),
        map_pair: std::mem::size_of::<(serde_cbor::Value, serde_cbor::Value)>(),
        set_element: 0,
        box_pointee: std::mem::size_of::<serde_cbor::Value>(),
        default_heap_per_value: 0,
        symbols_bytes: 0,
        visits: 1,
        missing: None,
    }
}
struct RuntimeEnvelopeMove;
impl CborDataMoveWrapV1<Action> for RuntimeEnvelopeMove {
    type Output = Envelope;
    fn wrap_data(action: Action) -> Envelope {
        Envelope::from_runtime_action(action)
    }
}
struct Ops<'a, 'c> {
    common: &'a mut FormalBoundedContextV1<'c>,
    layout: CborDataLayoutV1,
    limits: &'a CborDataLimitsV1,
    error_fees: [Option<MemoryReservation>; 4],
}
impl Ops<'_, '_> {
    fn codec_note(&mut self, refusal: PaidCborRefusalV1) -> Result<String, Stop> {
        refusal.validate_context(self.common)?;
        let note = refusal.message();
        self.common.charge(1, note.len() as u64)?;
        let fee = self
            .common
            .memory()
            .reserve(
                note.len() + std::mem::size_of::<String>(),
                MemoryClass::Ordinary,
            )
            .map_err(BoundedCborDataErrorV1::from)?;
        let copy = note.to_owned();
        if copy.capacity() > note.len() {
            return Err(limit().into());
        }
        let slot = self.error_fees.iter_mut().find(|fee| fee.is_none()).ok_or(
            BoundedCborDataErrorV1::Invalid("normal Runtime codec error count"),
        )?;
        *slot = Some(fee);
        Ok(copy)
    }
    fn decode<T: serde::de::DeserializeOwned>(
        &mut self,
        raw: &[u8],
        layout: CborDataLayoutV1,
    ) -> Result<Result<PaidCborDataV1<T>, String>, Stop> {
        match decode_closed_cbor_data_v1(raw, layout, self.limits, self.common)? {
            Ok(data) => Ok(Ok(data)),
            Err(refusal) => Ok(Err(self.codec_note(refusal)?)),
        }
    }
}
impl ConsensusPayloadDecodeOpsV1<Body> for Ops<'_, '_> {
    type Error = Stop;
    type Value = PaidCborDataV1<serde_cbor::Value>;
    type Envelope = PaidCborDataV1<Envelope>;
    type RuntimeAction = PaidCborDataV1<Action>;
    type Encoded = PaidCborDataV1<Vec<u8>>;
    fn decode_value(&mut self, raw: &[u8]) -> Result<Result<Self::Value, String>, Stop> {
        self.decode(raw, value_layout())
    }
    fn decode_envelope(&mut self, raw: &[u8]) -> Result<Result<Self::Envelope, String>, Stop> {
        match self.decode::<Envelope>(raw, self.layout)? {
            Ok(owner) => {
                let envelope: &Envelope = owner.borrow();
                if !matches!(envelope.body.runtime_action(),Some(action) if !matches!(action,Action::WorldServiceIntent{..}))
                {
                    Err(Stop::Side(owner))
                } else {
                    Ok(Ok(owner))
                }
            }
            Err(note) => Ok(Err(note)),
        }
    }
    fn decode_runtime(&mut self, raw: &[u8]) -> Result<Result<Self::RuntimeAction, String>, Stop> {
        self.decode(raw, self.layout)
    }
    fn encode_envelope(&mut self, value: &Envelope) -> Result<Result<Self::Encoded, String>, Stop> {
        match encode_closed_cbor_data_v1(value, self.limits, self.common)? {
            Ok(data) => Ok(Ok(data)),
            Err(refusal) => Ok(Err(self.codec_note(refusal)?)),
        }
    }
}
enum PayloadData {
    Runtime(PaidCborDataV1<Envelope>),
    Side(PaidCborDataV1<Envelope>),
    Refusal(String),
}
/// Private constructor, no Clone or bare DTO extraction. These are original
/// DATA classifications, never a processed-input or execution qualification.
pub(in crate::world) struct PaidRuntimeInputPayloadV1 {
    data: PayloadData,
    core: WorkIdentityV1,
    _kernel: MemoryReservation,
    _errors: [Option<MemoryReservation>; 4],
}
pub(in crate::world) enum RuntimePayloadConsumeErrorV1 {
    Context {
        error: BoundedCborDataErrorV1,
        source: PaidRuntimeInputPayloadV1,
    },
    Side(PaidRuntimeInputPayloadV1),
    Refusal(PaidRuntimeInputPayloadV1),
    Resource(BoundedCborDataErrorV1),
}
impl PaidRuntimeInputPayloadV1 {
    pub(in crate::world) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BoundedCborDataErrorV1> {
        common.consume_steps(1)?;
        if self.core != common.work().identity()
            || !self._kernel.uses_budget(common.memory())
            || self
                ._errors
                .iter()
                .flatten()
                .any(|fee| !fee.uses_budget(common.memory()))
        {
            return Err(BoundedCborDataErrorV1::Invalid(
                "normal Runtime payload context",
            ));
        }
        match &self.data {
            PayloadData::Runtime(owner) | PayloadData::Side(owner) => {
                owner.validate_context(common)
            }
            PayloadData::Refusal(_) => Ok(()),
        }
    }
    pub(in crate::world) fn refusal(&self) -> Option<&str> {
        match &self.data {
            PayloadData::Refusal(note) => Some(note),
            _ => None,
        }
    }
    pub(in crate::world) fn with_runtime_data<T>(
        self,
        common: &mut FormalBoundedContextV1<'_>,
        consume: impl FnOnce(Envelope, &mut FormalBoundedContextV1<'_>) -> T,
    ) -> Result<T, RuntimePayloadConsumeErrorV1> {
        if let Err(error) = self.validate_context(common) {
            return Err(RuntimePayloadConsumeErrorV1::Context {
                error,
                source: self,
            });
        }
        match &self.data {
            PayloadData::Side(_) => return Err(RuntimePayloadConsumeErrorV1::Side(self)),
            PayloadData::Refusal(_) => return Err(RuntimePayloadConsumeErrorV1::Refusal(self)),
            _ => {}
        }
        let Self {
            data,
            core: _,
            _kernel,
            _errors,
        } = self;
        let PayloadData::Runtime(owner) = data else {
            unreachable!("private classified Runtime source")
        };
        let result = owner
            .with_data(common, consume)
            .map_err(RuntimePayloadConsumeErrorV1::Resource);
        drop(_errors);
        drop(_kernel);
        result
    }
}
/// Always fed from the real bound input's original payload by the fixed driver.
/// Non-Runtime bodies retain a deferred owner for their full typed consumer;
/// skipping them here never accepts or executes their side payload.
pub(in crate::world) fn decode_runtime_input_payload_data_v1(
    raw: &[u8],
    limits: &CborDataLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidRuntimeInputPayloadV1, BoundedCborDataErrorV1> {
    common.consume_steps(1)?;
    let runtime = pending_action_owned_decode_layout_v1(&mut || common.consume_steps(1))?;
    let layout = to_formal(wire::runtime_payload_owned_layout_v1(
        to_client(runtime),
        &mut || common.consume_steps(1),
    )?);
    if layout.missing.is_some() {
        return Err(BoundedCborDataErrorV1::Invalid(
            "normal Runtime closed layout missing",
        ));
    }
    let kernel_bytes = raw
        .len()
        .checked_mul(32)
        .and_then(|n| {
            layout
                .symbols_bytes
                .checked_mul(8)
                .and_then(|m| n.checked_add(m))
        })
        .and_then(|n| n.checked_add(4096 + std::mem::size_of::<PaidRuntimeInputPayloadV1>()))
        .ok_or_else(limit)?;
    common.charge(1, kernel_bytes as u64)?;
    let kernel = common
        .memory()
        .reserve(kernel_bytes, MemoryClass::Ordinary)
        .map_err(BoundedCborDataErrorV1::from)?;
    let mut ops = Ops {
        common,
        layout,
        limits,
        error_fees: std::array::from_fn(|_| None),
    };
    let decoded = wire::decode_consensus_action_payload_envelope_with(raw, &mut ops);
    let data = match decoded {
        Err(Stop::Resource(error)) => return Err(error),
        Err(Stop::Side(owner)) => PayloadData::Side(owner),
        Ok(Err(note)) => PayloadData::Refusal(note),
        Ok(Ok(owner)) => {
            let owner = match owner {
                DecodedConsensusPayloadV1::Envelope(owner) => owner,
                DecodedConsensusPayloadV1::LegacyRuntime(owner) => {
                    owner.map_move::<RuntimeEnvelopeMove>(ops.common)?
                }
            };
            let envelope: &Envelope = owner.borrow();
            let count = match &envelope.auth {
                Some(wire::ConsensusActionAuthEnvelope::GovernanceAction(proof)) => {
                    proof.signatures.len()
                }
                _ => 0,
            };
            let steps = count
                .checked_mul(8)
                .and_then(|n| n.checked_add(64))
                .ok_or_else(limit)?;
            ops.common.charge(
                steps as u64,
                raw.len().checked_mul(8).ok_or_else(limit)? as u64,
            )?;
            match wire::validate_committed_payload_metadata_v1(envelope) {
                Ok(()) => {
                    if matches!(
                        envelope.body.runtime_action(),
                        Some(Action::WorldServiceIntent { .. })
                    ) {
                        PayloadData::Side(owner)
                    } else {
                        PayloadData::Runtime(owner)
                    }
                }
                Err(note) => PayloadData::Refusal(note),
            }
        }
    };
    ops.common.consume_steps(0)?;
    Ok(PaidRuntimeInputPayloadV1 {
        data,
        core: ops.common.work().identity(),
        _kernel: kernel,
        _errors: ops.error_fees,
    })
}

/// Validate the complete original reserved-input list before any ordinary
/// payload decode or queue mutation, preserving the original first-error cut.
/// Returned text is DATA and remains covered by its own reservation.
pub(in crate::world) fn validate_bound_reserved_input_data_v1(
    input: &oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1<'_>,
    limits: &CborDataLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Result<(), oasis7_proto::memory_budget::ReservedValue<String>>, BoundedCborDataErrorV1>
{
    use oasis7_consensus::{
        node_consensus_action::NodeConsensusAction,
        replicated_execution_input::{
            NodeReplicatedExecutionInputV1 as NodeInput, ReplicatedExecutionInputValidationOpsV1,
            decode_replicated_execution_input_action_with,
            validate_replicated_execution_input_actions_with,
        },
    };
    input.validate_context(common)?;
    struct ReservedOps<'a, 'c> {
        common: &'a mut FormalBoundedContextV1<'c>,
        limits: &'a CborDataLimitsV1,
    }
    impl ReplicatedExecutionInputValidationOpsV1 for ReservedOps<'_, '_> {
        type Owner = PaidCborDataV1<NodeInput>;
        type Fatal = BoundedCborDataErrorV1;
        fn before_action(&mut self, action: &NodeConsensusAction) -> Result<(), Self::Fatal> {
            let bytes = action
                .submitter_player_id
                .len()
                .checked_add(action.payload_cbor.len())
                .and_then(|n| n.checked_add(128))
                .ok_or_else(limit)?;
            self.common.charge(4, bytes as u64)?;
            Ok(())
        }
        fn decode_action(
            &mut self,
            action: &NodeConsensusAction,
        ) -> Result<Result<Option<Self::Owner>, String>, Self::Fatal> {
            let layout = CborDataLayoutV1 {
                root: std::mem::size_of::<NodeInput>(),
                sequence_element: 1,
                map_pair: 0,
                set_element: 0,
                box_pointee: 0,
                default_heap_per_value: 0,
                symbols_bytes: 96,
                visits: 1,
                missing: None,
            };
            decode_replicated_execution_input_action_with(action, |raw| {
                match decode_closed_cbor_data_v1::<NodeInput>(
                    raw,
                    layout,
                    self.limits,
                    self.common,
                )? {
                    Ok(owner) => Ok(Ok(owner)),
                    Err(refusal) => {
                        refusal.validate_context(self.common)?;
                        Ok(Err(()))
                    }
                }
            })
        }
    }
    // Original shared predicates may format the actual submitter in their first
    // error. Reserve this bound before those predicates allocate any message.
    let mut max_note = 1024usize;
    for action in &input.context().committed_actions {
        common.consume_steps(1)?;
        max_note = max_note.max(
            action
                .submitter_player_id
                .len()
                .checked_mul(2)
                .and_then(|n| n.checked_add(4096))
                .ok_or_else(limit)?,
        );
    }
    common.charge(1, max_note as u64)?;
    let note_layout = max_note
        .checked_add(std::mem::size_of::<String>())
        .ok_or_else(limit)?;
    let fee = common
        .memory()
        .reserve(note_layout, MemoryClass::Ordinary)?;
    let mut ops = ReservedOps { common, limits };
    let result = validate_replicated_execution_input_actions_with(
        &input.context().committed_actions,
        input.context().height,
        &mut ops,
    )?;
    ops.common.consume_steps(0)?;
    match result {
        Ok(()) => Ok(Ok(())),
        Err(note) => {
            if note.capacity() > max_note {
                return Err(limit());
            }
            Ok(Err(fee.retain(note)))
        }
    }
}

#[cfg(test)]
#[path = "incremental_normal_input_decode_tests.rs"]
mod tests;

impl std::fmt::Debug for RuntimePayloadConsumeErrorV1 {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        match self {
            Self::Context{error,..}=>f.debug_tuple("Context").field(error).finish(),
            Self::Resource(error)=>f.debug_tuple("Resource").field(error).finish(),
            Self::Side(_)=>f.write_str("UnappliedSideInput"),
            Self::Refusal(_)=>f.write_str("PayloadRefusal"),
        }
    }
}
