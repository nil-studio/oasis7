//! Bounded Current inputs for the original due material-transit tail.
//!
//! The parent identity scan is deliberately shared with the existing industry
//! outer-key verifier. Decoded rows and due-order keys are observations only;
//! the reducer must reopen every selected row from Current before changing it.

mod common {
    pub(super) use super::super::NormalTailWorkV1;
    pub(super) use crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1;
    pub(super) use crate::world::incremental_normal_sources::BoundNormalTickParentV1;
    pub(super) use crate::world::{
        incremental_body::BodyMutationError, incremental_step::IwsStepContext,
    };
    pub(super) use oasis7_formal_protocol::{
        controlled_authority::formal::bounded_context::WorkIdentityV1,
        incremental_execution::{ExecutionTransaction, RootSetV1},
        incremental_state::MerkleNodeReader,
        incremental_value::{ValueReadContext, ValueTreeError},
    };
    pub(super) use oasis7_formal_protocol::{
        incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
        incremental_state::StatePageWriter,
        incremental_value::{ValueNodeReader, ValueNodeWriter},
    };
    pub(super) use oasis7_proto::memory_budget::{
        MemoryBudget, MemoryClass, MemoryReservation, ReservedValue,
    };
    pub(super) use oasis7_world_state::{
        LogisticsPathAuthorityV1, LogisticsRouteV1, LogisticsSettlementReceiptV1, MaterialLedgerId,
        MaterialTransitJobState, MaterialTransitPriority,
        incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
        incremental_agent_fields::{AgentFieldV1, AgentScalarRefV1},
        incremental_agent_subtree::{
            AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1,
            AgentSubtreeStructureQualificationV1, AgentSubtreeValueV1,
            change_agent_subtree_scalar_v1, read_agent_subtree_scalar_overlay_v1,
        },
        incremental_factory_lifecycle_index::{
            FactoryLifecycleIndexV1, observe_current_factory_lifecycle_v1,
        },
        incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
        incremental_field_read::WorldFieldReadContextV1,
        incremental_industry_jobs::{
            IndustryCellReadV1, IndustryCellV1, IndustryCollectionV1, IndustryColumnV1,
            IndustryCurrentOuterChangeV1, IndustryFamilyQualificationV1,
            IndustryFamilyReadContextV1, IndustryQualifiedOuterIdentityV1, IndustryRecordCreateV1,
            IndustryRecordDeleteV1, IndustryRecordRefV1, IndustryTransitionReadContextV1,
            IndustryValueRefV1, IndustryValueV1, IndustryWorkBudgetV1,
            create_industry_record_overlay_v1, delete_industry_record_overlay_v1,
            read_industry_record_root_overlay_v1, visit_qualified_transit_parent_identities_v1,
            visit_transit_current_outer_changes_v1,
        },
        incremental_industry_jobs::{
            IndustryRecordKeyV1, IndustryRecordKindV1, read_industry_cell_overlay_v1,
        },
        incremental_inventory::{
            CurrentInventoryMaterialScopeV1, open_current_inventory_material_scope_v1,
        },
        incremental_inventory_change::InventoryChangeRefV1,
        incremental_inventory_overlay_read::InventoryOverlayReadContext,
        incremental_inventory_read::LedgerQuantityV1,
        incremental_ledger_value::LedgerValueKindV1,
        incremental_logistics_records::{
            LogisticsCellChangeV1, LogisticsCellReadV1 as LogisticsReadV1, LogisticsCellV1,
            LogisticsCollectionV1, LogisticsColumnV1 as LogisticsColumn,
            LogisticsFamilyQualificationV1, LogisticsRecordCreateV1, LogisticsRecordKeyV1,
            LogisticsRecordKindV1, LogisticsRecordRefV1, LogisticsValueRefV1, LogisticsValueV1,
            LogisticsWorkBudgetV1, change_logistics_cell_overlay_v1,
            create_logistics_record_overlay_v1, read_logistics_cell_overlay_v1,
        },
        incremental_value_mutation::ValueMutationContextV1,
        incremental_world_id_sets::{
            WorldIdSetChangeV1, WorldIdSetFieldV1, WorldIdSetKeyRefV1,
            change_world_id_set_overlay_v1, world_id_set_contains_overlay_v1,
        },
        incremental_world_native::{
            WorldNativeFieldsQualificationV1, increment_current_material_transit_progress_v1,
            recover_current_factory_industry_progress_v1, recover_gameplay_policy_overlay_v1,
        },
    };
    pub(super) use std::cell::RefCell;
    pub(super) use std::{
        collections::{BTreeMap, BTreeSet},
        io::Write,
        mem::size_of,
    };

    pub(super) fn invalid() -> BodyMutationError {
        BodyMutationError::Invalid("normal material transit Current record")
    }

    pub(super) fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
        match error {
            WorldFieldOverlayErrorV1::Execution(error) => error.into(),
            WorldFieldOverlayErrorV1::Value(error) => error.into(),
            WorldFieldOverlayErrorV1::Member(error) => BodyMutationError::Original(Box::new(error)),
        }
    }

    #[derive(Debug)]
    struct NormalTransitWorldErrorV1(crate::WorldError);

    impl std::fmt::Display for NormalTransitWorldErrorV1 {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match &self.0 {
                crate::WorldError::ResourceBalanceInvalid { reason } => formatter.write_str(reason),
                error => write!(formatter, "{error:?}"),
            }
        }
    }

    impl std::error::Error for NormalTransitWorldErrorV1 {}

    pub(super) fn original_world_error(error: crate::WorldError) -> BodyMutationError {
        BodyMutationError::Original(Box::new(NormalTransitWorldErrorV1(error)))
    }

    pub(super) fn charge_reserve(
        normal: &mut NormalTailWorkV1<'_, '_>,
        memory: &MemoryBudget,
        bytes: usize,
        _label: &'static str,
    ) -> Result<MemoryReservation, BodyMutationError> {
        if !normal.memory().same_pool(memory) {
            return Err(BodyMutationError::Invalid("normal transit work pool"));
        }
        normal.charge(bytes)?;
        memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)
            .map_err(Into::into)
    }
}

#[path = "incremental_normal_transits_current_read.rs"]
mod current_read;
#[path = "incremental_normal_transits_reduce.rs"]
mod reduce;
#[path = "incremental_normal_transits_settlement.rs"]
mod settlement;

pub(in crate::world) use current_read::{
    DueTransitOrderListV1, DueTransitOrderV1, collect_due_transit_order_v1,
};
pub(in crate::world) use reduce::{
    PreparedNormalMaterialTransitV1, reduce_current_material_transit_v1,
};
