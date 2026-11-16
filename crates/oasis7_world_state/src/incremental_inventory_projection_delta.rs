//! Actual Current Inventory projection source; no qualification constructor.
pub use crate::incremental_inventory::projection::{
    CurrentInventoryProjectionDeltaV1, is_inventory_projection_field_v1,
    read_current_inventory_projection_delta_v1,
};

pub use crate::incremental_inventory::projection::{QualifiedInventoryProjectionColumnsV1,read_qualified_inventory_projection_columns_v1};
