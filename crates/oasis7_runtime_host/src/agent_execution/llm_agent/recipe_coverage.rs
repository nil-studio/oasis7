use super::RECIPE_COMPLETION_REPLAY_WINDOW;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub(super) struct RecipeCoverageProgress {
    pub(super) completed: BTreeSet<String>,
    pub(super) completion_receipt_ids: BTreeSet<u64>,
    /// Receipt IDs in the order their terminal completion was first observed.
    /// This is the age authority for the bounded replay window; ActionIds are
    /// allocated globally and may be sparse or wrap independently of recipe
    /// completions.
    #[serde(default, skip_serializing_if = "VecDeque::is_empty")]
    pub(super) completion_receipt_order: VecDeque<u64>,
}

#[derive(Debug, Deserialize)]
struct RecipeCoverageProgressWire {
    #[serde(default)]
    completed: BTreeSet<String>,
    #[serde(default)]
    completion_receipt_ids: BTreeSet<u64>,
    #[serde(default)]
    completion_receipt_order: VecDeque<u64>,
}

impl<'de> Deserialize<'de> for RecipeCoverageProgress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = RecipeCoverageProgressWire::deserialize(deserializer)?;
        let mut completion_receipt_ids = BTreeSet::new();
        let mut completion_receipt_order = VecDeque::new();

        // The ordered field is authoritative when present. Keep the set and
        // order in lockstep, while accepting IDs from the legacy set-only
        // representation below for backwards-compatible snapshot restore.
        for job_id in wire.completion_receipt_order {
            if completion_receipt_ids.insert(job_id) {
                completion_receipt_order.push_back(job_id);
            }
        }
        for job_id in wire.completion_receipt_ids {
            if completion_receipt_ids.insert(job_id) {
                completion_receipt_order.push_back(job_id);
            }
        }

        while completion_receipt_order.len() > RECIPE_COMPLETION_REPLAY_WINDOW {
            if let Some(job_id) = completion_receipt_order.pop_front() {
                completion_receipt_ids.remove(&job_id);
            }
        }

        Ok(Self {
            completed: wire.completed,
            completion_receipt_ids,
            completion_receipt_order,
        })
    }
}
