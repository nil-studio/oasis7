//! Selected closed authorization DTO recovery from authenticated typed leaves.
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{
        JsonReadWorkV1, JsonRecordProfileV1, materialize_records, qualify_json_subtree_records_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryReservation;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy)]
pub enum AuthorizationBodyKindV1 {
    Audit,
    Invocation,
}
impl AuthorizationBodyKindV1 {
    pub(crate) fn descriptor(self) -> FieldDescriptor {
        FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: match self {
                Self::Audit => "capability_authorization_receipts.columns",
                Self::Invocation => "capability_invocation_contexts.columns",
            },
            destination: FieldDestination::State(StateNamespaceV1::Authorization),
            shape: FieldShape::Map,
        }
    }
    pub(crate) fn schema(self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"oasis7.iws.runtime-authorization.columns.v1\0");
        h.update(self.descriptor().name.as_bytes());
        *h.finalize().as_bytes()
    }
}
pub type AuthorizationRecoveredV1<T> =
    crate::incremental_record_columns_read::RecoveredColumnsV1<T>;
use crate::incremental_record_columns_read::*;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("authorization closed DTO recovery")
}
fn profile(kind: AuthorizationBodyKindV1) -> ColumnReadProfileV1 {
    let groups: &'static [&'static str] = &[];
    ColumnReadProfileV1 {
        field: kind.descriptor(),
        schema: kind.schema(),
        groups,
        reference_schema: None,
    }
}

fn json<M, R, E, C>(
    kind: AuthorizationBodyKindV1,
    map: &mut BTreeMap<ColumnKeyV1, ColumnCellV1>,
    key: &str,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
    max_heap: usize,
    charges: &mut Vec<MemoryReservation>,
) -> Result<serde_json::Value, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let ColumnCellV1::Root(root) = take(map, key)? else {
        return Err(invalid());
    };
    let profile = JsonRecordProfileV1 {
        field: kind.descriptor(),
        schema: kind.schema(),
    };
    let proof = qualify_json_subtree_records_v1(profile, &root, &mut ctx.records(), work)?;
    let (value, charge) = materialize_records(profile, &proof, &mut ctx.records(), work, max_heap)?;
    charges.push(charge);
    Ok(value)
}
pub fn recover_authorization_audit_body_v1<M, R, E, C>(
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut JsonReadWorkV1,
    max_heap: usize,
) -> Result<AuthorizationRecoveredV1<crate::CapabilityAuthorizationAuditReceipt>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut columns = read_columns_v1(
        profile(AuthorizationBodyKindV1::Audit),
        root,
        ctx,
        scan,
        max_heap,
    )?;
    columns.spend_output(
        std::mem::size_of::<crate::CapabilityAuthorizationAuditReceipt>()
            .checked_add(3 * std::mem::size_of::<MemoryReservation>() + 128)
            .ok_or_else(invalid)?,
    )?;
    let mut charges = Vec::with_capacity(3);
    let (subject, audience, presenter, count) = {
        let map = columns.cells_mut();
        let subject = json(
            AuthorizationBodyKindV1::Audit,
            map,
            "subject",
            ctx,
            work,
            max_heap,
            &mut charges,
        )?;
        let audience = json(
            AuthorizationBodyKindV1::Audit,
            map,
            "audience",
            ctx,
            work,
            max_heap,
            &mut charges,
        )?;
        let presenter = if present(map, "presenter")? {
            Some(json(
                AuthorizationBodyKindV1::Audit,
                map,
                "presenter",
                ctx,
                work,
                max_heap,
                &mut charges,
            )?)
        } else {
            None
        };
        let count = match map
            .remove(&ColumnKeyV1::pair(
                ColumnKeyV1::text("committed_effect_receipt_ids"),
                ColumnKeyV1::text("count"),
            ))
            .ok_or_else(invalid)?
        {
            ColumnCellV1::U64(value) => usize::try_from(value).map_err(|_| invalid())?,
            _ => return Err(invalid()),
        };
        if count > map.len() {
            return Err(invalid());
        }
        (subject, audience, presenter, count)
    };
    columns.spend_output(
        count
            .checked_mul(192)
            .and_then(|n| n.checked_add(384))
            .ok_or_else(invalid)?,
    )?;
    let map = columns.cells_mut();
    let mut value = crate::CapabilityAuthorizationAuditReceipt {
        receipt_id: text(map, "receipt_id")?,
        root_receipt_id: opt_text(map, "root_receipt_id")?,
        grant_id: opt_text(map, "grant_id")?,
        subject,
        presenter,
        audience,
        scope_hash: text(map, "scope_hash")?,
        module_id: opt_text(map, "module_id")?,
        module_version: opt_text(map, "module_version")?,
        manifest_hash: opt_text(map, "manifest_hash")?,
        catalog_snapshot_id: opt_text(map, "catalog_snapshot_id")?,
        response_nonce: opt_text(map, "response_nonce")?,
        authorization_nonce_key_hash: opt_text(map, "authorization_nonce_key_hash")?,
        decision: text(map, "decision")?,
        denial_code: opt_text(map, "denial_code")?,
        budget_before: signed(map, "budget_before")?,
        budget_after: opt_signed(map, "budget_after")?,
        world_head_before: unsigned(map, "world_head_before")?,
        world_head_after: opt_unsigned(map, "world_head_after")?,
        branch_id: text(map, "branch_id")?,
        finality_epoch: unsigned(map, "finality_epoch")?,
        finality_block_hash: opt_text(map, "finality_block_hash")?,
        finality_status: text(map, "finality_status")?,
        state_hash_before: text(map, "state_hash_before")?,
        state_hash_after: opt_text(map, "state_hash_after")?,
        committed_effect_receipt_id: opt_text(map, "committed_effect_receipt_id")?,
        committed_effect_receipt_ids: BTreeSet::new(),
        canonical_request_hash: text(map, "canonical_request_hash")?,
        canonical_result_hash: text(map, "canonical_result_hash")?,
    };
    // All fixed columns are consumed. Only the exact ordered set-member range
    // may remain; pop_first avoids rescanning or cloning every preceding key.
    for _ in 0..count {
        ctx.check()?;
        work.cells_remaining =
            work.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "authorization audit set recovery cells",
                ))?;
        let (key, cell) = map.pop_first().ok_or_else(invalid)?;
        if !matches!(cell, ColumnCellV1::Unit) {
            return Err(invalid());
        }
        let ColumnKeyV1::Pair(field, tail) = key else {
            return Err(invalid());
        };
        if !matches!(*field,ColumnKeyV1::Text(ref name) if name=="committed_effect_receipt_ids") {
            return Err(invalid());
        }
        let ColumnKeyV1::Pair(tag, id) = *tail else {
            return Err(invalid());
        };
        if !matches!(*tag,ColumnKeyV1::Text(ref name) if name=="member") {
            return Err(invalid());
        }
        let ColumnKeyV1::Text(id) = *id else {
            return Err(invalid());
        };
        ctx.check()?;
        if !value.committed_effect_receipt_ids.insert(id) {
            return Err(invalid());
        }
        ctx.check()?;
    }
    if !map.is_empty() {
        return Err(invalid());
    }
    ctx.check()?;
    columns.finish(value, charges)
}

