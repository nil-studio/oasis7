//! Concrete observation codecs and client verification adapters; no live World.
use oasis7_world_state::WorldState;
pub use oasis7_world_state::observation_projection::{
    WorldServiceAgentContext, WorldServiceContinuationContext, WorldServiceFeedbackHistory,
    WorldServiceFeedbackReplayRecord, WorldServiceProjection as WorldServiceProjectionData,
    current_commitment, current_commitment_history,
};
use serde::{Deserialize, Serialize};

/// Local client-proof adapter around the single shared observation DATA schema.
///
/// The transparent representation adds no allocation or charge-bearing owner.
/// Conversion only moves DATA; it does not authenticate a source or a reader.
/// The local type keeps the Game Client trait implementation in this upper layer.
#[repr(transparent)]
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorldServiceProjection<S = WorldState>(WorldServiceProjectionData<S>);

impl<S> WorldServiceProjection<S> {
    pub fn data(&self) -> &WorldServiceProjectionData<S> {
        &self.0
    }

    /// Borrow the original fields together when disjoint field borrows are needed.
    pub fn data_mut(&mut self) -> &mut WorldServiceProjectionData<S> {
        &mut self.0
    }

    /// Consume the adapter without cloning DATA or its allocation-charge owners.
    pub fn into_data(self) -> WorldServiceProjectionData<S> {
        self.0
    }
}

impl<S> From<WorldServiceProjectionData<S>> for WorldServiceProjection<S> {
    fn from(data: WorldServiceProjectionData<S>) -> Self {
        Self(data)
    }
}

impl<S> std::ops::Deref for WorldServiceProjection<S> {
    type Target = WorldServiceProjectionData<S>;

    fn deref(&self) -> &Self::Target {
        self.data()
    }
}

impl<S> std::ops::DerefMut for WorldServiceProjection<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data_mut()
    }
}

impl<S: std::fmt::Debug> std::fmt::Debug for WorldServiceProjection<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.data(), formatter)
    }
}

pub mod codec;
pub mod formal_read;
pub mod original_formatter;

/// Independent Game Client evidence specialized for the authoritative value schema.
pub type VerifiedWorldView =
    oasis7_game_client::verified_world_view::VerifiedWorldView<WorldServiceProjection>;

#[cfg(test)]
mod codec_tests;
