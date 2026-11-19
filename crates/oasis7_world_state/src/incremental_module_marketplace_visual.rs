//! Actual visual point ownership and bounded mutation in the one transaction.
//! Anchor admission remains with Runtime's real agent/location owner readers.
use super::*;
use crate::{
    incremental_field_member::{
        MemberPayloadCodecV1, MemberRecordLimitsV1, read_member_record_header_v1,
    },
    incremental_field_overlay::lookup,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_owner_closure::marketplace_bridge as bridge,
    state::{ModuleVisualPointErrorV1, check_module_visual_point_v1},
};
use oasis7_agent_api::module_visual::ModuleVisualMutation;
use oasis7_formal_protocol::incremental_value::ValueReadContext;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction, RootSetV1},
    incremental_state::{StateEntryV1, StateScanBudget, StateScanContext},
};
pub struct ModuleVisualOverlayWorkV1 {
    pub pending_entries_remaining: usize,
    pub body_scan: StateScanBudget,
    pub cells_remaining: u64,
    pub max_owner_bytes: usize,
}
#[derive(Debug)]
pub enum ModuleVisualOverlayErrorV1 {
    Storage(WorldFieldOverlayErrorV1),
    Rule {
        kind: ModuleVisualPointErrorV1,
        existing_owner: Option<ReservedValue<String>>,
    },
}
impl From<WorldFieldOverlayErrorV1> for ModuleVisualOverlayErrorV1 {
    fn from(e: WorldFieldOverlayErrorV1) -> Self {
        Self::Storage(e)
    }
}
impl From<ValueTreeError> for ModuleVisualOverlayErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Storage(e.into())
    }
}
impl From<ExecutionError> for ModuleVisualOverlayErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Storage(e.into())
    }
}
impl From<MemberRecordError> for ModuleVisualOverlayErrorV1 {
    fn from(e: MemberRecordError) -> Self {
        Self::Storage(e.into())
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("module visual current point")
}
fn owner_error(
    error: crate::incremental_owner_closure::WorldStateOwnerError,
) -> ModuleVisualOverlayErrorV1 {
    use crate::incremental_owner_closure::WorldStateOwnerError as E;
    match error {
        E::Value(e) => e.into(),
        E::Memory(e) => ValueTreeError::Memory(e).into(),
        E::ResourceLimited(s) => ValueTreeError::ResourceLimited(s).into(),
        E::RootSet(e) => e.into(),
        E::Scan(_) | E::Invalid(_) => invalid().into(),
    }
}
fn scan_error<E>(
    e: oasis7_formal_protocol::incremental_state::StateScanError<E>,
) -> ValueTreeError {
    use oasis7_formal_protocol::incremental_state::StateScanError as E;
    match e {
        E::Cancelled => ValueTreeError::Cancelled,
        E::Memory(e) => ValueTreeError::Memory(e),
        E::ResourceLimited => ValueTreeError::ResourceLimited("module visual point scan"),
        E::Visitor(_) | E::Invalid(_) | E::Tree(_) => invalid(),
    }
}
/// Parent closed-family count plus the ACTUAL coalesced transaction presence
/// delta. All admitted entries spend work, including unrelated reads/noops.
/// No caller count, new wire counter or full visual map scan is accepted.
fn count<M: MerkleNodeReader>(
    family: &ReservedValue<ModuleMarketplaceFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &ExecutionTransaction<'_, M>,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    work: &mut ModuleVisualOverlayWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<usize, ModuleVisualOverlayErrorV1> {
    if !family.uses_budget(memory)
        || family.allocation_class() != MemoryClass::Ordinary
        || family.reserved_bytes() < std::mem::size_of::<ModuleMarketplaceFamilyQualificationV1>()
        || !tx.uses_budget(memory)
    {
        return Err(invalid().into());
    }
    family.validate_binding(roots).map_err(owner_error)?;
    if roots.digest()? != tx.parent_fence().parent_roots {
        return Err(invalid().into());
    }
    let mut n = family.record_counts()[0];
    tx.visit_pending_writes(
        StateNamespaceV1::Business,
        &mut work.pending_entries_remaining,
        || {
            if cancelled() {
                Err(ModuleVisualOverlayErrorV1::from(ValueTreeError::Cancelled))
            } else {
                Ok(())
            }
        },
        |key, before, after| {
            let Some((slot, _, field)) = family::identify(key).map_err(owner_error)? else {
                return Ok(());
            };
            if slot != 0
                || WorldFieldKeyV1::from_bytes(field.as_bytes())
                    .map_err(ValueTreeError::Invalid)?
                    .kind
                    != WorldFieldRecordKindV1::Entry
            {
                return Ok(());
            }
            match (before.is_some(), after.is_some()) {
                (false, true) => n = n.checked_add(1).ok_or_else(invalid)?,
                (true, false) => n = n.checked_sub(1).ok_or_else(invalid)?,
                _ => {}
            }
            Ok(())
        },
    )?;
    usize::try_from(n).map_err(|_| ValueTreeError::ResourceLimited("module visual count").into())
}
struct Text {
    bytes: Vec<u8>,
    max: usize,
}
impl Write for Text {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        let b = &b[..b.len().min(64 << 10)];
        if b.len() > self.max.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("visual owner bound"));
        }
        self.bytes.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Reads one current immutable child binding and its module_id scalar only.
