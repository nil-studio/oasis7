use super::super::persist_error::PersistError;
use super::super::types::{
    CHUNK_GENERATION_SCHEMA_VERSION, JOURNAL_VERSION, SNAPSHOT_VERSION, WorldEventId,
};
use super::super::{KernelEvent, KernelJournal, KernelSnapshot};
use super::WorldKernel;
use std::collections::VecDeque;

impl WorldKernel {
    fn restore_persisted_state(snapshot: KernelSnapshot, journal_events: Vec<KernelEvent>) -> Self {
        Self {
            time: snapshot.time,
            config: snapshot.config.sanitized(),
            next_event_id: snapshot.next_event_id,
            next_action_id: snapshot.next_action_id,
            pending_actions: VecDeque::from(snapshot.pending_actions),
            journal: journal_events,
            model: snapshot.model,
            chunk_runtime: snapshot.chunk_runtime,
            intel_ttl_ticks: snapshot.intel_ttl_ticks,
            // Runtime-only caches and hooks are intentionally rebuilt per process.
            intel_cache: Default::default(),
            last_intent_batch_report: None,
            rule_hooks: Default::default(),
            source_compiler: Default::default(),
        }
    }

    pub fn kernel_snapshot(&self) -> KernelSnapshot {
        KernelSnapshot {
            version: SNAPSHOT_VERSION,
            chunk_generation_schema_version: CHUNK_GENERATION_SCHEMA_VERSION,
            time: self.time,
            config: self.config.clone(),
            model: self.model.clone(),
            chunk_runtime: self.chunk_runtime.clone(),
            intel_ttl_ticks: self.intel_ttl_ticks,
            next_event_id: self.next_event_id,
            next_action_id: self.next_action_id,
            pending_actions: self.pending_actions.iter().cloned().collect(),
            journal_len: self.journal.len(),
        }
    }

    pub fn kernel_journal_snapshot(&self) -> KernelJournal {
        KernelJournal {
            version: JOURNAL_VERSION,
            events: self.journal.clone(),
        }
    }

    pub fn from_kernel_snapshot(
        snapshot: KernelSnapshot,
        journal: KernelJournal,
    ) -> Result<Self, PersistError> {
        snapshot.validate_version()?;
        journal.validate_version()?;
        if snapshot.journal_len != journal.events.len() {
            return Err(PersistError::SnapshotMismatch {
                expected: snapshot.journal_len,
                actual: journal.events.len(),
            });
        }
        validate_journal_event_prefix(snapshot.next_event_id, &journal.events)?;
        Ok(Self::restore_persisted_state(snapshot, journal.events))
    }

    pub fn replay_from_kernel_snapshot(
        snapshot: KernelSnapshot,
        journal: KernelJournal,
    ) -> Result<Self, PersistError> {
        Self::replay_with_compiler(snapshot, journal, None)
    }

    pub fn replay_from_kernel_snapshot_with_source_compiler(
        snapshot: KernelSnapshot,
        journal: KernelJournal,
        compiler: std::sync::Arc<dyn oasis7_agent_api::module_source::ModuleSourceCompiler>,
    ) -> Result<Self, PersistError> {
        Self::replay_with_compiler(snapshot, journal, Some(compiler))
    }

    fn replay_with_compiler(
        snapshot: KernelSnapshot,
        journal: KernelJournal,
        compiler: Option<std::sync::Arc<dyn oasis7_agent_api::module_source::ModuleSourceCompiler>>,
    ) -> Result<Self, PersistError> {
        snapshot.validate_version()?;
        journal.validate_version()?;
        if journal.events.len() < snapshot.journal_len {
            return Err(PersistError::SnapshotMismatch {
                expected: snapshot.journal_len,
                actual: journal.events.len(),
            });
        }
        if !snapshot.pending_actions.is_empty() && journal.events.len() > snapshot.journal_len {
            return Err(PersistError::ReplayConflict {
                message: "cannot replay with pending actions in snapshot".to_string(),
            });
        }

        let journal_len = snapshot.journal_len;
        validate_journal_event_prefix(snapshot.next_event_id, &journal.events[..journal_len])?;
        let mut kernel = Self::restore_persisted_state(snapshot, journal.events.clone());
        if let Some(compiler) = compiler {
            kernel.set_module_source_compiler(compiler);
        }

        for event in journal.events.iter().skip(journal_len) {
            kernel.apply_event(event)?;
        }
        let events_after_snapshot = journal.events.len() - journal_len;
        if events_after_snapshot > 0 {
            kernel.next_action_id = kernel
                .next_action_id
                .saturating_add(events_after_snapshot as u64);
        }

        Ok(kernel)
    }
}

fn validate_journal_event_prefix(
    next_event_id: WorldEventId,
    events: &[KernelEvent],
) -> Result<(), PersistError> {
    if next_event_id as usize != events.len() {
        return Err(PersistError::ReplayConflict {
            message: format!(
                "journal next_event_id mismatch: expected {}, got {}",
                events.len(),
                next_event_id
            ),
        });
    }
    for (index, event) in events.iter().enumerate() {
        if event.id != index as WorldEventId {
            return Err(PersistError::ReplayConflict {
                message: format!(
                    "journal event id mismatch: expected {}, got {}",
                    index, event.id
                ),
            });
        }
    }
    Ok(())
}
