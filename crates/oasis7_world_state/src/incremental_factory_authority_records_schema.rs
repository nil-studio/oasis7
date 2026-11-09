use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryAuthorityRecordKindV1 {
    Site,
    Power,
}
impl FactoryAuthorityRecordKindV1 {
    pub const ALL: [Self; 2] = [Self::Site, Self::Power];
    pub fn field_name(self) -> &'static str {
        match self {
            Self::Site => "factory_site_authorities",
            Self::Power => "factory_construction_power_profiles",
        }
    }
    pub fn namespace(self) -> StateNamespaceV1 {
        StateNamespaceV1::Business
    }
    pub fn schema(self) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash.update(b"oasis7.iws.factory-authority.v1\0");
        hash.update(self.field_name().as_bytes());
        *hash.finalize().as_bytes()
    }
    pub(super) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|f| {
                f.owner == FieldOwner::WorldState
                    && f.name == self.field_name()
                    && f.shape == FieldShape::Map
                    && f.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(ValueTreeError::Invalid("factory authority registration"))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactoryAuthorityRecordKeyV1<'a> {
    pub kind: FactoryAuthorityRecordKindV1,
    pub id: &'a str,
}
impl FactoryAuthorityRecordKeyV1<'_> {
    pub fn kind(self) -> FactoryAuthorityRecordKindV1 {
        self.kind
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryAuthorityCellV1 {
    Record,
    SiteId,
    LocationId,
    OwnerAgentId,
    AuthorizedCount,
    AuthorizedAgent(u64),
    ChunkReady,
    Active,
    AuthorityRevision,
    RegisteredAt,
    FactoryId,
    FactoryKind,
    SourceModulePresent,
    SourceModuleId,
    ElectricityAmount,
    Mode,
}
impl FactoryAuthorityCellV1 {
    pub const FIXED: [Self; 15] = [
        Self::Record,
        Self::SiteId,
        Self::LocationId,
        Self::OwnerAgentId,
        Self::AuthorizedCount,
        Self::ChunkReady,
        Self::Active,
        Self::AuthorityRevision,
        Self::RegisteredAt,
        Self::FactoryId,
        Self::FactoryKind,
        Self::SourceModulePresent,
        Self::SourceModuleId,
        Self::ElectricityAmount,
        Self::Mode,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::SiteId => "site_id",
            Self::LocationId => "location_id",
            Self::OwnerAgentId => "owner_agent_id",
            Self::AuthorizedCount => "authorized_agent_count",
            Self::AuthorizedAgent(_) => "authorized_agent_ids",
            Self::ChunkReady => "chunk_ready",
            Self::Active => "active",
            Self::AuthorityRevision => "authority_revision",
            Self::RegisteredAt => "registered_at",
            Self::FactoryId => "factory_id",
            Self::FactoryKind => "factory_kind",
            Self::SourceModulePresent => "source_module_present",
            Self::SourceModuleId => "source_module_id",
            Self::ElectricityAmount => "electricity_amount",
            Self::Mode => "mode",
        }
    }
    pub fn accepts(self, kind: FactoryAuthorityRecordKindV1) -> bool {
        match self {
            Self::Record | Self::Active | Self::AuthorityRevision => true,
            Self::SiteId
            | Self::LocationId
            | Self::OwnerAgentId
            | Self::AuthorizedCount
            | Self::AuthorizedAgent(_)
            | Self::ChunkReady
            | Self::RegisteredAt => kind == FactoryAuthorityRecordKindV1::Site,
            _ => kind == FactoryAuthorityRecordKindV1::Power,
        }
    }
    pub fn codec(self) -> MemberPayloadCodecV1 {
        use MemberPayloadCodecV1 as C;
        match self {
            Self::Record => C::Unit,
            Self::Active | Self::ChunkReady | Self::SourceModulePresent => C::Bool,
            Self::AuthorizedCount | Self::AuthorityRevision | Self::RegisteredAt | Self::Mode => {
                C::U64
            }
            Self::ElectricityAmount => C::I64,
            _ => C::Text,
        }
    }
    pub fn integer_max(self) -> u64 {
        if self == Self::Mode { 0 } else { u64::MAX }
    }
}
