//! Original Runtime revocation DTO deltas from its genuine typed Current.
//! The original changed-path walker prunes equal subtrees. Collections emit
//! only actual changed members and their authenticated current lengths, never
//! a reconstructed World or a caller-supplied authorization/status verdict.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_typed_record_codec::recover_typed_cell_v1,
    incremental_typed_record_diff::{self as diff, ChangeDecoder, TypedCellRefV1},
    incremental_world_rule_projection_delta::encode_original_projection_json_data_v1,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation, ReservedValue};
use std::mem::size_of;

#[derive(Clone, Copy)]
pub enum RevocationProjectionContainerV1 {
    Object,
    Map,
    Set,
    Sequence,
}
/// DATA-only borrowed update. Container presence is the original serde emission
/// (including its fixed skip-empty predicates); counts are authenticated codec
/// observations. Neither grants access, task status, accepted history or execution permission.
pub enum RevocationProjectionPatchRefV1<'a> {
    Value {
        path: &'static [&'static str],
        json: Option<&'a [u8]>,
    },
    Member {
        path: &'static [&'static str],
        key: &'a str,
        json: Option<&'a [u8]>,
    },
    SetMember {
        path: &'static [&'static str],
        key: &'a str,
        present: bool,
    },
    SequenceMember {
        path: &'static [&'static str],
        index: u64,
        json: Option<&'a [u8]>,
    },
    Container {
        path: &'static [&'static str],
        kind: RevocationProjectionContainerV1,
        present: bool,
        length: u64,
    },
}
enum Selector {
    Value,
    Member(ReservedValue<String>),
    SetMember(ReservedValue<String>, bool),
    SequenceMember(u64),
    Container(RevocationProjectionContainerV1, bool, u64),
}
struct Patch {
    path: &'static [&'static str],
    selector: Selector,
    json: Option<ReservedValue<Vec<u8>>>,
}
pub struct CurrentRuntimeRevocationProjectionDeltaV1 {
    patches: Vec<ReservedValue<Patch>>,
    slots: MemoryReservation,
    guard: PreparedGuard,
    before: StateRootV1,
    after: StateRootV1,
    parent: [u8; 32],
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl CurrentRuntimeRevocationProjectionDeltaV1 {
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_read_source_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !self._fee.uses_budget(memory)
            || !self.slots.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.parent != tx.parent_fence().parent_roots
        {
            return Err(invalid().into());
        }
        Ok(())
    }
    pub fn before_body(&self) -> &StateRootV1 {
        &self.before
    }
    pub fn after_body(&self) -> &StateRootV1 {
        &self.after
    }
    pub fn visit_with_work_v1<E>(
        &self,
        mut work: impl FnMut(u64, u64) -> Result<(), E>,
        mut emit: impl FnMut(RevocationProjectionPatchRefV1<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        for patch in &self.patches {
            work(1, 0)?;
            let path = patch.path;
            let json = patch.json.as_deref().map(Vec::as_slice);
            emit(match &patch.selector {
                Selector::Value => RevocationProjectionPatchRefV1::Value { path, json },
                Selector::Member(key) => RevocationProjectionPatchRefV1::Member { path, key, json },
                Selector::SetMember(key, present) => RevocationProjectionPatchRefV1::SetMember {
                    path,
                    key,
                    present: *present,
                },
                Selector::SequenceMember(index) => RevocationProjectionPatchRefV1::SequenceMember {
                    path,
                    index: *index,
                    json,
                },
                Selector::Container(kind, present, length) => {
                    RevocationProjectionPatchRefV1::Container {
                        path,
                        kind: *kind,
                        present: *present,
                        length: *length,
                    }
                }
            })?;
        }
        Ok(())
    }
}
fn spend(
    processing: &mut BusinessDiffProcessingWorkV1,
    n: usize,
    stop: &mut impl FnMut() -> bool,
) -> Result<(), ValueTreeError> {
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    let bytes = u64::try_from(n).map_err(|_| invalid())?;
    let steps = bytes.checked_add(1).ok_or_else(invalid)?;
    let old = (processing.steps_remaining, processing.bytes_remaining);
    processing.steps_remaining = processing.steps_remaining.saturating_sub(steps);
    processing.bytes_remaining = processing.bytes_remaining.saturating_sub(bytes);
    if old.0 < steps || old.1 < bytes {
        return Err(ValueTreeError::ResourceLimited(
            "revocation projection processing",
        ));
    }
    Ok(())
}
struct Output {
    patches: Vec<ReservedValue<Patch>>,
    slots: MemoryReservation,
    memory: MemoryBudget,
}
impl Output {
    fn push(
        &mut self,
        path: &'static [&'static str],
        selector: Selector,
        json: Option<ReservedValue<Vec<u8>>>,
        processing: &mut BusinessDiffProcessingWorkV1,
        stop: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        let text = match &selector {
            Selector::Member(v) | Selector::SetMember(v, _) => v.capacity(),
            _ => 0,
        };
        let bytes = size_of::<Patch>().checked_add(text).ok_or_else(invalid)?;
        spend(processing, bytes, stop)?;
        let fee = self.memory.reserve(bytes, MemoryClass::Ordinary)?;
        if self.patches.len() == self.patches.capacity() {
            let cap = self.patches.capacity().saturating_mul(2).max(4);
            let bytes = cap
                .checked_mul(size_of::<ReservedValue<Patch>>())
                .ok_or_else(invalid)?;
            spend(processing, bytes, stop)?;
            let slots = self.memory.reserve(bytes, MemoryClass::Ordinary)?;
            self.patches
                .try_reserve_exact(cap - self.patches.len())
                .map_err(|_| invalid())?;
            if self.patches.capacity() != cap {
                return Err(invalid());
            }
            self.slots = slots;
        }
        self.patches.push(fee.retain(Patch {
            path,
            selector,
            json,
        }));
        Ok(())
    }
}
fn text(
    value: &str,
    memory: &MemoryBudget,
    processing: &mut BusinessDiffProcessingWorkV1,
    stop: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<String>, ValueTreeError> {
    spend(processing, value.len(), stop)?;
    let fee = memory.reserve(value.len() + size_of::<String>(), MemoryClass::Ordinary)?;
    let copied = value.to_owned();
    if copied.capacity() != value.len() {
        return Err(invalid());
    }
    Ok(fee.retain(copied))
}
fn cell_copy(
    cell: &ColumnCellV1,
    memory: &MemoryBudget,
    processing: &mut BusinessDiffProcessingWorkV1,
    stop: &mut impl FnMut() -> bool,
) -> Result<(ColumnCellV1, MemoryReservation), ValueTreeError> {
    let n = size_of::<ColumnCellV1>()
        .checked_add(match cell {
            ColumnCellV1::Text(v) => v.len(),
            ColumnCellV1::Root(v) => v.node_root.len(),
            _ => 0,
        })
        .ok_or_else(invalid)?;
    spend(processing, n, stop)?;
    let fee = memory.reserve(n, MemoryClass::Ordinary)?;
    let cell = match cell {
        ColumnCellV1::Unit => ColumnCellV1::Unit,
        ColumnCellV1::Bool(v) => ColumnCellV1::Bool(*v),
        ColumnCellV1::U64(v) => ColumnCellV1::U64(*v),
        ColumnCellV1::I64(v) => ColumnCellV1::I64(*v),
        ColumnCellV1::Text(v) => ColumnCellV1::Text(v.clone()),
        ColumnCellV1::Root(v) => ColumnCellV1::Root(v.clone()),
    };
    Ok((cell, fee))
}
fn value<
    T: Codec + serde::Serialize,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    processing: &mut BusinessDiffProcessingWorkV1,
) -> Result<Option<ReservedValue<Vec<u8>>>, ValueTreeError> {
    let Some(after) = after else { return Ok(None) };
    let profile = d.profile;
    let row = diff::row(&profile, after, d)?;
    let (cell, _copy) = cell_copy(
        &row.cell,
        d.ctx.values.memory,
        processing,
        &mut d.ctx.cancelled,
    )?;
    let recovered = recover_typed_cell_v1::<T, _, _, _, _>(cell, &profile, d.ctx, d.scan, d.work)?;
    encode_original_projection_json_data_v1(
        recovered.value(),
        d.ctx.values.memory,
        processing,
        &mut d.ctx.cancelled,
    )
    .map(Some)
}
fn scalar<
    T: Codec + serde::Serialize,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    path: &'static [&'static str],
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    processing: &mut BusinessDiffProcessingWorkV1,
    out: &mut Output,
) -> Result<(), ValueTreeError> {
    let json = value::<T, _, _, _, _>(after, d, processing)?;
    out.push(
        path,
        Selector::Value,
        json,
        processing,
        &mut d.ctx.cancelled,
    )
}
fn container<
    T: Codec + serde::Serialize,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    path: &'static [&'static str],
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    processing: &mut BusinessDiffProcessingWorkV1,
    out: &mut Output,
    kind: RevocationProjectionContainerV1,
) -> Result<(), ValueTreeError> {
    if diff::unchanged(before, after) {
        return Ok(());
    }
    let profile = d.profile;
    let physical_present = after.is_some();
    let length = if let Some(after) = after {
        let root = diff::root(&profile, after, d)?;
        root.leaf_count.checked_sub(2).ok_or_else(invalid)?
    } else {
        0
    };
    let skips_empty = matches!(
        path,
        ["manufactured_identity_acquisition_sequences"]
            | ["agent_signer_delegations"]
            | ["world_service_results"]
            | ["safe_alternatives"]
            | ["participants", "policy_history"]
            | ["participants", "migrated_bindings"]
            | ["participants", "devlocal_policy_history"]
    );
    let emitted = physical_present && (!skips_empty || length != 0);
    out.push(
        path,
        Selector::Container(kind, emitted, length),
        None,
        processing,
        &mut d.ctx.cancelled,
    )?;
    diff::accept_container::<T, _, _, _, _>(
        &profile,
        before,
        after,
        d,
        matches!(kind, RevocationProjectionContainerV1::Sequence),
        |key, old, new, d| {
            spend(processing, 0, &mut d.ctx.cancelled)?;
            let (selector, json) = match kind {
                RevocationProjectionContainerV1::Map | RevocationProjectionContainerV1::Set => {
                    let ColumnKeyV1::Pair(tag, key) = key else {
                        return Err(invalid());
                    };
                    let (ColumnKeyV1::Text(tag), ColumnKeyV1::Text(key)) = (&**tag, &**key) else {
                        return Err(invalid());
                    };
                    if tag != "entry" {
                        return Err(invalid());
                    }
                    let key = text(key, d.ctx.values.memory, processing, &mut d.ctx.cancelled)?;
                    if matches!(kind, RevocationProjectionContainerV1::Set) {
                        if let Some(new) = new {
                            let profile = d.profile;
                            let row = diff::row(&profile, new, d)?;
                            if !matches!(row.cell, ColumnCellV1::Bool(true)) {
                                return Err(invalid());
                            }
                        }
                        (Selector::SetMember(key, new.is_some()), None)
                    } else {
                        (
                            Selector::Member(key),
                            value::<T, _, _, _, _>(new, d, processing)?,
                        )
                    }
                }
                RevocationProjectionContainerV1::Sequence => {
                    let ColumnKeyV1::Ordinal(index) = key else {
                        return Err(invalid());
                    };
                    (
                        Selector::SequenceMember(*index),
                        value::<T, _, _, _, _>(new, d, processing)?,
                    )
                }
                RevocationProjectionContainerV1::Object => return Err(invalid()),
            };
            let _ = old;
            if emitted {
                out.push(path, selector, json, processing, &mut d.ctx.cancelled)
            } else {
                Ok(())
            }
        },
    )
}

