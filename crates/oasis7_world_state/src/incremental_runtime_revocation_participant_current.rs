//! Original-key ordered participant selection. A binding is DATA, not authority.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use crate::incremental_inventory_value_read::InventoryValueReader;
use crate::incremental_typed_record_diff::{self as diff, ChangeDecoder};
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation, ReservedValue};
fn spend(work: &mut RevocationReadWorkV1, n: usize) -> Result<(), ValueTreeError> {
    let n = u64::try_from(n)
        .ok()
        .and_then(|n| n.checked_add(1))
        .ok_or_else(invalid)?;
    work.json.cells_remaining =
        work.json
            .cells_remaining
            .checked_sub(n)
            .ok_or(ValueTreeError::ResourceLimited(
                "participant index comparisons",
            ))?;
    Ok(())
}
#[path = "incremental_runtime_revocation_participant_namespace.rs"]
mod namespaces;
use namespaces::{ParticipantNamespaceIndexV1, advance_namespaces};
pub(super) struct ParticipantBindingIndexV1 {
    namespaces: ReservedValue<ParticipantNamespaceIndexV1>,
    map: StateRootV1,
    entries: Vec<(String, String)>,
    _texts: Vec<MemoryReservation>,
}
fn slots(
    n: usize,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<
    (
        MemoryReservation,
        Vec<(String, String)>,
        Vec<MemoryReservation>,
    ),
    ValueTreeError,
> {
    let bytes = n
        .checked_mul(
            std::mem::size_of::<(String, String)>() + std::mem::size_of::<MemoryReservation>(),
        )
        .and_then(|n| n.checked_add(512))
        .ok_or_else(invalid)?;
    let fee = memory.reserve(bytes, class)?;
    let mut entries = Vec::with_capacity(n);
    let mut texts = Vec::with_capacity(n);
    if entries.capacity() > n || texts.capacity() > n {
        return Err(invalid());
    }
    entries.clear();
    texts.clear();
    Ok((fee, entries, texts))
}
fn copy_pair(
    key: &str,
    agent: &str,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<(MemoryReservation, (String, String)), ValueTreeError> {
    let fee = memory.reserve(
        key.len().checked_add(agent.len()).ok_or_else(invalid)?,
        class,
    )?;
    let mut k = String::with_capacity(key.len());
    let mut a = String::with_capacity(agent.len());
    if k.capacity() > key.len() || a.capacity() > agent.len() {
        return Err(invalid());
    }
    k.push_str(key);
    a.push_str(agent);
    Ok((fee, (k, a)))
}
impl ParticipantBindingIndexV1 {
    pub(super) fn readmit(
        &self,
        memory: &MemoryBudget,
        class: MemoryClass,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<Self>, ValueTreeError> {
        let (fee, mut entries, mut texts) = slots(self.entries.len(), memory, class)?;
        for (key, agent) in &self.entries {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            let (charge, pair) = copy_pair(key, agent, memory, class)?;
            texts.push(charge);
            entries.push(pair);
        }
        if cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        let namespaces = self.namespaces.readmit(memory, class, &mut cancelled)?;
        Ok(fee.retain(Self {
            namespaces,
            map: self.map.clone(),
            entries,
            _texts: texts,
        }))
    }
    fn first(
        &self,
        agent: &str,
        work: &mut RevocationReadWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Option<&str>, ValueTreeError> {
        for (key, id) in &self.entries {
            if cancelled() {
                return Err(ValueTreeError::Cancelled);
            }
            spend(work, id.len().checked_add(agent.len()).ok_or_else(invalid)?)?;
            if id == agent {
                return Ok(Some(key));
            }
        }
        Ok(None)
    }
}
pub(super) fn advance_participant_index<M, R, E, C>(
    parent: Option<&ReservedValue<ParticipantBindingIndexV1>>,
    body: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<ReservedValue<ParticipantBindingIndexV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if parent.is_some_and(|p| !p.uses_budget(ctx.values.memory)) {
        return Err(invalid());
    }
    let profile = profile();
    let mut d = ChangeDecoder {
        trees_remaining: work.max_retained_trees,
        tree_depth: 0,
        profile,
        ctx,
        scan,
        work,
    };
    let participants = diff::point_typed_column_v1(&profile, body, "participants", &mut d, 8192)?;
    let ColumnCellV1::Root(participants) = &participants.cell else {
        return Err(invalid());
    };
    let binding = diff::point_typed_column_v1(&profile, participants, "bindings", &mut d, 8192)?;
    let ColumnCellV1::Root(map) = &binding.cell else {
        return Err(invalid());
    };
    let length = diff::point_typed_column_v1(&profile, map, "_length", &mut d, 8192)?;
    let ColumnCellV1::U64(count) = length.cell else {
        return Err(invalid());
    };
    let count = usize::try_from(count).map_err(|_| invalid())?;
    if count.checked_add(2) != usize::try_from(map.leaf_count).ok()
        || count > d.work.max_tree_heap / 128
    {
        return Err(invalid());
    }
    let cap = count
        .checked_add(parent.map_or(0, |p| p.entries.len()))
        .ok_or_else(invalid)?;
    let memory = d.ctx.values.memory;
    let class = d.ctx.values.class;
    let (fee, mut entries, mut texts) = slots(cap, memory, class)?;
    if let Some(parent) = parent {
        for (key, agent) in &parent.entries {
            d.check()?;
            spend(
                d.work,
                key.len().checked_add(agent.len()).ok_or_else(invalid)?,
            )?;
            let (charge, pair) = copy_pair(key, agent, memory, class)?;
            texts.push(charge);
            entries.push(pair);
        }
    }
    let empty = StateRootV1::empty(map.namespace).map_err(|_| invalid())?;
    let before = parent.map_or(&empty, |p| &p.map);
    diff::visit_entries(before, map, &mut d, |old, new, d| {
        let cell = new.or(old).ok_or_else(invalid)?;
        let diff::TypedCellRefV1::Entry(entry) = cell else {
            return Err(invalid());
        };
        let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
        if parsed.kind == WorldFieldRecordKindV1::Metadata {
            return Ok(());
        }
        let key = diff::key(&profile, cell, d)?;
        let ColumnKeyV1::Pair(prefix, key) = &*key else {
            return Ok(());
        };
        if **prefix != ColumnKeyV1::text("entry") {
            return Err(invalid());
        }
        let ColumnKeyV1::Text(key) = &**key else {
            return Err(invalid());
        };
        for (k, _) in &entries {
            d.check()?;
            spend(
                d.work,
                k.len()
                    .checked_add(key.len())
                    .and_then(|n| n.checked_mul(2))
                    .ok_or_else(invalid)?,
            )?;
        }
        entries.retain(|(k, _)| k != key);
        if let Some(new) = new {
            let root = diff::root(&profile, new, d)?;
            let value = crate::incremental_typed_record_codec::recover_typed_record_body_v1::<
                oasis7_agent_api::game::GameParticipantReceiptV1,
                _,
                _,
                _,
                _,
            >(&root, &profile, d.ctx, d.scan, d.work)?;
            if entries.len() >= cap || texts.len() >= cap {
                return Err(invalid());
            }
            let (charge, pair) = copy_pair(key, &value.value().agent_id, memory, class)?;
            texts.push(charge);
            entries.push(pair);
        }
        Ok(())
    })?;
    // Insertion sort charges every key comparison and shift; map-key order is the
    // original bindings.values().find order, including duplicate agent IDs.
    for i in 1..entries.len() {
        let mut j = i;
        while j > 0 {
            d.check()?;
            spend(
                d.work,
                entries[j - 1]
                    .0
                    .len()
                    .checked_add(entries[j].0.len())
                    .and_then(|n| n.checked_add(std::mem::size_of::<(String, String)>() * 2))
                    .ok_or_else(invalid)?,
            )?;
            if entries[j - 1].0 <= entries[j].0 {
                break;
            }
            entries.swap(j - 1, j);
            j -= 1;
        }
    }
    let namespaces = advance_namespaces(parent.map(|p| &p.namespaces), participants, &mut d)?;
    d.check()?;
    Ok(fee.retain(ParticipantBindingIndexV1 {
        namespaces,
        map: map.clone(),
        entries,
        _texts: texts,
    }))
}
pub struct CurrentParticipantOwnerDataV1 {
    namespace_authorized: bool,
    receipt: Option<RevocationRecoveredV1<oasis7_agent_api::game::GameParticipantReceiptV1>>,
    policy: Option<
        RevocationRecoveredV1<crate::service_admission::participant::ParticipantEntryPolicyV1>,
    >,
}
impl CurrentParticipantOwnerDataV1 {
    pub fn namespace_was_authorized(&self) -> bool {
        self.namespace_authorized
    }
    pub fn receipt(&self) -> Option<&oasis7_agent_api::game::GameParticipantReceiptV1> {
        self.receipt.as_ref().map(|r| r.value())
    }
    pub fn policy(
        &self,
    ) -> Option<&crate::service_admission::participant::ParticipantEntryPolicyV1> {
        self.policy.as_ref().map(|r| r.value())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_participant_owner_data_v1<M, R, E, C, P, V, ER, CR>(
    agent: &str,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    current: &RuntimeRevocationCurrentV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentParticipantOwnerDataV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    overlay.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if overlay.values.class != MemoryClass::Ordinary
            || read.values.class != MemoryClass::Ordinary
            || !qualification.uses_budget(overlay.values.memory)
            || !tx.uses_budget(overlay.values.memory)
            || !read.values.memory.same_pool(overlay.values.memory)
        {
            return Err(invalid().into());
        }
        qualification.validate_digest(&tx.parent_fence().parent_roots)?;
        let fee = overlay
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentParticipantOwnerDataV1>() + 8192,
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
        current.check(tx, &body, overlay.values.memory)?;
        let index = current.participant_bindings();
        if !index.uses_budget(overlay.values.memory) {
            return Err(invalid().into());
        }
        let selected = index.first(agent, work, &mut overlay.cancelled)?;
        let mut receipt = None;
        let mut policy = None;
        let mut namespace_authorized = false;
        if let Some(key) = selected {
            let profile = profile();
            let participants = {
                let mut d = ChangeDecoder {
                    trees_remaining: work.max_retained_trees,
                    tree_depth: 0,
                    profile,
                    ctx: read,
                    scan,
                    work,
                };
                diff::point_typed_column_v1(&profile, &body, "participants", &mut d, 8192)?
            };
            let ColumnCellV1::Root(participants) = &participants.cell else {
                return Err(invalid().into());
            };
            let row = super::current_identity::map_entry(
                participants,
                "bindings",
                key,
                read,
                scan,
                work,
            )?
            .ok_or_else(invalid)?;
            let ColumnCellV1::Root(root) = &row.cell else {
                return Err(invalid().into());
            };
            let found = crate::incremental_typed_record_codec::recover_typed_record_body_v1::<
                oasis7_agent_api::game::GameParticipantReceiptV1,
                _,
                _,
                _,
                _,
            >(root, &profile, read, scan, work)?;
            if found.value().agent_id != agent {
                return Err(invalid().into());
            }
            namespace_authorized = index.namespaces.contains(
                &found.value().identity_namespace,
                work,
                &mut overlay.cancelled,
            )?;
            receipt = Some(found);
            let row = {
                let mut d = ChangeDecoder {
                    trees_remaining: work.max_retained_trees,
                    tree_depth: 0,
                    profile,
                    ctx: read,
                    scan,
                    work,
                };
                diff::point_typed_column_v1(&profile, participants, "policy", &mut d, 8192)?
            };
            policy = match &row.cell {
                ColumnCellV1::Unit => None,
                ColumnCellV1::Root(root) => Some(
                    crate::incremental_typed_record_codec::recover_typed_record_body_v1(
                        root, &profile, read, scan, work,
                    )?,
                ),
                _ => return Err(invalid().into()),
            };
        }
        read.check()?;
        if (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(e) = (overlay.take_state_error)() {
            return Err(e.into());
        }
        if let Some(e) = (read.take_state_error)() {
            return Err(e.into());
        }
        Ok(fee.retain(CurrentParticipantOwnerDataV1 {
            receipt,
            policy,
            namespace_authorized,
        }))
    })
}
