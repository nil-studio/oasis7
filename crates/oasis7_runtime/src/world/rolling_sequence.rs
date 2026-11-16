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
