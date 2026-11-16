//! Exact paid selected-record copies for the local industrial rehearsal.
//! No constructor here grants source, World, admission or execution authority.
use super::readonly_owner::IndustrialOwnerReadonlyFactsV1;
use crate::MaterialLedgerId;
use oasis7_agent_api::world_action::*;
use oasis7_wasm_abi::SubstitutionProfileRefV1;
use std::{
    collections::BTreeMap,
    mem::{align_of, size_of},
};
pub(super) fn text<S: IndustrialOwnerReadonlyFactsV1>(s: &S, v: &str) -> Result<String, S::Error> {
    s.owner_before_owned(v.len() + size_of::<String>())?;
    let mut out = Vec::with_capacity(v.len());
    assert_eq!(out.capacity(), v.len(), "industrial text exact capacity");
    for chunk in v.as_bytes().chunks(8192) {
        s.owner_work(1, chunk.len())?;
        out.extend_from_slice(chunk);
    }
    Ok(String::from_utf8(out).expect("copied original UTF-8"))
}
pub(super) fn tree_slot<S: IndustrialOwnerReadonlyFactsV1, V>(
    s: &S,
    key: &str,
    count: usize,
) -> Result<(), S::Error> {
    let align = align_of::<String>()
        .max(align_of::<V>())
        .max(align_of::<usize>());
    let node = 11usize
        .saturating_mul(size_of::<String>() + size_of::<V>())
        .saturating_add(13 * size_of::<usize>() + 2 * size_of::<u16>() + 7 * (align - 1));
    let levels = (count + 1).ilog2() as usize + 2;
    s.owner_work(
        1,
        (key.len() + 1)
            .saturating_mul(levels * 2 + 4)
            .saturating_add(node.saturating_mul(levels)),
    )?;
    s.owner_before_owned(node.saturating_mul(levels))
}
pub(super) fn reference<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &IndustrialRefV1,
) -> Result<IndustrialRefV1, S::Error> {
    s.owner_work(1, size_of::<IndustrialRefV1>())?;
    s.owner_before_owned(size_of::<IndustrialRefV1>())?;
    Ok(IndustrialRefV1 {
        world_binding: text(s, &v.world_binding)?,
        domain: text(s, &v.domain)?,
        schema: text(s, &v.schema)?,
        id: text(s, &v.id)?,
        revision: v.revision,
        digest: text(s, &v.digest)?,
    })
}
pub(super) fn profile<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &SubstitutionProfileRefV1,
) -> Result<SubstitutionProfileRefV1, S::Error> {
    Ok(SubstitutionProfileRefV1 {
        id: text(s, &v.id)?,
        version: text(s, &v.version)?,
        digest: text(s, &v.digest)?,
    })
}
pub(super) fn ledger<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &MaterialLedgerId,
) -> Result<MaterialLedgerId, S::Error> {
    s.owner_work(1, 0)?;
    Ok(match v {
        MaterialLedgerId::World => MaterialLedgerId::World,
        MaterialLedgerId::Agent(v) => MaterialLedgerId::Agent(text(s, v)?),
        MaterialLedgerId::Site(v) => MaterialLedgerId::Site(text(s, v)?),
        MaterialLedgerId::Factory(v) => MaterialLedgerId::Factory(text(s, v)?),
    })
}
pub(super) fn parent<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &IndustrialParentV1,
) -> Result<IndustrialParentV1, S::Error> {
    Ok(IndustrialParentV1 {
        lot_ref: reference(s, &v.lot_ref)?,
        settlement_ref: reference(s, &v.settlement_ref)?,
        quantity: v.quantity,
    })
}
pub(super) fn parents<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &[IndustrialParentV1],
) -> Result<Vec<IndustrialParentV1>, S::Error> {
    s.owner_before_owned(v.len().saturating_mul(size_of::<IndustrialParentV1>()))?;
    let mut out = Vec::with_capacity(v.len());
    assert_eq!(out.capacity(), v.len(), "industrial parents exact capacity");
    for value in v {
        s.owner_work(1, 0)?;
        out.push(parent(s, value)?);
    }
    Ok(out)
}
fn slot<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &IndustrialInputSlotV1,
) -> Result<IndustrialInputSlotV1, S::Error> {
    Ok(IndustrialInputSlotV1 {
        material_profile: profile(s, &v.material_profile)?,
        material: text(s, &v.material)?,
        unit: text(s, &v.unit)?,
        spec: text(s, &v.spec)?,
        quality: text(s, &v.quality)?,
        custody: text(s, &v.custody)?,
        quantity: v.quantity,
        binding_revision: v.binding_revision,
        parents: parents(s, &v.parents)?,
        consumed: v.consumed,
    })
}
pub(super) fn root<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &RootOperationV1,
) -> Result<RootOperationV1, S::Error> {
    s.owner_before_owned(size_of::<RootOperationV1>())?;
    let owner_binding_digest = text(s, &v.owner_binding_digest)?;
    s.owner_before_owned(
        v.allowed_dispositions
            .len()
            .saturating_mul(size_of::<IndustrialDispositionV1>()),
    )?;
    let mut allowed_dispositions = Vec::with_capacity(v.allowed_dispositions.len());
    assert_eq!(
        allowed_dispositions.capacity(),
        v.allowed_dispositions.len(),
        "industrial disposition exact capacity"
    );
    for value in &v.allowed_dispositions {
        s.owner_work(1, size_of::<IndustrialDispositionV1>())?;
        allowed_dispositions.push(*value);
    }
    let output_ledger = ledger(s, &v.output_ledger)?;
    let mut terminal_capacities = BTreeMap::new();
    for (key, value) in &v.terminal_capacities {
        tree_slot::<_, u64>(s, key, terminal_capacities.len())?;
        terminal_capacities.insert(text(s, key)?, *value);
    }
    let root_ref = reference(s, &v.root_ref)?;
    let owner_id = text(s, &v.owner_id)?;
    let factory_id = text(s, &v.factory_id)?;
    let pinned_recipe_id = text(s, &v.pinned_recipe_id)?;
    let pinned_profile_digest = text(s, &v.pinned_profile_digest)?;
    let causal_digest = text(s, &v.causal_digest)?;
    let mut slots = BTreeMap::new();
    for (key, value) in &v.slots {
        tree_slot::<_, IndustrialInputSlotV1>(s, key, slots.len())?;
        slots.insert(text(s, key)?, slot(s, value)?);
    }
    Ok(RootOperationV1 {
        owner_binding_digest,
        allowed_dispositions,
        output_ledger,
        terminal_capacities,
        started_job_id: v.started_job_id,
        root_ref,
        owner_id,
        factory_id,
        pinned_recipe_id,
        accepted_batches: v.accepted_batches,
        pinned_profile_digest,
        causal_digest,
        work: v.work,
        first_sink: v.first_sink,
        slots,
    })
}
pub(super) fn lot<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &IndustrialLotV1,
) -> Result<IndustrialLotV1, S::Error> {
    Ok(IndustrialLotV1 {
        lot_ref: reference(s, &v.lot_ref)?,
        owner_id: text(s, &v.owner_id)?,
        source_ledger: ledger(s, &v.source_ledger)?,
        material: text(s, &v.material)?,
        unit: text(s, &v.unit)?,
        spec: text(s, &v.spec)?,
        quality: text(s, &v.quality)?,
        custody: text(s, &v.custody)?,
        provenance_ref: reference(s, &v.provenance_ref)?,
        quantity: v.quantity,
        remaining: v.remaining,
    })
}
pub(super) fn authority<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &IndustrialLotAuthorityV1,
) -> Result<IndustrialLotAuthorityV1, S::Error> {
    Ok(IndustrialLotAuthorityV1 {
        owner_binding_digest: text(s, &v.owner_binding_digest)?,
        expires_at: v.expires_at,
        escrow_capacity: v.escrow_capacity,
        material_profile: profile(s, &v.material_profile)?,
    })
}
fn references<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &[IndustrialRefV1],
) -> Result<Vec<IndustrialRefV1>, S::Error> {
    s.owner_before_owned(v.len().saturating_mul(size_of::<IndustrialRefV1>()))?;
    let mut out = Vec::with_capacity(v.len());
    assert_eq!(
        out.capacity(),
        v.len(),
        "industrial references exact capacity"
    );
    for r in v {
        s.owner_work(1, 0)?;
        out.push(reference(s, r)?);
    }
    Ok(out)
}
pub(super) fn receipt<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &IndustrialReceiptV1,
) -> Result<IndustrialReceiptV1, S::Error> {
    Ok(IndustrialReceiptV1 {
        receipt_ref: reference(s, &v.receipt_ref)?,
        root_ref: reference(s, &v.root_ref)?,
        child_ref: reference(s, &v.child_ref)?,
        effect_role: text(s, &v.effect_role)?,
        payload_digest: text(s, &v.payload_digest)?,
        input: parents(s, &v.input)?,
        output: references(s, &v.output)?,
    })
}
pub(super) fn disposition<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    v: &DispositionPlanV1,
) -> Result<DispositionPlanV1, S::Error> {
    Ok(DispositionPlanV1 {
        root_ref: reference(s, &v.root_ref)?,
        plan_ref: reference(s, &v.plan_ref)?,
        expected_causal_digest: text(s, &v.expected_causal_digest)?,
        disposition: v.disposition,
    })
}