/// Its family bootstrap/changed-path receiver remains the structural authority.
#[allow(
    clippy::too_many_arguments,
    reason = "The transaction, value/key budgets, byte limits, state error, cancellation, and work account remain separate controls for this current-child proof."
)]
fn owner<M, R, E, C>(
    entity: &str,
    state: &M,
    tx: &mut ExecutionTransaction<'_, M>,
    values: &mut ValueReadContext<'_, R>,
    keys: &mut crate::incremental_semantic_key::SemanticKeyBudget,
    semantic: u64,
    record: u64,
    error: &mut E,
    cancelled: &mut C,
    work: &mut ModuleVisualOverlayWorkV1,
) -> Result<Option<ReservedValue<String>>, ModuleVisualOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("module visual cells"))?;
    let field = ModuleMarketplaceFieldV1::VisualEntities;
    let descriptor = field.descriptor()?;
    let (key, root) = {
        let mut records = InventoryValueReader {
            values,
            key_work: keys,
            semantic_bytes: semantic,
            record_bytes: record,
            cancelled,
        };
        let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let id = lookup(tx, &metadata, &mut records, error)?.ok_or_else(invalid)?;
        records.presence(&id, &metadata, 0)?;
        let (key, _) = records.key(descriptor, WorldFieldRecordKindV1::Entry, entity)?;
        let root = lookup(tx, &key, &mut records, error)?;
        (key, root)
    };
    let Some(root) = root else {
        return Ok(None);
    };
    let entry = StateEntryV1 {
        namespace: StateNamespaceV1::Business,
        key: key.as_bytes().to_vec(),
        value: root,
    };
    let child = bridge::child_root(descriptor, &entry, values, semantic, record, cancelled)
        .map_err(owner_error)?;
    let (key, key_bytes) = {
        let mut records = InventoryValueReader {
            values,
            key_work: keys,
            semantic_bytes: semantic,
            record_bytes: record,
            cancelled,
        };
        records.key(descriptor, WorldFieldRecordKindV1::Entry, "module_id")?
    };
    let mut scan = StateScanContext {
        source: state,
        memory: values.memory,
        class: values.class,
        io: &mut work.body_scan,
    };
    let result = scan.with_reader(&mut *cancelled, |guard| guard.get(&child, key.as_bytes()));
    if let Some(error) = error() {
        return Err(error.into());
    }
    let id = result
        .map_err(|e| match e {
            oasis7_formal_protocol::incremental_state::StateScanError::Visitor(inner) => {
                scan_error(inner)
            }
            other => scan_error(other),
        })?
        .ok_or_else(invalid)?;
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: semantic,
        payload_bytes: work.max_owner_bytes as u64,
        record_bytes: record,
    };
    let header = read_member_record_header_v1(&id, &key, values, limits, &mut *cancelled)?;
    if header.codec != MemberPayloadCodecV1::Text || header.semantic_bytes != key_bytes {
        return Err(invalid().into());
    }
    let len = usize::try_from(header.payload_bytes).map_err(|_| invalid())?;
    let charge = values
        .memory
        .reserve(len.checked_add(128).ok_or_else(invalid)?, values.class)
        .map_err(ValueTreeError::Memory)?;
    let mut text = Text {
        bytes: Vec::with_capacity(len),
        max: len,
    };
    read_member_record_profile_v1(
        &id,
        &key,
        values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Text,
            semantic_bytes: key_bytes,
            limits,
        },
        &mut io::sink(),
        &mut text,
        cancelled,
    )?;
    if text.bytes.len() != len {
        return Err(invalid().into());
    }
    let value = String::from_utf8(text.bytes).map_err(|_| invalid())?;
    Ok(Some(charge.retain(value)))
}
/// Data/ownership/count mutation only; real Runtime must first validate an
/// upsert's actual agent or active location anchor using its original kernel.
#[expect(
    clippy::too_many_arguments,
    reason = "Real parent/family, transaction, finite typed writer and original fence remain explicit."
)]
pub fn change_module_visual_overlay_v1<M, R, E, C, W, WC>(
    module_id: &str,
    mutation: &ModuleVisualMutation,
    roots: &RootSetV1,
    family: &ReservedValue<ModuleMarketplaceFamilyQualificationV1>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, WC>,
    work: &mut ModuleVisualOverlayWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), ModuleVisualOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    WC: FnMut() -> bool,
{
    let values = &mut ctx.values;
    let keys = &mut *ctx.key_work;
    let error = &mut ctx.take_state_error;
    let cancelled = &mut ctx.cancelled;
    let semantic = ctx.semantic_bytes;
    let record = ctx.record_bytes;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if values.class != MemoryClass::Ordinary
            || !tx.uses_budget(write.memory)
            || !values.memory.same_pool(write.memory)
        {
            return Err(invalid().into());
        }
        let _phase = values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let entity = match mutation {
            ModuleVisualMutation::Upsert(v) => v.entity_id.as_str(),
            ModuleVisualMutation::Remove(id) => id.as_str(),
        };
        let existing = owner(
            entity, state, tx, values, keys, semantic, record, error, cancelled, work,
        )?;
        let n = count(family, roots, tx, values.memory, work, cancelled)?;
        if let Err(kind) = check_module_visual_point_v1(
            module_id,
            mutation,
            existing.as_ref().map(|s| s.as_str()),
            n,
        ) {
            return Err(ModuleVisualOverlayErrorV1::Rule {
                kind,
                existing_owner: existing,
            });
        }
        let field = ModuleMarketplaceFieldV1::VisualEntities;
        let descriptor = field.descriptor()?;
        match mutation {
            ModuleVisualMutation::Upsert(value) => {
                let child = append_visual(descriptor, value, write, &mut work.cells_remaining)?;
                write_outer_root(
                    field,
                    descriptor,
                    &entity,
                    child,
                    write,
                    &mut work.cells_remaining,
                    &mut |row| {
                        tx.write(row.namespace, &row.mutation.key, row.mutation.after)
                            .map_err(|_| invalid())
                    },
                )?;
            }
            ModuleVisualMutation::Remove(_) => {
                let (key, _) = {
                    let mut records = InventoryValueReader {
                        values,
                        key_work: keys,
                        semantic_bytes: semantic,
                        record_bytes: record,
                        cancelled,
                    };
                    records.key(descriptor, WorldFieldRecordKindV1::Entry, entity)?
                };
                tx.write(StateNamespaceV1::Business, key.as_bytes(), None)?;
            }
        }
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        if let Some(error) = error() {
            return Err(error.into());
        }
        Ok(())
    })
}
