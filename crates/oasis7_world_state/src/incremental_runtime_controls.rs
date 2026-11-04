//! Original Runtime persistent controls; no Runtime/World activation authority.
use crate::{
    incremental_field_member::*,
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControlReadV1 {
    Unit,
    Unsigned(u64),
    Text { utf8_bytes: u64 },
}
impl TryFrom<RecordValueV1> for RuntimeControlReadV1 {
    type Error = ValueTreeError;
    fn try_from(value: RecordValueV1) -> Result<Self, Self::Error> {
        match value {
            RecordValueV1::Unit => Ok(Self::Unit),
            RecordValueV1::Unsigned(value) => Ok(Self::Unsigned(value)),
            RecordValueV1::Text { utf8_bytes } => Ok(Self::Text { utf8_bytes }),
            _ => Err(invalid()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControlFieldV1 {
    TickConsensusAuthoritySource,
    NextGovernanceIdentityPenaltyId,
    GovernanceEmergencyBrakeUntilTick,
}
impl RuntimeControlFieldV1 {
    pub const ALL: [Self; 3] = [
        Self::TickConsensusAuthoritySource,
        Self::NextGovernanceIdentityPenaltyId,
        Self::GovernanceEmergencyBrakeUntilTick,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::TickConsensusAuthoritySource => "tick_consensus_authority_source",
            Self::NextGovernanceIdentityPenaltyId => "next_governance_identity_penalty_id",
            Self::GovernanceEmergencyBrakeUntilTick => "governance_emergency_brake_until_tick",
        }
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        if self == Self::NextGovernanceIdentityPenaltyId {
            StateNamespaceV1::Allocator
        } else {
            StateNamespaceV1::Authorization
        }
    }
    pub fn descriptor(self) -> FieldDescriptor {
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: self.name(),
            shape: FieldShape::Value,
            destination: FieldDestination::State(self.namespace()),
        }
    }
    fn nullable(self) -> bool {
        self == Self::GovernanceEmergencyBrakeUntilTick
    }
    fn codec(self) -> MemberPayloadCodecV1 {
        if self == Self::TickConsensusAuthoritySource {
            MemberPayloadCodecV1::Text
        } else {
            MemberPayloadCodecV1::U64
        }
    }
    fn validate(self, value: RuntimeControlValueRefV1<'_>) -> Result<(), ValueTreeError> {
        match (self, value) {
            (Self::TickConsensusAuthoritySource, RuntimeControlValueRefV1::Text(_)) => Ok(()),
            (Self::GovernanceEmergencyBrakeUntilTick, RuntimeControlValueRefV1::Absent) => Ok(()),
            (_, RuntimeControlValueRefV1::Unsigned(_))
                if self != Self::TickConsensusAuthoritySource =>
            {
                Ok(())
            }
            _ => Err(invalid()),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeControlValueRefV1<'a> {
    Absent,
    Unsigned(u64),
    Text(&'a str),
}
impl<'a> RuntimeControlValueRefV1<'a> {
    fn payload(self) -> MemberPayloadRefV1<'a> {
        match self {
            Self::Absent => MemberPayloadRefV1::Unit,
            Self::Unsigned(v) => MemberPayloadRefV1::U64(v),
            Self::Text(v) => MemberPayloadRefV1::Text(v),
        }
    }
}
pub struct RuntimeControlsRefV1<'a> {
    pub tick_consensus_authority_source: &'a str,
    pub next_governance_identity_penalty_id: u64,
    pub governance_emergency_brake_until_tick: Option<u64>,
}
impl RuntimeControlsRefV1<'_> {
    pub fn value(&self, field: RuntimeControlFieldV1) -> RuntimeControlValueRefV1<'_> {
        use RuntimeControlFieldV1 as F;
        use RuntimeControlValueRefV1 as V;
        match field {
            F::TickConsensusAuthoritySource => V::Text(self.tick_consensus_authority_source),
            F::NextGovernanceIdentityPenaltyId => {
                V::Unsigned(self.next_governance_identity_penalty_id)
            }
            F::GovernanceEmergencyBrakeUntilTick => self
                .governance_emergency_brake_until_tick
                .map_or(V::Absent, V::Unsigned),
        }
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime control profile")
}
pub fn prepare_runtime_controls_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &RuntimeControlsRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    records_remaining: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    for field in RuntimeControlFieldV1::ALL {
        let value = source.value(field);
        field.validate(value)?;
        for metadata in [true, false] {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            *records_remaining =
                records_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "Runtime control GEN records",
                    ))?;
            let (key, id) = if metadata {
                let row = write_field_metadata_v1(
                    field.descriptor(),
                    &mut PresenceRecordWriteContext {
                        sink: &mut *ctx.sink,
                        memory: ctx.memory,
                        io: &mut *ctx.io,
                        key_work: &mut *ctx.key_work,
                        limits: PresenceRecordLimitsV1 {
                            semantic_bytes: ctx.limits.semantic_bytes,
                            record_bytes: ctx.limits.record_bytes,
                        },
                        cancelled: &mut ctx.cancelled,
                    },
                )?;
                (row.field_key, row.value_root)
            } else {
                let row = write_member_record_v1(
                    field.descriptor(),
                    WorldFieldRecordKindV1::Entry,
                    &(),
                    value.payload(),
                    ctx,
                )?;
                (row.field_key, row.value_root)
            };
            crate::incremental_field_write::emit(key, id, ctx, sink)?;
        }
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
const LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "Runtime cursor UTF8",
    ledger_utf8: "unused Runtime ledger",
    ledger_frame: "unused Runtime ledger",
    boolean: "unused Runtime bool",
    fixed: "Runtime control fixed",
    profile: "Runtime control profile",
    integer: "Runtime control integer",
};
fn read_value<R: ValueNodeReader, C: FnMut() -> bool>(
    field: RuntimeControlFieldV1,
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_text_bytes: u64,
    output: &mut impl Write,
) -> Result<RecordValueV1, ValueTreeError> {
    records
        .values
        .verify_closure(id, records.record_bytes, &mut *records.cancelled)?;
    read_record_value_v1(
        id,
        key,
        records,
        if field == RuntimeControlFieldV1::TickConsensusAuthoritySource {
            max_text_bytes
        } else {
            8
        },
        output,
        RecordReadProfileV1 {
            codec: field.codec(),
            nullable: field.nullable(),
            integer_max: u64::MAX,
            labels: LABELS,
        },
    )
}

#[path = "incremental_runtime_controls_read.rs"]
mod read;
pub use read::read_runtime_control_v1;

#[path = "incremental_runtime_controls_overlay.rs"]
mod overlay;
pub use overlay::{
    RuntimeControlChangeV1, change_runtime_control_overlay_v1, read_runtime_control_overlay_v1,
};

#[path = "incremental_runtime_controls_family.rs"]
mod family;
pub use family::{
    RuntimeControlsQualificationV1, accept_runtime_controls_transition_v1,
    qualify_runtime_controls_v1, readmit_runtime_controls_ordinary_v1,
};

#[cfg(test)]
#[path = "incremental_runtime_controls_tests.rs"]
mod tests;

#[path = "incremental_runtime_controls_pages.rs"]
mod pages;
pub use pages::*;
