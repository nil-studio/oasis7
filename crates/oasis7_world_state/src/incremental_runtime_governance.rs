//! Typed original Runtime governance controls, distinct from governance authority.
use crate::{
    GovernanceIdentityStatus,
    governance::{
        GovernanceExecutionPolicy, GovernanceFinalityEpochSnapshot,
        GovernanceIdentityPenaltyRecord, GovernanceIdentityPenaltyStatus,
    },
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_record_tree::{self, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeGovernanceKindV1 {
    Policy,
    EpochSnapshot,
    IdentityPenalty,
}
impl RuntimeGovernanceKindV1 {
    pub const ALL: [Self; 3] = [Self::Policy, Self::EpochSnapshot, Self::IdentityPenalty];
    pub fn descriptor(self) -> FieldDescriptor {
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: match self {
                Self::Policy => "governance_execution_policy",
                Self::EpochSnapshot => "governance_finality_epoch_snapshots",
                Self::IdentityPenalty => "governance_identity_penalties",
            },
            destination: FieldDestination::State(StateNamespaceV1::Authorization),
            shape: if self == Self::Policy {
                FieldShape::Value
            } else {
                FieldShape::Map
            },
        }
    }
    pub fn columns(self) -> FieldDescriptor {
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: match self {
                Self::Policy => "governance_execution_policy.columns",
                Self::EpochSnapshot => "governance_finality_epoch_snapshots.columns",
                Self::IdentityPenalty => "governance_identity_penalties.columns",
            },
            destination: FieldDestination::State(StateNamespaceV1::Authorization),
            shape: FieldShape::Map,
        }
    }
    pub fn schema(self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"oasis7.iws.runtime-governance.columns.v1\0");
        h.update(self.columns().name.as_bytes());
        *h.finalize().as_bytes()
    }
}
#[derive(Clone, Copy)]
pub enum RuntimeGovernanceRecordV1<'a> {
    Policy(&'a GovernanceExecutionPolicy),
    EpochSnapshot(&'a GovernanceFinalityEpochSnapshot),
    IdentityPenalty(&'a GovernanceIdentityPenaltyRecord),
}
impl RuntimeGovernanceRecordV1<'_> {
    pub fn kind(self) -> RuntimeGovernanceKindV1 {
        match self {
            Self::Policy(_) => RuntimeGovernanceKindV1::Policy,
            Self::EpochSnapshot(_) => RuntimeGovernanceKindV1::EpochSnapshot,
            Self::IdentityPenalty(_) => RuntimeGovernanceKindV1::IdentityPenalty,
        }
    }
}
pub struct RuntimeGovernanceWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "Runtime governance record read",
    pending: "Runtime governance pending nodes",
    tree: "Runtime governance record tree",
    digest: "Runtime governance digest",
    read_allowance: "Runtime governance read allowance",
    admission: "Runtime governance admission",
    bound: "Runtime governance bound",
    producer_pool: "Runtime governance producer pool",
    page: "Runtime governance page",
    write_allowance: "Runtime governance write allowance",
};
struct Builder<'a, 'b, W, C> {
    root: StateRootV1,
    kind: RuntimeGovernanceKindV1,
    ctx: &'a mut MemberRecordWriteContext<'b, W, C>,
    work: &'a mut RuntimeGovernanceWorkV1,
}
impl<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool> Builder<'_, '_, W, C> {
    fn spend(&mut self) -> Result<(), MemberRecordError> {
        if (self.ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        self.work.cells_remaining = self
            .work
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("Runtime governance cells"))?;
        Ok(())
    }
    fn append(&mut self, row: MemberRecordV1) -> Result<(), MemberRecordError> {
        self.root = incremental_record_tree::append(
            self.root.clone(),
            &row.field_key,
            row.value_root,
            self.ctx,
            RecordTreeWritePolicyV1 {
                max_pending_node_bytes: self.work.max_pending_node_bytes,
                labels: LABELS,
            },
        )?;
        Ok(())
    }
    fn column<K: SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
        value: MemberPayloadRefV1<'_>,
    ) -> Result<(), MemberRecordError> {
        self.spend()?;
        let row = write_member_record_v1(
            self.kind.columns(),
            WorldFieldRecordKindV1::Entry,
            key,
            value,
            self.ctx,
        )?;
        self.append(row)
    }
}
#[path = "incremental_runtime_governance_columns.rs"]
mod columns;
pub use columns::*;
macro_rules! with_gov_key {
    ($column:expr,$key:ident=>$body:expr) => {
        match $column {
            RuntimeGovernanceColumnV1::Policy(c) => {
                let $key = c.name();
                $body
            }
            RuntimeGovernanceColumnV1::Epoch(c) => {
                let $key = c.name();
                $body
            }
            RuntimeGovernanceColumnV1::Penalty(c) => {
                let $key = c.name();
                $body
            }
            RuntimeGovernanceColumnV1::SignerCount => {
                let $key = &("signer_node_ids", "count");
                $body
            }
            RuntimeGovernanceColumnV1::Signer(i) => {
                let $key = &("signer_node_ids", i);
                $body
            }
            RuntimeGovernanceColumnV1::StakeCount => {
                let $key = &("validator_stakes", "count");
                $body
            }
            RuntimeGovernanceColumnV1::Stake(id) => {
                let $key = &("validator_stakes", ("member", id));
                $body
            }
            RuntimeGovernanceColumnV1::OptionPresence(c) => {
                let $key = &(c.name(), "present");
                $body
            }
            RuntimeGovernanceColumnV1::OptionValue(c) => {
                let $key = &(c.name(), "value");
                $body
            }
        }
    };
}
fn emit_option_text<'a>(
    name: &str,
    value: Option<&'a str>,
    emit: &mut impl FnMut(
        RuntimeGovernanceColumnV1<'a>,
        RuntimeGovernanceValueV1<'a>,
    ) -> Result<(), MemberRecordError>,
) -> Result<(), MemberRecordError> {
    let option = GovernanceOptionV1::ALL
        .into_iter()
        .find(|c| c.name() == name)
        .ok_or(MemberRecordError::Invalid("governance option"))?;
    emit(
        RuntimeGovernanceColumnV1::OptionPresence(option),
        RuntimeGovernanceValueV1::Bool(value.is_some()),
    )?;
    if let Some(value) = value {
        emit(
            RuntimeGovernanceColumnV1::OptionValue(option),
            RuntimeGovernanceValueV1::Text(value),
        )?;
    }
    Ok(())
}
fn emit_option_u64<'a>(
    name: &str,
    value: Option<u64>,
    emit: &mut impl FnMut(
        RuntimeGovernanceColumnV1<'a>,
        RuntimeGovernanceValueV1<'a>,
    ) -> Result<(), MemberRecordError>,
) -> Result<(), MemberRecordError> {
    let option = GovernanceOptionV1::ALL
        .into_iter()
        .find(|c| c.name() == name)
        .ok_or(MemberRecordError::Invalid("governance option"))?;
    emit(
        RuntimeGovernanceColumnV1::OptionPresence(option),
        RuntimeGovernanceValueV1::Bool(value.is_some()),
    )?;
    if let Some(value) = value {
        emit(
            RuntimeGovernanceColumnV1::OptionValue(option),
            RuntimeGovernanceValueV1::Unsigned(value),
        )?;
    }
    Ok(())
}
pub fn visit_runtime_governance_columns_v1<'a>(
    source: RuntimeGovernanceRecordV1<'a>,
    mut emit: impl FnMut(
        RuntimeGovernanceColumnV1<'a>,
        RuntimeGovernanceValueV1<'a>,
    ) -> Result<(), MemberRecordError>,
) -> Result<(), MemberRecordError> {
    let kind = source.kind();
    macro_rules! u{($($name:ident),*$(,)?)=>{$(emit(RuntimeGovernanceColumnV1::scalar(kind,stringify!($name))?,RuntimeGovernanceValueV1::Unsigned(u64::from(*$name)))?;)*};}
    macro_rules! text{($($name:ident),*$(,)?)=>{$(emit(RuntimeGovernanceColumnV1::scalar(kind,stringify!($name))?,RuntimeGovernanceValueV1::Text($name))?;)*};}
    match source {
        RuntimeGovernanceRecordV1::Policy(source) => {
            let GovernanceExecutionPolicy {
                timelock_ticks,
                epoch_length_ticks,
                activation_delay_epochs,
                emergency_brake_guardian_threshold,
                emergency_veto_guardian_threshold,
                emergency_brake_max_ticks,
            } = source;
            u!(
                timelock_ticks,
                epoch_length_ticks,
                activation_delay_epochs,
                emergency_brake_guardian_threshold,
                emergency_veto_guardian_threshold,
                emergency_brake_max_ticks
            );
        }
        RuntimeGovernanceRecordV1::EpochSnapshot(source) => {
            let GovernanceFinalityEpochSnapshot {
                epoch_id,
                threshold_bps,
                min_unique_signers,
                validator_set_hash,
                stake_root,
                threshold,
                signer_node_ids,
                validator_stakes,
            } = source;
            u!(epoch_id, threshold_bps, min_unique_signers, threshold);
            text!(validator_set_hash, stake_root);
            emit(
                RuntimeGovernanceColumnV1::SignerCount,
                RuntimeGovernanceValueV1::Unsigned(
                    u64::try_from(signer_node_ids.len())
                        .map_err(|_| MemberRecordError::Invalid("signer count overflow"))?,
                ),
            )?;
            for (ordinal, id) in signer_node_ids.iter().enumerate() {
                emit(
                    RuntimeGovernanceColumnV1::Signer(
                        u64::try_from(ordinal)
                            .map_err(|_| MemberRecordError::Invalid("signer ordinal overflow"))?,
                    ),
                    RuntimeGovernanceValueV1::Text(id),
                )?;
            }
            emit(
                RuntimeGovernanceColumnV1::StakeCount,
                RuntimeGovernanceValueV1::Unsigned(
                    u64::try_from(validator_stakes.len())
                        .map_err(|_| MemberRecordError::Invalid("stake count overflow"))?,
                ),
            )?;
            for (id, stake) in validator_stakes {
                emit(
                    RuntimeGovernanceColumnV1::Stake(id.as_str()),
                    RuntimeGovernanceValueV1::Unsigned(*stake),
                )?;
            }
        }
        RuntimeGovernanceRecordV1::IdentityPenalty(source) => {
            let GovernanceIdentityPenaltyRecord {
                penalty_id,
                target_agent_id,
                evidence_hash,
                reason,
                slash_stake,
                appeal_deadline_tick,
                status,
                identity_status_before,
                detection_source,
                detection_risk_score,
                detection_incident_id,
                evidence_chain_hash,
                appeal_evidence_hash,
                resolution_evidence_hash,
                appellant,
                appeal_reason,
                resolved_by,
                resolution_reason,
                resolved_at_tick,
            } = source;
            u!(penalty_id, slash_stake, appeal_deadline_tick);
            text!(
                target_agent_id,
                evidence_hash,
                reason,
                detection_source,
                detection_incident_id,
                evidence_chain_hash
            );
            emit(
                RuntimeGovernanceColumnV1::scalar(kind, "detection_risk_score")?,
                RuntimeGovernanceValueV1::Signed(*detection_risk_score),
            )?;
            emit(
                RuntimeGovernanceColumnV1::scalar(kind, "status")?,
                RuntimeGovernanceValueV1::Unsigned(match status {
                    GovernanceIdentityPenaltyStatus::Applied => 0,
                    GovernanceIdentityPenaltyStatus::Appealed => 1,
                    GovernanceIdentityPenaltyStatus::AppealAccepted => 2,
                    GovernanceIdentityPenaltyStatus::AppealRejected => 3,
                }),
            )?;
            emit(
                RuntimeGovernanceColumnV1::scalar(kind, "identity_status_before")?,
                RuntimeGovernanceValueV1::Unsigned(match identity_status_before {
                    GovernanceIdentityStatus::Active => 0,
                    GovernanceIdentityStatus::Frozen => 1,
                    GovernanceIdentityStatus::Expelled => 2,
                }),
            )?;
            for (name, value) in [
                ("appeal_evidence_hash", appeal_evidence_hash),
                ("resolution_evidence_hash", resolution_evidence_hash),
                ("appellant", appellant),
                ("appeal_reason", appeal_reason),
                ("resolved_by", resolved_by),
                ("resolution_reason", resolution_reason),
            ] {
                emit_option_text(name, value.as_deref(), &mut emit)?;
            }
            emit_option_u64("resolved_at_tick", *resolved_at_tick, &mut emit)?;
        }
    }
    Ok(())
}
/// Emits a record-local authenticated tree; the caller publishes its outer reference atomically.
pub fn prepare_runtime_governance_record_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    source: RuntimeGovernanceRecordV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeGovernanceWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut builder = Builder {
        root: StateRootV1::empty(StateNamespaceV1::Authorization)
            .map_err(|_| MemberRecordError::Invalid("Runtime governance namespace"))?,
        kind: source.kind(),
        ctx,
        work,
    };
    builder.spend()?;
    let metadata = write_field_metadata_v1(
        builder.kind.columns(),
        &mut PresenceRecordWriteContext {
            sink: &mut *builder.ctx.sink,
            memory: builder.ctx.memory,
            io: &mut *builder.ctx.io,
            key_work: &mut *builder.ctx.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: builder.ctx.limits.semantic_bytes,
                record_bytes: builder.ctx.limits.record_bytes,
            },
            cancelled: &mut builder.ctx.cancelled,
        },
    )?;
    builder.root = incremental_record_tree::append(
        builder.root,
        &metadata.field_key,
        metadata.value_root,
        builder.ctx,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: builder.work.max_pending_node_bytes,
            labels: LABELS,
        },
    )?;
    visit_runtime_governance_columns_v1(source, |column, value| {
        let payload = column.payload(value)?;
        with_gov_key!(column,key=>builder.column(key,payload))
    })?;
    if (builder.ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(builder.root)
}
fn prepare_runtime_governance_member_for_key<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
>(
    source: RuntimeGovernanceRecordV1<'_>,
    original_key: &K,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeGovernanceWorkV1,
) -> Result<MemberRecordV1, MemberRecordError> {
    let root = prepare_runtime_governance_record_v1(source, ctx, work)?;
    let semantic = hash_semantic_key_v1(
        original_key,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(source.kind().schema(), semantic.digest, root)
        .map_err(|_| MemberRecordError::Invalid("Runtime governance root reference"))?;
    write_member_record_v1(
        source.kind().descriptor(),
        WorldFieldRecordKindV1::Entry,
        original_key,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeGovernanceKeyV1 {
    Policy,
    Epoch(u64),
    Penalty(u64),
}
pub fn prepare_runtime_governance_member_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
>(
    source: RuntimeGovernanceRecordV1<'_>,
    key: RuntimeGovernanceKeyV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeGovernanceWorkV1,
) -> Result<MemberRecordV1, MemberRecordError> {
    match (source.kind(), key) {
        (RuntimeGovernanceKindV1::Policy, RuntimeGovernanceKeyV1::Policy) => {
            prepare_runtime_governance_member_for_key(source, &(), ctx, work)
        }
        (RuntimeGovernanceKindV1::EpochSnapshot, RuntimeGovernanceKeyV1::Epoch(id))
        | (RuntimeGovernanceKindV1::IdentityPenalty, RuntimeGovernanceKeyV1::Penalty(id)) => {
            prepare_runtime_governance_member_for_key(source, &id, ctx, work)
        }
        _ => Err(MemberRecordError::Invalid("Runtime governance key kind")),
    }
}

pub struct RuntimeGovernanceControlsRefV1<'a> {
    pub policy: &'a GovernanceExecutionPolicy,
    pub snapshots: &'a std::collections::BTreeMap<u64, GovernanceFinalityEpochSnapshot>,
    pub penalties: &'a std::collections::BTreeMap<u64, GovernanceIdentityPenaltyRecord>,
}
pub fn prepare_runtime_governance_genesis_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(
        oasis7_proto::memory_budget::ReservedValue<
            oasis7_formal_protocol::incremental_execution::ExecutionWriteV1,
        >,
    ) -> Result<(), ValueTreeError>,
>(
    source: &RuntimeGovernanceControlsRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeGovernanceWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    for kind in RuntimeGovernanceKindV1::ALL {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        work.cells_remaining = work
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("Runtime governance cells"))?;
        let row = write_field_metadata_v1(
            kind.descriptor(),
            &mut PresenceRecordWriteContext {
                sink: &mut *ctx.sink,
                memory: ctx.memory,
                io: &mut *ctx.io,
                key_work: &mut *ctx.key_work,
                limits: PresenceRecordLimitsV1 {
                    semantic_bytes: ctx.limits.semantic_bytes,
                    record_bytes: ctx.limits.record_bytes,
                },
                cancelled: &mut ctx.cancelled,
            },
        )?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    fn emit<
        W: ValueNodeReader + StatePageWriter,
        C: FnMut() -> bool,
        S: FnMut(
            oasis7_proto::memory_budget::ReservedValue<
                oasis7_formal_protocol::incremental_execution::ExecutionWriteV1,
            >,
        ) -> Result<(), ValueTreeError>,
    >(
        source: RuntimeGovernanceRecordV1<'_>,
        key: RuntimeGovernanceKeyV1,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RuntimeGovernanceWorkV1,
        sink: &mut S,
    ) -> Result<(), MemberRecordError> {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        work.cells_remaining = work
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("Runtime governance cells"))?;
        let row = prepare_runtime_governance_member_v1(source, key, ctx, work)?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)
    }
    emit(
        RuntimeGovernanceRecordV1::Policy(source.policy),
        RuntimeGovernanceKeyV1::Policy,
        ctx,
        work,
        sink,
    )?;
    for (key, record) in source.snapshots {
        emit(
            RuntimeGovernanceRecordV1::EpochSnapshot(record),
            RuntimeGovernanceKeyV1::Epoch(*key),
            ctx,
            work,
            sink,
        )?;
    }
    for (key, record) in source.penalties {
        emit(
            RuntimeGovernanceRecordV1::IdentityPenalty(record),
            RuntimeGovernanceKeyV1::Penalty(*key),
            ctx,
            work,
            sink,
        )?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}

#[path = "incremental_runtime_governance_recovery.rs"]
mod recovery;
pub use recovery::*;

#[path = "incremental_runtime_governance_family.rs"]
mod family;
pub use family::*;

#[path = "incremental_runtime_governance_delta.rs"]
mod delta;
#[path = "incremental_runtime_governance_overlay.rs"]
mod overlay;
pub use overlay::{
    RuntimeGovernanceColumnChangeV1, RuntimeGovernanceRecordQualificationV1,
    change_runtime_governance_columns_v1, create_runtime_governance_record_v1,
    delete_runtime_governance_record_v1, open_runtime_governance_record_v1,
    readmit_runtime_governance_record_ordinary_v1,
};

#[cfg(test)]
#[path = "incremental_runtime_governance_tests.rs"]
mod tests;
