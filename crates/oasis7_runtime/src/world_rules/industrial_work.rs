//! Canonical industrial escrow helpers. Callers prepare on a private state clone;
//! authorization, finality evidence and aggregate ledger debit share its commit.
use super::invalid;
use crate::WorldError;
use oasis7_agent_api::world_action::*;
use serde::Serialize;
use std::collections::BTreeSet;

pub fn industrial_ref<T: Serialize>(
    world: &str,
    domain: &str,
    schema: &str,
    id: &str,
    revision: u64,
    payload: &T,
) -> Result<IndustrialRefV1, WorldError> {
    let digest = crate::util::hash_json(&(world, domain, schema, id, revision, payload))?;
    let value = IndustrialRefV1 {
        world_binding: world.into(),
        domain: domain.into(),
        schema: schema.into(),
        id: id.into(),
        revision,
        digest,
    };
    validate_ref(&value, world)?;
    Ok(value)
}
fn canonical_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn same_identity(a: &IndustrialRefV1, b: &IndustrialRefV1) -> bool {
    a.world_binding == b.world_binding
        && a.domain == b.domain
        && a.schema == b.schema
        && a.id == b.id
        && a.revision == b.revision
}
fn validate_ref(value: &IndustrialRefV1, world: &str) -> Result<(), WorldError> {
    if !value.is_qualified() || value.world_binding != world {
        return Err(invalid("industrial_reference_invalid"));
    }
    Ok(())
}
pub fn validate_lot(lot: &IndustrialLotV1, world: &str) -> Result<(), WorldError> {
    validate_ref(&lot.lot_ref, world)?;
    validate_ref(&lot.provenance_ref, world)?;
    if lot.owner_id.is_empty()
        || lot.material.is_empty()
        || lot.unit.is_empty()
        || lot.spec.is_empty()
        || lot.quality.is_empty()
        || lot.custody.is_empty()
        || lot.quantity == 0
        || lot.remaining > lot.quantity
    {
        return Err(invalid("industrial_lot_evidence_incomplete"));
    }
    Ok(())
}
/// Returns false only for an exact retry. A provenance certificate imports once.
pub fn import_lot(
    state: &mut IndustrialWorkStateV1,
    lot: IndustrialLotV1,
    world: &str,
) -> Result<bool, WorldError> {
    validate_lot(&lot, world)?;
    if state.lots.len() >= 16384 && !state.lots.contains_key(&lot.lot_ref.id) {
        return Err(invalid("industrial_capacity_exceeded"));
    }
    if let Some(old) = state.lots.get(&lot.lot_ref.id) {
        if old == &lot {
            return Ok(false);
        }
        return Err(invalid("industrial_lot_identity_conflict"));
    }
    if lot.remaining != lot.quantity
        || state
            .lots
            .values()
            .any(|old| old.provenance_ref == lot.provenance_ref)
    {
        return Err(invalid("industrial_provenance_already_imported"));
    }
    state.lots.insert(lot.lot_ref.id.clone(), lot);
    Ok(true)
}
pub fn accept_root(
    state: &mut IndustrialWorkStateV1,
    root: RootOperationV1,
    world: &str,
) -> Result<bool, WorldError> {
    validate_ref(&root.root_ref, world)?;
    if state.roots.len() >= 4096 && !state.roots.contains_key(&root.root_ref.id)
        || root.slots.len() > 128
    {
        return Err(invalid("industrial_capacity_exceeded"));
    }
    if let Some(old) = state.roots.get(&root.root_ref.id) {
        if old == &root {
            return Ok(false);
        }
        return Err(invalid("industrial_root_identity_conflict"));
    }
    if root.owner_id.is_empty()
        || root.factory_id.is_empty()
        || root.pinned_recipe_id.is_empty()
        || root.accepted_batches == 0
        || root.pinned_profile_digest.len() != 64
        || root.causal_digest.len() != 64
        || root.work != 0
        || root.first_sink
        || root.slots.is_empty()
        || !canonical_digest(&root.owner_binding_digest)
        || !canonical_digest(&root.pinned_profile_digest)
        || !canonical_digest(&root.causal_digest)
        || root.started_job_id.is_some()
        || root.allowed_dispositions.len() > 1
        || root
            .allowed_dispositions
            .iter()
            .any(|d| *d != IndustrialDispositionV1::Terminate)
        || root.output_ledger == MaterialLedgerId::world()
        || root.terminal_capacities.is_empty()
        || root.terminal_capacities.len() > 128
        || root
            .terminal_capacities
            .iter()
            .any(|(kind, cap)| kind.is_empty() || *cap == 0)
    {
        return Err(invalid("industrial_root_invalid"));
    }
    for (id, slot) in &root.slots {
        slot.material_profile
            .validate()
            .map_err(|_| invalid("industrial_slot_profile_invalid"))?;
        if id.is_empty() || slot.consumed || slot.binding_revision != 1 {
            return Err(invalid("industrial_slot_invalid"));
        }
        if slot.quantity == 0
            || slot.material.is_empty()
            || slot.unit.is_empty()
            || slot.spec.is_empty()
            || slot.quality.is_empty()
            || slot.custody.is_empty()
        {
            return Err(invalid("industrial_slot_evidence_incomplete"));
        }
        if !slot.parents.is_empty() {
            validate_parents(state, &root, slot, &slot.parents, world)?;
        }
    }
    state.roots.insert(root.root_ref.id.clone(), root);
    Ok(true)
}
fn validate_parents(
    state: &IndustrialWorkStateV1,
    root: &RootOperationV1,
    slot: &IndustrialInputSlotV1,
    parents: &[IndustrialParentV1],
    world: &str,
) -> Result<(), WorldError> {
    if parents.len() > 128 {
        return Err(invalid("industrial_capacity_exceeded"));
    }
    if slot.quantity == 0
        || slot.material.is_empty()
        || slot.unit.is_empty()
        || slot.spec.is_empty()
        || slot.quality.is_empty()
        || slot.custody.is_empty()
    {
        return Err(invalid("industrial_slot_evidence_incomplete"));
    }
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for parent in parents {
        validate_ref(&parent.lot_ref, world)?;
        validate_ref(&parent.settlement_ref, world)?;
        if parent.quantity == 0 || !seen.insert(&parent.lot_ref) {
            return Err(invalid("industrial_parent_duplicate_or_empty"));
        }
        let lot = state
            .lots
            .get(&parent.lot_ref.id)
            .filter(|l| l.lot_ref == parent.lot_ref)
            .ok_or_else(|| invalid("industrial_parent_missing"))?;
        if lot.owner_id != root.owner_id
            || lot.material != slot.material
            || lot.unit != slot.unit
            || lot.spec != slot.spec
            || lot.quality != slot.quality
            || lot.custody != slot.custody
            || lot.remaining < parent.quantity
        {
            return Err(invalid("industrial_parent_not_applicable"));
        }
        let imported = parent.settlement_ref == lot.provenance_ref;
        let converted = state
            .receipts
            .get(&parent.settlement_ref.id)
            .is_some_and(|r| {
                r.receipt_ref == parent.settlement_ref
                    && r.effect_role == "conversion_settlement"
                    && r.output.contains(&parent.lot_ref)
            });
        if !imported && !converted {
            return Err(invalid("industrial_parent_not_settled"));
        }
        total = total
            .checked_add(parent.quantity)
            .ok_or_else(|| invalid("industrial_quantity_overflow"))?;
    }
    if total != slot.quantity {
        return Err(invalid("industrial_parent_quantity_mismatch"));
    }
    Ok(())
}
/// Caller must first resolve the exact persisted substitution decision and scope.
pub fn apply_parent_binding(
    state: &mut IndustrialWorkStateV1,
    plan: ParentBindingRevisionV1,
    world: &str,
) -> Result<(), WorldError> {
    validate_ref(&plan.decision_ref, world)?;
    if state.bindings.len() >= 65536 {
        return Err(invalid("industrial_capacity_exceeded"));
    }
    let root = state
        .roots
        .get(&plan.root_ref.id)
        .filter(|r| r.root_ref == plan.root_ref)
        .ok_or_else(|| invalid("industrial_root_missing"))?;
    if state.dispositions.contains_key(&root.root_ref.id) {
        return Err(invalid("industrial_root_disposed"));
    }
    let slot = root
        .slots
        .get(&plan.slot_id)
        .ok_or_else(|| invalid("industrial_slot_missing"))?;
    if slot.consumed || slot.binding_revision != plan.expected_revision {
        return Err(invalid("industrial_binding_stale_or_consumed"));
    }
    validate_parents(state, root, slot, &plan.parents, world)?;
    let next = slot
        .binding_revision
        .checked_add(1)
        .ok_or_else(|| invalid("industrial_binding_revision_overflow"))?;
    let slot = state
        .roots
        .get_mut(&plan.root_ref.id)
        .expect("validated root")
        .slots
        .get_mut(&plan.slot_id)
        .expect("validated slot");
    slot.binding_revision = next;
    slot.parents = plan.parents.clone();
    state.bindings.push(plan);
    Ok(())
}
/// Consumes the canonical slot once, irrespective of later binding revisions.
pub fn consume_bound_slot(
    state: &mut IndustrialWorkStateV1,
    root_ref: &IndustrialRefV1,
    slot_id: &str,
    world: &str,
) -> Result<Vec<IndustrialParentV1>, WorldError> {
    let root = state
        .roots
        .get(&root_ref.id)
        .filter(|r| &r.root_ref == root_ref)
        .ok_or_else(|| invalid("industrial_root_missing"))?;
    if state.dispositions.contains_key(&root_ref.id) {
        return Err(invalid("industrial_root_disposed"));
    }
    let slot = root
        .slots
        .get(slot_id)
        .ok_or_else(|| invalid("industrial_slot_missing"))?;
    let key = crate::util::hash_json(&(root_ref, slot_id, "parent_sink"))?;
    if slot.consumed || state.sink_keys.contains(&key) {
        return Err(invalid("industrial_slot_already_consumed"));
    }
    validate_parents(state, root, slot, &slot.parents, world)?;
    let parents = slot.parents.clone();
    for parent in &parents {
        let lot = state
            .lots
            .get_mut(&parent.lot_ref.id)
            .expect("validated lot");
        lot.remaining = lot
            .remaining
            .checked_sub(parent.quantity)
            .ok_or_else(|| invalid("industrial_quantity_underflow"))?;
    }
    let root = state.roots.get_mut(&root_ref.id).expect("validated root");
    root.first_sink = true;
    root.slots
        .get_mut(slot_id)
        .expect("validated slot")
        .consumed = true;
    state.sink_keys.insert(key);
    Ok(parents)
}
pub fn apply_disposition(
    state: &mut IndustrialWorkStateV1,
    plan: DispositionPlanV1,
    world: &str,
) -> Result<bool, WorldError> {
    validate_ref(&plan.plan_ref, world)?;
    let root = state
        .roots
        .get(&plan.root_ref.id)
        .filter(|r| r.root_ref == plan.root_ref)
        .ok_or_else(|| invalid("industrial_root_missing"))?;
    if root.causal_digest != plan.expected_causal_digest {
        return Err(invalid("industrial_cutover_stale"));
    }
    if let Some(old) = state.dispositions.get(&plan.root_ref.id) {
        if old == &plan {
            return Ok(false);
        }
        return Err(invalid("industrial_cutover_already_applied"));
    }
    state.dispositions.insert(plan.root_ref.id.clone(), plan);
    Ok(true)
}

