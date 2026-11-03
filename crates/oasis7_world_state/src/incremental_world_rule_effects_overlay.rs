//! Point contents in the existing optional rule parent; no parent creation.
use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_world_rule_parent::read_parent_overlay,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::MemoryClass;

fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &str,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<(WorldFieldKeyV1, WorldRuleEffectValueV1), WorldFieldOverlayErrorV1> {
    let (field, present) = read_parent_overlay(transaction, records, error)?;
    let (key, bytes) = records.key(field, WorldFieldRecordKindV1::Entry, &(CHILD, key))?;
    let root = lookup(transaction, &key, records, error)?;
    if !present {
        if root.is_some() {
            return Err(ValueTreeError::Invalid("orphan world rule effect").into());
        }
        return Ok((key, WorldRuleEffectValueV1::RulesAbsent));
    }
    let Some(root) = root else {
        return Ok((key, WorldRuleEffectValueV1::EffectAbsent));
    };
    let length = read_text_value(
        &root,
        &key,
        records.values,
        profile(bytes, max_payload_bytes, records.record_bytes),
        output,
        UTF8_ERROR,
        records.cancelled,
    )?;
    Ok((key, WorldRuleEffectValueV1::Value(length)))
}
/// A read authenticates Metadata, parent presence and the requested member in
/// one contribution. Any error poisons the complete outer transaction.
pub fn read_world_rule_effect_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &str,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<WorldRuleEffectValueV1, WorldFieldOverlayErrorV1> {
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        if records.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(records.values.memory)
        {
            return Err(ValueTreeError::Invalid("world rule effect overlay pool/class").into());
        }
        resolve(
            key,
            transaction,
            &mut records,
            error,
            max_payload_bytes,
            output,
        )
        .map(|(_, value)| value)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldRuleEffectExpectedV1<'a> {
    RulesAbsent,
    EffectAbsent,
    Value(&'a str),
}
impl<'a> WorldRuleEffectExpectedV1<'a> {
    fn text(self) -> Option<&'a str> {
        match self {
            Self::Value(value) => Some(value),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct WorldRuleEffectChangeV1<'a> {
    pub key: &'a str,
    pub expected: WorldRuleEffectExpectedV1<'a>,
    pub after: Option<&'a str>,
    pub max_current_payload_bytes: u64,
}
/// Caller supplies the original owner decision and admits borrowed strings.
/// The optional parent remains immutable here; None cannot be partially created.
pub fn change_world_rule_effect_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: WorldRuleEffectChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let WorldRuleEffectChangeV1 {
        key,
        expected,
        after,
        max_current_payload_bytes,
    } = change;
    let cancelled = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("world rule effect overlay pool/class").into());
        }
        let mut compare = Compare::new(expected.text().unwrap_or("").as_bytes());
        let (encoded, current) = {
            let mut cancel = || cancelled() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancel,
            };
            resolve(
                key,
                transaction,
                &mut records,
                error,
                max_current_payload_bytes,
                &mut compare,
            )?
        };
        let matches = match (current, expected) {
            (WorldRuleEffectValueV1::RulesAbsent, WorldRuleEffectExpectedV1::RulesAbsent)
            | (WorldRuleEffectValueV1::EffectAbsent, WorldRuleEffectExpectedV1::EffectAbsent) => {
                true
            }
            (WorldRuleEffectValueV1::Value(length), WorldRuleEffectExpectedV1::Value(text)) => {
                length == text.len() as u64 && compare.is_equal()
            }
            _ => false,
        };
        if !matches {
            return Err(ValueTreeError::Invalid("world rule effect expected value").into());
        }
        if current == WorldRuleEffectValueV1::RulesAbsent && after.is_some() {
            return Err(ValueTreeError::Invalid("world rule effect cannot create parent").into());
        }
        if expected.text() == after {
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let root = if let Some(after) = after {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled() || (write.cancelled)(),
            };
            let record = write_member_record_v1(
                descriptor()?,
                WorldFieldRecordKindV1::Entry,
                &(CHILD, key),
                MemberPayloadRefV1::Text(after),
                &mut combined,
            )?;
            if record.field_key != encoded {
                return Err(ValueTreeError::Invalid("world rule effect writer key").into());
            }
            Some(record.value_root)
        } else {
            None
        };
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(StateNamespaceV1::Authorization, encoded.as_bytes(), root)?;
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
