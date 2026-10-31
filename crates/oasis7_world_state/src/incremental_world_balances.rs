//! Original account/node contents; no transfer, mint or redemption authority.
use crate::{
    MainTokenAccountBalance, NodeAssetBalance, incremental_field_member::*,
    incremental_field_presence::*, incremental_field_read::WorldFieldReadContextV1,
    incremental_identity_amounts::read_identity_amounts,
    incremental_projection::WORLD_STATE_FIELDS, state::WorldState,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::Write;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldBalanceFieldV1 {
    MainToken,
    NodeAsset,
}
impl WorldBalanceFieldV1 {
    pub const ALL: [Self; 2] = [Self::MainToken, Self::NodeAsset];
    pub fn name(self) -> &'static str {
        match self {
            Self::MainToken => "main_token_balances",
            Self::NodeAsset => "node_asset_balances",
        }
    }
    fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|f| {
                f.name == self.name()
                    && f.owner == FieldOwner::WorldState
                    && f.shape == FieldShape::Map
                    && f.destination == FieldDestination::State(StateNamespaceV1::Business)
            })
            .ok_or(ValueTreeError::Invalid("world balance registration"))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldBalanceReadV1 {
    MainToken {
        identity_utf8_bytes: u64,
        liquid_balance: u64,
        vested_balance: u64,
        restricted_starter_claim_balance: u64,
    },
    NodeAsset {
        identity_utf8_bytes: u64,
        power_credit_balance: u64,
        total_minted_credits: u64,
        total_burned_credits: u64,
    },
}
fn start<W, C: FnMut() -> bool>(
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    *records = records
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("world balance records"))?;
    Ok(())
}
fn entry<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    field: FieldDescriptor,
    key: &str,
    identity: &str,
    amounts: [u64; 3],
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    start(context, records)?;
    let row = write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        key,
        MemberPayloadRefV1::IdentityU64Triple { identity, amounts },
        context,
    )?;
    crate::incremental_field_write::emit(row.field_key, row.value_root, context, sink)
}
/// Borrow original map key AND inner identity. Caller admits the source and
/// batching/index storage; discard all emitted prefixes on any failure.
pub fn prepare_world_balances_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    let mut count = 0u64;
    for field in WorldBalanceFieldV1::ALL {
        let descriptor = field.descriptor()?;
        start(context, records)?;
        let metadata = write_field_metadata_v1(
            descriptor,
            &mut PresenceRecordWriteContext {
                sink: &mut *context.sink,
                memory: context.memory,
                io: &mut *context.io,
                key_work: &mut *context.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: context.limits.semantic_bytes,
                    record_bytes: context.limits.record_bytes,
                },
                cancelled: &mut context.cancelled,
            },
        )?;
        crate::incremental_field_write::emit(
            metadata.field_key,
            metadata.value_root,
            context,
            sink,
        )?;
        match field {
            WorldBalanceFieldV1::MainToken => {
                for (key, value) in &source.main_token_balances {
                    let balance = WorldBalanceRefV1::from(value);
                    entry(
                        descriptor,
                        key,
                        balance.identity(),
                        balance.amounts(),
                        context,
                        records,
                        sink,
                    )?;
                    count = count
                        .checked_add(1)
                        .ok_or(MemberRecordError::Invalid("world balance count"))?;
                }
            }
            WorldBalanceFieldV1::NodeAsset => {
                for (key, value) in &source.node_asset_balances {
                    let balance = WorldBalanceRefV1::from(value);
                    entry(
                        descriptor,
                        key,
                        balance.identity(),
                        balance.amounts(),
                        context,
                        records,
                        sink,
                    )?;
                    count = count
                        .checked_add(1)
                        .ok_or(MemberRecordError::Invalid("world balance count"))?;
                }
            }
        }
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(count)
}
/// Stream the inner identity; result contains only the original named amounts.
/// Query and retained output are caller-admitted, provisional until success.
pub fn read_world_balance_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldBalanceFieldV1,
    key: &str,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_identity_bytes: u64,
    output: &mut impl Write,
) -> Result<Option<WorldBalanceReadV1>, ValueTreeError> {
    context.check()?;
    let descriptor = field.descriptor()?;
    let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = context
        .lookup(&metadata)?
        .ok_or(ValueTreeError::Invalid("world balance missing metadata"))?;
    context.records().presence(&root, &metadata, 0)?;
    let (key, bytes) = context.key(descriptor, WorldFieldRecordKindV1::Entry, key)?;
    let Some(root) = context.lookup(&key)? else {
        return Ok(None);
    };
    let max_payload = max_identity_bytes
        .checked_add(32)
        .ok_or(ValueTreeError::ResourceLimited("balance identity limit"))?;
    let (identity_utf8_bytes, amounts) = read_identity_amounts(
        &root,
        &key,
        &mut context.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::IdentityU64Triple,
            semantic_bytes: bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: context.semantic_bytes,
                payload_bytes: max_payload,
                record_bytes: context.record_bytes,
            },
        },
        output,
        &mut context.cancelled,
    )?;
    Ok(Some(summary(field, identity_utf8_bytes, amounts)))
}
fn summary(
    field: WorldBalanceFieldV1,
    identity_utf8_bytes: u64,
    [first, second, third]: [u64; 3],
) -> WorldBalanceReadV1 {
    match field {
        WorldBalanceFieldV1::MainToken => WorldBalanceReadV1::MainToken {
            identity_utf8_bytes,
            liquid_balance: first,
            vested_balance: second,
            restricted_starter_claim_balance: third,
        },
        WorldBalanceFieldV1::NodeAsset => WorldBalanceReadV1::NodeAsset {
            identity_utf8_bytes,
            power_credit_balance: first,
            total_minted_credits: second,
            total_burned_credits: third,
        },
    }
}
#[path = "incremental_world_balance_ref.rs"]
mod borrowed;
pub use borrowed::WorldBalanceRefV1;
#[path = "incremental_world_balances_overlay.rs"]
mod overlay;
pub use overlay::{
    WorldBalanceChangeV1, change_world_balance_overlay_v1, read_world_balance_overlay_v1,
};
