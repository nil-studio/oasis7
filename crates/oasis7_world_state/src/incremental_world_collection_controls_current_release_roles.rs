//! Complete selected Current release-role set, with exact family-write CAS.
use super::*;
use crate::incremental_authorization_mapsets::{
    AuthorizationMapSetFieldV1 as F, AuthorizationMembershipChangeV1,
    AuthorizationMembershipV1 as Membership, change_authorization_membership_overlay_v1,
};
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_value::ValueNodeWriter,
};
use oasis7_proto::memory_budget::MemoryReservation;
use std::collections::{BTreeMap, BTreeSet};
struct PendingRoleV1 {
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
    _fee: MemoryReservation,
}
pub struct CurrentModuleReleaseRolesV1 {
    subject: String,
    value: Option<BTreeSet<String>>,
    pending: BTreeMap<Vec<u8>, PendingRoleV1>,
    group: WorldFieldKeyV1,
    group_root: Option<[u8; 32]>,
    guard: PreparedGuard,
    parent: [u8; 32],
    memory: MemoryBudget,
}
impl CurrentModuleReleaseRolesV1 {
    pub fn subject(&self) -> &str {
        &self.subject
    }
    pub fn value(&self) -> Option<&BTreeSet<String>> {
        self.value.as_ref()
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
            || self.parent != tx.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_module_release_roles_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    qualification: &ReservedValue<WorldCollectionControlsQualificationV1>,
    roots: &RootSetV1,
    subject: &str,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut WorldCollectionControlsWorkV1,
    pending_entries: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentModuleReleaseRolesV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        qualification.validate_binding(roots)?;
        qualification.check(read.values.memory, MemoryClass::Ordinary)?;
        read.check()?;
        if read.values.class != MemoryClass::Ordinary
            || !qualification.uses_budget(read.values.memory)
            || !tx.uses_budget(read.values.memory)
            || tx.parent_fence().parent_roots != roots.digest()?
        {
            return Err(invalid().into());
        }
        let mut directory = qualification.release_roles.selected(subject, read, work)?;
        let mut pending = BTreeMap::new();
        let stop = std::cell::RefCell::new(&mut read.cancelled);
        let mut local = WorldFieldReadContextV1 {
            root: read.root,
            state: read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: read.values.source,
                memory: read.values.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: || (**stop.borrow_mut())(),
        };
        tx.visit_pending_writes::<WorldFieldOverlayErrorV1>(
            StateNamespaceV1::Authorization,
            pending_entries,
            || {
                if (**stop.borrow_mut())() {
                    Err(ValueTreeError::Cancelled.into())
                } else {
                    Ok(())
                }
            },
            |raw, before, after| {
                let Some((field, key)) = key(raw)? else {
                    return Ok(());
                };
                if field != 1 {
                    return Ok(());
                }
                work.step(&mut local.cancelled)?;
                let fee = local
                    .values
                    .memory
                    .reserve(
                        raw.len().checked_add(4096).ok_or_else(invalid)?,
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                local.key_work.bytes_remaining = local
                    .key_work
                    .bytes_remaining
                    .checked_sub((raw.len() as u64 + 1) * 64)
                    .ok_or(ValueTreeError::ResourceLimited("release roles pending key"))?;
                if pending
                    .insert(
                        raw.to_vec(),
                        PendingRoleV1 {
                            before,
                            after,
                            _fee: fee,
                        },
                    )
                    .is_some()
                {
                    return Err(invalid().into());
                }
                if before == after {
                    return Ok(());
                }
                for (value, add) in [(before, false), (after, true)] {
                    if let Some(value) = value {
                        row(field, &key, &value, !add, &mut local, work)?;
                        directory.row_selected(
                            &key,
                            &value,
                            add,
                            Some(subject),
                            &mut local,
                            work,
                        )?;
                    }
                }
                Ok(())
            },
        )?;
        drop(local);
        drop(stop);
        work.step(&mut read.cancelled)?;
        // Metadata keys use the closed no-semantic constructor, unlike group/member keys.
        let metadata = WorldFieldKeyV1::new(descriptor(1)?, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let meta = tx
            .read(StateNamespaceV1::Authorization, metadata.as_bytes())?
            .ok_or_else(invalid)?;
        row(1, &metadata, &meta, false, read, work)?;
        let (group, _) = read.key(descriptor(1)?, WorldFieldRecordKindV1::Group, subject)?;
        let group_root = tx.read(StateNamespaceV1::Authorization, group.as_bytes())?;
        if let Some(root) = group_root {
            row(1, &group, &root, false, read, work)?;
        }
        work.records_remaining =
            work.records_remaining
                .checked_sub(128)
                .ok_or(ValueTreeError::ResourceLimited(
                    "selected release roles lookup",
                ))?;
        read.key_work.bytes_remaining = read
            .key_work
            .bytes_remaining
            .checked_sub(
                (subject.len() as u64 + 1)
                    .checked_mul(128)
                    .ok_or_else(invalid)?,
            )
            .ok_or(ValueTreeError::ResourceLimited(
                "selected release roles lookup bytes",
            ))?;
        let members = directory.members(subject);
        if group_root.is_none() && members.is_some_and(|m| !m.is_empty()) {
            return Err(invalid().into());
        }
        let heap = directory
            .member_bytes(subject)
            .checked_mul(2)
            .and_then(|n| {
                n.checked_add(
                    subject.len() + std::mem::size_of::<CurrentModuleReleaseRolesV1>() + 4096,
                )
            })
            .ok_or_else(invalid)?;
        read.key_work.bytes_remaining = read
            .key_work
            .bytes_remaining
            .checked_sub(heap as u64)
            .ok_or(ValueTreeError::ResourceLimited(
                "selected release roles clone",
            ))?;
        work.records_remaining = work
            .records_remaining
            .checked_sub(members.map_or(0, |m| m.len()) as u64 + 1)
            .ok_or(ValueTreeError::ResourceLimited(
                "selected release roles cells",
            ))?;
        let fee = read
            .values
            .memory
            .reserve(heap, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let value = group_root.map(|_| members.cloned().unwrap_or_default());
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        Ok(fee.retain(CurrentModuleReleaseRolesV1 {
            subject: subject.to_owned(),
            value,
            pending,
            group,
            group_root,
            guard,
            parent: tx.parent_fence().parent_roots,
            memory: read.values.memory.clone(),
        }))
    })
}
#[allow(clippy::too_many_arguments)]
pub fn replace_current_module_release_roles_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    expected: &ReservedValue<CurrentModuleReleaseRolesV1>,
    after: &BTreeSet<String>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut WorldCollectionControlsWorkV1,
    pending_entries: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        tx.validate_transaction_identity(&expected.guard)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !expected.uses_budget(ctx.values.memory)
            || !expected.memory.same_pool(ctx.values.memory)
            || !tx.uses_budget(ctx.values.memory)
            || !write.memory.same_pool(ctx.values.memory)
            || expected.parent != tx.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        let mut count = 0usize;
        let stop = std::cell::RefCell::new(&mut ctx.cancelled);
        tx.visit_pending_writes::<WorldFieldOverlayErrorV1>(
            StateNamespaceV1::Authorization,
            pending_entries,
            || {
                if (**stop.borrow_mut())() {
                    Err(ValueTreeError::Cancelled.into())
                } else {
                    Ok(())
                }
            },
            |raw, before, after| {
                let Some((field, _)) = key(raw)? else {
                    return Ok(());
                };
                if field != 1 {
                    return Ok(());
                }
                if (**stop.borrow_mut())() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                work.records_remaining = work
                    .records_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("release roles CAS cells"))?;
                ctx.key_work.bytes_remaining = ctx
                    .key_work
                    .bytes_remaining
                    .checked_sub((raw.len() as u64 + 1) * 64)
                    .ok_or(ValueTreeError::ResourceLimited("release roles CAS bytes"))?;
                let actual = expected.pending.get(raw).ok_or_else(invalid)?;
                if (actual.before, actual.after) != (before, after) {
                    return Err(invalid().into());
                }
                count = count.checked_add(1).ok_or_else(invalid)?;
                Ok(())
            },
        )?;
        drop(stop);
        if count != expected.pending.len()
            || tx.read(StateNamespaceV1::Authorization, expected.group.as_bytes())?
                != expected.group_root
        {
            return Err(invalid().into());
        }
        let old_count = expected.value().map_or(0, |v| v.len());
        let count = old_count
            .checked_add(after.len())
            .and_then(|n| n.checked_add(1))
            .ok_or_else(invalid)?;
        work.records_remaining = work.records_remaining.checked_sub(count as u64).ok_or(
            ValueTreeError::ResourceLimited("release roles full CAS loops"),
        )?;
        let mut text = 0usize;
        for role in expected.value().into_iter().flatten().chain(after.iter()) {
            text = text.checked_add(role.len() + 1).ok_or_else(invalid)?;
        }
        let comparisons = text
            .checked_mul(count)
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(invalid)?;
        ctx.key_work.bytes_remaining = ctx
            .key_work
            .bytes_remaining
            .checked_sub(comparisons as u64)
            .ok_or(ValueTreeError::ResourceLimited(
                "release roles full CAS comparisons",
            ))?;
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: tx,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        if let Some(before) = expected.value() {
            for role in before {
                if !after.contains(role) {
                    work.step(&mut overlay.cancelled)?;
                    let guard = overlay.transaction.guard();
                    change_authorization_membership_overlay_v1(
                        AuthorizationMembershipChangeV1 {
                            subject: expected.subject(),
                            member: role,
                            expected: Membership::Present,
                            after: false,
                            create_group: false,
                            field: F::ModuleReleaseRoleBindings,
                        },
                        &mut overlay,
                        write,
                        &guard,
                        live,
                    )?;
                }
            }
        }
        let mut present = expected.group_root.is_some();
        for role in after {
            if expected.value().is_none_or(|before| !before.contains(role)) {
                work.step(&mut overlay.cancelled)?;
                let guard = overlay.transaction.guard();
                change_authorization_membership_overlay_v1(
                    AuthorizationMembershipChangeV1 {
                        subject: expected.subject(),
                        member: role,
                        expected: if present {
                            Membership::MemberAbsent
                        } else {
                            Membership::GroupAbsent
                        },
                        after: true,
                        create_group: !present,
                        field: F::ModuleReleaseRoleBindings,
                    },
                    &mut overlay,
                    write,
                    &guard,
                    live,
                )?;
                present = true;
            }
        }
        if after.is_empty() && present {
            if overlay
                .transaction
                .read(StateNamespaceV1::Authorization, expected.group.as_bytes())?
                != expected.group_root
            {
                return Err(invalid().into());
            }
            overlay.transaction.write(
                StateNamespaceV1::Authorization,
                expected.group.as_bytes(),
                None,
            )?;
        }
        if (overlay.cancelled)() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        overlay
            .transaction
            .validate_guard(&overlay.transaction.guard(), live)?;
        Ok(())
    })
}
