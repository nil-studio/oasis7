//! Fixed projection observer over the resource family's sole closed grammar.
//! This is a descendant of receiver_record: its verifier, checkpoint, key
//! decoder and ScopedStateReader are shared rather than reconstructed.
use super::*;
use crate::incremental_field_member::read_member_record_header_v1;
use crate::incremental_json_records::CognitionAuxPathStepV1 as Step;
use crate::incremental_runtime_projection_aux::{
    Build, Payload, ReadWork, Selection, member::Text, owned_path,
};
use crate::incremental_semantic_key::{
    LedgerKeyRefV1, SemanticKeyRefV1 as Semantic, read_semantic_key_v1,
};
use crate::incremental_world_rule_projection_delta::{Processing, spend};
use oasis7_proto::memory_budget::MemoryReservation;
use std::mem::size_of;
pub(crate) fn project_actual_chain_resource_manifest_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    selected: Selection<'_>,
    read: &mut crate::incremental_field_read::WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut ReadWork<'_>,
    p: &Processing<'_>,
    build: &mut Build,
) -> Result<(), ValueTreeError> {
    read.check()?;
    let binding = WorldFieldKeyV1::from_bytes(selected.key()).map_err(ValueTreeError::Invalid)?;
    let field = Field::ChainResourceManifest;
    let key = WorldFieldKeyV1::new(field.descriptor(), binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if selected.namespace() != StateNamespaceV1::Artifacts || key.as_bytes() != selected.key() {
        return Err(invalid());
    }
    if binding.kind == WorldFieldRecordKindV1::Metadata {
        return Ok(());
    }
    if binding.kind != WorldFieldRecordKindV1::Entry {
        return Err(invalid());
    }
    let cancelled = RefCell::new(&mut read.cancelled);
    let mut verifier = FamilyVerifier {
        state: read.state,
        values: &mut read.values,
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        max_payload_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &cancelled,
        work: work.resource_cells,
    };
    verifier.spend_cell()?;
    let before = selected
        .before()
        .map(|id| verifier.read_outer_root(field, &id, &key))
        .transpose()?
        .flatten();
    let after = selected
        .after()
        .map(|id| verifier.read_outer_root(field, &id, &key))
        .transpose()?
        .flatten()
        .ok_or_else(invalid)?;
    match &before {
        Some(before) => verifier.advance_record(
            Record::ChainResourceManifest,
            before,
            &after,
            work.resource_scans,
        )?,
        None => {
            verifier.verify_record(Record::ChainResourceManifest, &after, work.resource_scans)?
        }
    }
    let path = Vec::new();
    let _path = build.memory.reserve(0, MemoryClass::Ordinary)?;
    project_record(
        &mut verifier,
        Record::ChainResourceManifest,
        before.as_ref(),
        &after,
        work.resource_scans,
        &mut work.json.paths_remaining,
        p,
        build,
        &path,
    )?;
    if verifier.cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = (verifier.take_state_error)() {
        return Err(error);
    }
    Ok(())
}
fn append_path<C: FnMut() -> bool>(
    path: &[Step],
    parts: &[Step],
    build: &Build,
    p: &Processing<'_>,
    cancel: &mut C,
) -> Result<(Vec<Step>, MemoryReservation), ValueTreeError> {
    let bytes = path.iter().chain(parts).try_fold(0usize, |n, v| {
        n.checked_add(
            size_of::<Step>()
                + match v {
                    Step::Name(v) => v.len(),
                    Step::Index(_) => 0,
                },
        )
        .ok_or_else(invalid)
    })?;
    spend(p, bytes, cancel)?;
    let fee = build.memory.reserve(bytes, MemoryClass::Ordinary)?;
    let mut out = Vec::new();
    out.try_reserve_exact(path.len() + parts.len())
        .map_err(|_| invalid())?;
    if out.capacity() > path.len() + parts.len() {
        return Err(invalid());
    }
    out.extend(path.iter().chain(parts).map(|v| match v {
        Step::Name(s) => Step::Name(s.clone()),
        Step::Index(n) => Step::Index(*n),
    }));
    Ok((out, fee))
}
fn container<C: FnMut() -> bool>(
    build: &mut Build,
    path: &[Step],
    array: bool,
    length: u64,
    reset: bool,
    p: &Processing<'_>,
    cancel: &mut C,
) -> Result<(), ValueTreeError> {
    build.push(
        "chain_resource_manifest",
        path,
        Payload::Container {
            array,
            length,
            reset,
        },
        p,
        cancel,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
fn project_record<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    verifier: &mut FamilyVerifier<'_, '_, '_, M, R, E, C>,
    record: Record,
    before: Option<&StateRootV1>,
    after: &StateRootV1,
    scans: &mut [StateScanBudget],
    frames: &mut u64,
    p: &Processing<'_>,
    build: &mut Build,
    path: &[Step],
) -> Result<(), ValueTreeError> {
    spend(p, 128, &mut || (verifier.cancelled.borrow_mut())())?;
    if before == Some(after) {
        return Ok(());
    }
    let (scan, child_scans) = scans.split_first_mut().ok_or_else(invalid)?;
    let source = ScopedStateReader {
        source: verifier.values.source,
        scan: RefCell::new(scan),
        state_reads: Cell::new(true),
    };
    let mut values = ValueReadContext {
        source: &source,
        memory: verifier.values.memory,
        class: verifier.values.class,
        io: &mut *verifier.values.io,
    };
    let mut row = FamilyVerifier {
        state: verifier.state,
        values: &mut values,
        key_work: &mut *verifier.key_work,
        semantic_bytes: verifier.semantic_bytes,
        record_bytes: verifier.record_bytes,
        max_payload_bytes: verifier.max_payload_bytes,
        take_state_error: &mut *verifier.take_state_error,
        cancelled: verifier.cancelled,
        work: &mut *verifier.work,
    };
    let state = row.checkpoint(record, after, &source)?;
    let cancel = row.cancelled;
    let mut stop = || (cancel.borrow_mut())();
    match record {
        Record::ChainResourceManifest => {
            container(
                build,
                path,
                false,
                14 + u64::from(state.present[0].ok_or_else(invalid)?)
                    + u64::from(state.present[1].ok_or_else(invalid)?),
                before.is_none(),
                p,
                &mut stop,
            )?;
            for (group, name) in ["resource_balances", "material_ledgers", "generated_chunks"]
                .into_iter()
                .enumerate()
            {
                let (v, _fee) = owned_path(&[name], p, &build.memory, &mut stop)?;
                let (next, _next_fee) = append_path(path, &v, build, p, &mut stop)?;
                container(
                    build,
                    &next,
                    false,
                    state.counts[group],
                    before.is_none(),
                    p,
                    &mut stop,
                )?;
            }
        }
        Record::MaterialLedger => container(
            build,
            path,
            false,
            state.counts[0],
            before.is_none(),
            p,
            &mut stop,
        )?,
        Record::ChunkManifest => {
            container(build, path, false, 17, before.is_none(), p, &mut stop)?;
            let (v, _fee) = owned_path(&["coord"], p, &build.memory, &mut stop)?;
            let (next, _next_fee) = append_path(path, &v, build, p, &mut stop)?;
            container(build, &next, false, 3, before.is_none(), p, &mut stop)?;
            for (group, name) in [
                "fragment_refs",
                "total_by_element_g",
                "remaining_by_element_g",
            ]
            .into_iter()
            .enumerate()
            {
                let (v, _fee) = owned_path(&[name], p, &build.memory, &mut stop)?;
                let (next, _next_fee) = append_path(path, &v, build, p, &mut stop)?;
                container(
                    build,
                    &next,
                    group == 0,
                    state.counts[group],
                    before.is_none(),
                    p,
                    &mut stop,
                )?;
            }
        }
        Record::FragmentRef => container(build, path, false, 5, before.is_none(), p, &mut stop)?,
        Record::CommitRef => container(
            build,
            path,
            false,
            1 + state.present.iter().filter(|v| **v == Some(true)).count() as u64,
            before.is_none(),
            p,
            &mut stop,
        )?,
        _ => return Err(invalid()),
    }
    let empty = StateRootV1::empty(StateNamespaceV1::Artifacts).map_err(|_| invalid())?;
    let mut records = InventoryValueReader {
        values: &mut *row.values,
        key_work: &mut *row.key_work,
        semantic_bytes: row.semantic_bytes,
        record_bytes: row.record_bytes,
        cancelled: &mut stop,
    };
    incremental_record_diff::visit_with_cells(
        before.unwrap_or(&empty),
        after,
        &mut records,
        frames,
        RecordDiffLabelsV1 {
            invalid: "Runtime resource projection paths",
            cells: "Runtime resource projection cells",
            stack: "Runtime resource projection stack",
            tree: TREE,
        },
        |old, new, records, frames| {
            let count = u64::from(old.is_some()) + u64::from(new.is_some());
            {
                let mut scan = source.scan.borrow_mut();
                scan.leaves_remaining = scan.leaves_remaining.checked_sub(count).ok_or(
                    ValueTreeError::ResourceLimited("Runtime resource projection leaves"),
                )?;
            }
            source.payload(|| {
                let entry = new.or(old).ok_or_else(invalid)?;
                let parsed =
                    WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                if matches!(
                    parsed.kind,
                    WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Group
                ) {
                    return Ok(());
                }
                let key =
                    WorldFieldKeyV1::new(record.descriptor(), parsed.kind, parsed.semantic_key)
                        .map_err(ValueTreeError::Invalid)?;
                if key.as_bytes() != entry.key || parsed.kind != WorldFieldRecordKindV1::Entry {
                    return Err(invalid());
                }
                let mut local = FamilyVerifier {
                    state: row.state,
                    values: &mut *records.values,
                    key_work: &mut *records.key_work,
                    semantic_bytes: row.semantic_bytes,
                    record_bytes: row.record_bytes,
                    max_payload_bytes: row.max_payload_bytes,
                    take_state_error: &mut *row.take_state_error,
                    cancelled: row.cancelled,
                    work: &mut *row.work,
                };
                local.spend_cell()?;
                let (cell, parsed_key) = local.key_only(record, entry)?;
                if matches!(
                    cell,
                    schema::Cell::Fixed {
                        count_group: Some(_),
                        ..
                    } | schema::Cell::Fixed {
                        option: Some((_, schema::OptionPart::Present)),
                        ..
                    }
                ) {
                    return Ok(());
                }
                if let schema::Cell::Dynamic {
                    group,
                    ordinal: true,
                    ..
                } = cell
                    && new.is_none()
                {
                    let key::KeyValue::Unsigned(index) = parsed_key.value else {
                        return Err(invalid());
                    };
                    if index >= state.counts[usize::from(group)] {
                        return Ok(());
                    }
                }
                let (path_tail, _tail_fee) =
                    semantic_path(&mut local, record, &key, &entry.value, parsed_key, p, build)?;
                let (next, _next_fee) =
                    append_path(path, &path_tail, build, p, &mut || (cancel.borrow_mut())())?;
                let Some(new) = new else {
                    return build.value::<u8, _>(
                        "chain_resource_manifest",
                        &next,
                        None,
                        p,
                        &mut || (cancel.borrow_mut())(),
                    );
                };
                let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
                    .map_err(ValueTreeError::Invalid)?
                    .semantic_key
                    .ok_or_else(invalid)?;
                let (_, _, _, child) = local.read_cell(record, &new.value, &key, owner)?;
                if let Some((child_record, after_child)) = child {
                    let before_child = old
                        .map(|old| local.read_cell(record, &old.value, &key, owner))
                        .transpose()?
                        .and_then(|v| v.3);
                    if before_child.as_ref().is_some_and(|v| v.0 != child_record) {
                        return Err(invalid());
                    }
                    return project_record(
                        &mut local,
                        child_record,
                        before_child.as_ref().map(|v| &v.1),
                        &after_child,
                        child_scans,
                        frames,
                        p,
                        build,
                        &next,
                    );
                }
                emit_scalar(
                    &mut local, record, &key, &new.value, cell, parsed_key, &next, p, build,
                )
            })
        },
    )
}

fn semantic_path<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    row: &mut FamilyVerifier<'_, '_, '_, M, R, E, C>,
    record: Record,
    key: &WorldFieldKeyV1,
    id: &[u8; 32],
    parsed: key::ParsedKey,
    p: &Processing<'_>,
    build: &Build,
) -> Result<(Vec<Step>, MemoryReservation), ValueTreeError> {
    let cancel = row.cancelled;
    let mut stop = || (cancel.borrow_mut())();
    let header = read_member_record_header_v1(
        id,
        key,
        row.values,
        MemberRecordLimitsV1 {
            semantic_bytes: row.semantic_bytes,
            payload_bytes: row.max_payload_bytes,
            record_bytes: row.record_bytes,
        },
        &mut stop,
    )?;
    let n = usize::try_from(header.semantic_bytes).map_err(|_| invalid())?;
    spend(p, n, &mut stop)?;
    let _raw_fee = build.memory.reserve(n, MemoryClass::Ordinary)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(n).map_err(|_| invalid())?;
    if bytes.capacity() > n {
        return Err(invalid());
    }
    bytes.resize(n, 0);
    read_member_record_key_v1(
        id,
        key,
        row.values,
        MemberRecordLimitsV1 {
            semantic_bytes: row.semantic_bytes,
            payload_bytes: row.max_payload_bytes,
            record_bytes: row.record_bytes,
        },
        &mut std::io::Cursor::new(&mut bytes[..]),
        &mut stop,
    )?;
    let semantic = read_semantic_key_v1(&bytes).map_err(ValueTreeError::Invalid)?;
    let label = parsed.label().map_err(ValueTreeError::Invalid)?;
    match semantic {
        Semantic::Text(actual) => {
            if actual != label {
                return Err(invalid());
            }
            if let Some(axis) = label.strip_prefix("coord.") {
                return owned_path(&["coord", axis], p, &build.memory, &mut stop);
            }
            owned_path(&[label], p, &build.memory, &mut stop)
        }
        Semantic::Pair { left, right } => {
            if left.decode().map_err(ValueTreeError::Invalid)? != Semantic::Text(label) {
                return Err(invalid());
            }
            match right.decode().map_err(ValueTreeError::Invalid)? {
                Semantic::Text("value")
                    if schema::options(record)
                        .iter()
                        .flatten()
                        .any(|v| v.0 == label) =>
                {
                    owned_path(&[label], p, &build.memory, &mut stop)
                }
                Semantic::Text(text) => {
                    if record == Record::MaterialLedger {
                        owned_path(&[text], p, &build.memory, &mut stop)
                    } else {
                        owned_path(&[label, text], p, &build.memory, &mut stop)
                    }
                }
                Semantic::Unsigned(index) => {
                    let (base, _basefee) = owned_path(&[label], p, &build.memory, &mut stop)?;
                    append_path(&base, &[Step::Index(index)], build, p, &mut stop)
                }
                Semantic::Resource(resource) => {
                    let text = json_key(&resource, build, p, &mut stop)?;
                    owned_path(&[label, text.as_str()], p, &build.memory, &mut stop)
                }
                Semantic::Ledger(ledger) => {
                    let bytes = match ledger {
                        LedgerKeyRefV1::World => 0,
                        LedgerKeyRefV1::Agent(v)
                        | LedgerKeyRefV1::Site(v)
                        | LedgerKeyRefV1::Factory(v) => v.len(),
                    };
                    spend(
                        p,
                        bytes + size_of::<oasis7_agent_api::world_action::MaterialLedgerId>(),
                        &mut stop,
                    )?;
                    let _clone = build.memory.reserve(
                        bytes + size_of::<oasis7_agent_api::world_action::MaterialLedgerId>(),
                        MemoryClass::Ordinary,
                    )?;
                    use oasis7_agent_api::world_action::MaterialLedgerId as Ledger;
                    let ledger = match ledger {
                        LedgerKeyRefV1::World => Ledger::World,
                        LedgerKeyRefV1::Agent(v) => Ledger::Agent(v.to_owned()),
                        LedgerKeyRefV1::Site(v) => Ledger::Site(v.to_owned()),
                        LedgerKeyRefV1::Factory(v) => Ledger::Factory(v.to_owned()),
                    };
                    let text = json_key(&ledger, build, p, &mut stop)?;
                    owned_path(&[label, text.as_str()], p, &build.memory, &mut stop)
                }
                _ => Err(invalid()),
            }
        }
        _ => Err(invalid()),
    }
}
fn json_key<T: serde::Serialize, C: FnMut() -> bool>(
    value: &T,
    build: &Build,
    p: &Processing<'_>,
    stop: &mut C,
) -> Result<ReservedValue<String>, ValueTreeError> {
    let encoded =
        crate::incremental_world_rule_projection_delta::json(value, &build.memory, p, stop)?;
    let n = encoded
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(size_of::<String>()))
        .ok_or_else(invalid)?;
    spend(p, n, stop)?;
    let fee = build.memory.reserve(n, MemoryClass::Ordinary)?;
    let text = serde_json::from_slice::<String>(&encoded).map_err(|_| invalid())?;
    Ok(fee.retain(text))
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
fn emit_scalar<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    row: &mut FamilyVerifier<'_, '_, '_, M, R, E, C>,
    record: Record,
    key: &WorldFieldKeyV1,
    id: &[u8; 32],
    cell: schema::Cell,
    parsed: key::ParsedKey,
    path: &[Step],
    p: &Processing<'_>,
    build: &mut Build,
) -> Result<(), ValueTreeError> {
    let payload = match cell {
        schema::Cell::Fixed { payload, .. } | schema::Cell::Dynamic { payload, .. } => payload,
        _ => return Err(invalid()),
    };
    let codec = payload.codec().ok_or_else(invalid)?;
    let cancel = row.cancelled;
    let mut stop = || (cancel.borrow_mut())();
    let memory = build.memory.clone();
    let mut output = Text {
        bytes: Vec::new(),
        fee: memory.reserve(0, MemoryClass::Ordinary)?,
        memory: &memory,
        p,
        first: None,
    };
    let info = read_member_record_header_v1(
        id,
        key,
        row.values,
        MemberRecordLimitsV1 {
            semantic_bytes: row.semantic_bytes,
            payload_bytes: row.max_payload_bytes,
            record_bytes: row.record_bytes,
        },
        &mut stop,
    )?;
    spend(
        p,
        key.as_bytes().len() + size_of::<WorldFieldKeyV1>(),
        &mut stop,
    )?;
    let _keyfee = memory.reserve(
        key.as_bytes().len() + size_of::<WorldFieldKeyV1>(),
        MemoryClass::Ordinary,
    )?;
    let mut records = InventoryValueReader {
        values: &mut *row.values,
        key_work: &mut *row.key_work,
        semantic_bytes: row.semantic_bytes,
        record_bytes: row.record_bytes,
        cancelled: &mut stop,
    };
    let value = read_record_value_v1(
        id,
        &(key.clone(), info.semantic_bytes),
        &mut records,
        row.max_payload_bytes,
        &mut output,
        RecordReadProfileV1 {
            codec,
            nullable: false,
            integer_max: match payload {
                schema::Payload::U64 { maximum } => maximum.unwrap_or(u64::MAX),
                _ => u64::MAX,
            },
            labels: RECORD_LABELS,
        },
    );
    if let Some(error) = output.first {
        return Err(error);
    }
    match value? {
        RecordValueV1::Unsigned(n)
            if record == Record::ChunkManifest
                && parsed.label().map_err(ValueTreeError::Invalid)? == "chunk_status" =>
        {
            use oasis7_agent_api::world_resource::ChainChunkResourceStatus as Status;
            let status = match n {
                0 => Status::Committed,
                1 => Status::ChainPending,
                2 => Status::Provisional,
                3 => Status::Exhausted,
                _ => return Err(invalid()),
            };
            build.value("chain_resource_manifest", path, Some(&status), p, &mut stop)
        }
        RecordValueV1::Unsigned(n) => {
            build.value("chain_resource_manifest", path, Some(&n), p, &mut stop)
        }
        RecordValueV1::Signed(n) => {
            build.value("chain_resource_manifest", path, Some(&n), p, &mut stop)
        }
        RecordValueV1::Bool(v) => {
            build.value("chain_resource_manifest", path, Some(&v), p, &mut stop)
        }
        RecordValueV1::Text { utf8_bytes } => {
            if utf8_bytes != output.bytes.len() as u64 {
                return Err(invalid());
            }
            let text = std::str::from_utf8(&output.bytes).map_err(|_| invalid())?;
            build.value("chain_resource_manifest", path, Some(text), p, &mut stop)
        }
        _ => Err(invalid()),
    }
}
