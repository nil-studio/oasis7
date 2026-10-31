//! Exact original logistics route/path storage profile, not business authority.
use crate::{incremental_field_member::*, incremental_projection::WORLD_STATE_FIELDS};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueTreeError,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogisticsRecordKindV1 {
    Route,
    Path,
}
impl LogisticsRecordKindV1 {
    pub const ALL: [Self; 2] = [Self::Route, Self::Path];
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::Route => StateNamespaceV1::Business,
            Self::Path => StateNamespaceV1::Authorization,
        }
    }
    pub fn field_name(self) -> &'static str {
        match self {
            Self::Route => "logistics_routes",
            Self::Path => "completed_logistics_paths",
        }
    }
    pub fn schema(self) -> [u8; 32] {
        let name: &[u8] = match self {
            Self::Route => b"oasis7.logistics.route.v1",
            Self::Path => b"oasis7.logistics.path.v1",
        };
        let mut out = [0; 32];
        out[..name.len()].copy_from_slice(name);
        out
    }
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Route => 0,
            Self::Path => 1,
        }
    }
    pub(crate) fn sample(self) -> LogisticsRecordKeyV1<'static> {
        LogisticsRecordKeyV1 { kind: self, id: "" }
    }
    pub(crate) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        self.sample().descriptor()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogisticsRecordKeyV1<'a> {
    pub kind: LogisticsRecordKindV1,
    pub id: &'a str,
}
impl LogisticsRecordKeyV1<'_> {
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
            .ok_or(ValueTreeError::Invalid("logistics field registration"))
    }
    pub(crate) fn accepts(self, column: LogisticsColumnV1) -> bool {
        column.accepts(self.kind)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogisticsColumnTypeV1 {
    Text,
    I64,
    Bool,
    Ledger,
    Priority,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogisticsColumnV1 {
    RouteId,
    PathId,
    FromLedger,
    ToLedger,
    Kind,
    DistanceKm,
    Priority,
    OwnerAgentId,
    Available,
    CapacityUnits,
    ReservedCapacityUnits,
    TariffElectricityPerUnit,
    SettledAmount,
    RemainingRecipeAmount,
}
impl LogisticsColumnV1 {
    pub const ALL: [Self; 14] = [
        Self::RouteId,
        Self::PathId,
        Self::FromLedger,
        Self::ToLedger,
        Self::Kind,
        Self::DistanceKm,
        Self::Priority,
        Self::OwnerAgentId,
        Self::Available,
        Self::CapacityUnits,
        Self::ReservedCapacityUnits,
        Self::TariffElectricityPerUnit,
        Self::SettledAmount,
        Self::RemainingRecipeAmount,
    ];
    pub fn path(self) -> (&'static str, &'static str) {
        (
            "record",
            match self {
                Self::RouteId => "route_id",
                Self::PathId => "path_id",
                Self::FromLedger => "from_ledger",
                Self::ToLedger => "to_ledger",
                Self::Kind => "kind",
                Self::DistanceKm => "distance_km",
                Self::Priority => "priority",
                Self::OwnerAgentId => "owner_agent_id",
                Self::Available => "available",
                Self::CapacityUnits => "capacity_units",
                Self::ReservedCapacityUnits => "reserved_capacity_units",
                Self::TariffElectricityPerUnit => "tariff_electricity_per_unit",
                Self::SettledAmount => "settled_amount",
                Self::RemainingRecipeAmount => "remaining_recipe_amount",
            },
        )
    }
    pub fn value_type(self) -> LogisticsColumnTypeV1 {
        match self {
            Self::RouteId => LogisticsColumnTypeV1::Text,
            Self::PathId => LogisticsColumnTypeV1::Text,
            Self::FromLedger => LogisticsColumnTypeV1::Ledger,
            Self::ToLedger => LogisticsColumnTypeV1::Ledger,
            Self::Kind => LogisticsColumnTypeV1::Text,
            Self::DistanceKm => LogisticsColumnTypeV1::I64,
            Self::Priority => LogisticsColumnTypeV1::Priority,
            Self::OwnerAgentId => LogisticsColumnTypeV1::Text,
            Self::Available => LogisticsColumnTypeV1::Bool,
            Self::CapacityUnits => LogisticsColumnTypeV1::I64,
            Self::ReservedCapacityUnits => LogisticsColumnTypeV1::I64,
            Self::TariffElectricityPerUnit => LogisticsColumnTypeV1::I64,
            Self::SettledAmount => LogisticsColumnTypeV1::I64,
            Self::RemainingRecipeAmount => LogisticsColumnTypeV1::I64,
        }
    }
    pub fn accepts(self, kind: LogisticsRecordKindV1) -> bool {
        match self {
            Self::RouteId => kind == LogisticsRecordKindV1::Route,
            Self::PathId => kind == LogisticsRecordKindV1::Path,
            Self::FromLedger => true,
            Self::ToLedger => true,
            Self::Kind => true,
            Self::DistanceKm => kind == LogisticsRecordKindV1::Route,
            Self::Priority => kind == LogisticsRecordKindV1::Route,
            Self::OwnerAgentId => kind == LogisticsRecordKindV1::Route,
            Self::Available => kind == LogisticsRecordKindV1::Route,
            Self::CapacityUnits => kind == LogisticsRecordKindV1::Route,
            Self::ReservedCapacityUnits => kind == LogisticsRecordKindV1::Route,
            Self::TariffElectricityPerUnit => kind == LogisticsRecordKindV1::Route,
            Self::SettledAmount => kind == LogisticsRecordKindV1::Path,
            Self::RemainingRecipeAmount => kind == LogisticsRecordKindV1::Path,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogisticsCollectionV1 {
    RouteIds,
}
impl LogisticsCollectionV1 {
    pub const ALL: [Self; 1] = [Self::RouteIds];
    pub fn name(self) -> &'static str {
        "route_ids"
    }
    pub fn accepts(self, kind: LogisticsRecordKindV1) -> bool {
        kind == LogisticsRecordKindV1::Path
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogisticsCellV1 {
    Record,
    Column(LogisticsColumnV1),
    Collection(LogisticsCollectionV1),
    TextItem(LogisticsCollectionV1, u64),
}
impl LogisticsCellV1 {
    pub(crate) fn integer_max(self) -> u64 {
        if self == Self::Column(LogisticsColumnV1::Priority) {
            1
        } else {
            u64::MAX
        }
    }
    pub(crate) fn profile(self) -> (MemberPayloadCodecV1, bool) {
        use LogisticsColumnTypeV1 as T;
        use MemberPayloadCodecV1 as P;
        (
            match self {
                Self::Record => P::Unit,
                Self::Collection(_) => P::U64,
                Self::TextItem(..) => P::Text,
                Self::Column(c) => match c.value_type() {
                    T::Text => P::Text,
                    T::I64 => P::I64,
                    T::Bool => P::Bool,
                    T::Ledger => P::Ledger,
                    T::Priority => P::U64,
                },
            },
            false,
        )
    }
}