/// Exactly ParticipantRegistryV1::is_empty, using authenticated small
/// headers. It never loads policy-history, binding or operation payloads.
fn participants_empty<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    cell: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    processing: &mut BusinessDiffProcessingWorkV1,
) -> Result<bool, ValueTreeError> {
    let Some(cell) = cell else { return Ok(true) };
    let profile = d.profile;
    let body = diff::root(&profile, cell, d)?;
    spend(processing, 0, &mut d.ctx.cancelled)?;
    let policy = diff::point_typed_column_v1(&profile, &body, "policy", d, 8192)?;
    if !matches!(policy.cell, ColumnCellV1::Unit) {
        if !matches!(policy.cell, ColumnCellV1::Root(_)) {
            return Err(invalid());
        }
        return Ok(false);
    }
    for name in [
        "policy_history",
        "devlocal_policy_history",
        "migrated_bindings",
        "bindings",
        "operations",
    ] {
        spend(processing, name.len(), &mut d.ctx.cancelled)?;
        let row = diff::point_typed_column_v1(&profile, &body, name, d, 8192)?;
        let ColumnCellV1::Root(root) = &row.cell else {
            return Err(invalid());
        };
        let length = diff::point_typed_column_v1(&profile, root, "_length", d, 8192)?;
        let ColumnCellV1::U64(count) = length.cell else {
            return Err(invalid());
        };
        if count.checked_add(2) != Some(root.leaf_count) {
            return Err(invalid());
        }
        if count != 0 {
            return Ok(false);
        }
    }
    Ok(true)
}
fn participants<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    processing: &mut BusinessDiffProcessingWorkV1,
    out: &mut Output,
) -> Result<(), ValueTreeError> {
    let profile = d.profile;
    let old_empty = participants_empty(before, d, processing)?;
    let new_empty = participants_empty(after, d, processing)?;
    if new_empty {
        return out.push(
            &["participants"],
            Selector::Value,
            None,
            processing,
            &mut d.ctx.cancelled,
        );
    }
    out.push(
        &["participants"],
        Selector::Container(RevocationProjectionContainerV1::Object, true, 6),
        None,
        processing,
        &mut d.ctx.cancelled,
    )?;
    // An omitted original parent had all six actual empty/null fields. Open
    // the full new struct once when it becomes emitted; all its payloads are
    // new delta in that case, including the original required empty members.
    let before = if old_empty { None } else { before };
    diff::accept_struct(
        &profile,
        before,
        after,
        d,
        &[
            "policy",
            "policy_history",
            "migrated_bindings",
            "devlocal_policy_history",
            "bindings",
            "operations",
        ],
        |name, old, new, d| {
            match name{
   "policy"=>scalar::<Option<crate::service_admission::participant::ParticipantEntryPolicyV1>,_,_,_,_>(&["participants","policy"],new,d,processing,out),
   "policy_history"=>container::<crate::service_admission::participant_policy::ParticipantPolicyAuthorizationV1,_,_,_,_>(&["participants","policy_history"],old,new,d,processing,out,RevocationProjectionContainerV1::Sequence),
   "devlocal_policy_history"=>container::<crate::service_admission::SignedReadRequest<crate::service_admission::participant_policy::DevLocalParticipantPolicyAuthorizationV1>,_,_,_,_>(&["participants","devlocal_policy_history"],old,new,d,processing,out,RevocationProjectionContainerV1::Sequence),
   "migrated_bindings"=>container::<crate::service_admission::participant_policy::ParticipantMigrationBindingV1,_,_,_,_>(&["participants","migrated_bindings"],old,new,d,processing,out,RevocationProjectionContainerV1::Map),
   "bindings"=>container::<oasis7_agent_api::game::GameParticipantReceiptV1,_,_,_,_>(&["participants","bindings"],old,new,d,processing,out,RevocationProjectionContainerV1::Map),
   "operations"=>container::<crate::service_admission::participant::ParticipantOperationV1,_,_,_,_>(&["participants","operations"],old,new,d,processing,out,RevocationProjectionContainerV1::Map),
   _=>Err(invalid()),
  }
        },
    )
}
fn task_index<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    processing: &mut BusinessDiffProcessingWorkV1,
    out: &mut Output,
) -> Result<(), ValueTreeError> {
    let profile = d.profile;
    let Some(after) = after else {
        return Err(invalid());
    };
    let new = diff::row(&profile, after, d)?;
    if matches!(new.cell, ColumnCellV1::Unit) {
        return out.push(
            &["task_terminals_v2"],
            Selector::Value,
            None,
            processing,
            &mut d.ctx.cancelled,
        );
    }
    if !matches!(new.cell, ColumnCellV1::Root(_)) {
        return Err(invalid());
    }
    let old = before.map(|v| diff::row(&profile, v, d)).transpose()?;
    let before = if old
        .as_ref()
        .is_some_and(|v| matches!(v.cell, ColumnCellV1::Unit))
    {
        None
    } else {
        before
    };
    out.push(
        &["task_terminals_v2"],
        Selector::Container(RevocationProjectionContainerV1::Object, true, 2),
        None,
        processing,
        &mut d.ctx.cancelled,
    )?;
    diff::accept_struct(
        &profile,
        before,
        Some(after),
        d,
        &["schema_revision", "records"],
        |name, old, new, d| match name {
            "schema_revision" => scalar::<u32, _, _, _, _>(
                &["task_terminals_v2", "schema_revision"],
                new,
                d,
                processing,
                out,
            ),
            "records" => container::<crate::TaskTerminalRecordV2, _, _, _, _>(
                &["task_terminals_v2", "records"],
                old,
                new,
                d,
                processing,
                out,
                RevocationProjectionContainerV1::Map,
            ),
            _ => Err(invalid()),
        },
    )
}
fn record<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    before: &StateRootV1,
    after: &StateRootV1,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
    processing: &mut BusinessDiffProcessingWorkV1,
    out: &mut Output,
) -> Result<(), ValueTreeError> {
    let profile = d.profile;
    diff::accept_struct(
        &profile,
        Some(&TypedCellRefV1::Body(before)),
        Some(&TypedCellRefV1::Body(after)),
        d,
        &[
            "participants",
            "epoch",
            "revoked_grant_ids",
            "superseded_by",
            "authority_records",
            "authority_finality_proofs",
            "finalized_receipt_id",
            "agent_identities",
            "manufactured_identity_acquisition_sequences",
            "agent_signer_delegations",
            "world_service_results",
            "task_terminals_v2",
            "safe_alternatives",
            "system_identities",
        ],
        |name, old, new, d| match name {
            "participants" => participants(old, new, d, processing, out),
            "epoch" => scalar::<u64, _, _, _, _>(&["epoch"], new, d, processing, out),
            "revoked_grant_ids" => container::<String, _, _, _, _>(
                &["revoked_grant_ids"],
                old,
                new,
                d,
                processing,
                out,
                RevocationProjectionContainerV1::Set,
            ),
            "superseded_by" => container::<String, _, _, _, _>(
                &["superseded_by"],
                old,
                new,
                d,
                processing,
                out,
                RevocationProjectionContainerV1::Map,
            ),
            "authority_records" => container::<crate::CapabilityAuthorityRecord, _, _, _, _>(
                &["authority_records"],
                old,
                new,
                d,
                processing,
                out,
                RevocationProjectionContainerV1::Map,
            ),
            "authority_finality_proofs" => {
                container::<crate::CapabilityAuthorityFinalityProof, _, _, _, _>(
                    &["authority_finality_proofs"],
                    old,
                    new,
                    d,
                    processing,
                    out,
                    RevocationProjectionContainerV1::Map,
                )
            }
            "finalized_receipt_id" => scalar::<Option<String>, _, _, _, _>(
                &["finalized_receipt_id"],
                new,
                d,
                processing,
                out,
            ),
            "agent_identities" => container::<crate::CapabilityAgentIdentity, _, _, _, _>(
                &["agent_identities"],
                old,
                new,
                d,
                processing,
                out,
                RevocationProjectionContainerV1::Map,
            ),
            "manufactured_identity_acquisition_sequences" => container::<u64, _, _, _, _>(
                &["manufactured_identity_acquisition_sequences"],
                old,
                new,
                d,
                processing,
                out,
                RevocationProjectionContainerV1::Map,
            ),
            "agent_signer_delegations" => {
                container::<crate::service_admission::AgentSignerDelegationChangeV1, _, _, _, _>(
                    &["agent_signer_delegations"],
                    old,
                    new,
                    d,
                    processing,
                    out,
                    RevocationProjectionContainerV1::Map,
                )
            }
            "world_service_results" => container::<serde_json::Value, _, _, _, _>(
                &["world_service_results"],
                old,
                new,
                d,
                processing,
                out,
                RevocationProjectionContainerV1::Map,
            ),
            "task_terminals_v2" => task_index(old, new, d, processing, out),
            "safe_alternatives" => container::<serde_json::Value, _, _, _, _>(
                &["safe_alternatives"],
                old,
                new,
                d,
                processing,
                out,
                RevocationProjectionContainerV1::Map,
            ),
            "system_identities" => container::<u64, _, _, _, _>(
                &["system_identities"],
                old,
                new,
                d,
                processing,
                out,
                RevocationProjectionContainerV1::Map,
            ),
            _ => Err(invalid()),
        },
    )
}
/// Only genuine qualified parent/Current owners select the two typed bodies.
/// IO/scan/codec/processing accounts are continuing caller loans; Runtime's
/// fixed source bridge maps these fine counters to its exclusive Common child.
#[allow(clippy::too_many_arguments)]
pub fn read_current_runtime_revocation_projection_delta_v1<M, R, E, C, P, V, PE, PC>(
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    processing: &mut BusinessDiffProcessingWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<CurrentRuntimeRevocationProjectionDeltaV1, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    overlay.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        qualification.validate_digest(&tx.parent_fence().parent_roots)?;
        if overlay.values.class != MemoryClass::Ordinary
            || read.values.class != MemoryClass::Ordinary
            || qualification.allocation_class() != MemoryClass::Ordinary
            || !qualification.uses_budget(overlay.values.memory)
            || !read.values.memory.same_pool(overlay.values.memory)
            || !tx.uses_budget(overlay.values.memory)
        {
            return Err(invalid().into());
        }
        read.check()?;
        spend(
            processing,
            size_of::<CurrentRuntimeRevocationProjectionDeltaV1>() + 8192,
            &mut overlay.cancelled,
        )?;
        let memory = overlay.values.memory.clone();
        let fee = memory
            .reserve(
                size_of::<CurrentRuntimeRevocationProjectionDeltaV1>() + 8192,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let body = {
            let mut records = InventoryValueReader {
                values: &mut overlay.values,
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                cancelled: &mut overlay.cancelled,
            };
            super::current_identity::current_body(tx, &mut records, &mut overlay.take_state_error)?
        };
        current.check(tx, &body, &memory)?;
        let before = qualification.body().clone();
        let after = body;
        let mut out = Output {
            patches: Vec::new(),
            slots: memory
                .reserve(0, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?,
            memory: memory.clone(),
        };
        let trees_remaining = work.max_retained_trees;
        let mut decoder = ChangeDecoder {
            ctx: read,
            scan,
            work,
            profile: profile(),
            trees_remaining,
            tree_depth: 0,
        };
        record(&before, &after, &mut decoder, processing, &mut out)?;
        read.check()?;
        if (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (overlay.take_state_error)() {
            return Err(error.into());
        }
        if let Some(error) = (read.take_state_error)() {
            return Err(error.into());
        }
        let guard = tx.guard();
        tx.validate_guard(&guard, live)?;
        Ok(CurrentRuntimeRevocationProjectionDeltaV1 {
            patches: out.patches,
            slots: out.slots,
            guard,
            before,
            after,
            parent: tx.parent_fence().parent_roots,
            memory,
            _fee: fee,
        })
    })
}
