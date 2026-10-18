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

impl WorldState {
    /// Deterministically allocate recipe input authority from completed paths.
    ///
    /// Path IDs are sorted before allocation so caller order cannot change the
    /// selected settled quantity.  The returned map contains only the amount
    /// to consume from each path; callers must apply it atomically with the
    /// recipe input/power sink.
    pub(crate) fn allocate_recipe_path_amounts(
        &self,
        consume_ledger: &MaterialLedgerId,
        route_ids: &[String],
        path_ids: &[String],
        consume: &[MaterialStack],
    ) -> Result<BTreeMap<String, i64>, String> {
        if path_ids.is_empty() {
            return Ok(BTreeMap::new());
        }

        let mut required_by_kind = BTreeMap::<String, i64>::new();
        for stack in consume {
            let kind = stack.kind.trim().to_ascii_lowercase();
            if kind.is_empty() {
                continue;
            }
            let required = required_by_kind.entry(kind).or_default();
            *required = required.checked_add(stack.amount).ok_or_else(|| {
                format!(
                    "recipe path authority requirement overflow: kind={} amount={}",
                    stack.kind, stack.amount
                )
            })?;
        }

        let mut sorted_path_ids = path_ids.to_vec();
        sorted_path_ids.sort();
        let mut seen_path_ids = BTreeSet::new();
        let mut path_route_ids = BTreeSet::new();
        let mut paths_by_kind = BTreeMap::<String, Vec<(String, i64)>>::new();
        for path_id in sorted_path_ids {
            if !seen_path_ids.insert(path_id.clone()) {
                return Err(format!(
                    "duplicate logistics path binding: path_id={path_id}"
                ));
            }
            let Some(path) = self.completed_logistics_paths.get(&path_id) else {
                return Err(format!(
                    "logistics path is not completed: path_id={path_id}"
                ));
            };
            if path.to_ledger != *consume_ledger {
                return Err(format!(
                    "logistics path destination mismatch: path_id={path_id}"
                ));
            }
            let kind = path.kind.trim().to_ascii_lowercase();
            if kind.is_empty() || !required_by_kind.contains_key(&kind) {
                return Err(format!(
                    "logistics path material mismatch: path_id={path_id}"
                ));
            }
            if path.settled_amount < 0
                || path.remaining_recipe_amount < 0
                || path.remaining_recipe_amount > path.settled_amount
            {
                return Err(format!(
                    "logistics path quantity authority is invalid: path_id={path_id}"
                ));
            }
            if path.remaining_recipe_amount == 0 {
                return Err(format!(
                    "logistics path recipe authority is exhausted: path_id={path_id}"
                ));
            }
            path_route_ids.extend(path.route_ids.iter().cloned());
            paths_by_kind
                .entry(kind)
                .or_default()
                .push((path_id, path.remaining_recipe_amount));
        }

        if !route_ids.is_empty() {
            let submitted_route_ids = route_ids.iter().cloned().collect::<BTreeSet<_>>();
            if submitted_route_ids != path_route_ids {
                return Err(
                    "logistics route binding does not match completed path authority".to_string(),
                );
            }
        }

        let mut allocations = BTreeMap::new();
        for (kind, paths) in paths_by_kind {
            let required = required_by_kind.get(&kind).copied().unwrap_or_default();
            let available = paths.iter().try_fold(0_i64, |total, (_, amount)| {
                total
                    .checked_add(*amount)
                    .ok_or_else(|| format!("logistics path quantity overflow: kind={kind}"))
            })?;
            if available < required {
                return Err(format!(
                    "logistics path quantity insufficient: kind={kind} requested={required} available={available}"
                ));
            }

            let mut remaining = required;
            for (path_id, path_amount) in paths {
                if remaining == 0 {
                    break;
                }
                let allocated = path_amount.min(remaining);
                if allocated > 0 {
                    allocations.insert(path_id, allocated);
                    remaining -= allocated;
                }
            }
        }
        Ok(allocations)
    }
}