pub fn recover_authorization_invocation_body_v1<M, R, E, C>(
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    max_heap: usize,
) -> Result<AuthorizationRecoveredV1<crate::CapabilityInvocationContext>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_wasm_abi::{CapabilityAudience, CapabilityPresenter, CapabilitySubject};
    let mut columns = read_columns_v1(
        profile(AuthorizationBodyKindV1::Invocation),
        root,
        ctx,
        scan,
        max_heap,
    )?;
    let map = columns.cells_mut();
    let subject = match text(map, "subject.kind")?.as_str() {
        "agent" => CapabilitySubject::Agent {
            agent_id: text(map, "subject.agent_id")?,
            owner_binding: text(map, "subject.owner_binding")?,
            generation: unsigned(map, "subject.generation")?,
        },
        "module" => CapabilitySubject::Module {
            module_id: text(map, "subject.module_id")?,
            module_version: text(map, "subject.module_version")?,
            instance_id: text(map, "subject.instance_id")?,
        },
        "system" => CapabilitySubject::System {
            system_id: text(map, "subject.system_id")?,
            epoch: unsigned(map, "subject.epoch")?,
        },
        _ => return Err(invalid()),
    };
    let presenter = CapabilityPresenter {
        presenter_id: text(map, "presenter.presenter_id")?,
        presenter_kind: text(map, "presenter.presenter_kind")?,
        session_id: opt_text(map, "presenter.session_id")?,
        attestation_ref: opt_text(map, "presenter.attestation_ref")?,
    };
    let audience = CapabilityAudience {
        world_id: text(map, "audience.world_id")?,
        branch_id: text(map, "audience.branch_id")?,
        finality_epoch: unsigned(map, "audience.finality_epoch")?,
        target_kind: text(map, "audience.target_kind")?,
        target_id: opt_text(map, "audience.target_id")?,
    };
    let value = crate::CapabilityInvocationContext {
        grant_id: text(map, "grant_id")?,
        subject,
        presenter,
        audience,
        catalog_snapshot_id: text(map, "catalog_snapshot_id")?,
        module_id: text(map, "module_id")?,
        module_version: text(map, "module_version")?,
        response_nonce: text(map, "response_nonce")?,
    };
    if !map.is_empty() {
        return Err(invalid());
    }
    ctx.check()?;
    columns.finish(value, Vec::new())
}

#[cfg(test)]
#[path = "incremental_runtime_authorization_recovery_tests.rs"]
mod tests;
