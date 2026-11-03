//! Explicit complete Option replacement; scalar point edits remain independent.
use super::*;
use crate::{
    agent_cell::AgentActivityV1,
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_value_mutation::ValueMutationContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[derive(Clone, Copy)]
pub enum AgentSubtreeOptionalRefV1<'a> {
    Activity(Option<&'a AgentActivityV1>),
    Intent(Option<&'a crate::AgentIntentV2>),
}
impl<'a> AgentSubtreeOptionalRefV1<'a> {
    fn flag(self) -> AgentFieldV1 {
        match self {
            Self::Activity(_) => AgentFieldV1::ActivityPresent,
            Self::Intent(_) => AgentFieldV1::IntentPresent,
        }
    }
    fn present(self) -> bool {
        match self {
            Self::Activity(value) => value.is_some(),
            Self::Intent(value) => value.is_some(),
        }
    }
    fn group(self) -> schema::Group {
        match self {
            Self::Activity(_) => schema::Group::Activity,
            Self::Intent(_) => schema::Group::Intent,
        }
    }
    fn columns(self, agent: &str) -> Result<[Option<AgentScalarRefV1<'a>>; 53], ValueTreeError> {
        let mut columns = [None; 53];
        let mut collect = |field: AgentFieldV1, value| -> Result<(), ValueTreeError> {
            field
                .validate_value(value)
                .map_err(ValueTreeError::Invalid)?;
            columns[(field as u16 - 1) as usize] = Some(value);
            Ok(())
        };
        match self {
            Self::Activity(Some(value)) => visit_agent_activity_values_v1(value, &mut collect)?,
            Self::Intent(Some(value)) => {
                if value.agent_id != agent {
                    return Err(invalid());
                }
                visit_agent_intent_values_v1(value, &mut collect)?;
            }
            Self::Activity(None) | Self::Intent(None) => (),
        }
        Ok(columns)
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("agent optional expected/binding")
}
pub struct AgentSubtreeOptionalChangeV1<'a> {
    pub agent: &'a str,
    pub qualification: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub expected: AgentSubtreeOptionalRefV1<'a>,
    pub after: AgentSubtreeOptionalRefV1<'a>,
    pub max_scalar_bytes: u64,
}
/// This operation explicitly requests comparison/replacement of the complete
/// bounded Activity or Intent group. Unrelated body/rows/mailbox are not scanned.
/// Partial group failures poison the enclosing transaction, including its prefix.
pub fn change_agent_subtree_optional_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: AgentSubtreeOptionalChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut ValueMutationContextV1<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !change.qualification.uses_budget(context.values.memory)
            || change.expected.flag() != change.after.flag()
        {
            return Err(invalid().into());
        }
        let _scratch = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let expected = change.expected.columns(change.agent)?;
        let after = change.after.columns(change.agent)?;
        let flag = change.expected.flag();
        let query = |field| AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent: change.agent,
                field,
                ordinal: 0,
            },
            qualification: change.qualification,
        };
        {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let resolved = point::resolve(
                query(flag),
                tx,
                &mut records,
                error,
                64,
                &mut std::io::sink(),
            )?;
            if !resolved.agent_exists
                || resolved.value != Some(AgentSubtreeValueV1::Bool(change.expected.present()))
            {
                return Err(invalid().into());
            }
            for field in AgentFieldV1::ALL
                .iter()
                .copied()
                .filter(|f| schema::group(*f) == change.expected.group())
            {
                let index = (field as u16 - 1) as usize;
                let mut compare = Compare::new(point::text(expected[index]).as_bytes());
                let resolved = point::resolve(
                    query(field),
                    tx,
                    &mut records,
                    error,
                    change.max_scalar_bytes,
                    &mut compare,
                )?;
                if !point::matches(expected[index], resolved.value) || !compare.is_equal() {
                    return Err(invalid().into());
                }
            }
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut changed = false;
        for field in AgentFieldV1::ALL
            .iter()
            .copied()
            .filter(|f| schema::group(*f) == change.expected.group())
        {
            let index = (field as u16 - 1) as usize;
            if expected[index] == after[index] {
                continue;
            }
            let key =
                AgentFieldKeyV1::new(change.agent, field, 0).map_err(ValueTreeError::Invalid)?;
            let digest = if let Some(value) = after[index] {
                if value.encoded_len().map_err(ValueTreeError::Invalid)? > change.max_scalar_bytes {
                    return Err(ValueTreeError::ResourceLimited("agent scalar bytes").into());
                }
                let mut combined = ValueMutationContextV1 {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                Some(write_agent_scalar_stream_v1(value, &mut combined)?.digest()?)
            } else {
                None
            };
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            tx.write(field.namespace(), key.as_bytes(), digest)?;
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            changed = true;
        }
        if change.expected.present() != change.after.present() {
            let mut combined = ValueMutationContextV1 {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                cancelled: || cancel() || (write.cancelled)(),
            };
            let value = write_agent_scalar_stream_v1(
                AgentScalarRefV1::Bool(change.after.present()),
                &mut combined,
            )?;
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let key =
                AgentFieldKeyV1::new(change.agent, flag, 0).map_err(ValueTreeError::Invalid)?;
            tx.write(flag.namespace(), key.as_bytes(), Some(value.digest()?))?;
            changed = true;
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(changed)
    })
}
