//! GEN/cold-only directory IO is allocated once with the Host work. The other
//! bootstrap lanes borrow the same counters later used by actions and tail.
use super::*;
use crate::world::{
    incremental_normal_host_resident::{NormalHostBootstrapTailWorkV1, NormalHostStartupWorkV1},
    incremental_normal_parent_caches::NormalParentCacheBootstrapWorkV1,
};

pub(in crate::world::incremental_normal_host_work) struct StartupDirectoryWork {
    factory_read: NormalHostReadWorkV1,
    recipe_read: NormalHostReadWorkV1,
    material_scan: StateScanBudget,
    cache_scan: StateScanBudget,
    factory_scan: StateScanBudget,
    recipe_scan: StateScanBudget,
    outer_scan: StateScanBudget,
    inner_scan: StateScanBudget,
    pristine_entries: usize,
    max_payload_bytes: u64,
}

impl FromLimits for StartupDirectoryWork {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self {
        Self {
            factory_read: NormalHostReadWorkV1::from_limits(limits, core),
            recipe_read: NormalHostReadWorkV1::from_limits(limits, core),
            material_scan: StateScanBudget::from_limits(limits, core),
            cache_scan: StateScanBudget::from_limits(limits, core),
            factory_scan: StateScanBudget::from_limits(limits, core),
            recipe_scan: StateScanBudget::from_limits(limits, core),
            outer_scan: StateScanBudget::from_limits(limits, core),
            inner_scan: StateScanBudget::from_limits(limits, core),
            pristine_entries: limits.entries_per_counter,
            max_payload_bytes: limits.max_payload_bytes as u64,
        }
    }
}

impl NormalHostWorkV1 {
    /// Disjoint, temporary loans from the actual continuing work owner. Once
    /// bootstrap returns, the fixed Root can borrow the complete owner again.
    pub(in crate::world) fn startup_loans(
        &mut self,
    ) -> (&mut NormalHostIoWorkV1, NormalHostStartupWorkV1<'_>) {
        let tail = NormalHostBootstrapTailWorkV1 {
            factory_read: &mut self.startup.factory_read,
            recipe_read: &mut self.startup.recipe_read,
            factory_scan: &mut self.startup.factory_scan,
            recipe_scan: &mut self.startup.recipe_scan,
            outer_scan: &mut self.startup.outer_scan,
            inner_scan: &mut self.startup.inner_scan,
            recipes: &mut self.driver.tail.recipes,
            native_read: &mut self.driver.tail.gameplay.native,
            claims: &mut self.driver.tail.world_delivery.claims,
            gameplay: &mut self.driver.tail.gameplay.directory,
            grant_cells: &mut self.driver.tail.world_delivery.fixed_cells_remaining,
            threats: &mut self.driver.tail.threat_keys,
            max_grant_payload_bytes: self.startup.max_payload_bytes,
        };
        let caches = NormalParentCacheBootstrapWorkV1 {
            scan: &mut self.startup.cache_scan,
            factory: &mut self.driver.factory.factory,
            industry: &mut self.driver.factory.recipe,
            inventory: &mut self.driver.factory.material_family,
            builds: &mut self.driver.actions.factory_schedule.builds,
            receipts: &mut self.driver.actions.factory_schedule.receipts,
            identity: &mut self.driver.actions.factory_schedule.inventory_directory,
            invocations: &mut self.driver.enumeration,
            order_read: &mut self.driver.factory.typed,
            orders: &mut self.driver.factory.orders,
            pristine_entries: &mut self.startup.pristine_entries,
            max_payload_bytes: self.startup.max_payload_bytes,
        };
        let startup = NormalHostStartupWorkV1 {
            manifest: &mut self.invocation.manifest,
            material_scan: &mut self.startup.material_scan,
            inventory: &mut self.materials.family,
            material: &mut self.materials.scope,
            caches,
            tail,
            delivery: &mut self.driver.product,
            authorization: &mut self.driver.final_sources.authorization,
        };
        (&mut self.io, startup)
    }
}
