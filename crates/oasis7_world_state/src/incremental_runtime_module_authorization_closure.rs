use super::{read::*, *};
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_semantic_key::SEMANTIC_KEY_DOMAIN,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{
    cell::RefCell,
    io::{Cursor, Read},
};
const GRANT_FIXED: &[&str] = &["name", "expiry", "effect_kinds.length"];
const RULE_FIXED: &[&str] = &[
    "when.effect_kind",
    "when.origin_kind",
    "when.cap_name",
    "decision.kind",
    "decision.reason",
];
fn fixed(field: ModuleAuthorizationFieldV1) -> &'static [&'static str] {
    match field {
        ModuleAuthorizationFieldV1::Capabilities => GRANT_FIXED,
        ModuleAuthorizationFieldV1::PolicyRules => RULE_FIXED,
    }
}
fn uint(bytes: &mut &[u8]) -> Result<u64, ValueTreeError> {
    if bytes.first() != Some(&2) {
        return Err(invalid());
    }
    *bytes = &bytes[1..];
    let mut value = [0; 8];
    bytes.read_exact(&mut value).map_err(|_| invalid())?;
    Ok(u64::from_be_bytes(value))
}
fn tag(bytes: &mut &[u8], expected: u8) -> Result<(), ValueTreeError> {
    if bytes.first() != Some(&expected) {
        return Err(invalid());
    }
    *bytes = &bytes[1..];
    Ok(())
}
fn text<'a>(bytes: &mut &'a [u8]) -> Result<&'a str, ValueTreeError> {
    tag(bytes, 1)?;
    let mut length = [0; 8];
    bytes.read_exact(&mut length).map_err(|_| invalid())?;
    let n = usize::try_from(u64::from_be_bytes(length)).map_err(|_| invalid())?;
    let value = bytes.get(..n).ok_or_else(invalid)?;
    *bytes = &bytes[n..];
    std::str::from_utf8(value).map_err(|_| invalid())
}
fn address(
    field: ModuleAuthorizationFieldV1,
    bytes: &[u8],
) -> Result<ModuleAuthorizationAddressV1, ValueTreeError> {
    let mut input = bytes
        .strip_prefix(SEMANTIC_KEY_DOMAIN)
        .ok_or_else(invalid)?;
    tag(&mut input, 5)?;
    tag(&mut input, 5)?;
    if text(&mut input)? != field.label() {
        return Err(invalid());
    }
    let name = text(&mut input)?;
    let name = fixed(field)
        .iter()
        .copied()
        .chain(std::iter::once("effect_kinds"))
        .find(|candidate| *candidate == name)
        .ok_or_else(invalid)?;
    tag(&mut input, 5)?;
    let present = uint(&mut input)?;
    let index = uint(&mut input)?;
    let index = match present {
        0 if index == 0 => None,
        1 => Some(index),
        _ => return Err(invalid()),
    };
    if !input.is_empty() {
        return Err(invalid());
    }
    let address = ModuleAuthorizationAddressV1 {
        column: name,
        index,
    };
    column(field, address).ok_or_else(invalid)?;
    Ok(address)
}
fn actual<R: ValueNodeReader, C: FnMut() -> bool>(
    field: ModuleAuthorizationFieldV1,
    entry: &StateEntryV1,
    length: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(), ValueTreeError> {
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let expected = WorldFieldKeyV1::new(
        genesis::body_descriptor(),
        binding.kind,
        binding.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if entry.namespace != StateNamespaceV1::Authorization || entry.key != expected.as_bytes() {
        return Err(invalid());
    }
    if binding.kind == WorldFieldRecordKindV1::Metadata {
        return Ok(());
    }
    let marker = records.key(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(),
    )?;
    if marker.0 == expected {
        return Ok(());
    }
    // Relative body keys contain only closed labels and a fixed U64 ordinal.
    // No original long cap_ref is materialized or copied into each child.
    let mut bytes = [0; 256];
    let mut output = Cursor::new(bytes.as_mut_slice());
    let result = read_member_record_key_v1(
        &entry.value,
        &expected,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes.min(256),
            payload_bytes: u64::MAX,
            record_bytes: records.record_bytes,
        },
        &mut output,
        &mut *records.cancelled,
    )?;
    let n = output.position() as usize;
    if result.semantic_bytes != n as u64 {
        return Err(invalid());
    }
    let address = address(field, &bytes[..n])?;
    let key = read::key(field, address, records)?;
    if key.0 != expected
        || key.1 != result.semantic_bytes
        || address.index.is_some_and(|i| i >= length)
    {
        return Err(invalid());
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
/// Full local child closure, with fixed-size working memory. This does not
/// certify an outer cap_ref/ordinal or authorize an effect by itself.
pub fn qualify_module_authorization_body_v1<M, R, E, C>(
    field: ModuleAuthorizationFieldV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ModuleAuthorizationReadWorkV1,
) -> Result<ReservedValue<ModuleAuthorizationBodyQualificationV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if ctx.root.namespace != StateNamespaceV1::Authorization {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _scratch = ctx.values.memory.reserve(32 << 10, ctx.values.class)?;
    work.cell()?;
    let meta = WorldFieldKeyV1::new(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let id = ctx.lookup(&meta)?.ok_or_else(invalid)?;
    ctx.records().presence(&id, &meta, 0)?;
    work.cell()?;
    let marker = ctx.records().key(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(),
    )?;
    let id = ctx.lookup(&marker.0)?.ok_or_else(invalid)?;
    let mut label = [0; 12];
    let mut output = Cursor::new(label.as_mut_slice());
    let info = read_member_record_profile_v1(
        &id,
        &marker.0,
        &mut ctx.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Text,
            semantic_bytes: marker.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: ctx.semantic_bytes,
                payload_bytes: 12,
                record_bytes: ctx.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut output,
        &mut ctx.cancelled,
    )?;
    if &label[..info.payload_bytes as usize] != field.label().as_bytes() {
        return Err(invalid());
    }
    let mut length = 0;
    let mut decision = None;
    let mut reason = false;
    for name in fixed(field) {
        let value = read::cell(
            field,
            ModuleAuthorizationAddressV1::fixed(name),
            ctx,
            work,
            &mut std::io::sink(),
        )?;
        match (*name, value) {
            ("effect_kinds.length", ModuleAuthorizationValueV1::Unsigned(n)) => length = n,
            ("decision.kind", ModuleAuthorizationValueV1::Label(kind)) => decision = Some(kind),
            ("decision.reason", ModuleAuthorizationValueV1::Text { .. }) => reason = true,
            ("decision.reason", ModuleAuthorizationValueV1::Unit) => {}
            _ => {}
        }
    }
    if field == ModuleAuthorizationFieldV1::PolicyRules && ((decision == Some("deny")) != reason) {
        return Err(invalid());
    }
    for index in 0..length {
        read::cell(
            field,
            ModuleAuthorizationAddressV1 {
                column: "effect_kinds",
                index: Some(index),
            },
            ctx,
            work,
            &mut std::io::sink(),
        )?;
    }
    let expected = length
        .checked_add(fixed(field).len() as u64 + 2)
        .ok_or_else(invalid)?;
    if ctx.root.leaf_count != expected {
        return Err(invalid());
    }
    let root = ctx.root.clone();
    let mut count = 0u64;
    let result = {
        let memory = ctx.values.memory;
        let class = ctx.values.class;
        let cancel = RefCell::new(&mut ctx.cancelled);
        let mut stop = || (cancel.borrow_mut())();
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut stop,
        };
        StateScanContext {
            source: ctx.state,
            memory,
            class,
            io: scan,
        }
        .visit_entries(
            &root,
            |entry| {
                work.cell()?;
                count = count.checked_add(1).ok_or_else(invalid)?;
                actual(field, entry, length, &mut records)
            },
            || (cancel.borrow_mut())(),
        )
    };
    let source_error = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(error) = source_error {
        return Err(error);
    }
    result.map_err(|error| match error {
        StateScanError::Visitor(e) => e,
        StateScanError::Cancelled => ValueTreeError::Cancelled,
        StateScanError::Memory(e) => ValueTreeError::Memory(e),
        StateScanError::Invalid(_) | StateScanError::Tree(_) => invalid(),
        StateScanError::ResourceLimited => {
            ValueTreeError::ResourceLimited("module authorization scan")
        }
    })?;
    if count != expected {
        return Err(invalid());
    }
    ctx.check()?;
    Ok(retained.retain(ModuleAuthorizationBodyQualificationV1 {
        root,
        field,
        class: ctx.values.class,
    }))
}
