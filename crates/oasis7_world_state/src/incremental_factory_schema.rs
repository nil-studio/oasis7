//! Closed factory/build-job schema. No business authority or complete World qualification.
use crate::{incremental_field_member::*, incremental_projection::WORLD_STATE_FIELDS};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};

/// Original outer map keys remain distinct from record.factory_id / job_id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryRecordKeyV1<'a> {
    Factory(&'a str),
    Build(u64),
}
impl FactoryRecordKeyV1<'_> {
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::Factory(_) => StateNamespaceV1::Business,
            Self::Build(_) => StateNamespaceV1::Scheduler,
        }
    }
    pub(crate) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        let name = match self {
            Self::Factory(_) => "factories",
            Self::Build(_) => "pending_factory_builds",
        };
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|f| {
                f.name == name
                    && f.owner == FieldOwner::WorldState
                    && f.shape == FieldShape::Map
                    && f.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(ValueTreeError::Invalid("factory field registration"))
    }
    pub(crate) fn accepts(self, column: FactoryColumnV1) -> bool {
        match self {
            Self::Factory(_) => !column.build_only(),
            Self::Build(_) => !column.factory_only(),
        }
    }
}
/// Every scalar/enum/text/ledger leaf has one closed profile; optional None is Unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryColumnV1 {
    FactoryFactoryId,
    FactorySiteId,
    FactoryBuilderAgentId,
    FactoryInputLedger,
    FactoryOutputLedger,
    FactoryDurabilityPpm,
    FactorySiteAuthorityRevision,
    FactorySiteLocationId,
    FactoryLocationAnchorRevision,
    FactoryConstructionPowerProfileKey,
    FactoryConstructionPowerProfileRevision,
    FactoryBuiltAt,
    BuildJobId,
    BuildBuilderAgentId,
    BuildSiteId,
    BuildConsumeLedger,
    BuildReadyAt,
    BuildContractVersion,
    BuildSiteAuthorityRevision,
    BuildSiteLocationId,
    BuildLocationAnchorRevision,
    SpecFactoryId,
    SpecDisplayName,
    SpecTier,
    SpecBuildTimeTicks,
    SpecBasePowerDraw,
    SpecRecipeSlots,
    SpecThroughputBps,
    SpecMaintenancePerTick,
    ProductionStatus,
    ProductionActiveJobs,
    ProductionCurrentJobId,
    ProductionCurrentRecipeId,
    ProductionLastStartedAt,
    ProductionLastCompletedAt,
    ProductionLastBlockedAt,
    ProductionLastResumedAt,
    ProductionCurrentBlockerKind,
    ProductionCurrentBlockerDetail,
    ProductionCurrentBlockerActionId,
    ProductionCompletedJobs,
    ProductionLastCompletedRecipeId,
    ProductionSameRecipeRepeatCount,
    ProductionSnapshotPresent,
    SnapshotRecipeId,
    SnapshotConsumeLedger,
    SnapshotPowerRequired,
    SnapshotOutputLedger,
    ObligationPresent,
    ObligationPayerAgentId,
    ObligationProfileKey,
    ObligationProfileRevision,
    ObligationElectricityAmount,
    ObligationMode,
    ObligationMaterialLedger,
    ObligationElectricityBefore,
    ObligationElectricityAfter,
}
impl FactoryColumnV1 {
    pub(crate) const ALL: [Self; 57] = [
        Self::FactoryFactoryId,
        Self::FactorySiteId,
        Self::FactoryBuilderAgentId,
        Self::FactoryInputLedger,
        Self::FactoryOutputLedger,
        Self::FactoryDurabilityPpm,
        Self::FactorySiteAuthorityRevision,
        Self::FactorySiteLocationId,
        Self::FactoryLocationAnchorRevision,
        Self::FactoryConstructionPowerProfileKey,
        Self::FactoryConstructionPowerProfileRevision,
        Self::FactoryBuiltAt,
        Self::BuildJobId,
        Self::BuildBuilderAgentId,
        Self::BuildSiteId,
        Self::BuildConsumeLedger,
        Self::BuildReadyAt,
        Self::BuildContractVersion,
        Self::BuildSiteAuthorityRevision,
        Self::BuildSiteLocationId,
        Self::BuildLocationAnchorRevision,
        Self::SpecFactoryId,
        Self::SpecDisplayName,
        Self::SpecTier,
        Self::SpecBuildTimeTicks,
        Self::SpecBasePowerDraw,
        Self::SpecRecipeSlots,
        Self::SpecThroughputBps,
        Self::SpecMaintenancePerTick,
        Self::ProductionStatus,
        Self::ProductionActiveJobs,
        Self::ProductionCurrentJobId,
        Self::ProductionCurrentRecipeId,
        Self::ProductionLastStartedAt,
        Self::ProductionLastCompletedAt,
        Self::ProductionLastBlockedAt,
        Self::ProductionLastResumedAt,
        Self::ProductionCurrentBlockerKind,
        Self::ProductionCurrentBlockerDetail,
        Self::ProductionCurrentBlockerActionId,
        Self::ProductionCompletedJobs,
        Self::ProductionLastCompletedRecipeId,
        Self::ProductionSameRecipeRepeatCount,
        Self::ProductionSnapshotPresent,
        Self::SnapshotRecipeId,
        Self::SnapshotConsumeLedger,
        Self::SnapshotPowerRequired,
        Self::SnapshotOutputLedger,
        Self::ObligationPresent,
        Self::ObligationPayerAgentId,
        Self::ObligationProfileKey,
        Self::ObligationProfileRevision,
        Self::ObligationElectricityAmount,
        Self::ObligationMode,
        Self::ObligationMaterialLedger,
        Self::ObligationElectricityBefore,
        Self::ObligationElectricityAfter,
    ];
    pub(crate) fn factory_only(self) -> bool {
        matches!(
            self,
            Self::FactoryFactoryId
                | Self::FactorySiteId
                | Self::FactoryBuilderAgentId
                | Self::FactoryInputLedger
                | Self::FactoryOutputLedger
                | Self::FactoryDurabilityPpm
                | Self::FactorySiteAuthorityRevision
                | Self::FactorySiteLocationId
                | Self::FactoryLocationAnchorRevision
                | Self::FactoryConstructionPowerProfileKey
                | Self::FactoryConstructionPowerProfileRevision
                | Self::FactoryBuiltAt
                | Self::ProductionStatus
                | Self::ProductionActiveJobs
                | Self::ProductionCurrentJobId
                | Self::ProductionCurrentRecipeId
                | Self::ProductionLastStartedAt
                | Self::ProductionLastCompletedAt
                | Self::ProductionLastBlockedAt
                | Self::ProductionLastResumedAt
                | Self::ProductionCurrentBlockerKind
                | Self::ProductionCurrentBlockerDetail
                | Self::ProductionCurrentBlockerActionId
                | Self::ProductionCompletedJobs
                | Self::ProductionLastCompletedRecipeId
                | Self::ProductionSameRecipeRepeatCount
                | Self::ProductionSnapshotPresent
                | Self::SnapshotRecipeId
                | Self::SnapshotConsumeLedger
                | Self::SnapshotPowerRequired
                | Self::SnapshotOutputLedger
        )
    }
    pub(crate) fn build_only(self) -> bool {
        matches!(
            self,
            Self::BuildJobId
                | Self::BuildBuilderAgentId
                | Self::BuildSiteId
                | Self::BuildConsumeLedger
                | Self::BuildReadyAt
                | Self::BuildContractVersion
                | Self::BuildSiteAuthorityRevision
                | Self::BuildSiteLocationId
                | Self::BuildLocationAnchorRevision
                | Self::ObligationPresent
                | Self::ObligationPayerAgentId
                | Self::ObligationProfileKey
                | Self::ObligationProfileRevision
                | Self::ObligationElectricityAmount
                | Self::ObligationMode
                | Self::ObligationMaterialLedger
                | Self::ObligationElectricityBefore
                | Self::ObligationElectricityAfter
        )
    }
    pub(crate) fn path(self) -> (&'static str, &'static str) {
        match self {
            Self::FactoryFactoryId => ("factory", "factory_id"),
            Self::FactorySiteId => ("factory", "site_id"),
            Self::FactoryBuilderAgentId => ("factory", "builder_agent_id"),
            Self::FactoryInputLedger => ("factory", "input_ledger"),
            Self::FactoryOutputLedger => ("factory", "output_ledger"),
            Self::FactoryDurabilityPpm => ("factory", "durability_ppm"),
            Self::FactorySiteAuthorityRevision => ("factory", "site_authority_revision"),
            Self::FactorySiteLocationId => ("factory", "site_location_id"),
            Self::FactoryLocationAnchorRevision => ("factory", "location_anchor_revision"),
            Self::FactoryConstructionPowerProfileKey => {
                ("factory", "construction_power_profile_key")
            }
            Self::FactoryConstructionPowerProfileRevision => {
                ("factory", "construction_power_profile_revision")
            }
            Self::FactoryBuiltAt => ("factory", "built_at"),
            Self::BuildJobId => ("build", "job_id"),
            Self::BuildBuilderAgentId => ("build", "builder_agent_id"),
            Self::BuildSiteId => ("build", "site_id"),
            Self::BuildConsumeLedger => ("build", "consume_ledger"),
            Self::BuildReadyAt => ("build", "ready_at"),
            Self::BuildContractVersion => ("build", "contract_version"),
            Self::BuildSiteAuthorityRevision => ("build", "site_authority_revision"),
            Self::BuildSiteLocationId => ("build", "site_location_id"),
            Self::BuildLocationAnchorRevision => ("build", "location_anchor_revision"),
            Self::SpecFactoryId => ("spec", "factory_id"),
            Self::SpecDisplayName => ("spec", "display_name"),
            Self::SpecTier => ("spec", "tier"),
            Self::SpecBuildTimeTicks => ("spec", "build_time_ticks"),
            Self::SpecBasePowerDraw => ("spec", "base_power_draw"),
            Self::SpecRecipeSlots => ("spec", "recipe_slots"),
            Self::SpecThroughputBps => ("spec", "throughput_bps"),
            Self::SpecMaintenancePerTick => ("spec", "maintenance_per_tick"),
            Self::ProductionStatus => ("production", "status"),
            Self::ProductionActiveJobs => ("production", "active_jobs"),
            Self::ProductionCurrentJobId => ("production", "current_job_id"),
            Self::ProductionCurrentRecipeId => ("production", "current_recipe_id"),
            Self::ProductionLastStartedAt => ("production", "last_started_at"),
            Self::ProductionLastCompletedAt => ("production", "last_completed_at"),
            Self::ProductionLastBlockedAt => ("production", "last_blocked_at"),
            Self::ProductionLastResumedAt => ("production", "last_resumed_at"),
            Self::ProductionCurrentBlockerKind => ("production", "current_blocker_kind"),
            Self::ProductionCurrentBlockerDetail => ("production", "current_blocker_detail"),
            Self::ProductionCurrentBlockerActionId => ("production", "current_blocker_action_id"),
            Self::ProductionCompletedJobs => ("production", "completed_jobs"),
            Self::ProductionLastCompletedRecipeId => ("production", "last_completed_recipe_id"),
            Self::ProductionSameRecipeRepeatCount => ("production", "same_recipe_repeat_count"),
            Self::ProductionSnapshotPresent => ("production", "last_completed_canonical_snapshot"),
            Self::SnapshotRecipeId => ("last_completed_canonical_snapshot", "recipe_id"),
            Self::SnapshotConsumeLedger => ("last_completed_canonical_snapshot", "consume_ledger"),
            Self::SnapshotPowerRequired => ("last_completed_canonical_snapshot", "power_required"),
            Self::SnapshotOutputLedger => ("last_completed_canonical_snapshot", "output_ledger"),
            Self::ObligationPresent => (
                "construction_power_obligation",
                "construction_power_obligation",
            ),
            Self::ObligationPayerAgentId => ("construction_power_obligation", "payer_agent_id"),
            Self::ObligationProfileKey => ("construction_power_obligation", "profile_key"),
            Self::ObligationProfileRevision => {
                ("construction_power_obligation", "profile_revision")
            }
            Self::ObligationElectricityAmount => {
                ("construction_power_obligation", "electricity_amount")
            }
            Self::ObligationMode => ("construction_power_obligation", "mode"),
            Self::ObligationMaterialLedger => ("construction_power_obligation", "material_ledger"),
            Self::ObligationElectricityBefore => {
                ("construction_power_obligation", "electricity_before")
            }
            Self::ObligationElectricityAfter => {
                ("construction_power_obligation", "electricity_after")
            }
        }
    }
    pub(crate) fn profile(self) -> (MemberPayloadCodecV1, bool) {
        match self {
            Self::FactoryFactoryId => (MemberPayloadCodecV1::Text, false),
            Self::FactorySiteId => (MemberPayloadCodecV1::Text, false),
            Self::FactoryBuilderAgentId => (MemberPayloadCodecV1::Text, false),
            Self::FactoryInputLedger => (MemberPayloadCodecV1::Ledger, false),
            Self::FactoryOutputLedger => (MemberPayloadCodecV1::Ledger, false),
            Self::FactoryDurabilityPpm => (MemberPayloadCodecV1::I64, false),
            Self::FactorySiteAuthorityRevision => (MemberPayloadCodecV1::U64, true),
            Self::FactorySiteLocationId => (MemberPayloadCodecV1::Text, true),
            Self::FactoryLocationAnchorRevision => (MemberPayloadCodecV1::U64, true),
            Self::FactoryConstructionPowerProfileKey => (MemberPayloadCodecV1::Text, true),
            Self::FactoryConstructionPowerProfileRevision => (MemberPayloadCodecV1::U64, true),
            Self::FactoryBuiltAt => (MemberPayloadCodecV1::U64, false),
            Self::BuildJobId => (MemberPayloadCodecV1::U64, false),
            Self::BuildBuilderAgentId => (MemberPayloadCodecV1::Text, false),
            Self::BuildSiteId => (MemberPayloadCodecV1::Text, false),
            Self::BuildConsumeLedger => (MemberPayloadCodecV1::Ledger, false),
            Self::BuildReadyAt => (MemberPayloadCodecV1::U64, false),
            Self::BuildContractVersion => (MemberPayloadCodecV1::U64, false),
            Self::BuildSiteAuthorityRevision => (MemberPayloadCodecV1::U64, true),
            Self::BuildSiteLocationId => (MemberPayloadCodecV1::Text, true),
            Self::BuildLocationAnchorRevision => (MemberPayloadCodecV1::U64, true),
            Self::SpecFactoryId => (MemberPayloadCodecV1::Text, false),
            Self::SpecDisplayName => (MemberPayloadCodecV1::Text, false),
            Self::SpecTier => (MemberPayloadCodecV1::U64, false),
            Self::SpecBuildTimeTicks => (MemberPayloadCodecV1::U64, false),
            Self::SpecBasePowerDraw => (MemberPayloadCodecV1::I64, false),
            Self::SpecRecipeSlots => (MemberPayloadCodecV1::U64, false),
            Self::SpecThroughputBps => (MemberPayloadCodecV1::U64, false),
            Self::SpecMaintenancePerTick => (MemberPayloadCodecV1::I64, false),
            Self::ProductionStatus => (MemberPayloadCodecV1::U64, false),
            Self::ProductionActiveJobs => (MemberPayloadCodecV1::U64, false),
            Self::ProductionCurrentJobId => (MemberPayloadCodecV1::U64, true),
            Self::ProductionCurrentRecipeId => (MemberPayloadCodecV1::Text, true),
            Self::ProductionLastStartedAt => (MemberPayloadCodecV1::U64, true),
            Self::ProductionLastCompletedAt => (MemberPayloadCodecV1::U64, true),
            Self::ProductionLastBlockedAt => (MemberPayloadCodecV1::U64, true),
            Self::ProductionLastResumedAt => (MemberPayloadCodecV1::U64, true),
            Self::ProductionCurrentBlockerKind => (MemberPayloadCodecV1::Text, true),
            Self::ProductionCurrentBlockerDetail => (MemberPayloadCodecV1::Text, true),
            Self::ProductionCurrentBlockerActionId => (MemberPayloadCodecV1::U64, true),
            Self::ProductionCompletedJobs => (MemberPayloadCodecV1::U64, false),
            Self::ProductionLastCompletedRecipeId => (MemberPayloadCodecV1::Text, true),
            Self::ProductionSameRecipeRepeatCount => (MemberPayloadCodecV1::U64, false),
            Self::ProductionSnapshotPresent => (MemberPayloadCodecV1::Bool, false),
            Self::SnapshotRecipeId => (MemberPayloadCodecV1::Text, true),
            Self::SnapshotConsumeLedger => (MemberPayloadCodecV1::Ledger, true),
            Self::SnapshotPowerRequired => (MemberPayloadCodecV1::I64, true),
            Self::SnapshotOutputLedger => (MemberPayloadCodecV1::Ledger, true),
            Self::ObligationPresent => (MemberPayloadCodecV1::Bool, false),
            Self::ObligationPayerAgentId => (MemberPayloadCodecV1::Text, true),
            Self::ObligationProfileKey => (MemberPayloadCodecV1::Text, true),
            Self::ObligationProfileRevision => (MemberPayloadCodecV1::U64, true),
            Self::ObligationElectricityAmount => (MemberPayloadCodecV1::I64, true),
            Self::ObligationMode => (MemberPayloadCodecV1::U64, true),
            Self::ObligationMaterialLedger => (MemberPayloadCodecV1::Ledger, true),
            Self::ObligationElectricityBefore => (MemberPayloadCodecV1::I64, true),
            Self::ObligationElectricityAfter => (MemberPayloadCodecV1::I64, true),
        }
    }
}
/// Ordered vectors preserve duplicate values and original positions. Optional
/// snapshot/map collections encode None as Unit and Some(empty) as U64(0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryCollectionV1 {
    SpecTags,
    SpecBuildCost,
    SnapshotConsume,
    SnapshotProduce,
    SnapshotByproducts,
    SnapshotBottleneckTags,
    SnapshotLogisticsRouteIds,
    SnapshotLogisticsPathIds,
    ObligationBalancesBefore,
    ObligationBalancesAfter,
}
impl FactoryCollectionV1 {
    pub(crate) fn path(self) -> (&'static str, &'static str) {
        match self {
            Self::SpecTags => ("spec", "tags"),
            Self::SpecBuildCost => ("spec", "build_cost"),
            Self::SnapshotConsume => ("last_completed_canonical_snapshot", "consume"),
            Self::SnapshotProduce => ("last_completed_canonical_snapshot", "produce"),
            Self::SnapshotByproducts => ("last_completed_canonical_snapshot", "byproducts"),
            Self::SnapshotBottleneckTags => {
                ("last_completed_canonical_snapshot", "bottleneck_tags")
            }
            Self::SnapshotLogisticsRouteIds => {
                ("last_completed_canonical_snapshot", "logistics_route_ids")
            }
            Self::SnapshotLogisticsPathIds => {
                ("last_completed_canonical_snapshot", "logistics_path_ids")
            }
            Self::ObligationBalancesBefore => {
                ("construction_power_obligation", "material_balances_before")
            }
            Self::ObligationBalancesAfter => {
                ("construction_power_obligation", "material_balances_after")
            }
        }
    }
    pub(crate) fn accepts(self, key: FactoryRecordKeyV1<'_>) -> bool {
        match self {
            Self::SpecTags | Self::SpecBuildCost => true,
            Self::ObligationBalancesBefore | Self::ObligationBalancesAfter => {
                matches!(key, FactoryRecordKeyV1::Build(_))
            }
            _ => matches!(key, FactoryRecordKeyV1::Factory(_)),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryStackColumnV1 {
    Kind,
    Amount,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryCellV1<'a> {
    Record,
    Column(FactoryColumnV1),
    Collection(FactoryCollectionV1),
    TextItem(FactoryCollectionV1, u64),
    StackItem(FactoryCollectionV1, u64, FactoryStackColumnV1),
    Balance(FactoryCollectionV1, &'a str),
}

impl FactoryCellV1<'_> {
    pub(crate) fn profile(self) -> (MemberPayloadCodecV1, bool) {
        use FactoryCollectionV1 as C;
        use MemberPayloadCodecV1 as P;
        match self {
            Self::Record => (P::Unit, false),
            Self::Column(c) => c.profile(),
            Self::Collection(c) => (P::U64, !matches!(c, C::SpecTags | C::SpecBuildCost)),
            Self::TextItem(..) | Self::StackItem(_, _, FactoryStackColumnV1::Kind) => {
                (P::Text, false)
            }
            Self::StackItem(_, _, FactoryStackColumnV1::Amount) | Self::Balance(..) => {
                (P::I64, false)
            }
        }
    }
    pub(crate) fn integer_max(self) -> u64 {
        use FactoryColumnV1 as C;
        match self {
            Self::Column(C::SpecTier | C::BuildContractVersion) => u8::MAX.into(),
            Self::Column(C::SpecRecipeSlots | C::ProductionActiveJobs) => u16::MAX.into(),
            Self::Column(
                C::SpecBuildTimeTicks | C::SpecThroughputBps | C::ProductionSameRecipeRepeatCount,
            ) => u32::MAX.into(),
            Self::Column(C::ProductionStatus) => 3,
            Self::Column(C::ObligationMode) => 0,
            _ => u64::MAX,
        }
    }
}

pub const FACTORY_STATE_SCHEMA_V1: [u8; 32] = *b"oasis7.factory.state.record.v1\0\0";
pub const FACTORY_BUILD_SCHEMA_V1: [u8; 32] = *b"oasis7.factory.build.record.v1\0\0";
pub(crate) const RECORD_FORMAT_KEY: &str = "factory_record_closure_v1";
impl FactoryRecordKeyV1<'_> {
    pub fn schema(self) -> [u8; 32] {
        match self {
            Self::Factory(_) => FACTORY_STATE_SCHEMA_V1,
            Self::Build(_) => FACTORY_BUILD_SCHEMA_V1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryRecordKindV1 {
    Factory,
    Build,
}
impl FactoryRecordKindV1 {
    pub(crate) fn sample(self) -> FactoryRecordKeyV1<'static> {
        match self {
            Self::Factory => FactoryRecordKeyV1::Factory(""),
            Self::Build => FactoryRecordKeyV1::Build(0),
        }
    }
    pub fn schema(self) -> [u8; 32] {
        self.sample().schema()
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        self.sample().namespace()
    }
    pub(crate) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        self.sample().descriptor()
    }
}
impl FactoryRecordKeyV1<'_> {
    pub fn kind(self) -> FactoryRecordKindV1 {
        match self {
            Self::Factory(_) => FactoryRecordKindV1::Factory,
            Self::Build(_) => FactoryRecordKindV1::Build,
        }
    }
}
impl FactoryCollectionV1 {
    pub(crate) const ALL: [Self; 10] = [
        Self::SpecTags,
        Self::SpecBuildCost,
        Self::SnapshotConsume,
        Self::SnapshotProduce,
        Self::SnapshotByproducts,
        Self::SnapshotBottleneckTags,
        Self::SnapshotLogisticsRouteIds,
        Self::SnapshotLogisticsPathIds,
        Self::ObligationBalancesBefore,
        Self::ObligationBalancesAfter,
    ];
    pub(crate) fn index(self) -> usize {
        match self {
            Self::SpecTags => 0,
            Self::SpecBuildCost => 1,
            Self::SnapshotConsume => 2,
            Self::SnapshotProduce => 3,
            Self::SnapshotByproducts => 4,
            Self::SnapshotBottleneckTags => 5,
            Self::SnapshotLogisticsRouteIds => 6,
            Self::SnapshotLogisticsPathIds => 7,
            Self::ObligationBalancesBefore => 8,
            Self::ObligationBalancesAfter => 9,
        }
    }
}
