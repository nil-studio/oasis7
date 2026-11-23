//! Strict zero-IO L0 domain primitives shared across Oasis7 layers.
//!
//! This crate intentionally contains pure value objects, checked arithmetic,
//! deterministic hashing, and canonical projection commitments only. It must
//! never depend on L1-L4 workspace crates, API/service facades, or runtime I/O.

pub mod canonical_digest;
pub mod chain_resource_hash;
pub mod rule_settlement;
pub mod formal_projection;
pub mod geometry;
pub mod ledger;
pub mod world_entity;
pub mod world_geometry;
pub mod world_resource;

pub use geometry::GeoPos;
pub use ledger::MaterialLedgerId;
pub use world_entity::{
    DEFAULT_AGENT_HEIGHT_CM, ModuleVisualAnchor, ModuleVisualEntity, RobotBodySpec,
};
pub use world_geometry::{
    CHUNK_SIZE_X_CM, CHUNK_SIZE_Y_CM, CHUNK_SIZE_Z_CM, ChunkBounds, ChunkCoord, chunk_bounds,
    chunk_coord_of, chunk_coords, chunk_grid_dims, chunk_seed,
};
pub use world_resource::ResourceKind;
