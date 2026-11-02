//! Ordinary point changes for the Runtime Artifacts family.
use super::*;
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberPayloadRefV1, MemberRecordError, MemberRecordLimitsV1,
        MemberRecordWriteContext, read_member_record_v1, write_member_record_v1,
    },
    incremental_field_overlay::{
        WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, required_member,
    },
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
    incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1},
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

const TREE_LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "runtime artifact body change read",
    pending: "runtime artifact body change pending allowance",
    tree: "runtime artifact body change child tree",
    digest: "runtime artifact body change node digest",
    read_allowance: "runtime artifact body change read allowance",
    admission: "runtime artifact body change admission",
    bound: "runtime artifact body change source bound",
    producer_pool: "runtime artifact body change producer pool",
    page: "runtime artifact body change page",
    write_allowance: "runtime artifact body change write allowance",
};

const READ_LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "runtime artifact body change UTF-8",
    ledger_utf8: "runtime artifact body change ledger UTF-8",
    ledger_frame: "runtime artifact body change ledger frame",
    boolean: "runtime artifact body change boolean",
    fixed: "runtime artifact body change fixed value",
    profile: "runtime artifact body change profile",
    integer: "runtime artifact body change integer range",
};

fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("runtime artifact body change")
}

fn state_root_digest(
    root: &oasis7_formal_protocol::incremental_state::StateRootV1,
) -> Result<[u8; 32], ValueTreeError> {
    let encoded = root.digest().map_err(|_| invalid())?;
    blake3::Hash::from_hex(encoded)
        .map(|digest| *digest.as_bytes())
        .map_err(|_| invalid())
}

