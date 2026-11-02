//! Versioned, root-covered intrinsic GuardedStop history index.
pub use oasis7_agent_api::game::GameTaskStopRecordV2 as TaskStopRecordV2;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStopIndexV2 {
    pub schema_revision: u32,
    pub records: BTreeMap<String, TaskStopRecordV2>,
}
impl TaskStopIndexV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_revision != 2 || self.records.is_empty() {
            return Err("invalid guarded Stop index shape".into());
        }
        for (key, r) in &self.records {
            r.validate()?;
            if key != &r.operation_key()? {
                return Err("guarded Stop index key mismatch".into());
            }
        }
        Ok(())
    }
}
