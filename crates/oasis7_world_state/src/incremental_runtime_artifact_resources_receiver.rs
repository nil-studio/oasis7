//! Changed-only replay for the currently supported Runtime Artifact tick edit.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::{FieldOwner, FieldShape},
    incremental_record_diff::{self, RecordDiffLabelsV1},
    incremental_record_tree::RecordTreeLabelsV1,
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
    incremental_runtime_artifact_resources::{
        RuntimeArtifactDeltaTickChangeV1, RuntimeArtifactFieldV1, RuntimeArtifactRecordV1,
        RuntimeArtifactResourcesWorkV1,
    },
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{PreparedTransition, TransitionRecordV1, TransitionVerifier},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::io;

const DIFF_TREE: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "runtime artifact transition child read",
    pending: "runtime artifact transition pending node allowance",
    tree: "runtime artifact transition child tree",
    digest: "runtime artifact transition node digest",
    read_allowance: "runtime artifact transition child read allowance",
    admission: "runtime artifact transition child admission",
    bound: "runtime artifact transition child bound",
    producer_pool: "runtime artifact transition producer pool",
    page: "runtime artifact transition state page",
    write_allowance: "runtime artifact transition write allowance",
};

const VALUE_LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "runtime artifact transition UTF-8",
    ledger_utf8: "runtime artifact transition ledger UTF-8",
    ledger_frame: "runtime artifact transition ledger frame",
    boolean: "runtime artifact transition boolean",
    fixed: "runtime artifact transition fixed value",
    profile: "runtime artifact transition member profile",
    integer: "runtime artifact transition integer range",
};

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("runtime artifact changed closure")
}

/// A generic replay plus this cohort's changed-only closure token. The prepared
/// transition is retained so the caller can publish only after all other owner
/// receipts have accepted the same record.
pub struct RuntimeArtifactResourcesPreparedTransitionV1 {
    transition: ReservedValue<PreparedTransition>,
    qualification: ReservedValue<RuntimeArtifactResourcesQualificationV1>,
}

impl RuntimeArtifactResourcesPreparedTransitionV1 {
    pub fn transition(&self) -> &PreparedTransition {
        &self.transition
    }

    pub fn qualification(&self) -> &ReservedValue<RuntimeArtifactResourcesQualificationV1> {
        &self.qualification
    }
}

