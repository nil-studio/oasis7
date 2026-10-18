//! Authenticated view evidence is owned by the independent Game Client.
pub use oasis7_game_client::verified_world_view::{
    CONTROLLED_PREREQUISITE_READ_SCOPE, ReadEvidencePolicy,
};
pub type VerifiedWorldView = oasis7_game_client::verified_world_view::VerifiedWorldView<
    crate::projection::WorldServiceProjection,
>;
