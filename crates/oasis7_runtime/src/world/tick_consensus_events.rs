use super::*;
impl World {
    pub(super) fn hash_tick_events_from_ids(
        &self,
        tick: WorldTime,
        ordered_event_ids: &[WorldEventId],
    ) -> Result<String, WorldError> {
        if ordered_event_ids.is_empty() {
            let empty_inputs: Vec<TickEventHashInput<'_>> = Vec::new();
            return hash_json(&empty_inputs);
        }
        let mut tick_events_by_id = BTreeMap::new();
        for event in self
            .journal
            .events
            .iter()
            .filter(|event| event.time == tick)
        {
            tick_events_by_id.entry(event.id).or_insert(event);
        }
        let mut events = Vec::with_capacity(ordered_event_ids.len());
        for event_id in ordered_event_ids {
            let event = if let Some(hot) = tick_events_by_id.get(event_id) {
                (**hot).clone()
            } else if self.state.retained_history_root.is_some() {
                self.read_retained_tick_journal_event(tick,*event_id)?.ok_or_else(||WorldError::DistributedValidationFailed {
                    reason:format!("tick event missing from authenticated retained history tick={tick} event_id={event_id}"),
                })?
            } else {
                return Err(WorldError::DistributedValidationFailed {
                    reason: format!(
                        "tick event missing during hash rebuild tick={tick} event_id={event_id}"
                    ),
                });
            };
            events.push(event);
        }
        self.hash_tick_event_refs(events.iter())
    }
}
