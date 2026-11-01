//! Bounded streaming reads of the unique pre-existing agent-scalar.v1 codec.
use super::*;
use crate::incremental_utf8_output::Utf8Output;
use oasis7_formal_protocol::incremental_value::{
    ValueNodeReader, ValueReadContext, ValueTreeError,
};
use std::io::{Cursor, Write};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSubtreeValueV1 {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I64(i64),
    Bool(bool),
    Text { utf8_bytes: u64 },
    OptionalText(Option<u64>),
    OptionalU64(Option<u64>),
}
/// Reads one explicitly requested scalar, preserving None and Some(empty/zero).
/// Caller output remains provisional until all framing/UTF-8/cancellation checks pass.
/// This authenticates the value only; it does not mint owner qualification.
pub fn read_agent_scalar_tree_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    field: AgentFieldV1,
    id: &[u8; 32],
    context: &mut ValueReadContext<'_, R>,
    max_scalar_bytes: u64,
    output: &mut impl Write,
    cancelled: &mut C,
) -> Result<AgentSubtreeValueV1, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let _scratch = context.memory.reserve(8192, context.class)?;
    let root = context.load_root(id)?;
    if root.0.logical_len > max_scalar_bytes {
        return Err(ValueTreeError::ResourceLimited("agent scalar bytes"));
    }
    let prefix = AGENT_SCALAR_DOMAIN_V1.len() + 1;
    if root.0.logical_len < prefix as u64 {
        return Err(ValueTreeError::Invalid("agent scalar header"));
    }
    let mut header = [0u8; 64];
    context.read_range(
        &root,
        0,
        prefix as u64,
        &mut Cursor::new(&mut header[..prefix]),
        &mut *cancelled,
    )?;
    if &header[..prefix - 1] != AGENT_SCALAR_DOMAIN_V1
        || header[prefix - 1] != field.kind().wire_tag()
    {
        return Err(ValueTreeError::Invalid("agent scalar domain/type"));
    }
    let value = if matches!(
        field.kind(),
        ScalarKindV1::Text | ScalarKindV1::OptionalText
    ) {
        let optional = field.kind() == ScalarKindV1::OptionalText;
        let mut start = prefix;
        if optional {
            context.read_range(
                &root,
                start as u64,
                1,
                &mut Cursor::new(&mut header[start..start + 1]),
                &mut *cancelled,
            )?;
            start += 1;
            match header[start - 1] {
                0 => {
                    if root.0.logical_len != start as u64 {
                        return Err(ValueTreeError::Invalid("agent optional trailing"));
                    }
                    if cancelled() {
                        return Err(ValueTreeError::Cancelled);
                    }
                    return Ok(AgentSubtreeValueV1::OptionalText(None));
                }
                1 => {}
                _ => return Err(ValueTreeError::Invalid("agent optional flag")),
            }
        }
        context.read_range(
            &root,
            start as u64,
            8,
            &mut Cursor::new(&mut header[start..start + 8]),
            &mut *cancelled,
        )?;
        let length = u64::from_be_bytes(header[start..start + 8].try_into().unwrap());
        start += 8;
        if (start as u64).checked_add(length) != Some(root.0.logical_len) {
            return Err(ValueTreeError::Invalid("agent text length"));
        }
        let mut utf8 = Utf8Output::new(output, "agent scalar UTF-8");
        let result = context.read_range(&root, start as u64, length, &mut utf8, &mut *cancelled);
        if utf8.is_invalid() {
            return Err(ValueTreeError::Invalid("agent scalar UTF-8"));
        }
        result?;
        if !utf8.is_complete() {
            return Err(ValueTreeError::Invalid("agent scalar UTF-8"));
        }
        if optional {
            AgentSubtreeValueV1::OptionalText(Some(length))
        } else {
            AgentSubtreeValueV1::Text { utf8_bytes: length }
        }
    } else {
        let length = usize::try_from(root.0.logical_len)
            .ok()
            .filter(|n| *n <= header.len())
            .ok_or(ValueTreeError::Invalid("agent fixed scalar length"))?;
        context.read_range(
            &root,
            0,
            length as u64,
            &mut Cursor::new(&mut header[..length]),
            &mut *cancelled,
        )?;
        match AgentScalarRefV1::from_canonical_bytes(&header[..length], field)
            .map_err(ValueTreeError::Invalid)?
        {
            AgentScalarRefV1::U8(v) => AgentSubtreeValueV1::U8(v),
            AgentScalarRefV1::U16(v) => AgentSubtreeValueV1::U16(v),
            AgentScalarRefV1::U32(v) => AgentSubtreeValueV1::U32(v),
            AgentScalarRefV1::U64(v) => AgentSubtreeValueV1::U64(v),
            AgentScalarRefV1::I64(v) => AgentSubtreeValueV1::I64(v),
            AgentScalarRefV1::Bool(v) => AgentSubtreeValueV1::Bool(v),
            AgentScalarRefV1::OptionalU64(v) => AgentSubtreeValueV1::OptionalU64(v),
            _ => return Err(ValueTreeError::Invalid("agent scalar profile")),
        }
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(value)
}

