//! Closed original industry scheduler/terminal-receipt schema; not business authority.
use crate::{incremental_field_member::*, incremental_projection::WORLD_STATE_FIELDS};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndustryRecordKindV1 {
    Recipe,
    Transit,
    Completion,
    Failure,
    Settlement,
}
impl IndustryRecordKindV1 {
    pub const ALL: [Self; 5] = [
        Self::Recipe,
        Self::Transit,
        Self::Completion,
        Self::Failure,
        Self::Settlement,
    ];
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::Recipe => StateNamespaceV1::Scheduler,
            Self::Transit => StateNamespaceV1::Scheduler,
            Self::Completion => StateNamespaceV1::Authorization,
            Self::Failure => StateNamespaceV1::Authorization,
            Self::Settlement => StateNamespaceV1::Authorization,
        }
    }
    pub fn field_name(self) -> &'static str {
        match self {
            Self::Recipe => "pending_recipe_jobs",
            Self::Transit => "pending_material_transits",
            Self::Completion => "recipe_completion_receipts",
            Self::Failure => "factory_production_failure_dispositions",
            Self::Settlement => "logistics_settlement_receipts",
        }
    }
    pub fn schema(self) -> [u8; 32] {
        match self {
            Self::Recipe => *b"oasis7.industry.recipe.v1\0\0\0\0\0\0\0",
            Self::Transit => *b"oasis7.industry.transit.v1\0\0\0\0\0\0",
            Self::Completion => *b"oasis7.industry.completion.v1\0\0\0",
            Self::Failure => *b"oasis7.industry.failure.v1\0\0\0\0\0\0",
            Self::Settlement => *b"oasis7.industry.settlement.v1\0\0\0",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndustryRecordKeyV1 {
    pub kind: IndustryRecordKindV1,
    pub id: u64,
}
impl IndustryRecordKeyV1 {
    pub fn namespace(self) -> StateNamespaceV1 {
        self.kind.namespace()
    }
    pub(crate) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|f| {
                f.name == self.kind.field_name()
                    && f.owner == FieldOwner::WorldState
                    && f.shape == FieldShape::Map
                    && f.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(ValueTreeError::Invalid("industry field registration"))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndustrySectionV1 {
    Record,
    IndustrialRoot,
    Origin,
    Submission,
}
impl IndustrySectionV1 {
    pub fn name(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::IndustrialRoot => "industrial_root_ref",
            Self::Origin => "committed_recipe_origin",
            Self::Submission => "submission",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndustryColumnTypeV1 {
    Text,
    OptionalText,
    U64,
    U32,
    I64,
    OptionalU64,
    Bool,
    Ledger,
    Priority,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndustryColumnV1 {
    RecordIndustrialRootRef,
    RecordEmergencyPowerWorldBinding,
    RecordCommittedRecipeOrigin,
    RecordJobId,
    RecordRequesterAgentId,
    RecordFactoryId,
    RecordRecipeId,
    RecordAcceptedBatches,
    RecordPowerRequired,
    RecordPowerOwnerAgentId,
    RecordDurationTicks,
    RecordConsumeLedger,
    RecordOutputLedger,
    RecordReadyAt,
    RecordFromLedger,
    RecordToLedger,
    RecordKind,
    RecordAmount,
    RecordDistanceKm,
    RecordLossBps,
    RecordPriority,
    RecordRouteId,
    RecordPathId,
    RecordTariffElectricityTotal,
    RecordRerouteCount,
    RecordActionId,
    RecordBlockerKind,
    RecordBlockerDetail,
    RecordDispositionKind,
    RecordConsumedPower,
    RecordLostPower,
    RecordNextAction,
    RecordNextRecheck,
    RecordGovernanceTaxElectricity,
    IndustrialRootWorldBinding,
    IndustrialRootDomain,
    IndustrialRootSchema,
    IndustrialRootId,
    IndustrialRootRevision,
    IndustrialRootDigest,
    OriginConsensusActionId,
    OriginConsensusSubmitterPlayerId,
    OriginActionPayloadHash,
    OriginCommittedHeight,
    OriginActionRoot,
    SubmissionVerifiedPlayerId,
    SubmissionPublicKey,
    SubmissionAuthNonce,
    SubmissionHostedRegistrationNonce,
    SubmissionRequesterAgentId,
    SubmissionFactoryId,
    SubmissionRecipeId,
}
impl IndustryColumnV1 {
    pub const ALL: [Self; 52] = [
        Self::RecordIndustrialRootRef,
        Self::RecordEmergencyPowerWorldBinding,
        Self::RecordCommittedRecipeOrigin,
        Self::RecordJobId,
        Self::RecordRequesterAgentId,
        Self::RecordFactoryId,
        Self::RecordRecipeId,
        Self::RecordAcceptedBatches,
        Self::RecordPowerRequired,
        Self::RecordPowerOwnerAgentId,
        Self::RecordDurationTicks,
        Self::RecordConsumeLedger,
        Self::RecordOutputLedger,
        Self::RecordReadyAt,
        Self::RecordFromLedger,
        Self::RecordToLedger,
        Self::RecordKind,
        Self::RecordAmount,
        Self::RecordDistanceKm,
        Self::RecordLossBps,
        Self::RecordPriority,
        Self::RecordRouteId,
        Self::RecordPathId,
        Self::RecordTariffElectricityTotal,
        Self::RecordRerouteCount,
        Self::RecordActionId,
        Self::RecordBlockerKind,
        Self::RecordBlockerDetail,
        Self::RecordDispositionKind,
        Self::RecordConsumedPower,
        Self::RecordLostPower,
        Self::RecordNextAction,
        Self::RecordNextRecheck,
        Self::RecordGovernanceTaxElectricity,
        Self::IndustrialRootWorldBinding,
        Self::IndustrialRootDomain,
        Self::IndustrialRootSchema,
        Self::IndustrialRootId,
        Self::IndustrialRootRevision,
        Self::IndustrialRootDigest,
        Self::OriginConsensusActionId,
        Self::OriginConsensusSubmitterPlayerId,
        Self::OriginActionPayloadHash,
        Self::OriginCommittedHeight,
        Self::OriginActionRoot,
        Self::SubmissionVerifiedPlayerId,
        Self::SubmissionPublicKey,
        Self::SubmissionAuthNonce,
        Self::SubmissionHostedRegistrationNonce,
        Self::SubmissionRequesterAgentId,
        Self::SubmissionFactoryId,
        Self::SubmissionRecipeId,
    ];
    pub fn path(self) -> (IndustrySectionV1, &'static str) {
        match self {
            Self::RecordIndustrialRootRef => (IndustrySectionV1::Record, "industrial_root_ref"),
            Self::RecordEmergencyPowerWorldBinding => {
                (IndustrySectionV1::Record, "emergency_power_world_binding")
            }
            Self::RecordCommittedRecipeOrigin => {
                (IndustrySectionV1::Record, "committed_recipe_origin")
            }
            Self::RecordJobId => (IndustrySectionV1::Record, "job_id"),
            Self::RecordRequesterAgentId => (IndustrySectionV1::Record, "requester_agent_id"),
            Self::RecordFactoryId => (IndustrySectionV1::Record, "factory_id"),
            Self::RecordRecipeId => (IndustrySectionV1::Record, "recipe_id"),
            Self::RecordAcceptedBatches => (IndustrySectionV1::Record, "accepted_batches"),
            Self::RecordPowerRequired => (IndustrySectionV1::Record, "power_required"),
            Self::RecordPowerOwnerAgentId => (IndustrySectionV1::Record, "power_owner_agent_id"),
            Self::RecordDurationTicks => (IndustrySectionV1::Record, "duration_ticks"),
            Self::RecordConsumeLedger => (IndustrySectionV1::Record, "consume_ledger"),
            Self::RecordOutputLedger => (IndustrySectionV1::Record, "output_ledger"),
            Self::RecordReadyAt => (IndustrySectionV1::Record, "ready_at"),
            Self::RecordFromLedger => (IndustrySectionV1::Record, "from_ledger"),
            Self::RecordToLedger => (IndustrySectionV1::Record, "to_ledger"),
            Self::RecordKind => (IndustrySectionV1::Record, "kind"),
            Self::RecordAmount => (IndustrySectionV1::Record, "amount"),
            Self::RecordDistanceKm => (IndustrySectionV1::Record, "distance_km"),
            Self::RecordLossBps => (IndustrySectionV1::Record, "loss_bps"),
            Self::RecordPriority => (IndustrySectionV1::Record, "priority"),
            Self::RecordRouteId => (IndustrySectionV1::Record, "route_id"),
            Self::RecordPathId => (IndustrySectionV1::Record, "path_id"),
            Self::RecordTariffElectricityTotal => {
                (IndustrySectionV1::Record, "tariff_electricity_total")
            }
            Self::RecordRerouteCount => (IndustrySectionV1::Record, "reroute_count"),
            Self::RecordActionId => (IndustrySectionV1::Record, "action_id"),
            Self::RecordBlockerKind => (IndustrySectionV1::Record, "blocker_kind"),
            Self::RecordBlockerDetail => (IndustrySectionV1::Record, "blocker_detail"),
            Self::RecordDispositionKind => (IndustrySectionV1::Record, "disposition_kind"),
            Self::RecordConsumedPower => (IndustrySectionV1::Record, "consumed_power"),
            Self::RecordLostPower => (IndustrySectionV1::Record, "lost_power"),
            Self::RecordNextAction => (IndustrySectionV1::Record, "next_action"),
            Self::RecordNextRecheck => (IndustrySectionV1::Record, "next_recheck"),
            Self::RecordGovernanceTaxElectricity => {
                (IndustrySectionV1::Record, "governance_tax_electricity")
            }
            Self::IndustrialRootWorldBinding => {
                (IndustrySectionV1::IndustrialRoot, "world_binding")
            }
            Self::IndustrialRootDomain => (IndustrySectionV1::IndustrialRoot, "domain"),
            Self::IndustrialRootSchema => (IndustrySectionV1::IndustrialRoot, "schema"),
            Self::IndustrialRootId => (IndustrySectionV1::IndustrialRoot, "id"),
            Self::IndustrialRootRevision => (IndustrySectionV1::IndustrialRoot, "revision"),
            Self::IndustrialRootDigest => (IndustrySectionV1::IndustrialRoot, "digest"),
            Self::OriginConsensusActionId => (IndustrySectionV1::Origin, "consensus_action_id"),
            Self::OriginConsensusSubmitterPlayerId => {
                (IndustrySectionV1::Origin, "consensus_submitter_player_id")
            }
            Self::OriginActionPayloadHash => (IndustrySectionV1::Origin, "action_payload_hash"),
            Self::OriginCommittedHeight => (IndustrySectionV1::Origin, "committed_height"),
            Self::OriginActionRoot => (IndustrySectionV1::Origin, "action_root"),
            Self::SubmissionVerifiedPlayerId => {
                (IndustrySectionV1::Submission, "verified_player_id")
            }
            Self::SubmissionPublicKey => (IndustrySectionV1::Submission, "public_key"),
            Self::SubmissionAuthNonce => (IndustrySectionV1::Submission, "auth_nonce"),
            Self::SubmissionHostedRegistrationNonce => {
                (IndustrySectionV1::Submission, "hosted_registration_nonce")
            }
            Self::SubmissionRequesterAgentId => {
                (IndustrySectionV1::Submission, "requester_agent_id")
            }
            Self::SubmissionFactoryId => (IndustrySectionV1::Submission, "factory_id"),
            Self::SubmissionRecipeId => (IndustrySectionV1::Submission, "recipe_id"),
        }
    }
    pub fn accepts(self, kind: IndustryRecordKindV1) -> bool {
        match self {
            Self::RecordIndustrialRootRef => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::RecordEmergencyPowerWorldBinding => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::RecordCommittedRecipeOrigin => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::RecordJobId => matches!(
                kind,
                IndustryRecordKindV1::Recipe
                    | IndustryRecordKindV1::Transit
                    | IndustryRecordKindV1::Completion
                    | IndustryRecordKindV1::Settlement
            ),
            Self::RecordRequesterAgentId => matches!(
                kind,
                IndustryRecordKindV1::Recipe
                    | IndustryRecordKindV1::Transit
                    | IndustryRecordKindV1::Completion
                    | IndustryRecordKindV1::Failure
            ),
            Self::RecordFactoryId => matches!(
                kind,
                IndustryRecordKindV1::Recipe
                    | IndustryRecordKindV1::Completion
                    | IndustryRecordKindV1::Failure
            ),
            Self::RecordRecipeId => matches!(
                kind,
                IndustryRecordKindV1::Recipe
                    | IndustryRecordKindV1::Completion
                    | IndustryRecordKindV1::Failure
            ),
            Self::RecordAcceptedBatches => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::RecordPowerRequired => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::RecordPowerOwnerAgentId => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::RecordDurationTicks => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::RecordConsumeLedger => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::RecordOutputLedger => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::RecordReadyAt => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Transit
            ),
            Self::RecordFromLedger => matches!(kind, IndustryRecordKindV1::Transit),
            Self::RecordToLedger => matches!(kind, IndustryRecordKindV1::Transit),
            Self::RecordKind => matches!(kind, IndustryRecordKindV1::Transit),
            Self::RecordAmount => matches!(kind, IndustryRecordKindV1::Transit),
            Self::RecordDistanceKm => matches!(kind, IndustryRecordKindV1::Transit),
            Self::RecordLossBps => matches!(kind, IndustryRecordKindV1::Transit),
            Self::RecordPriority => matches!(kind, IndustryRecordKindV1::Transit),
            Self::RecordRouteId => matches!(kind, IndustryRecordKindV1::Transit),
            Self::RecordPathId => matches!(
                kind,
                IndustryRecordKindV1::Transit | IndustryRecordKindV1::Settlement
            ),
            Self::RecordTariffElectricityTotal => matches!(
                kind,
                IndustryRecordKindV1::Transit | IndustryRecordKindV1::Settlement
            ),
            Self::RecordRerouteCount => matches!(kind, IndustryRecordKindV1::Transit),
            Self::RecordActionId => matches!(kind, IndustryRecordKindV1::Failure),
            Self::RecordBlockerKind => matches!(kind, IndustryRecordKindV1::Failure),
            Self::RecordBlockerDetail => matches!(kind, IndustryRecordKindV1::Failure),
            Self::RecordDispositionKind => matches!(kind, IndustryRecordKindV1::Failure),
            Self::RecordConsumedPower => matches!(kind, IndustryRecordKindV1::Failure),
            Self::RecordLostPower => matches!(kind, IndustryRecordKindV1::Failure),
            Self::RecordNextAction => matches!(kind, IndustryRecordKindV1::Failure),
            Self::RecordNextRecheck => matches!(kind, IndustryRecordKindV1::Failure),
            Self::RecordGovernanceTaxElectricity => {
                matches!(kind, IndustryRecordKindV1::Settlement)
            }
            Self::IndustrialRootWorldBinding => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::IndustrialRootDomain => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::IndustrialRootSchema => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::IndustrialRootId => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::IndustrialRootRevision => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::IndustrialRootDigest => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::OriginConsensusActionId => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::OriginConsensusSubmitterPlayerId => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::OriginActionPayloadHash => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::OriginCommittedHeight => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::OriginActionRoot => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::SubmissionVerifiedPlayerId => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::SubmissionPublicKey => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::SubmissionAuthNonce => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::SubmissionHostedRegistrationNonce => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::SubmissionRequesterAgentId => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::SubmissionFactoryId => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::SubmissionRecipeId => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
        }
    }
    pub fn value_type(self) -> IndustryColumnTypeV1 {
        match self {
            Self::RecordIndustrialRootRef => IndustryColumnTypeV1::Bool,
            Self::RecordEmergencyPowerWorldBinding => IndustryColumnTypeV1::OptionalText,
            Self::RecordCommittedRecipeOrigin => IndustryColumnTypeV1::Bool,
            Self::RecordJobId => IndustryColumnTypeV1::U64,
            Self::RecordRequesterAgentId => IndustryColumnTypeV1::Text,
            Self::RecordFactoryId => IndustryColumnTypeV1::Text,
            Self::RecordRecipeId => IndustryColumnTypeV1::Text,
            Self::RecordAcceptedBatches => IndustryColumnTypeV1::U32,
            Self::RecordPowerRequired => IndustryColumnTypeV1::I64,
            Self::RecordPowerOwnerAgentId => IndustryColumnTypeV1::OptionalText,
            Self::RecordDurationTicks => IndustryColumnTypeV1::U32,
            Self::RecordConsumeLedger => IndustryColumnTypeV1::Ledger,
            Self::RecordOutputLedger => IndustryColumnTypeV1::Ledger,
            Self::RecordReadyAt => IndustryColumnTypeV1::U64,
            Self::RecordFromLedger => IndustryColumnTypeV1::Ledger,
            Self::RecordToLedger => IndustryColumnTypeV1::Ledger,
            Self::RecordKind => IndustryColumnTypeV1::Text,
            Self::RecordAmount => IndustryColumnTypeV1::I64,
            Self::RecordDistanceKm => IndustryColumnTypeV1::I64,
            Self::RecordLossBps => IndustryColumnTypeV1::I64,
            Self::RecordPriority => IndustryColumnTypeV1::Priority,
            Self::RecordRouteId => IndustryColumnTypeV1::OptionalText,
            Self::RecordPathId => IndustryColumnTypeV1::OptionalText,
            Self::RecordTariffElectricityTotal => IndustryColumnTypeV1::I64,
            Self::RecordRerouteCount => IndustryColumnTypeV1::U32,
            Self::RecordActionId => IndustryColumnTypeV1::U64,
            Self::RecordBlockerKind => IndustryColumnTypeV1::Text,
            Self::RecordBlockerDetail => IndustryColumnTypeV1::Text,
            Self::RecordDispositionKind => IndustryColumnTypeV1::Text,
            Self::RecordConsumedPower => IndustryColumnTypeV1::I64,
            Self::RecordLostPower => IndustryColumnTypeV1::I64,
            Self::RecordNextAction => IndustryColumnTypeV1::Text,
            Self::RecordNextRecheck => IndustryColumnTypeV1::OptionalU64,
            Self::RecordGovernanceTaxElectricity => IndustryColumnTypeV1::I64,
            Self::IndustrialRootWorldBinding => IndustryColumnTypeV1::Text,
            Self::IndustrialRootDomain => IndustryColumnTypeV1::Text,
            Self::IndustrialRootSchema => IndustryColumnTypeV1::Text,
            Self::IndustrialRootId => IndustryColumnTypeV1::Text,
            Self::IndustrialRootRevision => IndustryColumnTypeV1::U64,
            Self::IndustrialRootDigest => IndustryColumnTypeV1::Text,
            Self::OriginConsensusActionId => IndustryColumnTypeV1::U64,
            Self::OriginConsensusSubmitterPlayerId => IndustryColumnTypeV1::Text,
            Self::OriginActionPayloadHash => IndustryColumnTypeV1::Text,
            Self::OriginCommittedHeight => IndustryColumnTypeV1::U64,
            Self::OriginActionRoot => IndustryColumnTypeV1::Text,
            Self::SubmissionVerifiedPlayerId => IndustryColumnTypeV1::Text,
            Self::SubmissionPublicKey => IndustryColumnTypeV1::Text,
            Self::SubmissionAuthNonce => IndustryColumnTypeV1::U64,
            Self::SubmissionHostedRegistrationNonce => IndustryColumnTypeV1::OptionalText,
            Self::SubmissionRequesterAgentId => IndustryColumnTypeV1::Text,
            Self::SubmissionFactoryId => IndustryColumnTypeV1::Text,
            Self::SubmissionRecipeId => IndustryColumnTypeV1::Text,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndustryCollectionTypeV1 {
    Text,
    Stack,
    Balance,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndustryCollectionV1 {
    Consume,
    Produce,
    Byproducts,
    BottleneckTags,
    LogisticsRouteIds,
    LogisticsPathIds,
    RouteIds,
    ConsumedInputs,
    LostInputs,
    OwnerPayouts,
}
impl IndustryCollectionV1 {
    pub const ALL: [Self; 10] = [
        Self::Consume,
        Self::Produce,
        Self::Byproducts,
        Self::BottleneckTags,
        Self::LogisticsRouteIds,
        Self::LogisticsPathIds,
        Self::RouteIds,
        Self::ConsumedInputs,
        Self::LostInputs,
        Self::OwnerPayouts,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Consume => "consume",
            Self::Produce => "produce",
            Self::Byproducts => "byproducts",
            Self::BottleneckTags => "bottleneck_tags",
            Self::LogisticsRouteIds => "logistics_route_ids",
            Self::LogisticsPathIds => "logistics_path_ids",
            Self::RouteIds => "route_ids",
            Self::ConsumedInputs => "consumed_inputs",
            Self::LostInputs => "lost_inputs",
            Self::OwnerPayouts => "owner_payouts",
        }
    }
    pub fn accepts(self, kind: IndustryRecordKindV1) -> bool {
        match self {
            Self::Consume => matches!(kind, IndustryRecordKindV1::Recipe),
            Self::Produce => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::Byproducts => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::BottleneckTags => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::LogisticsRouteIds => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::LogisticsPathIds => matches!(
                kind,
                IndustryRecordKindV1::Recipe | IndustryRecordKindV1::Completion
            ),
            Self::RouteIds => matches!(
                kind,
                IndustryRecordKindV1::Transit | IndustryRecordKindV1::Settlement
            ),
            Self::ConsumedInputs => matches!(kind, IndustryRecordKindV1::Failure),
            Self::LostInputs => matches!(kind, IndustryRecordKindV1::Failure),
            Self::OwnerPayouts => matches!(kind, IndustryRecordKindV1::Settlement),
        }
    }
    pub fn member_type(self) -> IndustryCollectionTypeV1 {
        match self {
            Self::Consume => IndustryCollectionTypeV1::Stack,
            Self::Produce => IndustryCollectionTypeV1::Stack,
            Self::Byproducts => IndustryCollectionTypeV1::Stack,
            Self::BottleneckTags => IndustryCollectionTypeV1::Text,
            Self::LogisticsRouteIds => IndustryCollectionTypeV1::Text,
            Self::LogisticsPathIds => IndustryCollectionTypeV1::Text,
            Self::RouteIds => IndustryCollectionTypeV1::Text,
            Self::ConsumedInputs => IndustryCollectionTypeV1::Stack,
            Self::LostInputs => IndustryCollectionTypeV1::Stack,
            Self::OwnerPayouts => IndustryCollectionTypeV1::Balance,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndustryStackColumnV1 {
    Kind,
    Amount,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndustryCellV1<'a> {
    Record,
    Column(IndustryColumnV1),
    Collection(IndustryCollectionV1),
    TextItem(IndustryCollectionV1, u64),
    StackItem(IndustryCollectionV1, u64, IndustryStackColumnV1),
    Balance(IndustryCollectionV1, &'a str),
}

impl IndustryCellV1<'_> {
    pub(crate) fn profile(self) -> (crate::incremental_field_member::MemberPayloadCodecV1, bool) {
        use crate::incremental_field_member::MemberPayloadCodecV1 as C;
        use IndustryColumnTypeV1 as T;
        match self {
            Self::Record => (C::Unit, false),
            Self::Collection(_) => (C::U64, false),
            Self::TextItem(_, _) | Self::StackItem(_, _, IndustryStackColumnV1::Kind) => {
                (C::Text, false)
            }
            Self::StackItem(_, _, IndustryStackColumnV1::Amount) | Self::Balance(_, _) => {
                (C::I64, false)
            }
            Self::Column(c) => {
                let (codec, optional) = match c.value_type() {
                    T::Text => (C::Text, false),
                    T::OptionalText => (C::Text, true),
                    T::U64 | T::U32 | T::Priority => (C::U64, false),
                    T::OptionalU64 => (C::U64, true),
                    T::I64 => (C::I64, false),
                    T::Bool => (C::Bool, false),
                    T::Ledger => (C::Ledger, false),
                };
                (codec, optional || c.path().0 != IndustrySectionV1::Record)
            }
        }
    }
    pub(crate) fn integer_max(self) -> u64 {
        match self {
            Self::Column(c) => match c.value_type() {
                IndustryColumnTypeV1::U32 => u32::MAX as u64,
                IndustryColumnTypeV1::Priority => 1,
                _ => u64::MAX,
            },
            _ => u64::MAX,
        }
    }
}

impl IndustryRecordKindV1 {
    pub(crate) fn sample(self) -> IndustryRecordKeyV1 {
        IndustryRecordKeyV1 { kind: self, id: 0 }
    }
    pub(crate) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        self.sample().descriptor()
    }
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Recipe => 0,
            Self::Transit => 1,
            Self::Completion => 2,
            Self::Failure => 3,
            Self::Settlement => 4,
        }
    }
}
impl IndustryRecordKeyV1 {
    pub(crate) fn accepts(self, column: IndustryColumnV1) -> bool {
        column.accepts(self.kind)
    }
}
impl IndustryCollectionV1 {
    pub(crate) fn path(self) -> (&'static str, &'static str) {
        ("record", self.name())
    }
    pub(crate) fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|c| *c == self)
            .expect("closed industry collection")
    }
}
