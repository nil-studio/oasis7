//! Current group observation from the authenticated parent index and typed
//! pending writes. A count is not permission or an accepted-state capability.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};

#[allow(clippy::too_many_arguments)]
pub fn read_current_data_access_group_size_v1<M, P, R, E, C>(
    qualification: &ReservedValue<WorldCollectionControlsQualificationV1>,
    roots: &RootSetV1,
    subject: &str,
    transaction: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    work: &mut WorldCollectionControlsWorkV1,
    pending_entries_remaining: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<u64>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        read.check()?;
        qualification.validate_binding(roots)?;
        qualification.check(read.values.memory, MemoryClass::Ordinary)?;
        if read.values.class != MemoryClass::Ordinary
            || !qualification.uses_budget(read.values.memory)
            || !transaction.uses_budget(read.values.memory)
            || roots.digest().map_err(|_| invalid())? != transaction.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        // Covers bounded key/probe scaffolding; record and index work remain
        // separately metered. No parent rows or whole collections are scanned.
        let _phase = read
            .values
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let (group, _) = read.key(descriptor(0)?, WorldFieldRecordKindV1::Group, subject)?;
        let target = *blake3::hash(group.as_bytes()).as_bytes();
        let mut iw = CollectionControlsIndexWorkV1 {
            steps_remaining: work.index_steps_remaining,
        };
        let result = (|| {
            let parent = qualification
                .groups
                .get(target, &mut iw, &mut read.cancelled)?;
            let mut present = parent.is_some_and(|meta| meta.present);
            let mut members = parent.map_or(0, |meta| meta.members);
            // Borrow cancellation separately because typed row validation also
            // polls it. Every inspected overlay entry consumes the supplied cap.
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
            transaction.visit_pending_writes::<WorldFieldOverlayErrorV1>(
                StateNamespaceV1::Authorization,
                pending_entries_remaining,
                || {
                    if (**stop.borrow_mut())() {
                        Err(ValueTreeError::Cancelled.into())
                    } else {
                        Ok(())
                    }
                },
                |raw, before, after| {
                    let Some((i, key)) = key(raw)? else {
                        return Ok(());
                    };
                    if i != 0 {
                        return Ok(());
                    }
                    for (root, add) in [(before, false), (after, true)] {
                        let Some(root) = root else {
                            continue;
                        };
                        match row(i, &key, &root, !add, &mut local, work)? {
                            Row::Group(_, digest) if digest == target => {
                                if present == add {
                                    return Err(invalid().into());
                                }
                                present = add;
                            }
                            Row::Member(_, digest) if digest == target => {
                                members = if add {
                                    members.checked_add(1)
                                } else {
                                    members.checked_sub(1)
                                }
                                .ok_or_else(invalid)?;
                            }
                            _ => {}
                        }
                    }
                    Ok(())
                },
            )?;
            local.check()?;
            if !present && members != 0 {
                return Err(invalid().into());
            }
            Ok(if present { Some(members) } else { None })
        })();
        work.index_steps_remaining = iw.steps_remaining;
        let final_check = read
            .check()
            .map_err(WorldFieldOverlayErrorV1::from)
            .and_then(|_| {
                transaction
                    .validate_guard(guard, live)
                    .map_err(WorldFieldOverlayErrorV1::from)
            });
        match result {
            Err(error) => Err(error),
            Ok(value) => {
                final_check?;
                Ok(value)
            }
        }
    })
}

/// A requested structural edit, not authorization to grant access. Runtime's
/// original action/event owner remains responsible for the business decision.
pub struct DataAccessMembershipChangeV1<'a> {
    pub subject: &'a str,
    pub member: &'a str,
    pub expected: crate::incremental_authorization_mapsets::AuthorizationMembershipV1,
    pub after: bool,
}

/// Preserve the original map semantics: removing its final member removes the
/// group, including a pre-existing empty group. No full set is reconstructed.
#[allow(clippy::too_many_arguments)]
pub fn change_current_data_access_membership_v1<M, P, R, E, C, W, CW>(
    change: DataAccessMembershipChangeV1<'_>,
    qualification: &ReservedValue<WorldCollectionControlsQualificationV1>,
    roots: &RootSetV1,
    transaction: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut WorldCollectionControlsWorkV1,
    pending_entries_remaining: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    CW: FnMut() -> bool,
{
    use crate::incremental_authorization_mapsets::{
        AuthorizationMapSetFieldV1, AuthorizationMembershipChangeV1, AuthorizationMembershipV1,
        change_authorization_membership_overlay_v1,
    };
    use crate::incremental_field_overlay::WorldFieldOverlayContextV1;
    transaction.contribute(|transaction| {
        let count = read_current_data_access_group_size_v1(
            qualification,
            roots,
            change.subject,
            transaction,
            read,
            work,
            pending_entries_remaining,
            guard,
            live,
        )?;
        let removed = u64::from(change.expected == AuthorizationMembershipV1::Present);
        let remaining = if change.after {
            None
        } else {
            Some(
                count
                    .unwrap_or(0)
                    .checked_sub(removed)
                    .ok_or_else(invalid)?,
            )
        };
        {
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction,
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
                cancelled: &mut read.cancelled,
            };
            let current = overlay.transaction.guard();
            change_authorization_membership_overlay_v1(
                AuthorizationMembershipChangeV1 {
                    subject: change.subject,
                    member: change.member,
                    expected: change.expected,
                    after: change.after,
                    create_group: change.after,
                    field: AuthorizationMapSetFieldV1::DataAccessPermissions,
                },
                &mut overlay,
                write,
                &current,
                live,
            )?;
        }
        if count.is_some() && remaining == Some(0) {
            let (group, bytes) = read.key(
                descriptor(0)?,
                WorldFieldRecordKindV1::Group,
                change.subject,
            )?;
            let root = transaction
                .read(StateNamespaceV1::Authorization, group.as_bytes())?
                .ok_or_else(invalid)?;
            read.records().presence(&root, &group, bytes)?;
            read.check()?;
            if (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let current = transaction.guard();
            transaction.validate_guard(&current, live)?;
            transaction.write(StateNamespaceV1::Authorization, group.as_bytes(), None)?;
        }
        read.check()?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let current = transaction.guard();
        transaction.validate_guard(&current, live)?;
        Ok(())
    })
}
