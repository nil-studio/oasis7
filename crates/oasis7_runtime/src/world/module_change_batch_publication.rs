//! Atomic publication for the standalone multi-`ModuleEvent` compatibility API.
use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    ModuleChangeSet, ModuleRegistry, ProposalId, TickConsensusRecord, WorldError, WorldEvent,
    WorldEventId,
};
use super::World;

#[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
struct PreparedModuleChangeBatch {
    module_registry: ModuleRegistry,
    module_artifacts: BTreeSet<String>,
    module_tick_schedule: BTreeMap<String, u64>,
    cache_invalidations: BTreeSet<String>,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
    consensus_record: TickConsensusRecord,
}

#[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
impl PreparedModuleChangeBatch {
    fn install(self, world: &mut World) -> Result<(), WorldError> {
        world.install_prepared_tick_consensus_record(self.consensus_record)?;
        world.module_registry = self.module_registry;
        world.module_artifacts = self.module_artifacts;
        world.module_tick_schedule = self.module_tick_schedule;
        world.next_event_id = self.next_event_id;
        world.next_event_id_era = self.next_event_id_era;
        world.journal.events = self.journal_events;
        world.runtime_backpressure_stats.journal_events_evicted = world
            .runtime_backpressure_stats
            .journal_events_evicted
            .saturating_add(self.journal_events_evicted);

        for record_key in self.cache_invalidations {
            let prefix = format!("{record_key}|");
            world
                .prepared_subscription_cache
                .retain(|key, _| !key.starts_with(prefix.as_str()));
        }
        Ok(())
    }
}

impl World {
    #[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
    pub(super) fn apply_prepared_module_change_batch(
        &mut self,
        proposal_id: ProposalId,
        changes: &ModuleChangeSet,
        actor: &str,
    ) -> Result<(), WorldError> {
        let mut module_registry = self.module_registry.clone();
        let mut module_artifacts = self.module_artifacts.clone();
        let mut module_tick_schedule = self.module_tick_schedule.clone();
        let mut cache_invalidations = BTreeSet::new();
        let mut event_bodies = Vec::new();
        super::governance_publication::project_module_changes(
            self.state.time,
            proposal_id,
            changes,
            actor,
            &mut module_registry,
            &mut module_artifacts,
            &mut module_tick_schedule,
            &mut cache_invalidations,
            &mut event_bodies,
        )?;
        if event_bodies.is_empty() {
            return Ok(());
        }

        let state_root = self.current_state_root_hash()?;
        let mut next_event_id = self.next_event_id;
        let mut next_event_id_era = self.next_event_id_era;
        let mut journal_events = self.journal.events.clone();
        let mut journal_events_evicted = 0u64;
        let mut consensus_record = None;
        for body in event_bodies {
            let (event_id, next_id, next_era) =
                Self::preview_next_event_id(next_event_id, next_event_id_era);
            next_event_id = next_id;
            next_event_id_era = next_era;
            journal_events.push(WorldEvent {
                id: event_id,
                time: self.state.time,
                caused_by: None,
                body,
            });
            let overflow = journal_events
                .len()
                .saturating_sub(self.runtime_memory_limits.max_journal_events.max(1));
            if overflow > 0 {
                journal_events.drain(0..overflow);
            }
            journal_events_evicted = journal_events_evicted.saturating_add(overflow as u64);
            let tick_events: Vec<_> = journal_events
                .iter()
                .filter(|event| event.time == self.state.time)
                .cloned()
                .collect();
            let record = self.build_tick_consensus_record_for_prepared_events(
                self.state.time,
                &tick_events,
                state_root.clone(),
            )?;
            self.validate_tick_consensus_candidate_for_prepared_publication(
                &record,
                &tick_events,
                &state_root,
            )?;
            if self.take_fail_next_append_after_publication_prepare_for_test() {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: "injected append_event failure after publication preparation".into(),
                });
            }
            consensus_record = Some(record);
        }

        PreparedModuleChangeBatch {
            module_registry,
            module_artifacts,
            module_tick_schedule,
            cache_invalidations,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted,
            consensus_record: consensus_record.expect("nonempty batch has consensus"),
        }
        .install(self)?;
        Ok(())
    }
}
