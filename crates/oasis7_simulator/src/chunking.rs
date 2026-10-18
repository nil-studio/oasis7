use super::world_model::SpaceConfig;
use crate::geometry::GeoPos;
pub use oasis7_agent_api::world_geometry::{
    CHUNK_SIZE_X_CM, CHUNK_SIZE_Y_CM, CHUNK_SIZE_Z_CM, ChunkBounds, ChunkCoord, chunk_seed,
};

pub fn chunk_grid_dims(space: &SpaceConfig) -> (i32, i32, i32) {
    oasis7_agent_api::world_geometry::chunk_grid_dims((
        space.width_cm,
        space.depth_cm,
        space.height_cm,
    ))
}

pub fn chunk_coords(space: &SpaceConfig) -> Vec<ChunkCoord> {
    oasis7_agent_api::world_geometry::chunk_coords((
        space.width_cm,
        space.depth_cm,
        space.height_cm,
    ))
}

pub fn chunk_coord_of(pos: GeoPos, space: &SpaceConfig) -> Option<ChunkCoord> {
    oasis7_agent_api::world_geometry::chunk_coord_of(
        pos,
        (space.width_cm, space.depth_cm, space.height_cm),
    )
}

pub fn chunk_bounds(coord: ChunkCoord, space: &SpaceConfig) -> Option<ChunkBounds> {
    oasis7_agent_api::world_geometry::chunk_bounds(
        coord,
        (space.width_cm, space.depth_cm, space.height_cm),
    )
}
