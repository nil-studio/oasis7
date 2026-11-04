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
    let mut map = columns.cells_mut();
    let mut charges = Vec::with_capacity(3);
    let subject = json(
        AuthorizationBodyKindV1::Audit,
        &mut map,
        "subject",
        ctx,
        work,
        max_heap,
        &mut charges,
    )?;
    let audience = json(
        AuthorizationBodyKindV1::Audit,
        &mut map,
        "audience",
        ctx,
        work,
        max_heap,
        &mut charges,
    )?;
    let presenter = if present(&mut map, "presenter")? {
        Some(json(
            AuthorizationBodyKindV1::Audit,
            &mut map,
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
        ColumnCellV1::U64(v) => v,
        _ => return Err(invalid()),
    };
    let mut effects = BTreeSet::new();
    loop {
        let next=map.keys().find(|k|matches!(k,ColumnKeyV1::Pair(a,b) if matches!(a.as_ref(),ColumnKeyV1::Text(v) if v=="committed_effect_receipt_ids") && matches!(b.as_ref(),ColumnKeyV1::Pair(c,d) if matches!(c.as_ref(),ColumnKeyV1::Text(v) if v=="member") && matches!(d.as_ref(),ColumnKeyV1::Text(_))))).cloned();
        let Some(key) = next else {
            break;
        };
        match map.remove(&key).ok_or_else(invalid)? {
            ColumnCellV1::Unit => (),
            _ => return Err(invalid()),
        }
        let ColumnKeyV1::Pair(_, tail) = key else {
            return Err(invalid());
        };
        let ColumnKeyV1::Pair(_, id) = *tail else {
            return Err(invalid());
        };
        let ColumnKeyV1::Text(id) = *id else {
            return Err(invalid());
        };
        if !effects.insert(id) {
            return Err(invalid());
        }
    }
    if effects.len() as u64 != count {
        return Err(invalid());
    }
    let value = crate::CapabilityAuthorizationAuditReceipt {
        receipt_id: text(&mut map, "receipt_id")?,
        root_receipt_id: opt_text(&mut map, "root_receipt_id")?,
        grant_id: opt_text(&mut map, "grant_id")?,
        subject,
        presenter,
        audience,
        scope_hash: text(&mut map, "scope_hash")?,
        module_id: opt_text(&mut map, "module_id")?,
        module_version: opt_text(&mut map, "module_version")?,
        manifest_hash: opt_text(&mut map, "manifest_hash")?,
        catalog_snapshot_id: opt_text(&mut map, "catalog_snapshot_id")?,
        response_nonce: opt_text(&mut map, "response_nonce")?,
        authorization_nonce_key_hash: opt_text(&mut map, "authorization_nonce_key_hash")?,
        decision: text(&mut map, "decision")?,
        denial_code: opt_text(&mut map, "denial_code")?,
        budget_before: signed(&mut map, "budget_before")?,
        budget_after: opt_signed(&mut map, "budget_after")?,
        world_head_before: unsigned(&mut map, "world_head_before")?,
        world_head_after: opt_unsigned(&mut map, "world_head_after")?,
        branch_id: text(&mut map, "branch_id")?,
        finality_epoch: unsigned(&mut map, "finality_epoch")?,
        finality_block_hash: opt_text(&mut map, "finality_block_hash")?,
        finality_status: text(&mut map, "finality_status")?,
        state_hash_before: text(&mut map, "state_hash_before")?,
        state_hash_after: opt_text(&mut map, "state_hash_after")?,
        committed_effect_receipt_id: opt_text(&mut map, "committed_effect_receipt_id")?,
        committed_effect_receipt_ids: effects,
        canonical_request_hash: text(&mut map, "canonical_request_hash")?,
        canonical_result_hash: text(&mut map, "canonical_result_hash")?,
    };
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
    let mut map = columns.cells_mut();
    let subject = match text(&mut map, "subject.kind")?.as_str() {
        "agent" => CapabilitySubject::Agent {
            agent_id: text(&mut map, "subject.agent_id")?,
            owner_binding: text(&mut map, "subject.owner_binding")?,
            generation: unsigned(&mut map, "subject.generation")?,
        },
        "module" => CapabilitySubject::Module {
            module_id: text(&mut map, "subject.module_id")?,
            module_version: text(&mut map, "subject.module_version")?,
            instance_id: text(&mut map, "subject.instance_id")?,
        },
        "system" => CapabilitySubject::System {
            system_id: text(&mut map, "subject.system_id")?,
            epoch: unsigned(&mut map, "subject.epoch")?,
        },
        _ => return Err(invalid()),
    };
    let presenter = CapabilityPresenter {
        presenter_id: text(&mut map, "presenter.presenter_id")?,
        presenter_kind: text(&mut map, "presenter.presenter_kind")?,
        session_id: opt_text(&mut map, "presenter.session_id")?,
        attestation_ref: opt_text(&mut map, "presenter.attestation_ref")?,
    };
    let audience = CapabilityAudience {
        world_id: text(&mut map, "audience.world_id")?,
        branch_id: text(&mut map, "audience.branch_id")?,
        finality_epoch: unsigned(&mut map, "audience.finality_epoch")?,
        target_kind: text(&mut map, "audience.target_kind")?,
        target_id: opt_text(&mut map, "audience.target_id")?,
    };
    let value = crate::CapabilityInvocationContext {
        grant_id: text(&mut map, "grant_id")?,
        subject,
        presenter,
        audience,
        catalog_snapshot_id: text(&mut map, "catalog_snapshot_id")?,
        module_id: text(&mut map, "module_id")?,
        module_version: text(&mut map, "module_version")?,
        response_nonce: text(&mut map, "response_nonce")?,
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
