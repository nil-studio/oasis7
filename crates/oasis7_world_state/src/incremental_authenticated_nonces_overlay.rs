use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::MemoryClass;
struct Point {
    group: WorldFieldKeyV1,
    group_root: Option<[u8; 32]>,
    member: WorldFieldKeyV1,
    value: AuthenticatedNonceV1,
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    player: &str,
    public_key: &str,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Point, WorldFieldOverlayErrorV1> {
    let field = descriptor()?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = lookup(transaction, &metadata, records, error)?.ok_or(ValueTreeError::Invalid(
        "authenticated nonce missing metadata",
    ))?;
    records.presence(&root, &metadata, 0)?;
    let (group, bytes) = records.key(field, WorldFieldRecordKindV1::Group, player)?;
    let group_root = lookup(transaction, &group, records, error)?;
    if let Some(root) = group_root {
        records.presence(&root, &group, bytes)?;
    }
    let (member, bytes) =
        records.key(field, WorldFieldRecordKindV1::Entry, &(player, public_key))?;
    let root = lookup(transaction, &member, records, error)?;
    if group_root.is_none() && root.is_some() {
        return Err(ValueTreeError::Invalid("authenticated nonce orphan key").into());
    }
    let value = if let Some(root) = root {
        AuthenticatedNonceV1::Value(records.unsigned(&root, &member, bytes)?)
    } else if group_root.is_some() {
        AuthenticatedNonceV1::KeyAbsent
    } else {
        AuthenticatedNonceV1::PlayerAbsent
    };
    Ok(Point {
        group,
        group_root,
        member,
        value,
    })
}
pub fn read_authenticated_nonce_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    player: &str,
    public_key: &str,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<AuthenticatedNonceV1, WorldFieldOverlayErrorV1> {
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
            return Err(ValueTreeError::Invalid("authenticated nonce overlay pool/class").into());
        }
        resolve(player, public_key, transaction, &mut records, error).map(|point| point.value)
    })
}
#[derive(Debug, Clone, Copy)]
pub struct AuthenticatedNonceChangeV1<'a> {
    pub player: &'a str,
    pub public_key: &'a str,
    pub expected: AuthenticatedNonceV1,
    pub after: Option<u64>,
    pub create_player: bool,
}
fn write_root<M: MerkleNodeReader>(
    transaction: &mut ExecutionTransaction<'_, M>,
    key: &WorldFieldKeyV1,
    root: Option<[u8; 32]>,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let namespace = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .namespace;
    transaction.write(namespace, key.as_bytes(), root)?;
    Ok(())
}
/// Original owner chooses final membership and nonce; this does not verify auth.
pub fn change_authenticated_nonce_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: AuthenticatedNonceChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<u8, WorldFieldOverlayErrorV1> {
    let cancelled = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("authenticated nonce overlay pool/class").into());
        }
        let point = {
            let mut cancel = || cancelled() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancel,
            };
            resolve(
                change.player,
                change.public_key,
                transaction,
                &mut records,
                error,
            )?
        };
        if point.value != change.expected {
            return Err(ValueTreeError::Invalid("authenticated nonce expected value").into());
        }
        if point.group_root.is_none() && change.after.is_some() && !change.create_player {
            return Err(ValueTreeError::Invalid("authenticated nonce missing player").into());
        }
        let create = point.group_root.is_none() && change.create_player;
        let before = match point.value {
            AuthenticatedNonceV1::Value(value) => Some(value),
            _ => None,
        };
        if !create && before == change.after {
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(0);
        }
        let mut count = 0;
        if create {
            let output = write_field_group_v1(
                descriptor()?,
                change.player,
                &mut PresenceRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: PresenceRecordLimitsV1 {
                        semantic_bytes: write.limits.semantic_bytes,
                        record_bytes: write.limits.record_bytes,
                    },
                    cancelled: || cancelled() || (write.cancelled)(),
                },
            )?;
            if output.field_key != point.group {
                return Err(ValueTreeError::Invalid("authenticated nonce group writer key").into());
            }
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            write_root(transaction, &point.group, Some(output.value_root))?;
            count += 1;
        }
        if before != change.after {
            let root = if let Some(value) = change.after {
                let output = write_member_record_v1(
                    descriptor()?,
                    WorldFieldRecordKindV1::Entry,
                    &(change.player, change.public_key),
                    MemberPayloadRefV1::U64(value),
                    &mut MemberRecordWriteContext {
                        sink: &mut *write.sink,
                        memory: write.memory,
                        io: &mut *write.io,
                        key_work: &mut *write.key_work,
                        limits: write.limits,
                        cancelled: || cancelled() || (write.cancelled)(),
                    },
                )?;
                if output.field_key != point.member {
                    return Err(
                        ValueTreeError::Invalid("authenticated nonce member writer key").into(),
                    );
                }
                Some(output.value_root)
            } else {
                None
            };
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            write_root(transaction, &point.member, root)?;
            count += 1;
        }
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(count)
    })
}
