use super::{MaterialLedgerId, MaterialStack, WorldState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Completed path authority used by recipe bindings and replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogisticsPathAuthorityV1 {
    pub path_id: String,
    pub route_ids: Vec<String>,
    pub from_ledger: MaterialLedgerId,
    pub to_ledger: MaterialLedgerId,
    pub kind: String,
    /// Total material quantity settled on this path.  Legacy snapshots that
    /// predate quantity authority decode this as zero and therefore fail
    /// closed when a recipe attempts to bind the path.
    #[serde(default)]
    pub settled_amount: i64,
    /// Quantity from this path that remains available for recipe input
    /// authority.  It is decremented atomically with RecipeStarted.
    #[serde(default)]
    pub remaining_recipe_amount: i64,
}

#[path = "logistics_path_authority_fixed.rs"]
mod fixed;
pub use fixed::allocate_recipe_path_amounts_from_records_v1;

impl WorldState {
    /// Deterministically allocate recipe input authority from completed paths.
    ///
    /// Path IDs are sorted before allocation so caller order cannot change the
    /// selected settled quantity.  The returned map contains only the amount
    /// to consume from each path; callers must apply it atomically with the
    /// recipe input/power sink.
    pub fn allocate_recipe_path_amounts(
        &self,
        consume_ledger: &MaterialLedgerId,
        route_ids: &[String],
        path_ids: &[String],
        consume: &[MaterialStack],
    ) -> Result<BTreeMap<String, i64>, String> {
        allocate_recipe_path_amounts_from_records_v1(
            &self.completed_logistics_paths,
            consume_ledger,
            route_ids,
            path_ids,
            consume,
            &mut |_| Ok(()),
        )
    }
}
