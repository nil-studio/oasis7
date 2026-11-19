//! The original industrial parent gates and sequential slot mutation, over a
//! selected paid DATA rehearsal. This does not create an execution permit.
use super::{
    invalid, readonly_industrial_data as data,
    readonly_industrial_scratch::IndustrialScratchReadonlyFactsV1,
};
use crate::WorldError;
use oasis7_agent_api::world_action::*;
use std::{collections::BTreeSet, mem::size_of};
pub(super) trait ParentWorkV1 {
    type Error;
    fn owner_work(&self, steps: u64, bytes: usize) -> Result<(), Self::Error>;
    fn owner_before_owned(&self, bytes: usize) -> Result<(), Self::Error>;
    fn copy_parents(
        &self,
        parents: &[IndustrialParentV1],
    ) -> Result<Vec<IndustrialParentV1>, Self::Error>;
    fn sink_slot(&self, key: &str, count: usize) -> Result<(), Self::Error>;
    fn disposition_present(
        &self,
        state: &IndustrialWorkStateV1,
        id: &str,
    ) -> Result<bool, Self::Error>;
    fn sink_present(&self, state: &IndustrialWorkStateV1, key: &str) -> Result<bool, Self::Error>;
    fn with_receipt<T>(
        &self,
        state: &IndustrialWorkStateV1,
        id: &str,
        visit: impl FnOnce(Option<&IndustrialReceiptV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
}
pub(super) struct PaidParentWorkV1<'a, S>(pub &'a S);
impl<S: IndustrialScratchReadonlyFactsV1> ParentWorkV1 for PaidParentWorkV1<'_, S> {
    type Error = S::Error;
    fn owner_work(&self, steps: u64, bytes: usize) -> Result<(), Self::Error> {
        self.0.owner_work(steps, bytes)
    }
    fn owner_before_owned(&self, bytes: usize) -> Result<(), Self::Error> {
        self.0.owner_before_owned(bytes)
    }
    fn copy_parents(
        &self,
        parents: &[IndustrialParentV1],
    ) -> Result<Vec<IndustrialParentV1>, Self::Error> {
        data::parents(self.0, parents)
    }
    fn sink_slot(&self, key: &str, count: usize) -> Result<(), Self::Error> {
        data::tree_slot::<_, ()>(self.0, key, count)
    }
    fn disposition_present(
        &self,
        _: &IndustrialWorkStateV1,
        id: &str,
    ) -> Result<bool, Self::Error> {
        self.0
            .industrial_with_disposition(id, |value| Ok(value.is_some()))
    }
    fn sink_present(&self, state: &IndustrialWorkStateV1, key: &str) -> Result<bool, Self::Error> {
        if state.sink_keys.contains(key) {
            Ok(true)
        } else {
            self.0.industrial_sink_present(key)
        }
    }
    fn with_receipt<T>(
        &self,
        _: &IndustrialWorkStateV1,
        id: &str,
        visit: impl FnOnce(Option<&IndustrialReceiptV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.0.industrial_with_receipt(id, visit)
    }
}
pub(super) struct LegacyParentWorkV1;
impl ParentWorkV1 for LegacyParentWorkV1 {
    type Error = std::convert::Infallible;
    fn owner_work(&self, _: u64, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn owner_before_owned(&self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn copy_parents(
        &self,
        parents: &[IndustrialParentV1],
    ) -> Result<Vec<IndustrialParentV1>, Self::Error> {
        Ok(parents.to_vec())
    }
    fn sink_slot(&self, _: &str, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn disposition_present(
        &self,
        state: &IndustrialWorkStateV1,
        id: &str,
    ) -> Result<bool, Self::Error> {
        Ok(state.dispositions.contains_key(id))
    }
    fn sink_present(&self, state: &IndustrialWorkStateV1, key: &str) -> Result<bool, Self::Error> {
        Ok(state.sink_keys.contains(key))
    }
    fn with_receipt<T>(
        &self,
        state: &IndustrialWorkStateV1,
        id: &str,
        visit: impl FnOnce(Option<&IndustrialReceiptV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        visit(state.receipts.get(id))
    }
}
fn reject<S: ParentWorkV1, T>(s: &S, reason: &str) -> Result<Result<T, WorldError>, S::Error> {
    s.owner_work(1, reason.len())?;
    s.owner_before_owned(size_of::<WorldError>() + reason.len())?;
    Ok(Err(invalid(reason)))
}
fn reference_bytes(v: &IndustrialRefV1) -> usize {
    v.world_binding.len()
        + v.domain.len()
        + v.schema.len()
        + v.id.len()
        + v.digest.len()
        + size_of::<u64>()
}
macro_rules! domain {
    ($value:expr) => {
        match $value {
            Ok(v) => v,
            Err(e) => return Ok(Err(e)),
        }
    };
}
pub(super) fn validate_parents<S: ParentWorkV1>(
    s: &S,
    state: &IndustrialWorkStateV1,
    root: &RootOperationV1,
    slot: &IndustrialInputSlotV1,
    parents: &[IndustrialParentV1],
    world: &str,
) -> Result<Result<(), WorldError>, S::Error> {
    s.owner_work(1, 0)?;
    if parents.len() > 128 {
        return reject(s, "industrial_capacity_exceeded");
    }
    if slot.quantity == 0
        || slot.material.is_empty()
        || slot.unit.is_empty()
        || slot.spec.is_empty()
        || slot.quality.is_empty()
        || slot.custody.is_empty()
    {
        return reject(s, "industrial_slot_evidence_incomplete");
    }
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for parent in parents {
        s.owner_work(1, 0)?;
        for value in [&parent.lot_ref, &parent.settlement_ref] {
            s.owner_work(1, reference_bytes(value) + world.len())?;
            if !value.is_qualified() || value.world_binding != world {
                return reject(s, "industrial_reference_invalid");
            }
        }
        // The original short circuit does not insert an empty-quantity parent.
        if parent.quantity == 0 {
            return reject(s, "industrial_parent_duplicate_or_empty");
        }
        let levels = (seen.len() + 1).ilog2() as usize + 2;
        s.owner_work(
            1,
            reference_bytes(&parent.lot_ref).saturating_mul(levels * 2 + 4),
        )?;
        s.owner_before_owned(512 * levels)?;
        if !seen.insert(&parent.lot_ref) {
            return reject(s, "industrial_parent_duplicate_or_empty");
        }
        s.owner_work(
            1,
            (parent.lot_ref.id.len() + 1)
                .saturating_mul((state.lots.len() + 1).ilog2() as usize * 2 + 4),
        )?;
        let lot = state.lots.get(&parent.lot_ref.id);
        if let Some(lot) = lot {
            s.owner_work(
                1,
                reference_bytes(&lot.lot_ref) + reference_bytes(&parent.lot_ref),
            )?
        }
        let Some(lot) = lot.filter(|l| l.lot_ref == parent.lot_ref) else {
            return reject(s, "industrial_parent_missing");
        };
        s.owner_work(
            1,
            lot.owner_id.len()
                + root.owner_id.len()
                + lot.material.len()
                + slot.material.len()
                + lot.unit.len()
                + slot.unit.len()
                + lot.spec.len()
                + slot.spec.len()
                + lot.quality.len()
                + slot.quality.len()
                + lot.custody.len()
                + slot.custody.len(),
        )?;
        if lot.owner_id != root.owner_id
            || lot.material != slot.material
            || lot.unit != slot.unit
            || lot.spec != slot.spec
            || lot.quality != slot.quality
            || lot.custody != slot.custody
            || lot.remaining < parent.quantity
        {
            return reject(s, "industrial_parent_not_applicable");
        }
        s.owner_work(
            1,
            reference_bytes(&parent.settlement_ref) + reference_bytes(&lot.provenance_ref),
        )?;
        let imported = parent.settlement_ref == lot.provenance_ref;
        // Legacy computes conversion evidence even for an imported parent.
        s.owner_work(
            1,
            (parent.settlement_ref.id.len() + 1)
                .saturating_mul((state.receipts.len() + 1).ilog2() as usize * 2 + 4),
        )?;
        let converted = s.with_receipt(state, &parent.settlement_ref.id, |value| {
            let Some(receipt) = value else {
                return Ok(false);
            };
            s.owner_work(
                1,
                reference_bytes(&receipt.receipt_ref)
                    + reference_bytes(&parent.settlement_ref)
                    + receipt.effect_role.len(),
            )?;
            if receipt.receipt_ref == parent.settlement_ref
                && receipt.effect_role == "conversion_settlement"
            {
                for output in &receipt.output {
                    s.owner_work(
                        1,
                        reference_bytes(output) + reference_bytes(&parent.lot_ref),
                    )?;
                    if output == &parent.lot_ref {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        })?;
        if !imported && !converted {
            return reject(s, "industrial_parent_not_settled");
        }
        let Some(next) = total.checked_add(parent.quantity) else {
            return reject(s, "industrial_quantity_overflow");
        };
        total = next;
    }
    if total != slot.quantity {
        return reject(s, "industrial_parent_quantity_mismatch");
    }
    Ok(Ok(()))
}
pub(super) fn consume_bound_slot<S: ParentWorkV1>(
    s: &S,
    state: &mut IndustrialWorkStateV1,
    root_ref: &IndustrialRefV1,
    slot_id: &str,
    world: &str,
) -> Result<Result<Vec<IndustrialParentV1>, WorldError>, S::Error> {
    s.owner_work(
        1,
        (root_ref.id.len() + 1).saturating_mul((state.roots.len() + 1).ilog2() as usize * 2 + 4),
    )?;
    let root = state.roots.get(&root_ref.id);
    if let Some(root) = root {
        s.owner_work(
            1,
            reference_bytes(&root.root_ref) + reference_bytes(root_ref),
        )?
    }
    let Some(root) = root.filter(|r| &r.root_ref == root_ref) else {
        return reject(s, "industrial_root_missing");
    };
    s.owner_work(
        1,
        (root_ref.id.len() + 1)
            .saturating_mul((state.dispositions.len() + 1).ilog2() as usize * 2 + 4),
    )?;
    if s.disposition_present(state, &root_ref.id)? {
        return reject(s, "industrial_root_disposed");
    }
    s.owner_work(
        1,
        (slot_id.len() + 1).saturating_mul((root.slots.len() + 1).ilog2() as usize * 2 + 4),
    )?;
    let Some(slot) = root.slots.get(slot_id) else {
        return reject(s, "industrial_slot_missing");
    };
    s.owner_before_owned(64 + size_of::<String>())?;
    let key = match crate::util::hash_json_with_work_v1(
        &(root_ref, slot_id, "parent_sink"),
        &mut |steps, bytes| s.owner_work(steps, bytes as usize),
    ) {
        Ok(v) => v,
        Err(crate::util::JsonHashWorkErrorV1::Work(e)) => return Err(e),
        Err(crate::util::JsonHashWorkErrorV1::Codec(error)) => {
            let text = super::readonly_format::format(
                format_args!("{}", error),
                &mut |steps, bytes, heap| {
                    s.owner_work(steps, bytes)?;
                    s.owner_before_owned(heap)
                },
            )?;
            return Ok(Err(WorldError::Serde(text)));
        }
    };
    if !slot.consumed {
        s.owner_work(
            1,
            (key.len() + 1).saturating_mul((state.sink_keys.len() + 1).ilog2() as usize * 2 + 4),
        )?
    }
    if slot.consumed || s.sink_present(state, &key)? {
        return reject(s, "industrial_slot_already_consumed");
    }
    domain!(validate_parents(
        s,
        state,
        root,
        slot,
        &slot.parents,
        world
    )?);
    let parents = s.copy_parents(&slot.parents)?;
    for parent in &parents {
        s.owner_work(
            1,
            (parent.lot_ref.id.len() + 1)
                .saturating_mul((state.lots.len() + 1).ilog2() as usize * 2 + 4),
        )?;
        let lot = state
            .lots
            .get_mut(&parent.lot_ref.id)
            .expect("validated lot");
        let Some(remaining) = lot.remaining.checked_sub(parent.quantity) else {
            return reject(s, "industrial_quantity_underflow");
        };
        lot.remaining = remaining;
    }
    s.owner_work(1, root_ref.id.len() + slot_id.len())?;
    let root = state.roots.get_mut(&root_ref.id).expect("validated root");
    root.first_sink = true;
    root.slots
        .get_mut(slot_id)
        .expect("validated slot")
        .consumed = true;
    s.sink_slot(&key, state.sink_keys.len())?;
    state.sink_keys.insert(key);
    Ok(Ok(parents))
}
