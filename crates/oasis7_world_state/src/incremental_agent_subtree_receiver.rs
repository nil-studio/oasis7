//! Qualified-parent inheritance for existing column/body changes.
//! Activity/Intent presence changes require the complete original group delta.
//! Row edits preserve peer presence, net counts and monotonic allocation controls.
//! New/deleted Agent scopes reuse full structural checks; existing mailbox edits
//! use dense live-interval delta checks. This returns
//! structure only, never history readiness.
use super::*;
use crate::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_expected_bytes::Compare,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::Cursor;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("agent changed structure")
}
/// Bounded changed-write admission; unrelated Agents/payloads are inherited.
pub struct AgentSubtreeTransitionWorkV1 {
    pub cells_remaining: u64,
    pub max_scalar_bytes: u64,
}
fn old_identity<
    'a,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: &RootSetV1,
    owner: [u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    buffer: &'a mut [u8; 1024],
) -> Result<&'a str, ValueTreeError> {
    let key = AgentFieldKeyV1::from_owner_digest(owner, AgentFieldV1::Identity, 0)
        .map_err(ValueTreeError::Invalid)?;
    let value = ctx
        .lookup_raw(
            before.state_root(AgentFieldV1::Identity.namespace()),
            key.as_bytes(),
        )?
        .ok_or_else(invalid)?;
    let decoded = read_agent_scalar_tree_v1(
        AgentFieldV1::Identity,
        &value,
        &mut ctx.values,
        2048,
        &mut Cursor::new(&mut buffer[..]),
        &mut ctx.cancelled,
    )?;
    let AgentSubtreeValueV1::Text { utf8_bytes } = decoded else {
        return Err(invalid());
    };
    let id = std::str::from_utf8(buffer.get(..utf8_bytes as usize).ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    if id.len() as u64 > ctx.semantic_bytes {
        return Err(ValueTreeError::ResourceLimited("agent identity allowance"));
    }
    let body = AgentBodyKeyV1::new(id).map_err(ValueTreeError::Invalid)?;
    if ctx
        .lookup_raw(
            before
                .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business),
            body.as_bytes(),
        )?
        .is_none()
    {
        return Err(invalid());
    }
    Ok(id)
}
fn flag_group(field: AgentFieldV1) -> Option<schema::Group> {
    match field {
        AgentFieldV1::ActivityPresent => Some(schema::Group::Activity),
        AgentFieldV1::IntentPresent => Some(schema::Group::Intent),
        _ => None,
    }
}
#[path = "incremental_agent_subtree_mailbox_verify.rs"]
mod mailbox_verify;
fn changed_key(
    record: &TransitionRecordV1,
    namespace: oasis7_formal_protocol::incremental_state::StateNamespaceV1,
    key: &[u8],
) -> Option<ChangedValueIds> {
    record
        .writes
        .binary_search_by(|write| {
            (write.namespace, write.mutation.key.as_slice()).cmp(&(namespace, key))
        })
        .ok()
        .map(|index| {
            let write = &record.writes[index];
            (write.mutation.before, write.mutation.after)
        })
}
type ChangedValueIds = (Option<[u8; 32]>, Option<[u8; 32]>);
fn changed_cell(
    record: &TransitionRecordV1,
    owner: [u8; 32],
    field: AgentFieldV1,
    ordinal: u64,
) -> Result<Option<ChangedValueIds>, ValueTreeError> {
    let key = AgentFieldKeyV1::from_owner_digest(owner, field, ordinal)
        .map_err(ValueTreeError::Invalid)?;
    Ok(changed_key(record, field.namespace(), key.as_bytes()))
}
fn row_columns(group: schema::Group) -> Option<([AgentFieldV1; 4], AgentFieldV1, AgentFieldV1)> {
    use AgentFieldV1 as F;
    match group {
        schema::Group::Slot => Some((
            [F::SlotId, F::SlotType, F::SlotInstalled, F::SlotLocked],
            F::SlotCount,
            F::SlotNext,
        )),
        schema::Group::Cargo => Some((
            [F::CargoId, F::CargoKind, F::CargoQuantity, F::CargoUnitSize],
            F::CargoCount,
            F::CargoNext,
        )),
        _ => None,
    }
}
fn row_group(field: AgentFieldV1) -> Option<schema::Group> {
    use AgentFieldV1 as F;
    match field {
        F::SlotCount | F::SlotNext | F::SlotId | F::SlotType | F::SlotInstalled | F::SlotLocked => {
            Some(schema::Group::Slot)
        }
        F::CargoCount
        | F::CargoNext
        | F::CargoId
        | F::CargoKind
        | F::CargoQuantity
        | F::CargoUnitSize => Some(schema::Group::Cargo),
        _ => None,
    }
}
fn row_relation<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    record: &TransitionRecordV1,
    owner: [u8; 32],
    group: schema::Group,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    let (columns, count, next) = row_columns(group).ok_or_else(invalid)?;
    let mut controls = [(0u64, 0u64); 2];
    for (index, field) in [count, next].into_iter().enumerate() {
        ctx.check()?;
        let key =
            AgentFieldKeyV1::from_owner_digest(owner, field, 0).map_err(ValueTreeError::Invalid)?;
        let old = ctx
            .lookup_raw(record.before.state_root(field.namespace()), key.as_bytes())?
            .ok_or_else(invalid)?;
        let after = changed_cell(record, owner, field, 0)?
            .map(|(_, after)| after)
            .unwrap_or(Some(old))
            .ok_or_else(invalid)?;
        for (slot, value) in [old, after].into_iter().enumerate() {
            let AgentSubtreeValueV1::U64(value) = read_agent_scalar_tree_v1(
                field,
                &value,
                &mut ctx.values,
                64,
                &mut std::io::sink(),
                &mut ctx.cancelled,
            )?
            else {
                return Err(invalid());
            };
            if slot == 0 {
                controls[index].0 = value;
            } else {
                controls[index].1 = value;
            }
        }
    }
    let [(old_count, new_count), (old_next, new_next)] = controls;
    if new_next < old_next || new_count > new_next {
        return Err(invalid());
    }
    let mut net = 0i128;
    for write in &record.writes {
        ctx.check()?;
        let Some(AgentSubtreeKeyV1::Field {
            owner: current,
            field,
            ordinal,
        }) = parse_agent_subtree_key_v1(write.namespace, &write.mutation.key)?
        else {
            continue;
        };
        if current != owner || schema::group(field) != group {
            continue;
        }
        let before = write.mutation.before.is_some();
        let after = write.mutation.after.is_some();
        if after && (ordinal >= new_next || !before && ordinal < old_next) {
            return Err(invalid());
        }
        if before != after {
            for member in columns {
                let (a, b) = changed_cell(record, owner, member, ordinal)?.ok_or_else(invalid)?;
                if a.is_some() != before || b.is_some() != after {
                    return Err(invalid());
                }
            }
            if field == columns[0] {
                net += i128::from(after) - i128::from(before);
            }
        }
    }
    if i128::from(old_count) + net != i128::from(new_count) {
        return Err(invalid());
    }
    Ok(())
}
/// Generic replay is independent. The private structural token remains bound
/// to the exact before/after RootSets; complete Agent/history acceptance is separate.
pub fn accept_agent_subtree_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut AgentSubtreeTransitionWorkV1,
) -> Result<ReservedValue<AgentSubtreeStructureQualificationV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || !verifier.memory.same_pool(ctx.values.memory)
        || verifier.class != ctx.values.class
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before)?;
    let verified = verifier.verify(record);
    let original = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(error) = original {
        return Err(error.into());
    }
    let verified = verified?;
    if !verified
        .record
        .after
        .state
        .iter()
        .any(|root| root == ctx.root)
    {
        return Err(invalid().into());
    }
    let _scratch = ctx
        .values
        .memory
        .reserve(32 << 10, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    use AgentFieldV1 as F;
    let lifecycle_bytes = record
        .writes
        .len()
        .checked_mul(256)
        .and_then(|n| n.checked_add(64 << 10))
        .ok_or(ValueTreeError::ResourceLimited("agent lifecycle metadata"))?;
    let _lifecycle_metadata = ctx
        .values
        .memory
        .reserve(lifecycle_bytes, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut lifecycles = std::collections::BTreeMap::new();
    let mut mailboxes =
        std::collections::BTreeMap::<[u8; 32], mailbox_verify::MailboxDelta<'_>>::new();
    for write in &verified.record.writes {
        ctx.check()?;
        work.cells_remaining = work
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("agent lifecycle index"))?;
        if let Some(AgentSubtreeKeyV1::Body { agent }) =
            parse_agent_subtree_key_v1(write.namespace, &write.mutation.key)?
            && (write.mutation.before.is_none() || write.mutation.after.is_none())
        {
            let owner = closure::owner(agent, ctx.key_work, ctx.semantic_bytes)?;
            if lifecycles
                .insert(owner, (agent, write.mutation.before.is_none()))
                .is_some()
            {
                return Err(invalid().into());
            }
        }
    }
    let mut agent_delta = 0i128;
    let mut reference_delta = 0i128;
    for write in &verified.record.writes {
        ctx.check()?;
        work.cells_remaining = work
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("agent changed writes"))?;
        let Some(key) = parse_agent_subtree_key_v1(write.namespace, &write.mutation.key)? else {
            continue;
        };
        match key {
            AgentSubtreeKeyV1::Field { owner, field, .. } => {
                if lifecycles.contains_key(&owner) {
                    continue;
                }
                if let Some(group) = row_group(field) {
                    row_relation(record, owner, group, ctx)?;
                }
                let group = schema::group(field);
                if !matches!(
                    group,
                    schema::Group::Resource | schema::Group::Slot | schema::Group::Cargo
                ) && (write.mutation.before.is_none() || write.mutation.after.is_none())
                {
                    let flag = match group {
                        schema::Group::Activity => F::ActivityPresent,
                        schema::Group::Intent => F::IntentPresent,
                        _ => return Err(invalid().into()),
                    };
                    let (before, after) =
                        changed_cell(record, owner, flag, 0)?.ok_or_else(invalid)?;
                    for (value, present) in [
                        (before, write.mutation.before.is_some()),
                        (after, write.mutation.after.is_some()),
                    ] {
                        let value = value.ok_or_else(invalid)?;
                        if read_agent_scalar_tree_v1(
                            flag,
                            &value,
                            &mut ctx.values,
                            64,
                            &mut std::io::sink(),
                            &mut ctx.cancelled,
                        )? != AgentSubtreeValueV1::Bool(present)
                        {
                            return Err(invalid().into());
                        }
                    }
                }
                if let Some(group) = flag_group(field) {
                    let old = write.mutation.before.ok_or_else(invalid)?;
                    let new = write.mutation.after.ok_or_else(invalid)?;
                    let old = read_agent_scalar_tree_v1(
                        field,
                        &old,
                        &mut ctx.values,
                        64,
                        &mut std::io::sink(),
                        &mut ctx.cancelled,
                    )?;
                    let new = read_agent_scalar_tree_v1(
                        field,
                        &new,
                        &mut ctx.values,
                        64,
                        &mut std::io::sink(),
                        &mut ctx.cancelled,
                    )?;
                    let (AgentSubtreeValueV1::Bool(old), AgentSubtreeValueV1::Bool(new)) =
                        (old, new)
                    else {
                        return Err(invalid().into());
                    };
                    if old != new {
                        for member in F::ALL
                            .iter()
                            .copied()
                            .filter(|f| schema::group(*f) == group)
                        {
                            let (before, after) =
                                changed_cell(record, owner, member, 0)?.ok_or_else(invalid)?;
                            if before.is_some() != old || after.is_some() != new {
                                return Err(invalid().into());
                            }
                        }
                    }
                }
                let mut identity = [0u8; 1024];
                let agent = old_identity(&record.before, owner, ctx, &mut identity)?;
                if let Some(value) = write.mutation.after {
                    ctx.values
                        .verify_closure(&value, work.max_scalar_bytes, &mut ctx.cancelled)?;
                    let mut compare = Compare::new(agent.as_bytes());
                    let decoded = if matches!(field, F::Identity | F::IntentAgent) {
                        read_agent_scalar_tree_v1(
                            field,
                            &value,
                            &mut ctx.values,
                            work.max_scalar_bytes,
                            &mut compare,
                            &mut ctx.cancelled,
                        )?
                    } else {
                        read_agent_scalar_tree_v1(
                            field,
                            &value,
                            &mut ctx.values,
                            work.max_scalar_bytes,
                            &mut std::io::sink(),
                            &mut ctx.cancelled,
                        )?
                    };
                    if matches!(field, F::Identity | F::IntentAgent)
                        && (decoded
                            != (AgentSubtreeValueV1::Text {
                                utf8_bytes: agent.len() as u64,
                            })
                            || !compare.is_equal())
                    {
                        return Err(invalid().into());
                    }
                }
            }
            AgentSubtreeKeyV1::Body { agent } => {
                let owner = closure::owner(agent, ctx.key_work, ctx.semantic_bytes)?;
                if let Some((_, added)) = lifecycles.get(&owner) {
                    let mut scope = AgentSubtreeScanWorkV1 {
                        max_agents: 1,
                        cells_remaining: work.cells_remaining,
                        max_scalar_bytes: work.max_scalar_bytes,
                    };
                    let references =
                        closure::verify_lifecycle_actor(&verified, agent, *added, ctx, &mut scope)?;
                    work.cells_remaining = scope.cells_remaining;
                    let direction = if *added { 1i128 } else { -1i128 };
                    agent_delta += direction;
                    reference_delta += direction * i128::from(references);
                    continue;
                }
                if write.mutation.before.is_none() {
                    return Err(invalid().into());
                }
                let value = write.mutation.after.ok_or_else(invalid)?;
                ctx.values.verify_closure(
                    &value,
                    AGENT_BODY_VALUE_BYTES as u64,
                    &mut ctx.cancelled,
                )?;
                AgentBodyValueV1::from_canonical_bytes(&read::fixed_tree::<
                    AGENT_BODY_VALUE_BYTES,
                    _,
                    _,
                >(
                    &value,
                    &mut ctx.values,
                    &mut ctx.cancelled,
                )?)
                .map_err(ValueTreeError::Invalid)?;
            }
            AgentSubtreeKeyV1::Mailbox { agent, ordinal } => {
                let owner = closure::owner(agent, ctx.key_work, ctx.semantic_bytes)?;
                if lifecycles.contains_key(&owner) {
                    continue;
                }
                if let std::collections::btree_map::Entry::Vacant(entry) = mailboxes.entry(owner) {
                    let mut identity = [0u8; 1024];
                    if old_identity(&record.before, owner, ctx, &mut identity)? != agent {
                        return Err(invalid().into());
                    }
                    entry.insert(mailbox_verify::MailboxDelta::new(record, agent, ctx)?);
                }
                let mailbox = mailboxes.get_mut(&owner).ok_or_else(invalid)?;
                if mailbox.agent != agent {
                    return Err(invalid().into());
                }
                mailbox.account(ordinal, &write.mutation, ctx)?;
            }
        }
        ctx.check()?;
    }
    for mailbox in mailboxes.values() {
        ctx.check()?;
        reference_delta += mailbox.finish()?;
    }
    let proof = closure::inherit(
        parent,
        &verified.record.after,
        ctx.values.memory,
        ctx.values.class,
        agent_delta,
        reference_delta,
    )?;
    ctx.check()?;
    Ok(proof)
}
