//! Complete structural listing/bid family, separate from market authority.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_owner_closure::{WorldStateOwnerError, marketplace_bridge as bridge},
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1, TransitionVerifier},
    incremental_state::{StateEntryV1, StateScanBudget, StateScanContext, StateScanError},
    incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::MemoryBudget;
use std::cell::RefCell;
#[path = "incremental_module_marketplace_family_cold.rs"]
mod cold;
#[path = "incremental_module_marketplace_family_receiver.rs"]
mod receiver;
pub use cold::{
    qualify_module_marketplace_family_v1, readmit_module_marketplace_family_ordinary_v1,
};
pub use receiver::accept_module_marketplace_family_transition_v1;
fn bad() -> WorldStateOwnerError {
    WorldStateOwnerError::Invalid("marketplace2 complete family")
}
pub struct ModuleMarketplaceFamilyWorkV1 {
    pub outer_rows_remaining: u64,
    pub changed_cells_remaining: u64,
    pub membership_cells_remaining: u64,
    pub max_record_bytes: u64,
}
impl ModuleMarketplaceFamilyWorkV1 {
    fn step(&mut self, c: &mut impl FnMut() -> bool) -> Result<(), WorldStateOwnerError> {
        if c() {
            return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
        }
        self.outer_rows_remaining = self.outer_rows_remaining.checked_sub(1).ok_or(
            WorldStateOwnerError::ResourceLimited("marketplace outer rows"),
        )?;
        Ok(())
    }
}
pub struct ModuleMarketplaceFamilyQualificationV1 {
    roots: [u8; 32],
    class: MemoryClass,
    counts: [u64; 3],
}
impl ModuleMarketplaceFamilyQualificationV1 {
    pub fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    pub fn record_counts(&self) -> [u64; 3] {
        self.counts
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), WorldStateOwnerError> {
        if self.roots != roots.digest().map_err(WorldStateOwnerError::RootSet)? {
            return Err(bad());
        }
        Ok(())
    }
}
pub(super) fn identify(
    raw: &[u8],
) -> Result<Option<(usize, FieldDescriptor, WorldFieldKeyV1)>, WorldStateOwnerError> {
    if !raw.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let p = WorldFieldKeyV1::from_bytes(raw).map_err(WorldStateOwnerError::Invalid)?;
    if p.owner != FieldOwner::WorldState {
        return Ok(None);
    }
    let (i, field) = match p.name {
        "module_visual_entities" => (0, ModuleMarketplaceFieldV1::VisualEntities),
        "module_artifact_listings" => (1, ModuleMarketplaceFieldV1::ArtifactListings),
        "module_artifact_bids" => (2, ModuleMarketplaceFieldV1::ArtifactBids),
        _ => return Ok(None),
    };
    let field = field.descriptor().map_err(|_| bad())?;
    let key = WorldFieldKeyV1::new(field, p.kind, p.semantic_key)
        .map_err(WorldStateOwnerError::Invalid)?;
    if key.as_bytes() != raw
        || !matches!(
            p.kind,
            WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Entry
        )
    {
        return Err(bad());
    }
    Ok(Some((i, field, key)))
}
fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), WorldStateOwnerError> {
    ctx.check().map_err(WorldStateOwnerError::Value)
}
fn map_scan<T>(
    r: Result<T, StateScanError<WorldStateOwnerError>>,
) -> Result<T, WorldStateOwnerError> {
    r.map_err(|e| match e {
        StateScanError::Visitor(e) => e,
        StateScanError::Memory(e) => WorldStateOwnerError::Memory(e),
        StateScanError::Tree(e) => WorldStateOwnerError::Scan(e),
        StateScanError::Cancelled => WorldStateOwnerError::Value(ValueTreeError::Cancelled),
        StateScanError::ResourceLimited => {
            WorldStateOwnerError::ResourceLimited("marketplace family scan")
        }
        StateScanError::Invalid(e) => WorldStateOwnerError::Invalid(e),
    })
}

#[cfg(test)]
#[path = "incremental_module_marketplace_family_tests.rs"]
mod tests;

pub(crate) use receiver::accept_module_marketplace_family_verified_v1;
