//! Tick verification may consume only the currently attached authenticated root.
use super::World;
use crate::{WorldError, WorldEvent, WorldEventId, WorldTime};
impl World {
    /// Reconstruct only the exact previously qualified parent Snapshot using
    /// this World's authenticated reader, even when candidate writes overlay it.
    /// A proposed candidate Snapshot cannot match the parent's signed hash.
    pub fn restore_snapshot_with_attached_formal_history(
        &self,
        snapshot: crate::Snapshot,
        journal: crate::Journal,
    ) -> Result<World, WorldError> {
        let parent = self
            .formal_history_reader
            .as_ref()
            .and_then(|attached| attached.parent.clone())
            .ok_or_else(|| WorldError::DistributedValidationFailed {
                reason: "exact formal parent restore requires qualified retained-history evidence"
                    .into(),
            })?;
        let (_, reader) = self.attached_formal_history()?;
        Self::from_verified_formal_snapshot(
            snapshot,
            journal,
            parent,
            std::sync::Arc::new(reader.clone()),
        )
        .map(|mut world| {
            world.source_compiler = self.source_compiler.clone();
            world
        })
    }
    pub fn read_retained_tick_journal_event(
        &self,
        tick: WorldTime,
        event_id: WorldEventId,
    ) -> Result<Option<WorldEvent>, WorldError> {
        if tick > self.state.time {
            return Err(WorldError::DistributedValidationFailed {
                reason: "retained Tick event is in the future".into(),
            });
        }
        let (view, reader) = self.attached_formal_history()?;
        view.read_tick_journal_event(tick, event_id, self.next_event_id_era, reader)
            .map_err(WorldError::from)
    }
}