fn decode_current_child<R: ValueNodeReader, C: FnMut() -> bool>(
    field: RuntimeArtifactFieldV1,
    outer_key: &WorldFieldKeyV1,
    value_root: &[u8; 32],
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<Option<oasis7_formal_protocol::incremental_state::StateRootV1>, WorldFieldOverlayErrorV1>
{
    let (expected_key, semantic_bytes) =
        records.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
    if expected_key != *outer_key {
        return Err(invalid().into());
    }
    let semantic = WorldFieldKeyV1::from_bytes(expected_key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let mut bytes = [0u8; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        value_root,
        outer_key,
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
    if expected_key != *outer_key || info.semantic_bytes != semantic_bytes {
        return Err(invalid().into());
    }
    if info.codec == MemberPayloadCodecV1::Unit {
        if field != RuntimeArtifactFieldV1::LatestChainResourceDelta || info.payload_bytes != 0 {
            return Err(invalid().into());
        }
        return Ok(None);
    }
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid().into());
    }
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    reference
        .validate_binding(&field.schema(), &semantic, StateNamespaceV1::Artifacts)
        .map_err(|_| invalid())
        .cloned()
        .map(Some)
        .map_err(Into::into)
}

fn write_outer_child<W, C>(
    field: RuntimeArtifactFieldV1,
    outer_key: &WorldFieldKeyV1,
    child: Option<oasis7_formal_protocol::incremental_state::StateRootV1>,
    writer: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<(WorldFieldKeyV1, [u8; 32]), MemberRecordError>
where
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
{
    let owner = hash_semantic_key_v1(
        &(),
        writer.memory,
        MemoryClass::Ordinary,
        writer.key_work,
        &mut writer.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = child
        .map(|child| {
            RecordRootRefV1::new(field.schema(), owner.digest, child)
                .map_err(|_| MemberRecordError::Invalid("runtime artifact body root reference"))
        })
        .transpose()?;
    let payload = match reference.as_ref() {
        Some(reference) => MemberPayloadRefV1::RecordRoot(reference),
        None if field == RuntimeArtifactFieldV1::LatestChainResourceDelta => {
            MemberPayloadRefV1::Unit
        }
        None => return Err(MemberRecordError::Invalid("runtime artifact required body")),
    };
    let row = write_member_record_v1(
        field.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(),
        payload,
        writer,
    )?;
    if row.field_key != *outer_key {
        return Err(MemberRecordError::Invalid("runtime artifact outer key"));
    }
    Ok((row.field_key, row.value_root))
}

/// Replace a complete snapshot-derived manifest or latest delta body. The
/// manifest is required; latest delta preserves the field's Unit/RecordRoot
/// None/Some distinction. Dynamic manifest and delta members are supplied by
/// the existing derivation input, while their child schema remains closed.
pub fn change_runtime_artifact_body_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    change: RuntimeArtifactBodyChangeV1<'_>,
    qualification: &ReservedValue<RuntimeArtifactResourcesQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeArtifactResourcesWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let callbacks = RefCell::new((&mut context.cancelled, &mut write.cancelled));
    let take_error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        let memory = context.values.memory;
        let class = context.values.class;
        if class != MemoryClass::Ordinary
            || !transaction.uses_budget(memory)
            || !transaction.uses_budget(write.memory)
            || !qualification.uses_budget(memory)
            || !memory.same_pool(write.memory)
        {
            return Err(invalid().into());
        }
        qualification.validate_binding(&transaction.parent_fence().parent_roots, class)?;
        if qualification.record_counts()[1] != 1 {
            return Err(invalid().into());
        }
        let _workspace = memory
            .reserve(16 << 10, class)
            .map_err(ValueTreeError::Memory)?;
        let mut cancelled = || {
            let mut both = callbacks.borrow_mut();
            (both.0)() || (both.1)()
        };
        work.spend(&mut cancelled)?;

        let (field, expected_root, after) = match change {
            RuntimeArtifactBodyChangeV1::ReplaceManifest {
                expected_root,
                after,
            } => (
                RuntimeArtifactFieldV1::ChainResourceManifest,
                Some(expected_root),
                Some(BodyRef::Manifest(after)),
            ),
            RuntimeArtifactBodyChangeV1::ReplaceLatestDelta {
                expected_root,
                after,
            } => (
                RuntimeArtifactFieldV1::LatestChainResourceDelta,
                expected_root,
                after.map(BodyRef::Delta),
            ),
        };
        let (outer_key, outer_value, child_before) = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancelled,
            };
            let (outer_key, outer_value, _) =
                required_member(field.descriptor(), transaction, &mut records, take_error)?;
            let child_before = decode_current_child(field, &outer_key, &outer_value, &mut records)?;
            (outer_key, outer_value, child_before)
        };
        let current_digest = child_before.as_ref().map(state_root_digest).transpose()?;
        if current_digest != expected_root {
            return Err(invalid().into());
        }
        if after.is_none() && current_digest.is_none() {
            if cancelled() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }

        let child_after = match after {
            Some(BodyRef::Manifest(manifest)) => {
                let mut writer = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || {
                        let mut both = callbacks.borrow_mut();
                        (both.0)() || (both.1)()
                    },
                };
                Some(genesis::record_manifest_body(manifest, &mut writer, work)?)
            }
            Some(BodyRef::Delta(delta)) => {
                let mut writer = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || {
                        let mut both = callbacks.borrow_mut();
                        (both.0)() || (both.1)()
                    },
                };
                Some(genesis::record_delta_body(delta, &mut writer, work)?)
            }
            None => None,
        };
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let after_digest = child_after.as_ref().map(state_root_digest).transpose()?;
        if after_digest == current_digest {
            if cancelled() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        work.spend(&mut cancelled)?;
        let outer = {
            let mut writer = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || {
                    let mut both = callbacks.borrow_mut();
                    (both.0)() || (both.1)()
                },
            };
            write_outer_child(field, &outer_key, child_after, &mut writer)?
        };
        if outer.0 != outer_key {
            return Err(invalid().into());
        }
        transaction.write(
            StateNamespaceV1::Artifacts,
            outer_key.as_bytes(),
            Some(outer.1),
        )?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let _ = outer_value;
        Ok(true)
    })
}