/// One original scalar address. The ordinal is the persisted stable position,
/// not an instruction to re-enumerate or normalize a current DTO collection.
#[derive(Clone, Copy)]
pub struct AgentSubtreeScalarRequestV1<'a> {
    pub agent: &'a str,
    pub field: AgentFieldV1,
    pub ordinal: u64,
}
/// Integrity-only raw view for an explicitly selected RootSet. Missing is a
/// missing leaf, never a statement that an Agent or required group is absent.
/// Structural/history qualification remains a separate capability.
pub fn read_agent_subtree_scalar_raw_v1<
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    request: AgentSubtreeScalarRequestV1<'_>,
    context: &mut crate::incremental_field_read::WorldFieldReadContextV1<'_, M, R, E, C>,
    max_scalar_bytes: u64,
    output: &mut impl Write,
) -> Result<Option<AgentSubtreeValueV1>, ValueTreeError> {
    context.check()?;
    roots
        .validate()
        .map_err(|_| ValueTreeError::Invalid("agent root set"))?;
    if !roots.state.iter().any(|root| root == context.root) {
        return Err(ValueTreeError::Invalid("agent read root binding"));
    }
    if request.agent.len() as u64 > context.semantic_bytes {
        return Err(ValueTreeError::ResourceLimited("agent identity allowance"));
    }
    let _scratch = context.values.memory.reserve(8192, context.values.class)?;
    let work = (AGENT_FIELD_KEY_BYTES as u64)
        .checked_add(request.agent.len() as u64)
        .ok_or(ValueTreeError::ResourceLimited("agent identity work"))?;
    context.key_work.bytes_remaining = context
        .key_work
        .bytes_remaining
        .checked_sub(work)
        .ok_or(ValueTreeError::ResourceLimited("agent identity work"))?;
    let key = AgentFieldKeyV1::new(request.agent, request.field, request.ordinal)
        .map_err(ValueTreeError::Invalid)?;
    let value = context.lookup_raw(roots.state_root(request.field.namespace()), key.as_bytes())?;
    let decoded = value
        .map(|id| {
            read_agent_scalar_tree_v1(
                request.field,
                &id,
                &mut context.values,
                max_scalar_bytes,
                output,
                &mut context.cancelled,
            )
        })
        .transpose()?;
    context.check()?;
    Ok(decoded)
}

pub(super) fn fixed_tree<const N: usize, R: ValueNodeReader, C: FnMut() -> bool>(
    id: &[u8; 32],
    context: &mut ValueReadContext<'_, R>,
    cancelled: &mut C,
) -> Result<[u8; N], ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let _scratch = context.memory.reserve(8192, context.class)?;
    let root = context.load_root(id)?;
    if root.0.logical_len != N as u64 {
        return Err(ValueTreeError::Invalid("agent fixed value length"));
    }
    let mut bytes = [0u8; N];
    context.read_range(
        &root,
        0,
        N as u64,
        &mut Cursor::new(&mut bytes[..]),
        &mut *cancelled,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(bytes)
}
