//! Signed-role selection and promotion. Source work is borrowed from the
//! original caller's JSON/IO lanes; accepted source DATA grants no business rule.
use super::*;
use oasis7_formal_protocol::incremental_retention::{
    RETENTION_LEDGER_MANIFEST_MAX_BYTES_V1, RetentionLedgerBindingErrorV1, retention_root_id_v1,
};
#[derive(Debug)]
pub enum CognitionRetentionSourceErrorV1 {
    Value(ValueTreeError),
    MissingCommitment,
}
impl From<ValueTreeError> for CognitionRetentionSourceErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
pub struct CheckedCognitionRetentionLedgerV1 {
    class: MemoryClass,
    manifest: ReservedValue<RetentionLedgerManifestV1>,
    payload: Option<ReservedValue<JsonSubtreeQualificationV1>>,
    record: [u8; 32],
    body: [u8; 32],
    artifact: [u8; 32],
}
pub struct PreparedCognitionRetentionLedgerPromotionV1 {
    source: CheckedCognitionRetentionLedgerV1,
    fee: MemoryReservation,
}
impl CheckedCognitionRetentionLedgerV1 {
    pub fn validate_core_v1(&self, core: &CheckedCommitCore) -> Result<(), ValueTreeError> {
        if self.record != core.candidate_record_id()
            || self.body != core.catalog().entry.commit
            || self.manifest.roots != core.body().roots
            || self.manifest.previous_record != core.parent_record_id()
            || Some(self.artifact) != core.retention_ledger_artifact_id()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn descriptor(
    core: &CheckedCommitCore,
    raw: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    steps: &mut u64,
    bytes: &mut u64,
    stop: &mut impl FnMut() -> bool,
) -> Result<(ReservedValue<RetentionLedgerManifestV1>, [u8; 32]), CognitionRetentionSourceErrorV1> {
    let id = core
        .retention_ledger_artifact_id()
        .ok_or(CognitionRetentionSourceErrorV1::MissingCommitment)?;
    descriptor_binding(
        core.body().roots,
        core.parent_record_id(),
        id,
        raw,
        memory,
        class,
        steps,
        bytes,
        stop,
    )
}
fn descriptor_binding(
    roots: [u8; 32],
    previous_record: Option<[u8; 32]>,
    id: [u8; 32],
    raw: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    steps: &mut u64,
    bytes: &mut u64,
    stop: &mut impl FnMut() -> bool,
) -> Result<(ReservedValue<RetentionLedgerManifestV1>, [u8; 32]), CognitionRetentionSourceErrorV1> {
    if raw.len() > RETENTION_LEDGER_MANIFEST_MAX_BYTES_V1 {
        return Err(invalid().into());
    }
    // Admit the fixed original StateRoot/descriptor codec graph before decode.
    let phase = memory
        .reserve(128 * 1024 + size_of::<RetentionLedgerManifestV1>(), class)
        .map_err(ValueTreeError::Memory)?;
    let mut first = None;
    let mut pay = |s, b| {
        if first.is_some() {
            return Err(ExecutionError::Aborted);
        }
        let result = (|| {
            if stop() {
                return Err(ValueTreeError::Cancelled);
            }
            *steps = steps.checked_sub(s).ok_or(ValueTreeError::ResourceLimited(
                "retention source codec steps",
            ))?;
            *bytes = bytes.checked_sub(b).ok_or(ValueTreeError::ResourceLimited(
                "retention source codec bytes",
            ))?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                first = Some(e);
                Err(ExecutionError::Aborted)
            }
        }
    };
    let decoded = (|| {
        pay(1, raw.len() as u64)?;
        if *blake3::hash(raw).as_bytes() != id {
            return Err(ExecutionError::Invalid("retention signed descriptor hash"));
        }
        RetentionLedgerManifestV1::decode_with_work(raw, &mut pay)
    })();
    let manifest = match first {
        Some(e) => return Err(e.into()),
        None => decoded.map_err(execution)?,
    };
    if manifest.roots != roots || manifest.previous_record != previous_record {
        return Err(invalid().into());
    }
    Ok((phase.retain(manifest), id))
}
fn payload<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    manifest: &RetentionLedgerManifestV1,
    parent: Option<&ReservedValue<AcceptedCognitionRetentionLedgerV1>>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    cold: &mut JsonReadWorkV1,
    changed: &mut JsonChangeWorkV1,
) -> Result<Option<ReservedValue<JsonSubtreeQualificationV1>>, ValueTreeError> {
    let Some(root) = &manifest.payload else {
        return Ok(None);
    };
    let caller_stop = &mut read.cancelled;
    let mut source = WorldFieldReadContextV1 {
        root,
        state: read.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: read.values.source,
            memory: read.values.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: caller_stop,
    };
    match parent.and_then(|p| p.payload.as_ref()) {
        Some(q) if q.root() == root => {
            cold.cells_remaining = cold
                .cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("retention metadata copy"))?;
            copy_json_subtree_metadata_ordinary_v1(
                q,
                profile(),
                source.values.memory,
                &mut source.cancelled,
            )
            .map(Some)
        }
        Some(q) => advance_json_subtree_v1(profile(), q, root, &mut source, changed).map(Some),
        None => qualify_json_subtree_v1(profile(), root, &mut source, cold).map(Some),
    }
}
pub fn check_candidate_cognition_retention_ledger_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    core: &CheckedCommitCore,
    raw: &[u8],
    parent: Option<&ReservedValue<AcceptedCognitionRetentionLedgerV1>>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    cold: &mut JsonReadWorkV1,
    changed: &mut JsonChangeWorkV1,
) -> Result<ReservedValue<CheckedCognitionRetentionLedgerV1>, CognitionRetentionSourceErrorV1> {
    read.check()?;
    if read.values.class != MemoryClass::Ordinary {
        return Err(invalid().into());
    }
    let memory = read.values.memory;
    let (manifest, artifact) = descriptor(
        core,
        raw,
        memory,
        MemoryClass::Ordinary,
        &mut cold.cells_remaining,
        &mut read.values.io.read_bytes_remaining,
        &mut read.cancelled,
    )?;
    match parent {
        None if core.parent_record_id().is_none() => {}
        Some(parent)
            if parent.uses_budget(memory)
                && Some(parent.record) == core.parent_record_id()
                && core
                    .parent_body()
                    .is_some_and(|b| b.roots == parent.manifest.roots)
                && core.body().parent_commit == Some(parent.body) =>
        {
            parent.check(memory, MemoryClass::Ordinary)?
        }
        _ => return Err(invalid().into()),
    }
    let q = payload(&manifest, parent, read, cold, changed)?;
    let fee = memory
        .reserve(
            size_of::<CheckedCognitionRetentionLedgerV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    read.check()?;
    Ok(fee.retain(CheckedCognitionRetentionLedgerV1 {
        class: read.values.class,
        manifest,
        payload: q,
        record: core.candidate_record_id(),
        body: core.catalog().entry.commit,
        artifact,
    }))
}
/// The actual IO caller supplies raw bytes from the dedicated signed role.
/// This kernel rechecks the bytes, role, head/Core and entire JSON closure.
pub fn recover_qualified_cognition_retention_ledger_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    head: &VerifiedFormalCommitV1,
    core: &CheckedCommitCore,
    raw: &[u8],
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    cold: &mut JsonReadWorkV1,
) -> Result<ReservedValue<AcceptedCognitionRetentionLedgerV1>, CognitionRetentionSourceErrorV1> {
    read.check()?;
    if head.verified_record_id() != core.candidate_record_id() {
        return Err(invalid().into());
    }
    cold.cells_remaining = cold
        .cells_remaining
        .checked_sub(2)
        .ok_or(ValueTreeError::ResourceLimited("retention role selection"))?;
    let role = match retention_root_id_v1(head.record()) {
        Ok(id) => id,
        Err(RetentionLedgerBindingErrorV1::MissingCommitment) => {
            return Err(CognitionRetentionSourceErrorV1::MissingCommitment);
        }
        Err(_) => return Err(invalid().into()),
    };
    if core.retention_ledger_artifact_id() != Some(role) {
        return Err(invalid().into());
    }
    let memory = read.values.memory;
    let class = read.values.class;
    let (manifest, artifact) = descriptor(
        core,
        raw,
        memory,
        class,
        &mut cold.cells_remaining,
        &mut read.values.io.read_bytes_remaining,
        &mut read.cancelled,
    )?;
    let q = if let Some(root) = &manifest.payload {
        let mut source = WorldFieldReadContextV1 {
            root,
            state: read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: read.values.source,
                memory,
                class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: &mut read.cancelled,
        };
        Some(qualify_json_subtree_v1(profile(), root, &mut source, cold)?)
    } else {
        None
    };
    let fee = memory
        .reserve(size_of::<AcceptedCognitionRetentionLedgerV1>(), class)
        .map_err(ValueTreeError::Memory)?;
    read.check()?;
    Ok(fee.retain(AcceptedCognitionRetentionLedgerV1 {
        class,
        manifest,
        payload: q,
        record: head.verified_record_id(),
        body: core.catalog().entry.commit,
        artifact,
    }))
}
pub fn prepare_cognition_retention_ledger_promotion_v1(
    source: &ReservedValue<CheckedCognitionRetentionLedgerV1>,
    memory: &MemoryBudget,
    mut stop: impl FnMut() -> bool,
) -> Result<PreparedCognitionRetentionLedgerPromotionV1, ValueTreeError> {
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.class != MemoryClass::Ordinary
        || !source.manifest.uses_budget(memory)
    {
        return Err(invalid());
    }
    let fee = memory.reserve(
        size_of::<AcceptedCognitionRetentionLedgerV1>(),
        MemoryClass::Ordinary,
    )?;
    let mf = memory.reserve(
        size_of::<RetentionLedgerManifestV1>() + 512,
        MemoryClass::Ordinary,
    )?;
    let q = source
        .payload
        .as_ref()
        .map(|q| copy_json_subtree_metadata_ordinary_v1(q, profile(), memory, &mut stop))
        .transpose()?;
    let manifest = mf.retain(RetentionLedgerManifestV1 {
        roots: source.manifest.roots,
        previous_record: source.manifest.previous_record,
        payload: source.manifest.payload.clone(),
    });
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(PreparedCognitionRetentionLedgerPromotionV1 {
        source: CheckedCognitionRetentionLedgerV1 {
            class: source.class,
            manifest,
            payload: q,
            record: source.record,
            body: source.body,
            artifact: source.artifact,
        },
        fee,
    })
}
pub fn finish_prepared_cognition_retention_ledger_promotion_v1(
    prepared: PreparedCognitionRetentionLedgerPromotionV1,
    head: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<AcceptedCognitionRetentionLedgerV1>, ValueTreeError> {
    let source = prepared.source;
    if source.record != head.verified_record_id() {
        return Err(invalid());
    }
    Ok(prepared.fee.retain(AcceptedCognitionRetentionLedgerV1 {
        class: source.class,
        manifest: source.manifest,
        payload: source.payload,
        record: source.record,
        body: source.body,
        artifact: source.artifact,
    }))
}
pub fn readmit_cognition_retention_ordinary_v1(
    source: &ReservedValue<AcceptedCognitionRetentionLedgerV1>,
    head: &VerifiedFormalCommitV1,
    core: &CheckedCommitCore,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<AcceptedCognitionRetentionLedgerV1>, ValueTreeError> {
    common
        .charge(
            4,
            (size_of::<AcceptedCognitionRetentionLedgerV1>()
                + size_of::<RetentionLedgerManifestV1>()
                + 1536) as u64,
        )
        .map_err(formal)?;
    let memory = common.memory().clone();
    source.check(&memory, MemoryClass::Recovery)?;
    if !source.uses_budget(&memory)
        || head.verified_record_id() != source.record
        || core.candidate_record_id() != source.record
        || core.catalog().entry.commit != source.body
        || core.body().roots != source.manifest.roots
        || core.retention_ledger_artifact_id() != Some(source.artifact)
    {
        return Err(invalid());
    }
    let fee = memory.reserve(
        size_of::<AcceptedCognitionRetentionLedgerV1>(),
        MemoryClass::Ordinary,
    )?;
    let mf = memory.reserve(
        size_of::<RetentionLedgerManifestV1>() + 512,
        MemoryClass::Ordinary,
    )?;
    let (_, _, stop) = common.parts();
    let q = source
        .payload
        .as_ref()
        .map(|q| copy_json_subtree_metadata_ordinary_v1(q, profile(), &memory, &mut *stop))
        .transpose()?;
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(fee.retain(AcceptedCognitionRetentionLedgerV1 {
        class: MemoryClass::Ordinary,
        manifest: mf.retain(RetentionLedgerManifestV1 {
            roots: source.manifest.roots,
            previous_record: source.manifest.previous_record,
            payload: source.manifest.payload.clone(),
        }),
        payload: q,
        record: source.record,
        body: source.body,
        artifact: source.artifact,
    }))
}

fn accepted_history_binding(
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    runtime: &ReservedValue<
        crate::incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    >,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<([u8; 32], Option<[u8; 32]>, [u8; 32]), CognitionRetentionSourceErrorV1> {
    let occurrence = runtime.occurrence();
    if !runtime.uses_budget(memory)
        || runtime.allocation_class() != class
        || !occurrence.uses_budget(memory)
        || occurrence.allocation_class() != class
        || occurrence.head_record_id() != head.verified_record_id()
        || occurrence.fact_body_id() != occurrence.head_body_id()
    {
        return Err(invalid().into());
    }
    let _phase = memory
        .reserve(8192, class)
        .map_err(ValueTreeError::Memory)?;
    let digest = roots.digest().map_err(|_| invalid())?;
    if runtime.manifest().roots != digest || occurrence.roots_id() != digest {
        return Err(invalid().into());
    }
    let role = match retention_root_id_v1(head.record()) {
        Ok(id) => id,
        Err(RetentionLedgerBindingErrorV1::MissingCommitment) => {
            return Err(CognitionRetentionSourceErrorV1::MissingCommitment);
        }
        Err(_) => return Err(invalid().into()),
    };
    let text=head.record().artifact_roots.get(&oasis7_formal_protocol::controlled_authority::replicated_protocol::ArtifactRole::IncrementalCommitBody).ok_or_else(invalid)?;
    let mut body = [0; 32];
    hex::decode_to_slice(text, &mut body).map_err(|_| invalid())?;
    if body != occurrence.head_body_id() {
        return Err(invalid().into());
    }
    let previous=if head.record().kind==oasis7_formal_protocol::controlled_authority::formal::FormalRecordKindV1::InitialActivation{None}else{let mut previous=[0;32];hex::decode_to_slice(&head.record().parent_record_id,&mut previous).map_err(|_|invalid())?;Some(previous)};
    Ok((role, previous, body))
}
pub fn recover_qualified_cognition_retention_ledger_from_accepted_history_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    runtime: &ReservedValue<
        crate::incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    >,
    raw: &[u8],
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    json: &mut JsonReadWorkV1,
) -> Result<ReservedValue<AcceptedCognitionRetentionLedgerV1>, CognitionRetentionSourceErrorV1> {
    read.check()?;
    json.cells_remaining =
        json.cells_remaining
            .checked_sub(4096)
            .ok_or(ValueTreeError::ResourceLimited(
                "retention accepted history binding",
            ))?;
    read.values.io.read_bytes_remaining = read
        .values
        .io
        .read_bytes_remaining
        .checked_sub(8192)
        .ok_or(ValueTreeError::ResourceLimited(
            "retention accepted history binding bytes",
        ))?;
    let memory = read.values.memory;
    let class = read.values.class;
    let (artifact, previous, body) = accepted_history_binding(head, roots, runtime, memory, class)?;
    let (manifest, _) = descriptor_binding(
        runtime.manifest().roots,
        previous,
        artifact,
        raw,
        memory,
        class,
        &mut json.cells_remaining,
        &mut read.values.io.read_bytes_remaining,
        &mut read.cancelled,
    )?;
    let q = if let Some(root) = &manifest.payload {
        let mut source = WorldFieldReadContextV1 {
            root,
            state: read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: read.values.source,
                memory,
                class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: &mut read.cancelled,
        };
        Some(qualify_json_subtree_v1(profile(), root, &mut source, json)?)
    } else {
        None
    };
    read.check()?;
    let fee = memory
        .reserve(size_of::<AcceptedCognitionRetentionLedgerV1>(), class)
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(AcceptedCognitionRetentionLedgerV1 {
        class,
        manifest,
        payload: q,
        record: head.verified_record_id(),
        body,
        artifact,
    }))
}
pub fn readmit_cognition_retention_ordinary_from_accepted_history_v1(
    source: &ReservedValue<AcceptedCognitionRetentionLedgerV1>,
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    runtime: &ReservedValue<
        crate::incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    >,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<AcceptedCognitionRetentionLedgerV1>, CognitionRetentionSourceErrorV1> {
    common.charge(4096, 16384).map_err(formal)?;
    let memory = common.memory().clone();
    source.check(&memory, MemoryClass::Recovery)?;
    let (artifact, previous, body) =
        accepted_history_binding(head, roots, runtime, &memory, MemoryClass::Recovery)?;
    if !source.uses_budget(&memory)
        || source.record != head.verified_record_id()
        || source.artifact != artifact
        || source.body != body
        || source.manifest.roots != runtime.manifest().roots
        || source.manifest.previous_record != previous
    {
        return Err(invalid().into());
    }
    let fee = memory
        .reserve(
            size_of::<AcceptedCognitionRetentionLedgerV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mf = memory
        .reserve(
            size_of::<RetentionLedgerManifestV1>() + 512,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let (_, _, stop) = common.parts();
    let q = source
        .payload
        .as_ref()
        .map(|q| copy_json_subtree_metadata_ordinary_v1(q, profile(), &memory, &mut *stop))
        .transpose()?;
    if stop() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(fee.retain(AcceptedCognitionRetentionLedgerV1 {
        class: MemoryClass::Ordinary,
        manifest: mf.retain(RetentionLedgerManifestV1 {
            roots: source.manifest.roots,
            previous_record: source.manifest.previous_record,
            payload: source.manifest.payload.clone(),
        }),
        payload: q,
        record: source.record,
        body,
        artifact,
    }))
}