/// Apply the real Runtime upsert/remove unit: one module entry under the
/// original two String keys. A whole entry is an explicit new input, while
/// module-set and module counts are changed locally in the same contribution.
pub fn change_runtime_builtin_entry_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    change: RuntimeBuiltinEntryChangeV1<'_>,
    qualification: &ReservedValue<RuntimeArtifactResourcesQualificationV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeArtifactResourcesWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let callbacks = RefCell::new((&mut context.cancelled, &mut write.cancelled));
    let take_error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        let memory = context.values.memory;
        let class = context.values.class;
        if class != MemoryClass::Ordinary
            || !transaction.uses_budget(memory)
            || !transaction.uses_budget(write.memory)
            || !qualification.uses_budget(memory)
            || !memory.same_pool(write.memory)
        {
            return Err(invalid().into());
        }
        qualification.validate_binding(&transaction.parent_fence().parent_roots, class)?;
        if qualification.record_counts()[2] != 1
            || change.module_set.is_empty()
            || change.module_id.is_empty()
        {
            return Err(invalid().into());
        }
        let _workspace = memory
            .reserve(32 << 10, class)
            .map_err(ValueTreeError::Memory)?;
        let mut cancelled = || {
            let mut both = callbacks.borrow_mut();
            (both.0)() || (both.1)()
        };
        work.spend(&mut cancelled)?;
        let field = RuntimeArtifactFieldV1::BuiltinReleaseManifest;
        let record = RuntimeArtifactRecordV1::BuiltinReleaseManifest;
        let set_record = RuntimeArtifactRecordV1::BuiltinModuleSet;
        let entry_record = RuntimeArtifactRecordV1::BuiltinReleaseEntry;
        let (outer_key, top_root) = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancelled,
            };
            let (outer_key, outer_value, _) =
                required_member(field.descriptor(), transaction, &mut records, take_error)?;
            let top_root = decode_current_child(field, &outer_key, &outer_value, &mut records)?
                .ok_or_else(invalid)?;
            (outer_key, top_root)
        };

        let set_key = ("module_sets", change.module_set);
        let module_key = ("modules", change.module_id);
        let (set_root_before, set_value_before, set_count, top_count_root, top_count) = {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancelled,
            };
            let top_count_root =
                lookup_member_root(&top_root, record, &"module_sets.count", &mut records, work)?
                    .ok_or_else(invalid)?;
            let top_count =
                read_u64_root(&top_count_root, record, &"module_sets.count", &mut records)?;
            let set =
                read_nested_root(&top_root, record, &set_key, set_record, &mut records, work)?;
            match set {
                Some((value_root, child_root)) => {
                    let count_root = lookup_member_root(
                        &child_root,
                        set_record,
                        &"modules.count",
                        &mut records,
                        work,
                    )?
                    .ok_or_else(invalid)?;
                    let count =
                        read_u64_root(&count_root, set_record, &"modules.count", &mut records)?;
                    (
                        Some(child_root),
                        Some(value_root),
                        count,
                        top_count_root,
                        top_count,
                    )
                }
                None => {
                    if change.expected_entry_root.is_some() || change.after.is_none() {
                        if cancelled() {
                            return Err(ValueTreeError::Cancelled.into());
                        }
                        return if change.expected_entry_root.is_none() {
                            Ok(false)
                        } else {
                            Err(invalid().into())
                        };
                    }
                    (None, None, 0, top_count_root, top_count)
                }
            }
        };
        if top_count == 0 && (set_root_before.is_some() || top_root.leaf_count == 0) {
            return Err(invalid().into());
        }

        let module_before = if let Some(set_root) = &set_root_before {
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancelled,
            };
            read_nested_root(
                set_root,
                set_record,
                &module_key,
                entry_record,
                &mut records,
                work,
            )?
        } else {
            None
        };
        let (module_value_before, module_root_before) = match module_before {
            Some((value, root)) => (Some(value), Some(root)),
            None => (None, None),
        };
        let actual_entry_root = module_root_before
            .as_ref()
            .map(state_root_digest)
            .transpose()?;
        if actual_entry_root != change.expected_entry_root {
            return Err(invalid().into());
        }
        if change.after.is_none() && actual_entry_root.is_none() {
            if cancelled() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }

        let after_entry_root = if let Some(after) = &change.after {
            let mut writer = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || {
                    let mut both = callbacks.borrow_mut();
                    (both.0)() || (both.1)()
                },
            };
            Some(genesis::record_builtin_entry_body(
                after,
                &mut writer,
                work,
            )?)
        } else {
            None
        };
        let after_entry_digest = after_entry_root
            .as_ref()
            .map(state_root_digest)
            .transpose()?;
        if after_entry_digest == actual_entry_root {
            if cancelled() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }

        let set_is_new = set_root_before.is_none();
        let module_count_after = if change.after.is_some() {
            set_count.checked_add(u64::from(module_root_before.is_none()))
        } else {
            set_count.checked_sub(1)
        }
        .ok_or_else(invalid)?;
        let mut top_root_after = top_root.clone();
        if set_is_new {
            let entry = after_entry_root.ok_or_else(invalid)?;
            let mut set_root = oasis7_formal_protocol::incremental_state::StateRootV1::empty(
                StateNamespaceV1::Artifacts,
            )
            .map_err(|_| invalid())?;
            {
                let mut writer = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || {
                        let mut both = callbacks.borrow_mut();
                        (both.0)() || (both.1)()
                    },
                };
                genesis::record_builtin_set_prefix(&mut set_root, 1, &mut writer, work)?;
                genesis::append_root_member(
                    &mut set_root,
                    set_record,
                    &module_key,
                    entry_record,
                    entry,
                    &mut writer,
                    work,
                )?;
            }
            let mut writer = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || {
                    let mut both = callbacks.borrow_mut();
                    (both.0)() || (both.1)()
                },
            };
            edit_root_member(
                &mut top_root_after,
                record,
                &set_key,
                None,
                Some(write_root_member(record, &set_key, set_record, set_root, &mut writer)?.1),
                &mut context.values,
                &mut writer,
                work,
                &mut cancelled,
            )?;
            edit_u64(
                &mut top_root_after,
                record,
                &"module_sets.count",
                Some(top_count_root),
                top_count.checked_add(1).ok_or_else(invalid)?,
                &mut context.values,
                &mut writer,
                work,
                &mut cancelled,
            )?;
        } else if let Some(mut set_root_after) = set_root_before.clone() {
            if module_count_after == 0 {
                // Runtime's remove API prunes the parent set after its last
                // module; a preserved empty set would change that behavior.
                let mut writer = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || {
                        let mut both = callbacks.borrow_mut();
                        (both.0)() || (both.1)()
                    },
                };
                edit_root_member(
                    &mut top_root_after,
                    record,
                    &set_key,
                    set_value_before,
                    None,
                    &mut context.values,
                    &mut writer,
                    work,
                    &mut cancelled,
                )?;
                edit_u64(
                    &mut top_root_after,
                    record,
                    &"module_sets.count",
                    Some(top_count_root),
                    top_count.checked_sub(1).ok_or_else(invalid)?,
                    &mut context.values,
                    &mut writer,
                    work,
                    &mut cancelled,
                )?;
            } else {
                let mut writer = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || {
                        let mut both = callbacks.borrow_mut();
                        (both.0)() || (both.1)()
                    },
                };
                let after_member = after_entry_root
                    .map(|child| {
                        write_root_member(set_record, &module_key, entry_record, child, &mut writer)
                    })
                    .transpose()?
                    .map(|(_, value)| value);
                edit_root_member(
                    &mut set_root_after,
                    set_record,
                    &module_key,
                    module_value_before,
                    after_member,
                    &mut context.values,
                    &mut writer,
                    work,
                    &mut cancelled,
                )?;
                edit_u64(
                    &mut set_root_after,
                    set_record,
                    &"modules.count",
                    lookup_member_root(
                        set_root_before.as_ref().ok_or_else(invalid)?,
                        set_record,
                        &"modules.count",
                        &mut InventoryValueReader {
                            values: &mut context.values,
                            key_work: &mut *context.key_work,
                            semantic_bytes: context.semantic_bytes,
                            record_bytes: context.record_bytes,
                            cancelled: &mut cancelled,
                        },
                        work,
                    )?,
                    module_count_after,
                    &mut context.values,
                    &mut writer,
                    work,
                    &mut cancelled,
                )?;
                let mut writer = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || {
                        let mut both = callbacks.borrow_mut();
                        (both.0)() || (both.1)()
                    },
                };
                let after_set =
                    write_root_member(record, &set_key, set_record, set_root_after, &mut writer)?;
                edit_root_member(
                    &mut top_root_after,
                    record,
                    &set_key,
                    set_value_before,
                    Some(after_set.1),
                    &mut context.values,
                    &mut writer,
                    work,
                    &mut cancelled,
                )?;
            }
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut writer = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: || {
                let mut both = callbacks.borrow_mut();
                (both.0)() || (both.1)()
            },
        };
        let outer = write_outer_child(field, &outer_key, Some(top_root_after), &mut writer)?;
        if outer.0 != outer_key {
            return Err(invalid().into());
        }
        transaction.write(
            StateNamespaceV1::Artifacts,
            outer_key.as_bytes(),
            Some(outer.1),
        )?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}

