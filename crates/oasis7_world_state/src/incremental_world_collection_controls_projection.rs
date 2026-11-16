//! Original collection-control rows, read only from the transaction's actual
//! net changes. Independent empty-group presence is retained as source DATA.
use super::*;
use crate::{
    incremental_agent_mutation::BodyMutationError,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::MemoryReservation;
use std::{cell::RefCell, io, mem::size_of};
struct Change {
    field: usize,
    key: Vec<u8>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
struct ValueRow {
    field: usize,
    semantic: ReservedValue<Vec<u8>>,
    key: ReservedValue<Vec<u8>>,
    json: Option<ReservedValue<Vec<u8>>>,
}
struct GroupRow {
    scope: [u8; 32],
    members: Option<u64>,
    field: usize,
    semantic: ReservedValue<Vec<u8>>,
    key: ReservedValue<Vec<u8>>,
    before: bool,
    after: bool,
}
pub struct CurrentCollectionControlsProjectionDeltaV1 {
    rows: Vec<ValueRow>,
    groups: Vec<GroupRow>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current collection control projection")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current collection control projection")
}
fn oe(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        e => BodyMutationError::Original(Box::new(e)),
    }
}
fn pay(
    p: &mut BusinessDiffProcessingWorkV1,
    n: usize,
    stop: &mut impl FnMut() -> bool,
) -> Result<(), BodyMutationError> {
    if stop() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let n = u64::try_from(n).map_err(|_| limit())?;
    p.steps_remaining = p
        .steps_remaining
        .checked_sub(n.checked_add(1).ok_or_else(limit)?)
        .ok_or_else(limit)?;
    p.bytes_remaining = p.bytes_remaining.checked_sub(n).ok_or_else(limit)?;
    Ok(())
}
fn reserve(
    memory: &MemoryBudget,
    n: usize,
    p: &mut BusinessDiffProcessingWorkV1,
    stop: &mut impl FnMut() -> bool,
) -> Result<MemoryReservation, BodyMutationError> {
    pay(p, n, stop)?;
    memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(Into::into)
}
fn name(field: usize) -> Result<&'static str, ValueTreeError> {
    Ok(descriptor(field)?.name)
}
impl CurrentCollectionControlsProjectionDeltaV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.parent != tx.parent_fence().parent_roots
        {
            return Err(bad().into());
        }
        Ok(())
    }
    pub fn visit_with_work_v1<E: From<ValueTreeError>>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, &[u8], &[u8], Option<&[u8]>, bool) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.rows {
            work(1, 0)?;
            emit(
                name(row.field)?,
                &row.semantic,
                &row.key,
                row.json.as_deref().map(Vec::as_slice),
                row.field < 2,
            )?
        }
        Ok(())
    }
    pub fn visit_groups_with_work_v1<E: From<ValueTreeError>>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(&'static str, &[u8], &[u8], bool, bool, bool) -> Result<(), E>,
    ) -> Result<(), E> {
        for row in &self.groups {
            work(1, 0)?;
            emit(
                name(row.field)?,
                &row.semantic,
                &row.key,
                row.before,
                row.after,
                row.field < 2,
            )?
        }
        Ok(())
    }
}
pub fn is_collection_controls_projection_field_v1(name: &str) -> bool {
    (0..4).any(|i| descriptor(i).is_ok_and(|d| d.name == name))
}
fn semantic<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    p: &mut BusinessDiffProcessingWorkV1,
) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let group = parsed.kind == WorldFieldRecordKindV1::Group;
    let n = if group {
        read_field_presence_v1(
            value,
            key,
            &mut ctx.values,
            PresenceRecordLimitsV1 {
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
            },
            &mut io::sink(),
            &mut ctx.cancelled,
        )?
        .semantic_bytes
    } else {
        read_member_record_header_v1(
            value,
            key,
            &mut ctx.values,
            MemberRecordLimitsV1 {
                semantic_bytes: ctx.semantic_bytes,
                payload_bytes: 16,
                record_bytes: ctx.record_bytes,
            },
            &mut ctx.cancelled,
        )?
        .semantic_bytes
    };
    let n = usize::try_from(n).map_err(|_| limit())?;
    let memory = ctx.values.memory.clone();
    let fee = reserve(
        &memory,
        n.checked_add(size_of::<Vec<u8>>()).ok_or_else(limit)?,
        p,
        &mut ctx.cancelled,
    )?;
    let mut bytes = Vec::with_capacity(n);
    if bytes.capacity() != n {
        return Err(limit().into());
    }
    if group {
        read_field_presence_v1(
            value,
            key,
            &mut ctx.values,
            PresenceRecordLimitsV1 {
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
            },
            &mut bytes,
            &mut ctx.cancelled,
        )?;
    } else {
        read_member_record_key_v1(
            value,
            key,
            &mut ctx.values,
            MemberRecordLimitsV1 {
                semantic_bytes: ctx.semantic_bytes,
                payload_bytes: 16,
                record_bytes: ctx.record_bytes,
            },
            &mut bytes,
            &mut ctx.cancelled,
        )?;
    }
    if bytes.len() != n {
        return Err(bad().into());
    }
    pay(p, n, &mut ctx.cancelled)?;
    Ok(fee.retain(bytes))
}
enum Decoded {
    Value(ValueRow),
    Group(GroupRow),
}
fn decode_projection_change_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    change: Change,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    controls: &mut WorldCollectionControlsWorkV1,
    p: &mut BusinessDiffProcessingWorkV1,
) -> Result<Option<Decoded>, BodyMutationError> {
    let memory = ctx.values.memory.clone();
    pay(p, change.key.len(), &mut ctx.cancelled)?;
    let ns = namespace(change.field);
    if ctx.transaction.read(ns, &change.key)? != change.after {
        return Err(bad().into());
    }
    let key = WorldFieldKeyV1::from_bytes(&change.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(descriptor(change.field)?, key.kind, key.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    let value = change.after.or(change.before).ok_or_else(bad)?;
    let top = roots
        .state
        .iter()
        .find(|root| root.namespace == ns)
        .ok_or_else(bad)?;
    let mut reader = WorldFieldReadContextV1 {
        root: top,
        state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let mut selected = None;
    for value in [change.before, change.after].into_iter().flatten() {
        pay(p, 0, &mut reader.cancelled)?;
        selected = Some(super::row(
            change.field,
            &key,
            &value,
            false,
            &mut reader,
            controls,
        )?)
    }
    let selected = selected.ok_or_else(bad)?;
    if matches!(selected, super::Row::Meta(_)) {
        if change.after.is_none() {
            return Err(bad().into());
        }
        return Ok(None);
    }
    if matches!(selected, super::Row::Cursor) && change.after.is_none() {
        return Err(bad().into());
    }
    let semantic = semantic(&key, &value, &mut reader, p)?;
    let (key_json, data) = match read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)? {
        SemanticKeyRefV1::Text(group) => {
            if !matches!(selected, super::Row::Group(..)) {
                return Err(bad().into());
            }
            let json=crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(&group,&memory,p,&mut ctx.cancelled)?;
            return Ok(Some(Decoded::Group(GroupRow {
                scope: match &selected {
                    super::Row::Group(_, scope) => *scope,
                    _ => return Err(bad().into()),
                },
                members: None,
                field: change.field,
                semantic,
                key: json,
                before: change.before.is_some(),
                after: change.after.is_some(),
            })));
        }
        SemanticKeyRefV1::Pair { left, right } => {
            let (SemanticKeyRefV1::Text(group), SemanticKeyRefV1::Text(member)) = (
                left.decode().map_err(ValueTreeError::Invalid)?,
                right.decode().map_err(ValueTreeError::Invalid)?,
            ) else {
                return Err(bad().into());
            };
            if !matches!(selected, super::Row::Member(..)) {
                return Err(bad().into());
            }
            let json=crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(&(group,member),&memory,p,&mut ctx.cancelled)?;
            let data = if change.after.is_none() {
                None
            } else if change.field < 2 {
                use crate::incremental_authorization_mapsets::{
                    AuthorizationMapSetFieldV1 as F, AuthorizationMembershipV1 as V,
                };
                let field = if change.field == 0 {
                    F::DataAccessPermissions
                } else {
                    F::ModuleReleaseRoleBindings
                };
                if crate::incremental_authorization_mapsets::read_authorization_membership_overlay_v1(field,group,member,ctx).map_err(oe)?!=V::Present{return Err(bad().into())}
                Some(crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(&(),&memory,p,&mut ctx.cancelled)?)
            } else {
                let nonce =
                    crate::incremental_authenticated_nonces::read_authenticated_nonce_overlay_v1(
                        group, member, ctx,
                    )
                    .map_err(oe)?;
                let crate::incremental_authenticated_nonces::AuthenticatedNonceV1::Value(nonce) =
                    nonce
                else {
                    return Err(bad().into());
                };
                Some(crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(&nonce,&memory,p,&mut ctx.cancelled)?)
            };
            (json, data)
        }
        SemanticKeyRefV1::Unit => {
            if change.field != 3 || !matches!(selected, super::Row::Cursor) {
                return Err(bad().into());
            }
            let cursor = crate::incremental_delivery_cursor::read_delivery_cursor_overlay_v1(ctx)
                .map_err(oe)?;
            let key_json=crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(&(),&memory,p,&mut ctx.cancelled)?;
            let data=crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(&cursor,&memory,p,&mut ctx.cancelled)?;
            (key_json, Some(data))
        }
        _ => return Err(bad().into()),
    };
    Ok(Some(Decoded::Value(ValueRow {
        field: change.field,
        semantic,
        key: key_json,
        json: data,
    })))
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub fn read_current_collection_controls_projection_delta_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<WorldCollectionControlsQualificationV1>,
    roots: &RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    controls: &mut WorldCollectionControlsWorkV1,
    pending: &mut usize,
    p: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentCollectionControlsProjectionDeltaV1, BodyMutationError> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        let memory = ctx.values.memory.clone();
        if ctx.values.class != MemoryClass::Ordinary
            || !family.uses_budget(&memory)
            || !ctx.transaction.uses_budget(&memory)
        {
            return Err(bad().into());
        }
        family.check(&memory, MemoryClass::Ordinary)?;
        family.validate_binding(roots)?;
        if roots.digest().map_err(|_| bad())? != ctx.transaction.parent_fence().parent_roots {
            return Err(bad().into());
        }
        let mut count = 0usize;
        let mut keybytes = 0usize;
        for ns in [StateNamespaceV1::Authorization, StateNamespaceV1::Scheduler] {
            let p = RefCell::new(&mut *p);
            let stop = RefCell::new(&mut ctx.cancelled);
            ctx.transaction.visit_pending_writes(
                ns,
                pending,
                || pay(&mut p.borrow_mut(), 0, &mut **stop.borrow_mut()),
                |raw, before, after| {
                    pay(&mut p.borrow_mut(), raw.len(), &mut **stop.borrow_mut())?;
                    if before != after
                        && let Some((field, _)) = super::key(raw)?
                    {
                        if namespace(field) != ns {
                            return Err(bad().into());
                        }
                        count = count.checked_add(1).ok_or_else(limit)?;
                        keybytes = keybytes.checked_add(raw.len()).ok_or_else(limit)?
                    }
                    Ok::<_, BodyMutationError>(())
                },
            )?;
        }
        let _changes = reserve(
            &memory,
            count
                .checked_mul(size_of::<Change>())
                .and_then(|n| n.checked_add(keybytes))
                .ok_or_else(limit)?,
            p,
            &mut ctx.cancelled,
        )?;
        let mut changes = Vec::with_capacity(count);
        if changes.capacity() != count {
            return Err(limit().into());
        }
        for ns in [StateNamespaceV1::Authorization, StateNamespaceV1::Scheduler] {
            let p = RefCell::new(&mut *p);
            let stop = RefCell::new(&mut ctx.cancelled);
            ctx.transaction.visit_pending_writes(
                ns,
                pending,
                || pay(&mut p.borrow_mut(), 0, &mut **stop.borrow_mut()),
                |raw, before, after| {
                    pay(&mut p.borrow_mut(), raw.len(), &mut **stop.borrow_mut())?;
                    if before != after
                        && let Some((field, _)) = super::key(raw)?
                    {
                        if changes.len() == count {
                            return Err(limit().into());
                        }
                        let mut key = Vec::with_capacity(raw.len());
                        if key.capacity() != raw.len() {
                            return Err(limit().into());
                        }
                        key.extend_from_slice(raw);
                        changes.push(Change {
                            field,
                            key,
                            before,
                            after,
                        })
                    }
                    Ok::<_, BodyMutationError>(())
                },
            )?;
        }
        let fee = reserve(
            &memory,
            count
                .checked_mul(size_of::<ValueRow>() + size_of::<GroupRow>())
                .and_then(|n| {
                    n.checked_add(size_of::<CurrentCollectionControlsProjectionDeltaV1>())
                })
                .ok_or_else(limit)?,
            p,
            &mut ctx.cancelled,
        )?;
        let mut rows = Vec::with_capacity(count);
        let mut groups = Vec::with_capacity(count);
        if rows.capacity() != count || groups.capacity() != count {
            return Err(limit().into());
        }
        for change in changes {
            match decode_projection_change_v1(change, roots, state, ctx, controls, p)? {
                Some(Decoded::Value(row)) => rows.push(row),
                Some(Decoded::Group(row)) => groups.push(row),
                None => (),
            }
        }

        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        pay(p, 0, &mut ctx.cancelled)?;
        let output = CurrentCollectionControlsProjectionDeltaV1 {
            rows,
            groups,
            guard: ctx.transaction.guard(),
            parent: ctx.transaction.parent_fence().parent_roots,
            memory,
            _fee: fee,
        };
        output.validate(ctx.transaction, ctx.values.memory, live)?;
        Ok(output)
    })();
    let result = match (ctx.take_state_error)() {
        Some(e) => Err(e.into()),
        None => result,
    };
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

#[path = "incremental_world_collection_controls_projection_cold.rs"]
mod cold;
pub use cold::{
    QualifiedCollectionControlsProjectionColumnsV1,
    read_qualified_collection_controls_projection_columns_v1,
};
