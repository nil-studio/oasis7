use crate::runtime::{FactoryState, WorldState};
pub(super) use oasis7_world_state::gameplay_capability::current_repeat_count;
use oasis7_world_state::gameplay_capability::{GameplayCapabilityQuery, GameplaySubject};

pub(super) fn query(
    state: &WorldState,
    owner: Option<&str>,
    personal: bool,
) -> GameplayCapabilityQuery {
    state.gameplay_capability_query(
        GameplaySubject::resolve(owner, personal),
        "current_industrial_capability",
    )
}

pub(super) fn primary_factory<'a>(
    state: &'a WorldState,
    query: &GameplayCapabilityQuery,
) -> Option<&'a FactoryState> {
    match query.subject {
        GameplaySubject::Spectator => {
            super::super::gameplay_snapshot_helpers::primary_factory_for_player_gameplay(state)
        }
        _ => query
            .primary_factory_id
            .as_ref()
            .and_then(|id| state.factories.get(id)),
    }
}