fn lookup_member_root<R, C, K>(
    parent: &oasis7_formal_protocol::incremental_state::StateRootV1,
    record: RuntimeArtifactRecordV1,
    semantic: &K,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<Option<[u8; 32]>, ValueTreeError>
where
    R: ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
{
    work.spend(records.cancelled)?;
    let (key, _) = records.key(record.descriptor(), WorldFieldRecordKindV1::Entry, semantic)?;
    incremental_record_tree::lookup(parent, &key, records, TREE_LABELS)
}

fn read_u64_root<R, C, K>(
    value_root: &[u8; 32],
    record: RuntimeArtifactRecordV1,
    semantic: &K,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<u64, ValueTreeError>
where
    R: ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
{
    let (key, semantic_bytes) =
        records.key(record.descriptor(), WorldFieldRecordKindV1::Entry, semantic)?;
    match read_record_value_v1(
        value_root,
        &(key, semantic_bytes),
        records,
        8,
        &mut io::sink(),
        RecordReadProfileV1 {
            codec: MemberPayloadCodecV1::U64,
            nullable: false,
            integer_max: u64::MAX,
            labels: READ_LABELS,
        },
    )? {
        RecordValueV1::Unsigned(value) => Ok(value),
        _ => Err(invalid()),
    }
}

fn read_nested_root<R, C, K>(
    parent: &oasis7_formal_protocol::incremental_state::StateRootV1,
    parent_record: RuntimeArtifactRecordV1,
    semantic: &K,
    child_record: RuntimeArtifactRecordV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> Result<
    Option<(
        [u8; 32],
        oasis7_formal_protocol::incremental_state::StateRootV1,
    )>,
    WorldFieldOverlayErrorV1,
>
where
    R: ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
{
    let Some(value_root) = lookup_member_root(parent, parent_record, semantic, records, work)?
    else {
        return Ok(None);
    };
    let (key, semantic_bytes) = records.key(
        parent_record.descriptor(),
        WorldFieldRecordKindV1::Entry,
        semantic,
    )?;
    let mut bytes = [0u8; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        &value_root,
        &key,
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
        return Err(invalid().into());
    }
    let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let child = reference
        .validate_binding(&child_record.schema(), &owner, StateNamespaceV1::Artifacts)
        .map_err(|_| invalid())?
        .clone();
    Ok(Some((value_root, child)))
}

fn write_root_member<W, C, K>(
    parent: RuntimeArtifactRecordV1,
    semantic: &K,
    child_record: RuntimeArtifactRecordV1,
    child: oasis7_formal_protocol::incremental_state::StateRootV1,
    writer: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<(WorldFieldKeyV1, [u8; 32]), MemberRecordError>
where
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
{
    let reference = genesis::root_reference_for(child_record, semantic, child, writer)?;
    let row = write_member_record_v1(
        parent.descriptor(),
        WorldFieldRecordKindV1::Entry,
        semantic,
        MemberPayloadRefV1::RecordRoot(&reference),
        writer,
    )?;
    Ok((row.field_key, row.value_root))
}

fn write_u64_member<W, C, K>(
    record: RuntimeArtifactRecordV1,
    semantic: &K,
    value: u64,
    writer: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<(WorldFieldKeyV1, [u8; 32]), MemberRecordError>
where
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
{
    let row = write_member_record_v1(
        record.descriptor(),
        WorldFieldRecordKindV1::Entry,
        semantic,
        MemberPayloadRefV1::U64(value),
        writer,
    )?;
    Ok((row.field_key, row.value_root))
}

fn edit_root_member<R, W, C, CW, K>(
    root: &mut oasis7_formal_protocol::incremental_state::StateRootV1,
    record: RuntimeArtifactRecordV1,
    semantic: &K,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    writer: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeArtifactResourcesWorkV1,
    cancelled: &mut C,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    R: ValueNodeReader,
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
{
    work.spend(cancelled)?;
    let (key, _) = {
        let mut reader = InventoryValueReader {
            values,
            key_work: &mut *writer.key_work,
            semantic_bytes: writer.limits.semantic_bytes,
            record_bytes: writer.limits.record_bytes,
            cancelled: &mut *cancelled,
        };
        reader.key(record.descriptor(), WorldFieldRecordKindV1::Entry, semantic)?
    };
    *root = incremental_record_tree::edit(
        root.clone(),
        &key,
        (before, after),
        values,
        writer,
        cancelled,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: TREE_LABELS,
        },
    )?;
    Ok(())
}

fn edit_u64<R, W, C, CW, K>(
    root: &mut oasis7_formal_protocol::incremental_state::StateRootV1,
    record: RuntimeArtifactRecordV1,
    semantic: &K,
    before: Option<[u8; 32]>,
    after: u64,
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    writer: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut RuntimeArtifactResourcesWorkV1,
    cancelled: &mut C,
) -> Result<(), WorldFieldOverlayErrorV1>
where
    R: ValueNodeReader,
    W: ValueNodeWriter + StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
    K: SemanticKeyV1 + ?Sized,
{
    let (key, value) = write_u64_member(record, semantic, after, writer)?;
    let old = before.ok_or_else(invalid)?;
    let _ = key;
    edit_root_member(
        root,
        record,
        semantic,
        Some(old),
        Some(value),
        values,
        writer,
        work,
        cancelled,
    )?;
    Ok(())
}

enum BodyRef<'a> {
    Manifest(&'a ChainResourceManifest),
    Delta(&'a ChainResourceDelta),
}
