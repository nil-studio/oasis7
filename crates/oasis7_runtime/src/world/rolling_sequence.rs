//! Shared original rolling arithmetic for cached and root-backed allocators.
use super::{ActionId, IntentSeq, ProposalId, World, WorldEventId};

impl World {
    // Child pub(super) retains the former private World-module scope.
    pub(super) fn allocate_rolling_sequence_id(next_id: &mut u64, era: &mut u64) -> u64 {
        let (allocated, next_id_after, era_after) =
            Self::preview_rolling_sequence_id(*next_id, *era);
        *next_id = next_id_after;
        *era = era_after;
        allocated
    }

    pub(super) fn preview_rolling_sequence_id(next_id: u64, era: u64) -> (u64, u64, u64) {
        let allocated = next_id.max(1);
        if allocated == u64::MAX {
            (allocated, 1, era.saturating_add(1))
        } else {
            (allocated, allocated + 1, era)
        }
    }

    pub(crate) fn preview_next_event_id(
        next_id: WorldEventId,
        era: u64,
    ) -> (WorldEventId, WorldEventId, u64) {
        Self::preview_rolling_sequence_id(next_id, era)
    }

    pub(crate) fn allocate_next_action_id(&mut self) -> ActionId {
        Self::allocate_rolling_sequence_id(&mut self.next_action_id, &mut self.next_action_id_era)
    }

    /// Reconcile an authenticated journal action causality or disposition
    /// for the next locally allocated action on C-to-T replay. Plain Move and
    /// Transfer use event.caused_by without emitting ActionAccepted. One action
    /// can appear in multiple records; only the first matching ID advances the
    /// original rolling allocator. Do not infer gaps or synthesize a queue.
    pub(super) fn reconcile_replayed_action_allocator(
        &mut self,
        action_id: ActionId,
        seen: &mut std::collections::BTreeSet<ActionId>,
    ) {
        if !seen.insert(action_id) {
            return;
        }
        let (expected, next, era) =
            Self::preview_rolling_sequence_id(self.next_action_id, self.next_action_id_era);
        if action_id == expected {
            self.next_action_id = next;
            self.next_action_id_era = era;
        }
    }

    /// Reconcile one authenticated C-to-T event with the original action
    /// allocator. Ordinary Move/Transfer can have a causal action without
    /// the optional accepted/rejected disposition; both carriers agree when
    /// present. A missing ID or gap cannot synthesize a queued action.
    pub(super) fn reconcile_replayed_action_event(
        &mut self,
        event: &crate::WorldEvent,
        seen: &mut std::collections::BTreeSet<ActionId>,
    ) -> Result<(), crate::WorldError> {
        if let Some(crate::CausedBy::Action(causal_id)) = &event.caused_by {
            if let crate::WorldEventBody::Domain(
                crate::DomainEvent::ActionAccepted { action_id, .. }
                | crate::DomainEvent::ActionRejected { action_id, .. },
            ) = &event.body
                && causal_id != action_id
                && (*causal_id == self.next_action_id || *action_id == self.next_action_id)
            {
                // The old raw-event publication contract accepts archival
                // events that carry unrelated external action attribution.
                // Reject a contradiction only when it purports to consume
                // the immediate next real local allocation. Otherwise the
                // no-gap allocator rule below ignores both distant IDs.
                return Err(crate::WorldError::JournalMismatch);
            }
            self.reconcile_replayed_action_allocator(*causal_id, seen);
        }
        if let crate::WorldEventBody::Domain(
            crate::DomainEvent::ActionAccepted { action_id, .. }
            | crate::DomainEvent::ActionRejected { action_id, .. },
        ) = &event.body
        {
            self.reconcile_replayed_action_allocator(*action_id, seen);
        }
        Ok(())
    }

    pub(crate) fn allocate_next_intent_seq(&mut self) -> IntentSeq {
        Self::allocate_rolling_sequence_id(&mut self.next_intent_id, &mut self.next_intent_id_era)
    }

    pub(crate) fn allocate_next_proposal_id(&mut self) -> ProposalId {
        Self::allocate_rolling_sequence_id(
            &mut self.next_proposal_id,
            &mut self.next_proposal_id_era,
        )
    }

    pub(crate) fn preview_next_intent_seq(
        next_id: IntentSeq,
        era: u64,
    ) -> (IntentSeq, IntentSeq, u64) {
        Self::preview_rolling_sequence_id(next_id, era)
    }

    pub(crate) fn preview_next_proposal_id(
        next_id: ProposalId,
        era: u64,
    ) -> (ProposalId, ProposalId, u64) {
        Self::preview_rolling_sequence_id(next_id, era)
    }
}

#[cfg(test)]
mod replay_action_allocator_tests {
    use super::*;

    #[test]
    fn replayed_action_uses_original_rollover_and_deduplicates_receipts() {
        let mut world = World::new();
        world.next_action_id = u64::MAX;
        world.next_action_id_era = 7;
        let mut seen = std::collections::BTreeSet::new();
        world.reconcile_replayed_action_allocator(u64::MAX, &mut seen);
        assert_eq!(world.next_action_id, 1);
        assert_eq!(world.next_action_id_era, 8);
        world.reconcile_replayed_action_allocator(u64::MAX, &mut seen);
        assert_eq!(world.next_action_id, 1);
        assert_eq!(world.next_action_id_era, 8);
        world.reconcile_replayed_action_allocator(1, &mut seen);
        assert_eq!(world.next_action_id, 2);
        assert_eq!(world.next_action_id_era, 8);
    }

    #[test]
    fn replayed_action_does_not_invent_allocations_across_unproven_gaps() {
        let mut world = World::new();
        world.next_action_id = 7;
        let mut seen = std::collections::BTreeSet::new();
        world.reconcile_replayed_action_allocator(9, &mut seen);
        assert_eq!(world.next_action_id, 7);
        world.reconcile_replayed_action_allocator(7, &mut seen);
        assert_eq!(world.next_action_id, 8);
        // A gap cannot be filled by the already-seen out-of-order identity.
        world.reconcile_replayed_action_allocator(9, &mut seen);
        assert_eq!(world.next_action_id, 8);
    }
}
