//! Shared fixed-profile assembly for the six remaining original rule branches.
//! These selected-record operations do not grant complete World/History readiness.
pub use crate::incremental_typed_record_codec::{
    RecoveredTypedRecordV1 as RecoveredWorldRuleCompositeV1,
    TypedRecordReadWorkV1 as WorldRuleCompositeReadWorkV1,
    TypedRecordWriteWorkV1 as WorldRuleCompositeWriteWorkV1,
};
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_projection::{FieldDescriptor, FieldDestination, FieldOwner, FieldShape},
    incremental_record_tree::RecordTreeLabelsV1,
    incremental_semantic_key::hash_semantic_key_v1,
    incremental_typed_record_codec::{
        Codec, TypedRecordProfileV1, prepare_typed_record_body_v1, recover_typed_record_body_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldRuleCompositeBranchV1 {
    Requests,
    OrganizationContinuity,
    EmergencySupply,
    AgentOrders,
    IndustrialWork,
    Substitution,
}
impl WorldRuleCompositeBranchV1 {
    pub const ALL: [Self; 6] = [
        Self::Requests,
        Self::OrganizationContinuity,
        Self::EmergencySupply,
        Self::AgentOrders,
        Self::IndustrialWork,
        Self::Substitution,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Requests => "requests",
            Self::OrganizationContinuity => "organization_continuity",
            Self::EmergencySupply => "emergency_supply",
            Self::AgentOrders => "agent_orders",
            Self::IndustrialWork => "industrial_work",
            Self::Substitution => "substitution",
        }
    }
    pub(crate) fn profile(self) -> TypedRecordProfileV1 {
        let (name, domain): (&'static str, &[u8]) = match self {
            Self::Requests => (
                "world_rules.requests.columns",
                b"oasis7.iws.world-rule-requests.columns.v1\0",
            ),
            Self::OrganizationContinuity => (
                "world_rules.organization_continuity.columns",
                b"oasis7.iws.world-rule-organization-continuity.columns.v1\0",
            ),
            Self::EmergencySupply => (
                "world_rules.emergency_supply.columns",
                b"oasis7.iws.world-rule-emergency-supply.columns.v1\0",
            ),
            Self::AgentOrders => (
                "world_rules.agent_orders.columns",
                b"oasis7.iws.world-rule-agent-orders.columns.v1\0",
            ),
            Self::IndustrialWork => (
                "world_rules.industrial_work.columns",
                b"oasis7.iws.world-rule-industrial-work.columns.v1\0",
            ),
            Self::Substitution => (
                "world_rules.substitution.columns",
                b"oasis7.iws.world-rule-substitution.columns.v1\0",
            ),
        };
        TypedRecordProfileV1 {
            field: FieldDescriptor {
                owner: FieldOwner::WorldState,
                name,
                destination: FieldDestination::State(StateNamespaceV1::Authorization),
                shape: FieldShape::Map,
            },
            schema: *blake3::hash(domain).as_bytes(),
            reference_schema: None,
            labels: RecordTreeLabelsV1 {
                read: "world rule composite state read",
                pending: "world rule composite pending nodes",
                tree: "world rule composite state tree",
                digest: "world rule composite node digest",
                read_allowance: "world rule composite read allowance",
                admission: "world rule composite admission",
                bound: "world rule composite bound",
                producer_pool: "world rule composite producer pool",
                page: "world rule composite state page",
                write_allowance: "world rule composite write allowance",
            },
        }
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("world rule composite binding")
}

pub(crate) fn prepare_branch_v1<
    T: Codec,
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    branch: WorldRuleCompositeBranchV1,
    source: &T,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut WorldRuleCompositeWriteWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _phase = ctx
        .memory
        .reserve(128 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let profile = branch.profile();
    let body = prepare_typed_record_body_v1(source, &profile, ctx, work)?;
    let semantic = (branch.name(), ());
    let owner = hash_semantic_key_v1(
        &semantic,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(profile.schema, owner.digest, (*body).clone())
        .map_err(|_| invalid())?;
    work.cells_remaining =
        work.cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "world rule composite output rows",
            ))?;
    let row = write_member_record_v1(
        crate::incremental_world_rule_parent::descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &semantic,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )?;
    crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)
}

pub(crate) fn recover_branch_v1<
    T: Codec,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    branch: WorldRuleCompositeBranchV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut WorldRuleCompositeReadWorkV1,
) -> Result<Option<RecoveredWorldRuleCompositeV1<T>>, ValueTreeError> {
    ctx.check()?;
    let _phase = ctx.values.memory.reserve(128 << 10, ctx.values.class)?;
    let (field, present) = crate::incremental_world_rule_parent::read_parent_fixed(ctx)?;
    let (key, _) = ctx.key(field, WorldFieldRecordKindV1::Entry, &(branch.name(), ()))?;
    let root = ctx.lookup(&key)?;
    if !present {
        return if root.is_none() {
            Ok(None)
        } else {
            Err(invalid())
        };
    }
    let root = root.ok_or_else(invalid)?;
    ctx.values
        .verify_closure(&root, ctx.record_bytes, &mut ctx.cancelled)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        &root,
        &key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: ctx.record_bytes,
        },
        &mut std::io::sink(),
        &mut &mut bytes[..],
        &mut ctx.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let profile = branch.profile();
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let child = reference
        .validate_binding(&profile.schema, &owner, StateNamespaceV1::Authorization)
        .map_err(|_| invalid())?;
    recover_typed_record_body_v1(child, &profile, ctx, scan, work).map(Some)
}
