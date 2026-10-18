//! List/bid and their immediate matching sale share one publication boundary.
use super::super::state::module_marketplace_transition::PreparedModuleMarketplace;
use super::super::{CausedBy, DomainEvent, WorldError, WorldEvent, WorldEventBody};
use super::World;

impl World {
    pub(super) fn append_module_marketplace_order(
        &mut self,
        order: DomainEvent,
        caused_by: Option<CausedBy>,
    ) -> Result<(), WorldError> {
        assert!(matches!(
            order,
            DomainEvent::ModuleArtifactListed { .. } | DomainEvent::ModuleArtifactBidPlaced { .. }
        ));
        let mut state = PreparedModuleMarketplace::new(&self.state, &order);
        let mut next_event_id = self.next_event_id;
        let mut next_event_id_era = self.next_event_id_era;
        let mut journal = self.journal.events.clone();
        let mut evicted = 0u64;
        let mut pending = Some(order);
        let mut first = true;
        let mut final_record = None;
        while let Some(event) = pending.take() {
            state.apply_event(&self.state, &event, self.state.time)?;
            let (id, next_id, next_era) =
                Self::preview_next_event_id(next_event_id, next_event_id_era);
            next_event_id = next_id;
            next_event_id_era = next_era;
            journal.push(WorldEvent {
                id,
                time: self.state.time,
                caused_by: caused_by.clone(),
                body: WorldEventBody::Domain(event),
            });
            let overflow = journal
                .len()
                .saturating_sub(self.runtime_memory_limits.max_journal_events.max(1));
            if overflow > 0 {
                journal.drain(0..overflow);
            }
            evicted = evicted.saturating_add(overflow as u64);
            let tick_events: Vec<_> = journal
                .iter()
                .filter(|event| event.time == self.state.time)
                .cloned()
                .collect();
            let root = self.state_root_hash_with_module_marketplace_overlay(&state)?;
            let record = self.build_tick_consensus_record_for_prepared_events(
                self.state.time,
                &tick_events,
                root.clone(),
            )?;
            self.validate_tick_consensus_candidate_for_prepared_publication(
                &record,
                &tick_events,
                &root,
            )?;
            if self.take_fail_next_append_after_publication_prepare_for_test() {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: "injected append_event failure after publication preparation".into(),
                });
            }
            final_record = Some(record);
            if first {
                pending = state.matching_sale(&self.state);
                first = false;
            }
        }
        self.install_prepared_tick_consensus_record(
            final_record.expect("marketplace batch includes its order"),
        )?;
        state.install_routed(&mut self.state);
        self.next_event_id = next_event_id;
        self.next_event_id_era = next_event_id_era;
        self.journal.events = journal;
        self.runtime_backpressure_stats.journal_events_evicted = self
            .runtime_backpressure_stats
            .journal_events_evicted
            .saturating_add(evicted);

        Ok(())
    }
}
