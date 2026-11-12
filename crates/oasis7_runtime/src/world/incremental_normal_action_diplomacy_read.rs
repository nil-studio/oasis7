//! Complete selected Alliance decode from its genuine Current child.
//! The outer map key remains separate from the stored alliance_id.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    AllianceState, incremental_diplomacy::*, incremental_field_read::WorldFieldReadContextV1,
};
use std::{
    io::{self, Write},
    mem::size_of,
};
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Current Alliance fixed record")
}
fn limit() -> BodyMutationError {
    ValueTreeError::ResourceLimited("Current Alliance record layout").into()
}
fn charge(ctx: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    ctx.charge(1, u64::try_from(n).map_err(|_| limit())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
struct TextOutput<'a, 'b> {
    bytes: Vec<u8>,
    cap: usize,
    common: &'a mut FormalBoundedContextV1<'b>,
    first: Option<BodyMutationError>,
}
impl Write for TextOutput<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Err(e) = charge(self.common, bytes.len()) {
            self.first = Some(e);
            return Err(io::Error::other("Alliance text work"));
        }
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.cap)
        {
            self.first = Some(limit());
            return Err(io::Error::other("Alliance text bound"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn text_output<'a, 'b>(
    cap: usize,
    common: &'a mut FormalBoundedContextV1<'b>,
) -> Result<TextOutput<'a, 'b>, BodyMutationError> {
    charge(common, 0)?;
    let bytes = Vec::with_capacity(cap);
    if bytes.capacity() > cap {
        return Err(limit());
    }
    Ok(TextOutput {
        bytes,
        cap,
        common,
        first: None,
    })
}
fn text(
    output: TextOutput<'_, '_>,
    result: Result<Option<DiplomacyValueV1>, ValueTreeError>,
) -> Result<String, BodyMutationError> {
    if let Some(e) = output.first {
        return Err(e);
    }
    match result? {
        Some(DiplomacyValueV1::Text { .. }) => String::from_utf8(output.bytes).map_err(|_| bad()),
        _ => Err(bad()),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn read_alliance_v1<P, V, E, C>(
    id: &str,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    max_text: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<AllianceState>, BodyMutationError>
where
    P: MerkleNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    if !proof.uses_budget(common.memory())
        || !read.values.memory.same_pool(common.memory())
        || read.values.class != MemoryClass::Ordinary
    {
        return Err(bad());
    }
    let mut ctx = WorldFieldReadContextV1 {
        root: proof.root(),
        state: read.state,
        values: ValueReadContext {
            source: read.values.source,
            memory: read.values.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &mut read.cancelled,
    };
    charge(common, id.len())?;
    let length =
        read_qualified_diplomacy_length_v1(DiplomacyFieldV1::Alliances, id, proof, &mut ctx)?;
    let n = usize::try_from(length).map_err(|_| limit())?;
    // Three stored scalar strings and every actual member retain their payloads;
    // one scratch text buffer is transient in the numeric scalar read.
    let layout = n
        .checked_add(4)
        .and_then(|v| v.checked_mul(max_text))
        .and_then(|v| v.checked_add(n.checked_mul(size_of::<String>())?))
        .and_then(|v| v.checked_add(size_of::<AllianceState>() + size_of::<TextOutput<'_, '_>>()))
        .ok_or_else(limit)?;
    let fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut scalar = |column| -> Result<(Option<DiplomacyValueV1>, String), BodyMutationError> {
        charge(common, id.len())?;
        let mut output = text_output(max_text, common)?;
        let result = read_qualified_diplomacy_scalar_v1(
            id,
            DiplomacyFieldV1::Alliances,
            DiplomacyScalarV1::Alliance(column),
            proof,
            &mut ctx,
            max_text as u64,
            &mut output,
        );
        if let Some(e) = output.first {
            return Err(e);
        }
        let value = result?;
        let bytes = String::from_utf8(output.bytes).map_err(|_| bad())?;
        Ok((value, bytes))
    };
    fn scalar_text(pair: (Option<DiplomacyValueV1>, String)) -> Result<String, BodyMutationError> {
        match pair {
            (Some(DiplomacyValueV1::Text { .. }), s) => Ok(s),
            _ => Err(bad()),
        }
    }
    let alliance_id = scalar_text(scalar(AllianceScalarV1::AllianceId)?)?;
    let charter = scalar_text(scalar(AllianceScalarV1::Charter)?)?;
    let formed_by_agent_id = scalar_text(scalar(AllianceScalarV1::FormedByAgentId)?)?;
    let formed_at = match scalar(AllianceScalarV1::FormedAt)?.0 {
        Some(DiplomacyValueV1::Unsigned(n)) => n,
        _ => return Err(bad()),
    };
    drop(scalar);
    charge(
        common,
        n.checked_mul(size_of::<String>()).ok_or_else(limit)?,
    )?;
    let mut members = Vec::with_capacity(n);
    if members.capacity() > n {
        return Err(limit());
    }
    for ordinal in 0..length {
        charge(common, id.len())?;
        let mut output = text_output(max_text, common)?;
        let result = read_qualified_diplomacy_member_v1(
            DiplomacyMemberReadV1 {
                field: DiplomacyFieldV1::Alliances,
                id,
                ordinal,
                column: DiplomacyMemberColumnV1::AllianceMember,
                qualification: proof,
                max_payload_bytes: max_text as u64,
            },
            &mut ctx,
            &mut output,
        );
        members.push(text(output, result)?);
    }
    Ok(fee.retain(AllianceState {
        alliance_id,
        members,
        charter,
        formed_by_agent_id,
        formed_at,
    }))
}
