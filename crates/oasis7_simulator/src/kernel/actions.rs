use crate::geometry::{GeoPos, space_distance_cm};
use std::collections::BTreeMap;

use super::super::chunking::CHUNK_SIZE_X_CM;
use super::super::module_visual::ModuleVisualAnchor;
use super::super::power::{
    AgentPowerState, PlantStatus, PowerEvent, PowerPlant, PowerSaleQuote, PowerSurvivalQuote,
};
use super::super::types::{
    Action, CM_PER_KM, ElementBudgetError, FragmentElementKind, PPM_BASE, PowerOrderSide,
    RefineQuote, ResourceKind, ResourceOwner, ScheduleQuote, StockError,
};
use super::super::world_model::{Agent, Factory, FragmentResourceError, Location, PowerOrderState};
use super::WorldKernel;
use super::types::{ChunkGenerationCause, PowerOrderFill, RejectReason, WorldEventKind};

#[derive(Debug, Clone, Copy)]
struct RecipePlan {
    required_factory_kind: &'static str,
    electricity_per_batch: i64,
    hardware_per_batch: i64,
    data_output_per_batch: i64,
    finished_product_id: &'static str,
    finished_product_units_per_batch: i64,
}

const LOCATION_ELECTRICITY_POOL_REMOVED_NOTE: &str = "location electricity pool removed";
const FACTORY_KIND_SMELTER_MK1: &str = "factory.smelter.mk1";
const FACTORY_KIND_ASSEMBLER_MK1: &str = "factory.assembler.mk1";
const FACTORY_KIND_RADIATION_POWER_MK1: &str = "factory.power.radiation.mk1";

#[derive(Debug, Clone, Copy)]
struct PreparedPowerTransfer {
    loss: i64,
    quoted_price_per_pu: i64,
}

include!("actions_core.rs");
include!("actions_power_order_matching.rs");
include!("actions_power_sale_quote.rs");
include!("actions_refine_quote.rs");
include!("actions_schedule_quote.rs");
include!("actions_resolution.rs");
include!("actions_regressions.rs");
