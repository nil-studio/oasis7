//! Original player/public-key nonces; no signature or replay authorization.
use crate::{
    incremental_field_member::*, incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1, incremental_projection::WORLD_STATE_FIELDS,
    state::WorldState,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticatedNonceV1 {
    /// This query's Group marker is absent; sibling members are not scanned.
    /// Authorization requires a separately qualified complete field family.
    PlayerAbsent,
    KeyAbsent,
    Value(u64),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthenticatedNonceGenesisV1 {
    pub players: u64,
    pub keys: u64,
}
fn descriptor() -> Result<FieldDescriptor, ValueTreeError> {
    WORLD_STATE_FIELDS
        .iter()
        .copied()
        .find(|field| {
            field.name == "authenticated_collect_data_last_nonces"
                && field.owner == FieldOwner::WorldState
                && field.shape == FieldShape::NestedMap
                && field.destination == FieldDestination::State(StateNamespaceV1::Authorization)
        })
        .ok_or(ValueTreeError::Invalid("authenticated nonce registration"))
}
fn start<W, C: FnMut() -> bool>(
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if *records == 0 {
        return Err(ValueTreeError::ResourceLimited("authenticated nonce records").into());
    }
    *records -= 1;
    Ok(())
}
fn presence<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    player: Option<&str>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    start(context, records)?;
    let field = descriptor()?;
    let mut write = PresenceRecordWriteContext {
        sink: &mut *context.sink,
        memory: context.memory,
        io: &mut *context.io,
        key_work: &mut *context.key_work,
        limits: PresenceRecordLimitsV1 {
            semantic_bytes: context.limits.semantic_bytes,
            record_bytes: context.limits.record_bytes,
        },
        cancelled: &mut context.cancelled,
    };
    let record = if let Some(player) = player {
        write_field_group_v1(field, player, &mut write)?
    } else {
        write_field_metadata_v1(field, &mut write)?
    };
    crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)
}
/// Caller admits source and batch storage, discarding every prefix on error.
pub fn prepare_authenticated_nonces_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<AuthenticatedNonceGenesisV1, MemberRecordError> {
    let field = descriptor()?;
    let mut summary = AuthenticatedNonceGenesisV1 {
        players: 0,
        keys: 0,
    };
    presence(None, context, records, sink)?;
    for (player, nonces) in &source.authenticated_collect_data_last_nonces {
        presence(Some(player), context, records, sink)?;
        summary.players = summary
            .players
            .checked_add(1)
            .ok_or(MemberRecordError::Invalid(
                "authenticated nonce player count",
            ))?;
        for (public_key, nonce) in nonces {
            start(context, records)?;
            let record = write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                &(player.as_str(), public_key.as_str()),
                MemberPayloadRefV1::U64(*nonce),
                context,
            )?;
            crate::incremental_field_write::emit(
                record.field_key,
                record.value_root,
                context,
                sink,
            )?;
            summary.keys = summary
                .keys
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid("authenticated nonce key count"))?;
        }
    }
    Ok(summary)
}
pub fn read_authenticated_nonce_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    player: &str,
    public_key: &str,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<AuthenticatedNonceV1, ValueTreeError> {
    context.check()?;
    let field = descriptor()?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = context.lookup(&metadata)?.ok_or(ValueTreeError::Invalid(
        "authenticated nonce missing metadata",
    ))?;
    context.records().presence(&root, &metadata, 0)?;
    let (group, group_bytes) = context.key(field, WorldFieldRecordKindV1::Group, player)?;
    let group_root = context.lookup(&group)?;
    if let Some(root) = group_root {
        context.records().presence(&root, &group, group_bytes)?;
    }
    let (key, key_bytes) =
        context.key(field, WorldFieldRecordKindV1::Entry, &(player, public_key))?;
    let root = context.lookup(&key)?;
    if group_root.is_none() && root.is_some() {
        return Err(ValueTreeError::Invalid("authenticated nonce orphan key"));
    }
    if let Some(root) = root {
        Ok(AuthenticatedNonceV1::Value(
            context.records().unsigned(&root, &key, key_bytes)?,
        ))
    } else if group_root.is_some() {
        Ok(AuthenticatedNonceV1::KeyAbsent)
    } else {
        Ok(AuthenticatedNonceV1::PlayerAbsent)
    }
}

#[path = "incremental_authenticated_nonces_overlay.rs"]
mod overlay;
pub use overlay::*;
