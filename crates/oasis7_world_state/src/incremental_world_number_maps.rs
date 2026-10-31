//! Original scalar map values; no business authorization or full World root.
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
pub enum WorldNumberV1 {
    Unsigned(u64),
    Signed(i64),
}
#[derive(Debug, Clone, Copy)]
pub enum WorldNumberMapKeyRefV1<'a> {
    Text(&'a str),
    Action(u64),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldNumberMapFieldV1 {
    IndustrySettlementOrders,
    AgentClaimAcquisitionSequences,
    ContractPairLastSuccess,
    ReputationWindowStarted,
    ReputationWindowAccumulated,
    ReputationScores,
    MainTokenTreasuryBalances,
    MainTokenClaimNonces,
    MainTokenTransferNonces,
    NodeRedeemNonces,
    GovernanceAuthorizationNonces,
}
impl WorldNumberMapFieldV1 {
    pub const ALL: [Self; 11] = [
        Self::IndustrySettlementOrders,
        Self::AgentClaimAcquisitionSequences,
        Self::ContractPairLastSuccess,
        Self::ReputationWindowStarted,
        Self::ReputationWindowAccumulated,
        Self::ReputationScores,
        Self::MainTokenTreasuryBalances,
        Self::MainTokenClaimNonces,
        Self::MainTokenTransferNonces,
        Self::NodeRedeemNonces,
        Self::GovernanceAuthorizationNonces,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::IndustrySettlementOrders => "industry_settlement_orders",
            Self::AgentClaimAcquisitionSequences => "agent_claim_acquisition_sequences",
            Self::ContractPairLastSuccess => "contract_pair_last_success_settled_at",
            Self::ReputationWindowStarted => "reputation_reward_window_started_at",
            Self::ReputationWindowAccumulated => "reputation_reward_window_accumulated",
            Self::ReputationScores => "reputation_scores",
            Self::MainTokenTreasuryBalances => "main_token_treasury_balances",
            Self::MainTokenClaimNonces => "main_token_claim_nonces",
            Self::MainTokenTransferNonces => "main_token_transfer_nonces",
            Self::NodeRedeemNonces => "node_redeem_nonces",
            Self::GovernanceAuthorizationNonces => "governance_authorization_nonces",
        }
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::IndustrySettlementOrders | Self::AgentClaimAcquisitionSequences => {
                StateNamespaceV1::Allocator
            }
            Self::MainTokenClaimNonces
            | Self::MainTokenTransferNonces
            | Self::NodeRedeemNonces
            | Self::GovernanceAuthorizationNonces => StateNamespaceV1::Authorization,
            _ => StateNamespaceV1::Business,
        }
    }
    fn signed(self) -> bool {
        matches!(
            self,
            Self::ReputationWindowAccumulated | Self::ReputationScores
        )
    }
    fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|field| {
                field.name == self.name()
                    && field.owner == FieldOwner::WorldState
                    && field.shape == FieldShape::Map
                    && field.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(ValueTreeError::Invalid("world number map registration"))
    }
    fn check_key(self, key: WorldNumberMapKeyRefV1<'_>) -> Result<(), ValueTreeError> {
        if matches!(self, Self::IndustrySettlementOrders)
            != matches!(key, WorldNumberMapKeyRefV1::Action(_))
        {
            return Err(ValueTreeError::Invalid("world number map key type"));
        }
        Ok(())
    }
    fn check_value(self, value: WorldNumberV1) -> Result<(), ValueTreeError> {
        if self.signed() != matches!(value, WorldNumberV1::Signed(_)) {
            return Err(ValueTreeError::Invalid("world number map value type"));
        }
        Ok(())
    }
}
fn start<W, C: FnMut() -> bool>(
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if *records == 0 {
        return Err(ValueTreeError::ResourceLimited("world number map records").into());
    }
    *records -= 1;
    Ok(())
}
fn entry<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    field: WorldNumberMapFieldV1,
    key: WorldNumberMapKeyRefV1<'_>,
    value: WorldNumberV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    field.check_key(key)?;
    field.check_value(value)?;
    start(context, records)?;
    let field = field.descriptor()?;
    let payload = match value {
        WorldNumberV1::Unsigned(v) => MemberPayloadRefV1::U64(v),
        WorldNumberV1::Signed(v) => MemberPayloadRefV1::I64(v),
    };
    let record = match key {
        WorldNumberMapKeyRefV1::Text(key) => {
            write_member_record_v1(field, WorldFieldRecordKindV1::Entry, key, payload, context)?
        }
        WorldNumberMapKeyRefV1::Action(key) => {
            write_member_record_v1(field, WorldFieldRecordKindV1::Entry, &key, payload, context)?
        }
    };
    crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)
}
/// Outputs stay provisional; caller admits source and batch/index storage.
pub fn prepare_world_number_maps_genesis_v1<
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
    for field in WorldNumberMapFieldV1::ALL {
        start(context, records)?;
        let record = write_field_metadata_v1(
            field.descriptor()?,
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
        crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)?;
        macro_rules! members {
            ($values:expr,$key:ident,$value:ident) => {
                for (key, value) in $values {
                    entry(
                        field,
                        WorldNumberMapKeyRefV1::$key(key),
                        WorldNumberV1::$value(value),
                        context,
                        records,
                        sink,
                    )?;
                    count = count
                        .checked_add(1)
                        .ok_or(MemberRecordError::Invalid("world number map count"))?;
                }
            };
        }
        match field {
            WorldNumberMapFieldV1::IndustrySettlementOrders => members!(
                source
                    .industry_settlement_orders
                    .iter()
                    .map(|(key, value)| (*key, *value)),
                Action,
                Unsigned
            ),
            WorldNumberMapFieldV1::AgentClaimAcquisitionSequences => members!(
                source
                    .agent_claim_acquisition_sequences
                    .iter()
                    .map(|(key, value)| (key.as_str(), *value)),
                Text,
                Unsigned
            ),
            WorldNumberMapFieldV1::ContractPairLastSuccess => members!(
                source
                    .contract_pair_last_success_settled_at
                    .iter()
                    .map(|(key, value)| (key.as_str(), *value)),
                Text,
                Unsigned
            ),
            WorldNumberMapFieldV1::ReputationWindowStarted => members!(
                source
                    .reputation_reward_window_started_at
                    .iter()
                    .map(|(key, value)| (key.as_str(), *value)),
                Text,
                Unsigned
            ),
            WorldNumberMapFieldV1::ReputationWindowAccumulated => members!(
                source
                    .reputation_reward_window_accumulated
                    .iter()
                    .map(|(key, value)| (key.as_str(), *value)),
                Text,
                Signed
            ),
            WorldNumberMapFieldV1::ReputationScores => members!(
                source
                    .reputation_scores
                    .iter()
                    .map(|(key, value)| (key.as_str(), *value)),
                Text,
                Signed
            ),
            WorldNumberMapFieldV1::MainTokenTreasuryBalances => members!(
                source
                    .main_token_treasury_balances
                    .iter()
                    .map(|(key, value)| (key.as_str(), *value)),
                Text,
                Unsigned
            ),
            WorldNumberMapFieldV1::MainTokenClaimNonces => members!(
                source
                    .main_token_claim_nonces
                    .iter()
                    .map(|(key, value)| (key.as_str(), *value)),
                Text,
                Unsigned
            ),
            WorldNumberMapFieldV1::MainTokenTransferNonces => members!(
                source
                    .main_token_transfer_nonces
                    .iter()
                    .map(|(key, value)| (key.as_str(), *value)),
                Text,
                Unsigned
            ),
            WorldNumberMapFieldV1::NodeRedeemNonces => members!(
                source
                    .node_redeem_nonces
                    .iter()
                    .map(|(key, value)| (key.as_str(), *value)),
                Text,
                Unsigned
            ),
            WorldNumberMapFieldV1::GovernanceAuthorizationNonces => members!(
                source
                    .governance_authorization_nonces
                    .iter()
                    .map(|(key, value)| (key.as_str(), *value)),
                Text,
                Unsigned
            ),
        }
    }
    Ok(count)
}
pub fn read_world_number_map_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldNumberMapFieldV1,
    key: WorldNumberMapKeyRefV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<WorldNumberV1>, ValueTreeError> {
    context.check()?;
    field.check_key(key)?;
    let descriptor = field.descriptor()?;
    let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let root = context
        .lookup(&metadata)?
        .ok_or(ValueTreeError::Invalid("world number map missing metadata"))?;
    context.records().presence(&root, &metadata, 0)?;
    let (key, bytes) = match key {
        WorldNumberMapKeyRefV1::Text(key) => {
            context.key(descriptor, WorldFieldRecordKindV1::Entry, key)?
        }
        WorldNumberMapKeyRefV1::Action(key) => {
            context.key(descriptor, WorldFieldRecordKindV1::Entry, &key)?
        }
    };
    let Some(root) = context.lookup(&key)? else {
        return Ok(None);
    };
    let value = if field.signed() {
        WorldNumberV1::Signed(context.records().quantity(&root, &key, bytes)?)
    } else {
        WorldNumberV1::Unsigned(context.records().unsigned(&root, &key, bytes)?)
    };
    Ok(Some(value))
}

#[path = "incremental_world_number_maps_overlay.rs"]
mod overlay;
pub use overlay::*;
