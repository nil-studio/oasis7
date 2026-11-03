//! Same-transaction changes to already qualified Runtime Artifact records.
use super::*;
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberPayloadRefV1, MemberRecordLimitsV1, MemberRecordWriteContext,
        read_member_record_v1, write_member_record_v1,
    },
    incremental_field_overlay::{
        WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, required_member,
    },
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::{cell::RefCell, io};

const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "runtime artifact delta state read",
    pending: "runtime artifact delta pending node allowance",
    tree: "runtime artifact delta typed child state tree",
    digest: "runtime artifact delta state node digest",
    read_allowance: "runtime artifact delta child read allowance",
    admission: "runtime artifact delta child source admission",
    bound: "runtime artifact delta child source bound",
    producer_pool: "runtime artifact delta producer pool",
    page: "runtime artifact delta state page",
    write_allowance: "runtime artifact delta write allowance",
};

const READ_LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "runtime artifact delta UTF-8",
    ledger_utf8: "runtime artifact delta ledger UTF-8",
    ledger_frame: "runtime artifact delta ledger framing",
    boolean: "runtime artifact delta boolean",
    fixed: "runtime artifact delta fixed payload",
    profile: "runtime artifact delta member profile",
    integer: "runtime artifact delta integer range",
};

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("runtime artifact delta tick overlay")
}

/// The first changed-only Runtime Artifacts transaction: update the required
/// `tick` cell inside an existing, qualified `latest_chain_resource_delta`.
/// Creation, deletion and full-delta replacement use separate owner operations.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeArtifactDeltaTickChangeV1 {
    pub expected: u64,
    pub after: u64,
}

/// Change only the delta's `tick` member. The qualified token must describe
/// this transaction's exact parent RootSet and the delta must be `Some` there.
/// The outer Metadata and Entry are the only ExecutionTransaction reads;
/// nested child pages are authenticated through the bound RootRef.
pub fn change_runtime_artifact_delta_tick_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter + StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: RuntimeArtifactDeltaTickChangeV1,
    qualification: &ReservedValue<RuntimeArtifactResourcesQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeArtifactResourcesWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let cancel_callbacks = RefCell::new((&mut context.cancelled, &mut write.cancelled));
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        let memory = context.values.memory;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(memory)
            || !transaction.uses_budget(write.memory)
            || !qualification.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        qualification.validate_binding(
            &transaction.parent_fence().parent_roots,
            context.values.class,
        )?;
        if qualification.record_counts()[1] != 1 {
            return Err(invalid().into());
        }
        let _workspace = memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;

        let mut cancelled = || {
            let mut callbacks = cancel_callbacks.borrow_mut();
            (callbacks.0)() || (callbacks.1)()
        };
        work.spend(&mut cancelled)?;

        let (outer_key, child_root, tick_key, current_root, current) = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancelled,
            };
            let field = RuntimeArtifactFieldV1::LatestChainResourceDelta;
            let (outer_key, outer_root, outer_semantic_bytes) =
                required_member(field.descriptor(), transaction, &mut records, error)?;
            work.spend(&mut *records.cancelled)?;
            let mut root_bytes = [0u8; RECORD_ROOT_BYTES];
            let info = read_member_record_v1(
                &outer_root,
                &outer_key,
                records.values,
                MemberRecordLimitsV1 {
                    semantic_bytes: records.semantic_bytes,
                    payload_bytes: RECORD_ROOT_BYTES as u64,
                    record_bytes: records.record_bytes,
                },
                &mut io::sink(),
                &mut &mut root_bytes[..],
                &mut *records.cancelled,
            )?;
            if info.codec != MemberPayloadCodecV1::RecordRoot
                || info.semantic_bytes != outer_semantic_bytes
                || info.payload_bytes != RECORD_ROOT_BYTES as u64
            {
                return Err(invalid().into());
            }
            let binding = WorldFieldKeyV1::from_bytes(outer_key.as_bytes())
                .map_err(ValueTreeError::Invalid)?;
            if binding.kind != WorldFieldRecordKindV1::Entry
                || binding.namespace != StateNamespaceV1::Artifacts
                || binding.semantic_key.is_none()
            {
                return Err(invalid().into());
            }
            let owner = binding.semantic_key.ok_or_else(invalid)?;
            let root_ref =
                RecordRootRefV1::from_canonical_bytes(&root_bytes).map_err(|_| invalid())?;
            let child_root = root_ref
                .validate_binding(&field.schema(), &owner, StateNamespaceV1::Artifacts)
                .map_err(|_| invalid())?
                .clone();

            work.spend(&mut *records.cancelled)?;
            let delta = RuntimeArtifactRecordV1::ChainResourceDelta;
            let (tick_key, tick_semantic_bytes) =
                records.key(delta.descriptor(), WorldFieldRecordKindV1::Entry, &"tick")?;
            let current_root =
                incremental_record_tree::lookup(&child_root, &tick_key, &mut records, LABELS)?
                    .ok_or_else(invalid)?;
            work.spend(&mut *records.cancelled)?;
            let current = read_record_value_v1(
                &current_root,
                &(tick_key.clone(), tick_semantic_bytes),
                &mut records,
                8,
                &mut io::sink(),
                RecordReadProfileV1 {
                    codec: MemberPayloadCodecV1::U64,
                    nullable: false,
                    integer_max: u64::MAX,
                    labels: READ_LABELS,
                },
            )?;
            (outer_key, child_root, tick_key, current_root, current)
        };
        if current != RecordValueV1::Unsigned(change.expected) {
            return Err(invalid().into());
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if change.expected == change.after {
            return Ok(false);
        }

        work.spend(&mut cancelled)?;
        let field = RuntimeArtifactFieldV1::LatestChainResourceDelta;
        let delta = RuntimeArtifactRecordV1::ChainResourceDelta;
        let mut writer = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || {
                let mut callbacks = cancel_callbacks.borrow_mut();
                (callbacks.0)() || (callbacks.1)()
            },
        };
        let row = write_member_record_v1(
            delta.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &"tick",
            MemberPayloadRefV1::U64(change.after),
            &mut writer,
        )?;
        if row.field_key != tick_key {
            return Err(invalid().into());
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let child_after = incremental_record_tree::edit(
            child_root,
            &tick_key,
            (Some(current_root), Some(row.value_root)),
            &mut context.values,
            &mut writer,
            &mut cancelled,
            RecordTreeWritePolicyV1 {
                max_pending_node_bytes: work.max_pending_node_bytes,
                labels: LABELS,
            },
        )?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let binding =
            WorldFieldKeyV1::from_bytes(outer_key.as_bytes()).map_err(ValueTreeError::Invalid)?;
        let owner = binding.semantic_key.ok_or_else(invalid)?;
        let root_ref =
            RecordRootRefV1::new(field.schema(), owner, child_after).map_err(|_| invalid())?;
        let outer = write_member_record_v1(
            field.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(),
            MemberPayloadRefV1::RecordRoot(&root_ref),
            &mut writer,
        )?;
        if outer.field_key != outer_key {
            return Err(invalid().into());
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(
            StateNamespaceV1::Artifacts,
            outer.field_key.as_bytes(),
            Some(outer.value_root),
        )?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
