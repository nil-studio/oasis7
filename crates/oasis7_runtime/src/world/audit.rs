use std::path::Path;

use super::super::util::write_json_to_path;
use super::super::{AuditFilter, WorldError, WorldEvent};
use super::World;

impl World {
    // ---------------------------------------------------------------------
    // Audit
    // ---------------------------------------------------------------------

    pub fn audit_events(&self, filter: &AuditFilter) -> Vec<WorldEvent> {
        self.journal
            .events
            .iter()
            .filter(|event| filter.matches(event))
            .cloned()
            .collect()
    }

    pub fn save_audit_log(
        &self,
        path: impl AsRef<Path>,
        filter: &AuditFilter,
    ) -> Result<(), WorldError> {
        let events = self.audit_events(filter);
        write_json_to_path(&events, path.as_ref())
    }
}