/// All effect writers use the same root/revision/child/role uniqueness key.
pub fn record_receipt(
    state: &mut IndustrialWorkStateV1,
    receipt: IndustrialReceiptV1,
    world: &str,
) -> Result<bool, WorldError> {
    validate_ref(&receipt.receipt_ref, world)?;
    if state.receipts.len() >= 65536 && !state.receipts.contains_key(&receipt.receipt_ref.id) {
        return Err(invalid("industrial_capacity_exceeded"));
    }
    validate_ref(&receipt.child_ref, world)?;
    if state
        .roots
        .get(&receipt.root_ref.id)
        .is_none_or(|r| r.root_ref != receipt.root_ref)
        || receipt.effect_role.is_empty()
        || receipt.payload_digest.len() != 64
    {
        return Err(invalid("industrial_receipt_invalid"));
    }
    if let Some(old) = state.receipts.get(&receipt.receipt_ref.id) {
        if old == &receipt {
            return Ok(false);
        }
        return Err(invalid("industrial_receipt_identity_conflict"));
    }
    if state.receipts.values().any(|old| {
        same_identity(&old.child_ref, &receipt.child_ref)
            && old.child_ref.digest != receipt.child_ref.digest
    }) {
        return Err(invalid("industrial_child_identity_conflict"));
    }
    if state.receipts.values().any(|old| {
        old.root_ref == receipt.root_ref
            && old.child_ref == receipt.child_ref
            && old.effect_role == receipt.effect_role
    }) {
        return Err(invalid("industrial_effect_already_committed"));
    }
    state
        .receipts
        .insert(receipt.receipt_ref.id.clone(), receipt);
    Ok(true)
}

