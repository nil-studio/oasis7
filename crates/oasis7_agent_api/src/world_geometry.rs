//! Shared chunk geometry values.
use crate::world_action::GeoPos;
use serde::{Deserialize, Serialize};

pub const CHUNK_SIZE_X_CM: i64 = 2_000_000;
pub const CHUNK_SIZE_Y_CM: i64 = 2_000_000;
pub const CHUNK_SIZE_Z_CM: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkBounds {
    pub min: GeoPos,
    pub max: GeoPos,
}

pub fn chunk_grid_dims(dimensions: (i64, i64, i64)) -> (i32, i32, i32) {
    (
        ceil_div_i64(dimensions.0.max(0), CHUNK_SIZE_X_CM) as i32,
        ceil_div_i64(dimensions.1.max(0), CHUNK_SIZE_Y_CM) as i32,
        ceil_div_i64(dimensions.2.max(0), CHUNK_SIZE_Z_CM) as i32,
    )
}

pub fn chunk_coords(dimensions: (i64, i64, i64)) -> Vec<ChunkCoord> {
    let (gx, gy, gz) = chunk_grid_dims(dimensions);
    let mut coords = Vec::new();
    for x in 0..gx {
        for y in 0..gy {
            for z in 0..gz {
                coords.push(ChunkCoord { x, y, z });
            }
        }
    }
    coords
}

pub fn chunk_coord_of(pos: GeoPos, dimensions: (i64, i64, i64)) -> Option<ChunkCoord> {
    if pos.x_cm < 0
        || pos.y_cm < 0
        || pos.z_cm < 0
        || pos.x_cm > dimensions.0
        || pos.y_cm > dimensions.1
        || pos.z_cm > dimensions.2
    {
        return None;
    }
    let (gx, gy, gz) = chunk_grid_dims(dimensions);
    if gx <= 0 || gy <= 0 || gz <= 0 {
        return None;
    }

    let x = clamp_chunk_index((pos.x_cm / CHUNK_SIZE_X_CM) as i32, gx);
    let y = clamp_chunk_index((pos.y_cm / CHUNK_SIZE_Y_CM) as i32, gy);
    let z = clamp_chunk_index((pos.z_cm / CHUNK_SIZE_Z_CM) as i32, gz);
    Some(ChunkCoord { x, y, z })
}

pub fn chunk_bounds(coord: ChunkCoord, dimensions: (i64, i64, i64)) -> Option<ChunkBounds> {
    let (gx, gy, gz) = chunk_grid_dims(dimensions);
    if coord.x < 0 || coord.x >= gx || coord.y < 0 || coord.y >= gy || coord.z < 0 || coord.z >= gz
    {
        return None;
    }

    let min_x = coord.x as i64 * CHUNK_SIZE_X_CM;
    let min_y = coord.y as i64 * CHUNK_SIZE_Y_CM;
    let min_z = coord.z as i64 * CHUNK_SIZE_Z_CM;

    let max_x = ((coord.x as i64 + 1) * CHUNK_SIZE_X_CM).min(dimensions.0);
    let max_y = ((coord.y as i64 + 1) * CHUNK_SIZE_Y_CM).min(dimensions.1);
    let max_z = ((coord.z as i64 + 1) * CHUNK_SIZE_Z_CM).min(dimensions.2);

    Some(ChunkBounds {
        min: GeoPos::new(min_x, min_y, min_z),
        max: GeoPos::new(max_x, max_y, max_z),
    })
}

pub fn chunk_seed(world_seed: u64, coord: ChunkCoord) -> u64 {
    let mut x = world_seed ^ 0x9E37_79B9_7F4A_7C15;
    x ^= zigzag_i32(coord.x).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = splitmix64(x);
    x ^= zigzag_i32(coord.y).wrapping_mul(0x94D0_49BB_1331_11EB);
    x = splitmix64(x);
    x ^= zigzag_i32(coord.z).wrapping_mul(0xD6E8_FEB8_6659_FD93);
    splitmix64(x)
}

fn ceil_div_i64(value: i64, divisor: i64) -> i64 {
    if value <= 0 {
        return 0;
    }
    (value + divisor - 1) / divisor
}

fn clamp_chunk_index(index: i32, grid: i32) -> i32 {
    if grid <= 0 {
        return 0;
    }
    if index < 0 {
        0
    } else if index >= grid {
        grid - 1
    } else {
        index
    }
}

fn zigzag_i32(value: i32) -> u64 {
    let v = value as i64;
    ((v << 1) ^ (v >> 63)) as u64
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

pub const SPACE_UNIT_CM: i64 = 1;
pub const DEFAULT_CLOUD_WIDTH_KM: i64 = 100;
pub const DEFAULT_CLOUD_DEPTH_KM: i64 = 100;
pub const DEFAULT_CLOUD_HEIGHT_KM: i64 = 10;
pub const DEFAULT_CLOUD_WIDTH_CM: i64 = DEFAULT_CLOUD_WIDTH_KM * 100_000;
pub const DEFAULT_CLOUD_DEPTH_CM: i64 = DEFAULT_CLOUD_DEPTH_KM * 100_000;
pub const DEFAULT_CLOUD_HEIGHT_CM: i64 = DEFAULT_CLOUD_HEIGHT_KM * 100_000;

pub fn space_distance_m(a: GeoPos, b: GeoPos) -> f64 {
    let dx_m = (a.x_cm - b.x_cm) as f64 / 100.0;
    let dy_m = (a.y_cm - b.y_cm) as f64 / 100.0;
    let dz_m = (a.z_cm - b.z_cm) as f64 / 100.0;
    ((dx_m * dx_m) + (dy_m * dy_m) + (dz_m * dz_m)).sqrt()
}

pub fn space_distance_cm(a: GeoPos, b: GeoPos) -> i64 {
    let distance_m = space_distance_m(a, b);
    (distance_m * 100.0).round().max(0.0) as i64
}

#[cfg(test)]
mod coordinate_tests {
    use super::*;

    #[test]
    fn geo_pos_new_keeps_integer_centimeter_coordinates() {
        let pos = GeoPos::new(10, -3, 4);

        assert_eq!(pos.x_cm, 10);
        assert_eq!(pos.y_cm, -3);
        assert_eq!(pos.z_cm, 4);
    }

    #[test]
    fn geo_pos_deserializes_legacy_float_centimeter_coordinates() {
        let decoded: GeoPos = serde_json::from_str(r#"{"x_cm":10.0,"y_cm":-3.0,"z_cm":4.0}"#)
            .expect("decode legacy float geopo");

        assert_eq!(decoded, GeoPos::new(10, -3, 4));
    }

    #[test]
    fn geo_pos_rejects_non_integral_float_coordinates() {
        let err = serde_json::from_str::<GeoPos>(r#"{"x_cm":10.5,"y_cm":-3.0,"z_cm":4.0}"#)
            .expect_err("non-integral coordinates should fail");

        assert!(err.to_string().contains("integer centimeter value"));
    }
}