fn read_outer_root<R: ValueNodeReader, C: FnMut() -> bool>(
    field: RuntimeArtifactFieldV1,
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<StateRootV1, ValueTreeError> {
    let (expected, semantic_bytes) =
        records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
    if expected != *key {
        return Err(invalid());
    }
    let mut bytes = [0u8; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        value,
        key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: records.record_bytes,
        },
        &mut io::sink(),
        &mut &mut bytes[..],
        &mut *records.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.semantic_bytes != semantic_bytes
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let binding = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    if binding.owner != FieldOwner::Runtime
        || binding.name != field.name()
        || binding.kind != WorldFieldRecordKindV1::Entry
        || binding.namespace != StateNamespaceV1::Artifacts
        || binding.shape != FieldShape::Value
    {
        return Err(invalid());
    }
    let owner = binding.semantic_key.ok_or_else(invalid)?;
    let root = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    root.validate_binding(&field.schema(), &owner, StateNamespaceV1::Artifacts)
        .cloned()
        .map_err(|_| invalid())
}

fn verify_tick_child<R: ValueNodeReader, C: FnMut() -> bool>(
    before: &StateRootV1,
    after: &StateRootV1,
    expected: u64,
    value: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<(), ValueTreeError> {
    if expected == value
        || before.namespace != StateNamespaceV1::Artifacts
        || after.namespace != StateNamespaceV1::Artifacts
    {
        return Err(invalid());
    }
    let field = RuntimeArtifactRecordV1::ChainResourceDelta;
    let (tick_key, semantic_bytes) =
        records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &"tick")?;
    let mut changed_ticks = 0u8;
    incremental_record_diff::visit(
        before,
        after,
        records,
        &mut work.cells_remaining,
        RecordDiffLabelsV1 {
            invalid: "runtime artifact child diff",
            cells: "runtime artifact changed child cells",
            stack: "runtime artifact child diff stack",
            tree: DIFF_TREE,
        },
        |old, new, records| {
            let (Some(old), Some(new)) = (old, new) else {
                return Err(invalid());
            };
            if old.namespace != StateNamespaceV1::Artifacts
                || new.namespace != StateNamespaceV1::Artifacts
                || old.key != tick_key.as_bytes()
                || new.key != tick_key.as_bytes()
            {
                return Err(invalid());
            }
            let profile = RecordReadProfileV1 {
                codec: MemberPayloadCodecV1::U64,
                nullable: false,
                integer_max: u64::MAX,
                labels: VALUE_LABELS,
            };
            for (root, wanted) in [(old.value, expected), (new.value, value)] {
                if read_record_value_v1(
                    &root,
                    &(tick_key.clone(), semantic_bytes),
                    records,
                    8,
                    &mut io::sink(),
                    profile,
                )? != RecordValueV1::Unsigned(wanted)
                {
                    return Err(invalid());
                }
            }
            changed_ticks = changed_ticks.checked_add(1).ok_or_else(invalid)?;
            Ok(())
        },
    )?;
    if changed_ticks != 1 {
        return Err(invalid());
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}

/// Replays a transition and validates its Artifact writes against the exact
/// qualified parent. This version accepts only a single `tick` change or no
/// Artifact changes; sibling replacement and family creation/deletion remain
/// separate operations.
pub fn verify_runtime_artifact_resources_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    change: Option<RuntimeArtifactDeltaTickChangeV1>,
    parent: &ReservedValue<RuntimeArtifactResourcesQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<RuntimeArtifactResourcesPreparedTransitionV1, WorldFieldOverlayErrorV1> {
    context.check()?;
    let memory = context.values.memory;
    let class = context.values.class;
    if class != MemoryClass::Ordinary
        || !parent.uses_budget(memory)
        || parent.reserved_bytes() < std::mem::size_of::<RuntimeArtifactResourcesQualificationV1>()
        || !verifier.memory.same_pool(memory)
        || verifier.class != class
        || verifier.source.allocation_class() != Some(class)
        || !verifier
            .source
            .allocation_budget()
            .is_some_and(|budget| budget.same_pool(memory))
    {
        return Err(invalid().into());
    }
    let before_digest = record.before.digest().map_err(|_| invalid())?;
    parent.validate_binding(&before_digest, class)?;
    if parent.record_counts()[1] != 1
        || context.root.namespace != StateNamespaceV1::Artifacts
        || context.root != record.after.state_root(StateNamespaceV1::Artifacts)
    {
        return Err(invalid().into());
    }
    let _workspace = memory
        .reserve(32 << 10, class)
        .map_err(ValueTreeError::Memory)?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let verified = verifier.verify(record);
    let source_error = (context.take_state_error)();
    context.check()?;
    if let Some(error) = source_error {
        return Err(error.into());
    }
    let verified = verified?;
    let mut changed_delta = false;
    for write in &verified.record.writes {
        work.spend(&mut context.cancelled)?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let binding =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        let Some(field) = RuntimeArtifactFieldV1::ALL
            .into_iter()
            .find(|field| field.name() == binding.name)
        else {
            continue;
        };
        let key = WorldFieldKeyV1::new(field.descriptor(), binding.kind, binding.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
        if write.namespace != StateNamespaceV1::Artifacts
            || binding.owner != FieldOwner::Runtime
            || binding.namespace != StateNamespaceV1::Artifacts
            || binding.shape != FieldShape::Value
            || key.as_bytes() != write.mutation.key
            || binding.kind != WorldFieldRecordKindV1::Entry
            || field != RuntimeArtifactFieldV1::LatestChainResourceDelta
            || changed_delta
        {
            return Err(invalid().into());
        }
        let (expected_key, _) =
            context.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
        if key != expected_key {
            return Err(invalid().into());
        }
        let (Some(before), Some(after), Some(change)) =
            (write.mutation.before, write.mutation.after, change)
        else {
            return Err(invalid().into());
        };
        let old_root = {
            let mut records = context.records();
            read_outer_root(field, &key, &before, &mut records)?
        };
        let new_root = {
            let mut records = context.records();
            read_outer_root(field, &key, &after, &mut records)?
        };
        let mut records = context.records();
        verify_tick_child(
            &old_root,
            &new_root,
            change.expected,
            change.after,
            &mut records,
            work,
        )?;
        changed_delta = true;
    }
    if change.is_some_and(|change| change.expected != change.after) != changed_delta {
        return Err(invalid().into());
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let after_digest = verified.record.after.digest().map_err(|_| invalid())?;
    let charge = memory
        .reserve(
            std::mem::size_of::<RuntimeArtifactResourcesQualificationV1>(),
            class,
        )
        .map_err(ValueTreeError::Memory)?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let qualification = charge.retain(RuntimeArtifactResourcesQualificationV1 {
        roots: after_digest,
        counts: parent.counts,
        class,
    });
    Ok(RuntimeArtifactResourcesPreparedTransitionV1 {
        transition: verified,
        qualification,
    })
}
