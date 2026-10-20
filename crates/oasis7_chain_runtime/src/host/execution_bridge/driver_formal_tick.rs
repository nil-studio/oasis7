//! Formal batches open the successor before administrative publications and
//! close its final TC after all result events. Legacy time semantics are intact.
use super::*;
impl NodeRuntimeExecutionDriver {
    pub(super) fn begin_formal_tick(
        &mut self,
        context: &RuntimeCommittedTickContext,
        resuming: bool,
    ) -> Result<(), String> {
        if !self.formal_retention_enabled {
            return Ok(());
        }
        if resuming {
            return Err("formal execution resumes only original certified Frozen input, never a local partial Tick".into());
        }
        self.execution_world
            .begin_formal_execution_tick(context)
            .map_err(|error| format!("formal successor begin: {error:?}"))
    }
    pub(super) fn finish_formal_tick(
        &mut self,
        context: &RuntimeCommittedTickContext,
        admin_only: bool,
    ) -> Result<(), String> {
        if !self.formal_retention_enabled {
            return Ok(());
        }
        let result = if admin_only {
            self.execution_world
                .finish_formal_administrative_tick(context)
        } else {
            self.execution_world.finish_formal_execution_tick(context)
        };
        result.map_err(|error| format!("formal successor completion: {error:?}"))
    }
}
