//! Qualified-parent/current-root point reads, never a revocation qualification.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_typed_record_diff::{ChangeDecoder, point_typed_column_v1},
};
use oasis7_formal_protocol::incremental_execution::{ExecutionFence, PreparedGuard};
use oasis7_proto::memory_budget::ReservedValue;
pub struct CurrentRevocationAgentIdentityV1 {
    identity: Option<RevocationRecoveredV1<crate::CapabilityAgentIdentity>>,
    acquisition_sequence: Option<u64>,
}
impl CurrentRevocationAgentIdentityV1 {
    pub fn identity(&self) -> Option<&crate::CapabilityAgentIdentity> {
        self.identity.as_ref().map(|v| v.value())
    }
    pub fn acquisition_sequence(&self) -> Option<u64> {
        self.acquisition_sequence
    }
    pub fn manufactured_generation(&self, acquisition_sequence: u64) -> Option<u64> {
        resolve_manufactured_identity_generation_v1(
            self.identity(),
            self.acquisition_sequence,
            acquisition_sequence,
        )
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_revocation_agent_identity_v1<M, R, E, C, P, V, ER, CR>(
    agent: &str,
    qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentRevocationAgentIdentityV1>, WorldFieldOverlayErrorV1>
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
            || qualification.allocation_class() != MemoryClass::Ordinary
            || !qualification.uses_budget(overlay.values.memory)
            || !tx.uses_budget(overlay.values.memory)
            || !read.values.memory.same_pool(overlay.values.memory)
            || qualification.reserved_bytes() < 512
        {
            return Err(invalid().into());
        }
        let fee = overlay
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentRevocationAgentIdentityV1>() + 8192,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        qualification.validate_digest(&tx.parent_fence().parent_roots)?;
        read.check()?;
        if (overlay.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (overlay.take_state_error)() {
            return Err(error.into());
        }
        let body = {
            let mut records = InventoryValueReader {
                values: &mut overlay.values,
                key_work: &mut *overlay.key_work,
                semantic_bytes: overlay.semantic_bytes,
                record_bytes: overlay.record_bytes,
                cancelled: &mut overlay.cancelled,
            };
            let field = family::outer();
            let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
                .map_err(ValueTreeError::Invalid)?;
            let metadata_root = lookup(tx, &metadata, &mut records, &mut overlay.take_state_error)?
                .ok_or_else(invalid)?;
            records.presence(&metadata_root, &metadata, 0)?;
            let (key, _) = records.key(field, WorldFieldRecordKindV1::Entry, &())?;
            let value = lookup(tx, &key, &mut records, &mut overlay.take_state_error)?
                .ok_or_else(invalid)?;
            family::child_records(&key, &value, &mut records)?
        };
        let identity_root = map_entry(&body, "agent_identities", agent, read, scan, work)?;
        let sequence = map_entry(
            &body,
            "manufactured_identity_acquisition_sequences",
            agent,
            read,
            scan,
            work,
        )?;
        let acquisition_sequence = match sequence.as_ref().map(|row| &row.cell) {
            None => None,
            Some(ColumnCellV1::U64(value)) => Some(*value),
            _ => return Err(invalid().into()),
        };
        let identity = match identity_root.as_ref().map(|row| &row.cell) {
            None => None,
            Some(ColumnCellV1::Root(root)) => Some(
                crate::incremental_typed_record_codec::recover_typed_record_body_v1(
                    root,
                    &profile(),
                    read,
                    scan,
                    work,
                )?,
            ),
            _ => return Err(invalid().into()),
        };
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
        Ok(fee.retain(CurrentRevocationAgentIdentityV1 {
            identity,
            acquisition_sequence,
        }))
    })
}
fn map_entry<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    body: &StateRootV1,
    name: &str,
    agent: &str,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RevocationReadWorkV1,
) -> Result<
    Option<ReservedValue<crate::incremental_record_columns_read::ColumnRowV1>>,
    ValueTreeError,
> {
    let profile = profile();
    let trees_remaining = work.max_retained_trees;
    let map = {
        let mut d = ChangeDecoder {
            ctx,
            scan,
            work,
            profile,
            trees_remaining,
            tree_depth: 0,
        };
        let map = point_typed_column_v1(&profile, body, name, &mut d, 8192)?;
        let ColumnCellV1::Root(root) = &map.cell else {
            return Err(invalid());
        };
        let length = point_typed_column_v1(&profile, root, "_length", &mut d, 8192)?;
        let ColumnCellV1::U64(count) = &length.cell else {
            return Err(invalid());
        };
        if count.checked_add(2) != Some(root.leaf_count) {
            return Err(invalid());
        };
        map
    };
    let ColumnCellV1::Root(root) = &map.cell else {
        return Err(invalid());
    };
    work.json.cells_remaining =
        work.json
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "revocation selected map cell",
            ))?;
    let mut records = ctx.records();
    let metadata = WorldFieldKeyV1::new(profile.field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let metadata_root =
        crate::incremental_record_tree::lookup(root, &metadata, &mut records, profile.labels)?
            .ok_or_else(invalid)?;
    records.presence(&metadata_root, &metadata, 0)?;
    let (key, _) = records.key(
        profile.field,
        WorldFieldRecordKindV1::Entry,
        &("entry", agent),
    )?;
    let Some(value) =
        crate::incremental_record_tree::lookup(root, &key, &mut records, profile.labels)?
    else {
        return Ok(None);
    };
    let entry = oasis7_formal_protocol::incremental_state::StateEntryV1 {
        namespace: profile.namespace()?,
        key: key.as_bytes().to_vec(),
        value,
    };
    crate::incremental_record_columns_read::read_column_row_v1(
        ColumnReadProfileV1 {
            field: profile.field,
            schema: profile.schema,
            groups: &[],
            reference_schema: profile.reference_schema,
        },
        &entry,
        &mut records,
        8192,
    )
    .map(Some)
}

/// Pure original checkpoint arithmetic; callers supply actual owner-read facts.
/// This creates no identity or authority capability.
pub fn resolve_manufactured_identity_generation_v1(
    identity: Option<&crate::CapabilityAgentIdentity>,
    checkpoint_sequence: Option<u64>,
    acquisition_sequence: u64,
) -> Option<u64> {
    if acquisition_sequence == 0 {
        return None;
    }
    match checkpoint_sequence {
        None => Some(acquisition_sequence),
        Some(checkpoint_sequence) => {
            let checkpoint = identity?;
            let delta = acquisition_sequence.checked_sub(checkpoint_sequence)?;
            checkpoint
                .generation
                .checked_add(delta)
                .filter(|generation| *generation > 0)
        }
    }
}
