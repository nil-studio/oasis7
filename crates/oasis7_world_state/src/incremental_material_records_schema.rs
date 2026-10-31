use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialRecordKindV1 {
    Profile,
    Transfer,
}
impl MaterialRecordKindV1 {
    pub const ALL: [Self; 2] = [Self::Profile, Self::Transfer];
    pub fn field_name(self) -> &'static str {
        match self {
            Self::Profile => "material_profiles",
            Self::Transfer => "direct_material_transfer_receipts",
        }
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::Profile => StateNamespaceV1::Business,
            Self::Transfer => StateNamespaceV1::Authorization,
        }
    }
    pub fn schema(self) -> [u8; 32] {
        match self {
            Self::Profile => *b"oasis7.material.profile.v1\0\0\0\0\0\0",
            Self::Transfer => *b"oasis7.material.transfer.v1\0\0\0\0\0",
        }
    }
    pub(crate) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|f| {
                f.owner == FieldOwner::WorldState
                    && f.name == self.field_name()
                    && f.shape == FieldShape::Map
                    && f.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(ValueTreeError::Invalid("material record registration"))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialRecordKeyV1<'a> {
    Profile(&'a str),
    Transfer(u64),
}
impl MaterialRecordKeyV1<'_> {
    pub fn kind(self) -> MaterialRecordKindV1 {
        match self {
            Self::Profile(_) => MaterialRecordKindV1::Profile,
            Self::Transfer(_) => MaterialRecordKindV1::Transfer,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialCellV1 {
    Record,
    Kind,
    Tier,
    Category,
    StackLimit,
    TransportLossClass,
    DecayBpsPerTick,
    DefaultPriority,
    TransferId,
    RequesterAgentId,
    FromLedger,
    ToLedger,
    Amount,
    DistanceKm,
    Priority,
    RoutePresent,
    RouteId,
}
impl MaterialCellV1 {
    pub const ALL: [Self; 17] = [
        Self::Record,
        Self::Kind,
        Self::Tier,
        Self::Category,
        Self::StackLimit,
        Self::TransportLossClass,
        Self::DecayBpsPerTick,
        Self::DefaultPriority,
        Self::TransferId,
        Self::RequesterAgentId,
        Self::FromLedger,
        Self::ToLedger,
        Self::Amount,
        Self::DistanceKm,
        Self::Priority,
        Self::RoutePresent,
        Self::RouteId,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Kind => "kind",
            Self::Tier => "tier",
            Self::Category => "category",
            Self::StackLimit => "stack_limit",
            Self::TransportLossClass => "transport_loss_class",
            Self::DecayBpsPerTick => "decay_bps_per_tick",
            Self::DefaultPriority => "default_priority",
            Self::TransferId => "transfer_id",
            Self::RequesterAgentId => "requester_agent_id",
            Self::FromLedger => "from_ledger",
            Self::ToLedger => "to_ledger",
            Self::Amount => "amount",
            Self::DistanceKm => "distance_km",
            Self::Priority => "priority",
            Self::RoutePresent => "route_present",
            Self::RouteId => "route_id",
        }
    }
    pub fn accepts(self, kind: MaterialRecordKindV1) -> bool {
        match self {
            Self::Record | Self::Kind => true,
            Self::Tier
            | Self::Category
            | Self::StackLimit
            | Self::TransportLossClass
            | Self::DecayBpsPerTick
            | Self::DefaultPriority => kind == MaterialRecordKindV1::Profile,
            _ => kind == MaterialRecordKindV1::Transfer,
        }
    }
    pub fn codec(self) -> MemberPayloadCodecV1 {
        use MemberPayloadCodecV1 as C;
        match self {
            Self::Record => C::Unit,
            Self::Kind | Self::Category | Self::RequesterAgentId | Self::RouteId => C::Text,
            Self::FromLedger | Self::ToLedger => C::Ledger,
            Self::StackLimit | Self::DecayBpsPerTick | Self::Amount | Self::DistanceKm => C::I64,
            Self::RoutePresent => C::Bool,
            _ => C::U64,
        }
    }
    pub fn integer_max(self) -> u64 {
        match self {
            Self::Tier => u8::MAX as u64,
            Self::TransportLossClass => 2,
            Self::DefaultPriority | Self::Priority => 1,
            _ => u64::MAX,
        }
    }
}