/// Shared conversion/join escrow sink. Validation completes before any debit.
/// The caller commits the sink key and settlement receipt in one world delta.
pub fn consume_lots(
    state: &mut IndustrialWorkStateV1,
    owner: &str,
    parents: &[IndustrialParentV1],
    sink_key: &str,
    world: &str,
) -> Result<(), WorldError> {
    if sink_key.is_empty()
        || state.sink_keys.contains(sink_key)
        || parents.len() > 128
        || parents.is_empty()
    {
        return Err(invalid("industrial_sink_invalid_or_consumed"));
    }
    let mut seen = BTreeSet::new();
    for parent in parents {
        validate_ref(&parent.lot_ref, world)?;
        validate_ref(&parent.settlement_ref, world)?;
        let lot = state
            .lots
            .get(&parent.lot_ref.id)
            .filter(|l| l.lot_ref == parent.lot_ref)
            .ok_or_else(|| invalid("industrial_parent_missing"))?;
        if parent.quantity == 0
            || !seen.insert(&parent.lot_ref)
            || lot.owner_id != owner
            || lot.remaining < parent.quantity
        {
            return Err(invalid("industrial_sink_quantity_or_owner_invalid"));
        }
        if parent.settlement_ref != lot.provenance_ref
            && !state
                .receipts
                .get(&parent.settlement_ref.id)
                .is_some_and(|r| {
                    r.receipt_ref == parent.settlement_ref
                        && r.effect_role == "conversion_settlement"
                        && r.output.contains(&parent.lot_ref)
                })
        {
            return Err(invalid("industrial_parent_not_settled"));
        }
    }
    for parent in parents {
        let lot = state
            .lots
            .get_mut(&parent.lot_ref.id)
            .expect("validated lot");
        lot.remaining = lot
            .remaining
            .checked_sub(parent.quantity)
            .ok_or_else(|| invalid("industrial_quantity_underflow"))?;
    }
    state.sink_keys.insert(sink_key.into());
    Ok(())
}

