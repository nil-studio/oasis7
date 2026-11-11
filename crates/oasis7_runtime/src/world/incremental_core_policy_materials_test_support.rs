//! Test-only genuine parent discovery reused across all concrete event callers.
//! Limits are allocation/work declarations; only the real qualifier mints data.
use super::CorePolicyMaterialContextV1;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    MaterialLedgerId, WorldState,
    incremental_field_member::MemberRecordLimitsV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory::{
        InventoryFamilyWorkV1, InventoryMaterialScopeWorkV1, InventoryWorldGroupWorkV1,
        ParentInventoryMaterialScopeV1, build_parent_inventory_material_scope_v1,
        qualify_inventory_family_v1,
    },
    incremental_semantic_key::SemanticKeyBudget,
};
#[derive(Clone, Copy)]
pub(in crate::world) struct MaterialFixtureLimitsV1 {
    max_entries: usize,
    max_key_bytes: usize,
}
impl MaterialFixtureLimitsV1 {
    /// This observes only initial fixture sizes, never supplies source values.
    pub(in crate::world) fn for_state(state: &WorldState) -> Self {
        let world = state.material_ledgers.get(&MaterialLedgerId::World);
        Self {
            max_entries: state
                .materials
                .len()
                .max(world.map_or(0, |map| map.len()))
                .max(1),
            max_key_bytes: state
                .materials
                .keys()
                .chain(world.into_iter().flat_map(|map| map.keys()))
                .map(String::len)
                .max()
                .unwrap_or(1)
                .max(1),
        }
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 1 << 30,
    }
}
fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 1 << 30,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 1 << 30,
        leaves_remaining: 1_000_000,
    }
}
pub(in crate::world) struct MaterialFixtureV1 {
    parent: ReservedValue<ParentInventoryMaterialScopeV1>,
    limits: MaterialFixtureLimitsV1,
    read_io: ValueIoBudget,
    read_keys: SemanticKeyBudget,
    write_keys: SemanticKeyBudget,
    family: InventoryFamilyWorkV1,
    scope: InventoryMaterialScopeWorkV1,
    group: InventoryWorldGroupWorkV1,
    normalization_cells: u64,
    normalization_comparisons: u64,
}
impl MaterialFixtureV1 {
    /// Call once at the real GEN/cold boundary before begin_counted. Both
    /// qualifiers read actual authenticated records. Missing metadata fails;
    /// no empty scope or synthetic group is substituted for an incomplete GEN.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn build<
        M: MerkleNodeReader,
        V: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        roots: &RootSetV1,
        state: &M,
        values: &V,
        memory: &MemoryBudget,
        limits: MaterialFixtureLimitsV1,
        take_error: E,
        cancelled: C,
    ) -> Self {
        let mut read_io = io();
        let mut read_keys = keys();
        let mut family = InventoryFamilyWorkV1 {
            records_remaining: 1_000_000,
            index_steps_remaining: 1_000_000,
            max_semantic_bytes: 4096,
        };
        let mut scope = InventoryMaterialScopeWorkV1 {
            max_entries_per_map: limits.max_entries,
            max_material_bytes: limits.max_key_bytes,
            pending_entries_remaining: 1_000_000,
            comparison_bytes_remaining: 64 << 20,
        };
        let parent = {
            let mut read = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Business),
                state,
                values: ValueReadContext {
                    source: values,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut read_io,
                },
                key_work: &mut read_keys,
                semantic_bytes: 4096,
                record_bytes: 16 << 20,
                take_state_error: take_error,
                cancelled,
            };
            let qualification =
                qualify_inventory_family_v1(roots, &mut read, &mut scan(), &mut family).unwrap();
            build_parent_inventory_material_scope_v1(
                &qualification,
                roots,
                &mut read,
                &mut scan(),
                &mut family,
                &mut scope,
            )
            .unwrap()
        };
        Self {
            parent,
            limits,
            read_io,
            read_keys,
            write_keys: keys(),
            family,
            scope,
            group: InventoryWorldGroupWorkV1 {
                steps_remaining: 1_000_000,
            },
            normalization_cells: 1_000_000,
            normalization_comparisons: 64 << 20,
        }
    }
    /// Borrow the same parent and continuous counters for all events. Actual
    /// Current opening occurs only in the concrete production context method.
    pub(in crate::world) fn context<'a, M, E>(
        &'a mut self,
        roots: &'a RootSetV1,
        pages: &'a M,
        take_state_error: E,
    ) -> CorePolicyMaterialContextV1<'a, M, E> {
        CorePolicyMaterialContextV1 {
            parent: &self.parent,
            roots,
            pages,
            read_io: &mut self.read_io,
            read_keys: &mut self.read_keys,
            write_keys: &mut self.write_keys,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            write_limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 8 << 20,
                record_bytes: 16 << 20,
            },
            take_state_error,
            family_work: &mut self.family,
            scope_work: &mut self.scope,
            group_work: &mut self.group,
            normalization_max_union_entries: self.limits.max_entries.checked_mul(2).unwrap(),
            normalization_cells_remaining: &mut self.normalization_cells,
            normalization_comparison_bytes_remaining: &mut self.normalization_comparisons,
        }
    }
}
