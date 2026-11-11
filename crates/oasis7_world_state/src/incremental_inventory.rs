//! Complete structural inventory ownership, separate from business authority.
use crate::{
    incremental_field_member::*,
    incremental_field_presence::{PresenceRecordLimitsV1, read_field_presence_v1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_genesis::descriptor,
    incremental_semantic_key::{
        CanonicalSemanticKeyRefV1, SEMANTIC_KEY_DOMAIN, SemanticKeyRefV1, read_semantic_key_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_state::{
        MerkleNodeReader, StateNamespaceV1, StateScanBudget, StateScanContext, StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[path = "incremental_inventory_index.rs"]
mod index;
use index::{InventoryGroupMetaV1, InventoryIndexV1, InventoryIndexWorkV1};
#[path = "incremental_inventory_family.rs"]
mod family;
#[path = "incremental_inventory_receiver.rs"]
mod receiver;
pub use family::{qualify_inventory_family_v1, readmit_inventory_family_ordinary_v1};
pub use receiver::accept_inventory_transition_v1;
#[path = "incremental_inventory_agent_transfer.rs"]
mod agent_transfer;
#[path = "incremental_factory_lifecycle_ledger.rs"]
mod factory_output_group;
#[path = "incremental_inventory_group_overlay.rs"]
mod group_overlay;
pub use factory_output_group::{
    FactoryOutputLedgerGroupWorkV1, ensure_current_factory_output_ledger_group_v1,
};
#[path = "incremental_inventory_material_scope.rs"]
mod material_scope;
pub use agent_transfer::{
    AgentLedgerTransferOutcomeV1, AgentLedgerTransferWorkV1,
    transfer_current_agent_material_overlay_v1,
};
pub use group_overlay::{InventoryWorldGroupWorkV1, ensure_world_ledger_group_overlay_v1};
pub use material_scope::{
    CurrentInventoryMaterialScopeV1, FrozenInventoryMaterialScopeV1, InventoryMaterialScopeWorkV1,
    MaterialQuantityV1, ParentInventoryMaterialScopeV1, build_parent_inventory_material_scope_v1,
    freeze_current_inventory_material_scope_v1, open_current_inventory_material_scope_v1,
};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("complete inventory family")
}
pub struct InventoryFamilyWorkV1 {
    pub records_remaining: u64,
    pub index_steps_remaining: u64,
    pub max_semantic_bytes: usize,
}
impl InventoryFamilyWorkV1 {
    fn step(&mut self, c: &mut impl FnMut() -> bool) -> Result<(), ValueTreeError> {
        if c() {
            return Err(ValueTreeError::Cancelled);
        }
        self.records_remaining = self
            .records_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("inventory records"))?;
        Ok(())
    }
}
pub struct InventoryFamilyQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    counts: [u64; 4],
    groups: InventoryIndexV1,
}
impl InventoryFamilyQualificationV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn record_counts(&self) -> [u64; 4] {
        self.counts
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
    fn check(&self, memory: &MemoryBudget, class: MemoryClass) -> Result<(), ValueTreeError> {
        if self.class != class || self.groups.class() != class || !self.groups.uses_budget(memory) {
            return Err(invalid());
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Row {
    Meta(usize),
    Entry(usize),
    Group([u8; 32]),
    Member([u8; 32]),
}
fn key(raw: &[u8]) -> Result<Option<WorldFieldKeyV1>, ValueTreeError> {
    if !raw.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let p = WorldFieldKeyV1::from_bytes(raw).map_err(ValueTreeError::Invalid)?;
    if p.owner != FieldOwner::WorldState {
        return Ok(None);
    }
    let shape = match p.name {
        "resources" | "materials" => FieldShape::Map,
        "material_ledgers" => FieldShape::NestedMap,
        _ => return Ok(None),
    };
    let field = descriptor(p.name, shape).map_err(|_| invalid())?;
    let key =
        WorldFieldKeyV1::new(field, p.kind, p.semantic_key).map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != raw {
        return Err(invalid());
    }
    Ok(Some(key))
}
fn row<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut InventoryFamilyWorkV1,
) -> Result<Row, ValueTreeError> {
    ctx.check()?;
    work.step(&mut ctx.cancelled)?;
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let field = match parsed.name {
        "resources" => 0,
        "materials" => 1,
        "material_ledgers" => 2,
        _ => return Err(invalid()),
    };
    ctx.values
        .verify_closure(value, ctx.record_bytes, &mut ctx.cancelled)?;
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        ctx.records().presence(value, key, 0)?;
        ctx.check()?;
        return Ok(Row::Meta(field));
    }
    let max = work
        .max_semantic_bytes
        .min(usize::try_from(ctx.semantic_bytes).unwrap_or(usize::MAX));
    let _charge = ctx
        .values
        .memory
        .reserve(max.checked_add(128).ok_or_else(invalid)?, ctx.values.class)?;
    let mut semantic = Vec::with_capacity(max);
    let n = if parsed.kind == WorldFieldRecordKindV1::Group {
        read_field_presence_v1(
            value,
            key,
            &mut ctx.values,
            PresenceRecordLimitsV1 {
                semantic_bytes: max as u64,
                record_bytes: ctx.record_bytes,
            },
            &mut semantic,
            &mut ctx.cancelled,
        )?
        .semantic_bytes
    } else if parsed.kind == WorldFieldRecordKindV1::Entry {
        let info = read_member_record_v1(
            value,
            key,
            &mut ctx.values,
            MemberRecordLimitsV1 {
                semantic_bytes: max as u64,
                payload_bytes: 8,
                record_bytes: ctx.record_bytes,
            },
            &mut semantic,
            &mut std::io::sink(),
            &mut ctx.cancelled,
        )?;
        if info.codec != MemberPayloadCodecV1::I64 || info.payload_bytes != 8 {
            return Err(invalid());
        }
        info.semantic_bytes
    } else {
        return Err(invalid());
    };
    if n as usize != semantic.len() || semantic.capacity() > max {
        return Err(invalid());
    }
    let decoded = read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?;
    let result = match (field, parsed.kind, decoded) {
        (0, WorldFieldRecordKindV1::Entry, SemanticKeyRefV1::Resource(_)) => Row::Entry(0),
        (1, WorldFieldRecordKindV1::Entry, SemanticKeyRefV1::Text(_)) => Row::Entry(1),
        (2, WorldFieldRecordKindV1::Group, SemanticKeyRefV1::Ledger(_)) => {
            Row::Group(parsed.semantic_key.ok_or_else(invalid)?)
        }
        (2, WorldFieldRecordKindV1::Entry, SemanticKeyRefV1::Pair { left, right }) => {
            if !matches!(
                left.decode().map_err(ValueTreeError::Invalid)?,
                SemanticKeyRefV1::Ledger(_)
            ) {
                return Err(invalid());
            }
            let SemanticKeyRefV1::Text(material) =
                right.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Err(invalid());
            };
            let end = semantic
                .len()
                .checked_sub(9 + material.len())
                .ok_or_else(invalid)?;
            let domain = SEMANTIC_KEY_DOMAIN.len();
            semantic.copy_within(domain + 1..end, domain);
            semantic.truncate(end - 1);
            let canonical =
                CanonicalSemanticKeyRefV1::new(&semantic).map_err(ValueTreeError::Invalid)?;
            let (group, _) = ctx.key(
                descriptor("material_ledgers", FieldShape::NestedMap).map_err(|_| invalid())?,
                WorldFieldRecordKindV1::Group,
                &canonical,
            )?;
            let p =
                WorldFieldKeyV1::from_bytes(group.as_bytes()).map_err(ValueTreeError::Invalid)?;
            Row::Member(p.semantic_key.ok_or_else(invalid)?)
        }
        _ => return Err(invalid()),
    };
    ctx.check()?;
    Ok(result)
}
fn adjust(
    index: &mut InventoryIndexV1,
    row: Row,
    add: bool,
    counts: &mut [u64; 4],
    work: &mut InventoryIndexWorkV1,
    c: &mut impl FnMut() -> bool,
) -> Result<Option<[u8; 32]>, ValueTreeError> {
    let slot = match row {
        Row::Meta(_) => return Ok(None),
        Row::Entry(i) => i,
        Row::Group(_) => 2,
        Row::Member(_) => 3,
    };
    counts[slot] = if add {
        counts[slot].checked_add(1)
    } else {
        counts[slot].checked_sub(1)
    }
    .ok_or_else(invalid)?;
    let key = match row {
        Row::Group(k) | Row::Member(k) => k,
        _ => return Ok(None),
    };
    let mut meta = index.get(key, work, c)?.unwrap_or(InventoryGroupMetaV1 {
        key,
        present: false,
        members: 0,
    });
    match row {
        Row::Group(_) => {
            if meta.present == add {
                return Err(invalid());
            }
            meta.present = add;
        }
        Row::Member(_) => {
            meta.members = if add {
                meta.members.checked_add(1)
            } else {
                meta.members.checked_sub(1)
            }
            .ok_or_else(invalid)?
        }
        _ => unreachable!(),
    }
    *index = index.insert(meta, work, c)?;
    Ok(Some(key))
}
#[cfg(test)]
#[path = "incremental_inventory_family_tests.rs"]
mod tests;

pub(crate) use receiver::accept_inventory_verified_v1;