/// Private graph projection. No receipt or binding may bridge into another owner.
pub fn for_reader(state: &IndustrialWorkStateV1, owner: &str) -> IndustrialWorkStateV1 {
    let mut view = state.clone();
    view.roots.retain(|_, root| {
        root.owner_id == owner
            && root.slots.values().all(|slot| {
                slot.parents.iter().all(|parent| {
                    state
                        .lots
                        .get(&parent.lot_ref.id)
                        .is_some_and(|lot| lot.owner_id == owner && lot.lot_ref == parent.lot_ref)
                })
            })
    });
    view.lots.retain(|_, lot| lot.owner_id == owner);
    view.imported_lot_authorities
        .retain(|id, _| view.lots.contains_key(id));
    let root_visible = |reference: &IndustrialRefV1| {
        view.roots
            .get(&reference.id)
            .is_some_and(|root| &root.root_ref == reference)
    };
    let lot_visible = |reference: &IndustrialRefV1| {
        view.lots
            .get(&reference.id)
            .is_some_and(|lot| &lot.lot_ref == reference)
    };
    view.receipts.retain(|_, receipt| {
        root_visible(&receipt.root_ref)
            && receipt.input.iter().all(|p| lot_visible(&p.lot_ref))
            && receipt
                .output
                .iter()
                .all(|reference| lot_visible(reference) || root_visible(reference))
    });
    view.bindings.retain(|binding| {
        root_visible(&binding.root_ref) && binding.parents.iter().all(|p| lot_visible(&p.lot_ref))
    });
    view.dispositions
        .retain(|_, plan| root_visible(&plan.root_ref));
    // Sink keys are opaque world authority internals, never a player read surface.
    view.sink_keys.clear();
    view
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference(id: &str) -> IndustrialRefV1 {
        industrial_ref("world", "industrial", "industrial-work/v1", id, 1, &id).unwrap()
    }
    fn lot() -> IndustrialLotV1 {
        IndustrialLotV1 {
            lot_ref: reference("lot"),
            owner_id: "owner".into(),
            source_ledger: MaterialLedgerId::world(),
            material: "iron".into(),
            unit: "piece".into(),
            spec: "iron-v1".into(),
            quality: "qualified".into(),
            custody: "owner".into(),
            provenance_ref: reference("certificate"),
            quantity: 4,
            remaining: 4,
        }
    }
    fn root(lot: &IndustrialLotV1) -> RootOperationV1 {
        RootOperationV1 {
            owner_binding_digest: "c".repeat(64),
            allowed_dispositions: vec![IndustrialDispositionV1::Terminate],
            output_ledger: MaterialLedgerId::agent("owner"),
            terminal_capacities: std::collections::BTreeMap::from([("output".into(), 10)]),
            started_job_id: None,
            root_ref: reference("root"),
            owner_id: "owner".into(),
            factory_id: "factory".into(),
            pinned_recipe_id: "recipe".into(),
            accepted_batches: 1,
            pinned_profile_digest: "a".repeat(64),
            causal_digest: "b".repeat(64),
            work: 0,
            first_sink: false,
            slots: std::collections::BTreeMap::from([(
                "input".into(),
                IndustrialInputSlotV1 {
                    material_profile: oasis7_wasm_abi::SubstitutionProfileRefV1 {
                        id: "material".into(),
                        version: "1".into(),
                        digest: "d".repeat(64),
                    },
                    material: lot.material.clone(),
                    unit: lot.unit.clone(),
                    spec: lot.spec.clone(),
                    quality: lot.quality.clone(),
                    custody: lot.custody.clone(),
                    quantity: 4,
                    binding_revision: 1,
                    parents: vec![IndustrialParentV1 {
                        lot_ref: lot.lot_ref.clone(),
                        settlement_ref: lot.provenance_ref.clone(),
                        quantity: 4,
                    }],
                    consumed: false,
                },
            )]),
        }
    }
    #[test]
    fn certified_escrow_sink_is_single_use_across_retry_and_binding() {
        let mut state = IndustrialWorkStateV1::default();
        let lot = lot();
        assert!(import_lot(&mut state, lot.clone(), "world").unwrap());
        assert!(!import_lot(&mut state, lot.clone(), "world").unwrap());
        let root = root(&lot);
        accept_root(&mut state, root.clone(), "world").unwrap();
        consume_bound_slot(&mut state, &root.root_ref, "input", "world").unwrap();
        let before = state.clone();
        assert!(consume_bound_slot(&mut state, &root.root_ref, "input", "world").is_err());
        assert_eq!(state, before);
        let plan = ParentBindingRevisionV1 {
            root_ref: root.root_ref,
            slot_id: "input".into(),
            expected_revision: 1,
            decision_ref: reference("decision"),
            parents: root.slots["input"].parents.clone(),
        };
        assert!(apply_parent_binding(&mut state, plan, "world").is_err());
        assert_eq!(state, before);
        assert_eq!(state.lots["lot"].remaining, 0);
        assert_eq!(state.roots["root"].work, 0);
    }
    #[test]
    fn blocked_input_restore_keeps_zero_work_and_private_projection() {
        let mut state = IndustrialWorkStateV1::default();
        let lot = lot();
        import_lot(&mut state, lot.clone(), "world").unwrap();
        let mut root = root(&lot);
        root.slots.get_mut("input").unwrap().parents.clear();
        accept_root(&mut state, root.clone(), "world").unwrap();
        let encoded = serde_json::to_vec(&state).unwrap();
        let mut restored: IndustrialWorkStateV1 = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(state, restored);
        assert!(consume_bound_slot(&mut restored, &root.root_ref, "input", "world").is_err());
        assert_eq!(state, restored);
        assert_eq!(
            for_reader(&state, "stranger"),
            IndustrialWorkStateV1::default()
        );
        assert_eq!(for_reader(&state, "owner"), state);
        let binding = ParentBindingRevisionV1 {
            root_ref: root.root_ref.clone(),
            slot_id: "input".into(),
            expected_revision: 1,
            decision_ref: reference("decision"),
            parents: vec![IndustrialParentV1 {
                lot_ref: lot.lot_ref,
                settlement_ref: lot.provenance_ref,
                quantity: 4,
            }],
        };
        apply_parent_binding(&mut restored, binding.clone(), "world").unwrap();
        let rebound = restored.clone();
        assert!(apply_parent_binding(&mut restored, binding, "world").is_err());
        assert_eq!(restored, rebound);
        consume_bound_slot(&mut restored, &root.root_ref, "input", "world").unwrap();
        assert_eq!(restored.roots["root"].work, 0);
    }
    #[test]
    fn provenance_and_qualification_cannot_be_relabelled() {
        let mut state = IndustrialWorkStateV1::default();
        let lot = lot();
        import_lot(&mut state, lot.clone(), "world").unwrap();
        let mut duplicate = lot.clone();
        duplicate.lot_ref = reference("other-lot");
        assert!(import_lot(&mut state, duplicate, "world").is_err());
        let mut root = root(&lot);
        root.slots.get_mut("input").unwrap().quality = "premium".into();
        assert!(accept_root(&mut state, root, "world").is_err());
        assert!(state.roots.is_empty());
    }
    #[test]
    fn same_child_identity_cannot_rewrite_digest_under_new_receipt() {
        let mut state = IndustrialWorkStateV1::default();
        let lot = lot();
        import_lot(&mut state, lot.clone(), "world").unwrap();
        let root = root(&lot);
        accept_root(&mut state, root.clone(), "world").unwrap();
        let receipt = IndustrialReceiptV1 {
            receipt_ref: reference("receipt-one"),
            root_ref: root.root_ref,
            child_ref: reference("child"),
            effect_role: "effect".into(),
            payload_digest: "a".repeat(64),
            input: vec![],
            output: vec![],
        };
        record_receipt(&mut state, receipt.clone(), "world").unwrap();
        let before = state.clone();
        let mut conflicting = receipt;
        conflicting.receipt_ref = reference("receipt-two");
        conflicting.child_ref.digest = "f".repeat(64);
        assert!(record_receipt(&mut state, conflicting, "world").is_err());
        assert_eq!(state, before);
    }
    #[test]
    fn shared_root_acceptance_rejects_missing_new_authority_and_started_state() {
        let mut state = IndustrialWorkStateV1::default();
        let lot = lot();
        import_lot(&mut state, lot.clone(), "world").unwrap();
        for mutation in 0..6 {
            let mut root = root(&lot);
            match mutation {
                0 => root.owner_binding_digest.clear(),
                1 => root.started_job_id = Some(1),
                2 => root.allowed_dispositions = vec![IndustrialDispositionV1::Hold],
                3 => root
                    .slots
                    .get_mut("input")
                    .unwrap()
                    .material_profile
                    .digest
                    .clear(),
                4 => root.terminal_capacities.clear(),
                _ => root.output_ledger = MaterialLedgerId::world(),
            }
            assert!(accept_root(&mut state, root, "world").is_err());
            assert!(state.roots.is_empty());
        }
    }
}
