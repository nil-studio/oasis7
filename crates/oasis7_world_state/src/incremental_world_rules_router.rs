//! Complete global semantic grammar; child payloads stay with their owners.
use super::*;
use crate::incremental_world_rule_effects::world_rule_child_from_semantic_v1;
use oasis7_formal_protocol::incremental_state::{StateScanContext, StateScanError};
use std::cell::RefCell;
pub(super) fn qualify<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRulesFamilyWorkV1,
) -> Result<(), ValueTreeError> {
    check(ctx)?;
    let _phase = ctx.values.memory.reserve(32 << 10, ctx.values.class)?;
    let field = crate::incremental_world_rule_parent::descriptor()?;
    let mut metadata = false;
    let mut parent = false;
    {
        let cancelled = RefCell::new(&mut ctx.cancelled);
        let errors = RefCell::new(&mut ctx.take_state_error);
        for root in &roots.state {
            let result = StateScanContext {
                source: ctx.state,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *scan,
            }
            .visit_entries(
                root,
                |row| {
                    if let Some(error) = (errors.borrow_mut())() {
                        return Err(error);
                    }
                    work.outer_records_remaining =
                        work.outer_records_remaining.checked_sub(1).ok_or(
                            ValueTreeError::ResourceLimited("WorldRules13 outer records"),
                        )?;
                    if !row.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                        return Ok(());
                    }
                    let parsed =
                        WorldFieldKeyV1::from_bytes(&row.key).map_err(ValueTreeError::Invalid)?;
                    if parsed.owner != FieldOwner::WorldState || parsed.name != field.name {
                        return Ok(());
                    }
                    let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
                        .map_err(ValueTreeError::Invalid)?;
                    if row.namespace != StateNamespaceV1::Authorization || key.as_bytes() != row.key
                    {
                        return Err(invalid());
                    }
                    let mut poll = || (cancelled.borrow_mut())();
                    if parsed.kind == WorldFieldRecordKindV1::Metadata {
                        if std::mem::replace(&mut metadata, true) || parsed.semantic_key.is_some() {
                            return Err(invalid());
                        }
                        crate::incremental_field_presence::read_field_presence_v1(
                            &row.value,
                            &key,
                            &mut ctx.values,
                            crate::incremental_field_presence::PresenceRecordLimitsV1 {
                                semantic_bytes: 0,
                                record_bytes: 512,
                            },
                            &mut std::io::sink(),
                            &mut poll,
                        )?;
                        return Ok(());
                    }
                    if parsed.kind != WorldFieldRecordKindV1::Entry {
                        return Err(invalid());
                    }
                    let limits = MemberRecordLimitsV1 {
                        semantic_bytes: work.max_semantic_bytes,
                        payload_bytes: work.max_record_bytes,
                        record_bytes: work.max_record_bytes,
                    };
                    let header = crate::incremental_field_member::read_member_record_header_v1(
                        &row.value,
                        &key,
                        &mut ctx.values,
                        limits,
                        &mut poll,
                    )?;
                    let length = usize::try_from(header.semantic_bytes).map_err(|_| invalid())?;
                    let charge = ctx.values.memory.reserve(
                        length.checked_add(128).ok_or_else(invalid)?,
                        ctx.values.class,
                    )?;
                    let mut bytes = Vec::with_capacity(length);
                    crate::incremental_field_member::read_member_record_key_v1(
                        &row.value,
                        &key,
                        &mut ctx.values,
                        limits,
                        &mut bytes,
                        &mut poll,
                    )?;
                    if bytes.len() != length || bytes.capacity() > charge.bytes() {
                        return Err(invalid());
                    }
                    if world_rule_child_from_semantic_v1(&bytes)?.is_none()
                        && std::mem::replace(&mut parent, true)
                    {
                        return Err(invalid());
                    }
                    Ok(())
                },
                || (cancelled.borrow_mut())(),
            );
            if (cancelled.borrow_mut())() {
                return Err(ValueTreeError::Cancelled);
            }
            if let Some(error) = (errors.borrow_mut())() {
                return Err(error);
            }
            match result {
                Ok(_) => {}
                Err(StateScanError::Visitor(e)) => return Err(e),
                Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
                Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e)),
                Err(StateScanError::ResourceLimited) => {
                    return Err(ValueTreeError::ResourceLimited("WorldRules13 scan"));
                }
                Err(StateScanError::Tree(error)) => {
                    return Err(ValueTreeError::Io(std::io::Error::other(error)));
                }
                Err(StateScanError::Invalid(e)) => return Err(ValueTreeError::Invalid(e)),
            }
        }
    }
    if !metadata || !parent {
        return Err(invalid());
    }
    check(ctx)
}

/// The qualified parent already closes all unchanged keys. Inspect only changed
/// originals; a removed child still has to be a member of the closed grammar.
pub(super) fn transition<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    record: &oasis7_formal_protocol::incremental_execution::TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldRulesFamilyWorkV1,
) -> Result<(), ValueTreeError> {
    let _phase = ctx.values.memory.reserve(32 << 10, ctx.values.class)?;
    let field = crate::incremental_world_rule_parent::descriptor()?;
    for write in &record.writes {
        check(ctx)?;
        work.outer_records_remaining =
            work.outer_records_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "WorldRules13 changed records",
                ))?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let parsed =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        if parsed.owner != FieldOwner::WorldState || parsed.name != field.name {
            continue;
        }
        let key = WorldFieldKeyV1::new(field, parsed.kind, parsed.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        if write.namespace != StateNamespaceV1::Authorization
            || key.as_bytes() != write.mutation.key
        {
            return Err(invalid());
        }
        if parsed.kind == WorldFieldRecordKindV1::Metadata {
            if parsed.semantic_key.is_some() {
                return Err(invalid());
            }
            let id = write.mutation.after.ok_or_else(invalid)?;
            crate::incremental_field_presence::read_field_presence_v1(
                &id,
                &key,
                &mut ctx.values,
                crate::incremental_field_presence::PresenceRecordLimitsV1 {
                    semantic_bytes: 0,
                    record_bytes: 512,
                },
                &mut std::io::sink(),
                &mut ctx.cancelled,
            )?;
            continue;
        }
        if parsed.kind != WorldFieldRecordKindV1::Entry {
            return Err(invalid());
        }
        for id in [write.mutation.before, write.mutation.after]
            .into_iter()
            .flatten()
        {
            let limits = MemberRecordLimitsV1 {
                semantic_bytes: work.max_semantic_bytes,
                payload_bytes: work.max_record_bytes,
                record_bytes: work.max_record_bytes,
            };
            let header = crate::incremental_field_member::read_member_record_header_v1(
                &id,
                &key,
                &mut ctx.values,
                limits,
                &mut ctx.cancelled,
            )?;
            let length = usize::try_from(header.semantic_bytes).map_err(|_| invalid())?;
            let charge = ctx.values.memory.reserve(
                length.checked_add(128).ok_or_else(invalid)?,
                ctx.values.class,
            )?;
            let mut bytes = Vec::with_capacity(length);
            crate::incremental_field_member::read_member_record_key_v1(
                &id,
                &key,
                &mut ctx.values,
                limits,
                &mut bytes,
                &mut ctx.cancelled,
            )?;
            if bytes.len() != length || bytes.capacity() > charge.bytes() {
                return Err(invalid());
            }
            world_rule_child_from_semantic_v1(&bytes)?;
            check(ctx)?;
        }
    }
    check(ctx)
}
