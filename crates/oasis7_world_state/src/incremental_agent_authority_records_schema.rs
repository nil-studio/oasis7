//! Closed original World intent/location field schemas.
use super::*;
use crate::incremental_projection::{
    FieldDescriptor, FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS,
};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentAuthorityRecordKindV1 {
    IntentLedger,
    AgentLocation,
    LocationAnchor,
}
impl AgentAuthorityRecordKindV1 {
    pub const ALL: [Self; 3] = [
        Self::IntentLedger,
        Self::AgentLocation,
        Self::LocationAnchor,
    ];
    pub fn namespace(self) -> StateNamespaceV1 {
        match self {
            Self::IntentLedger => StateNamespaceV1::Authorization,
            Self::AgentLocation | Self::LocationAnchor => StateNamespaceV1::Business,
        }
    }
    pub fn field_name(self) -> &'static str {
        match self {
            Self::IntentLedger => "agent_intent_ledger",
            Self::AgentLocation => "agent_location_authorities",
            Self::LocationAnchor => "location_anchors",
        }
    }
    pub(crate) fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|field| {
                field.name == self.field_name()
                    && field.owner == FieldOwner::WorldState
                    && field.shape == FieldShape::Map
                    && field.destination == FieldDestination::State(self.namespace())
            })
            .ok_or(ValueTreeError::Invalid(
                "agent authority field registration",
            ))
    }
    pub fn schema(self) -> [u8; 32] {
        let text: &[u8] = match self {
            Self::IntentLedger => b"oasis7.agent.intent-ledger.v1",
            Self::AgentLocation => b"oasis7.agent.location.v1",
            Self::LocationAnchor => b"oasis7.location.anchor.v1",
        };
        let mut out = [0; 32];
        out[..text.len()].copy_from_slice(text);
        out
    }
    pub(crate) fn sample(self) -> AgentAuthorityRecordKeyV1<'static> {
        AgentAuthorityRecordKeyV1 { kind: self, id: "" }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct AgentAuthorityRecordKeyV1<'a> {
    pub kind: AgentAuthorityRecordKindV1,
    pub id: &'a str,
}
impl AgentAuthorityRecordKeyV1<'_> {
    pub fn namespace(self) -> StateNamespaceV1 {
        self.kind.namespace()
    }
    pub fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        self.kind.descriptor()
    }
    pub(crate) fn accepts(self, column: AgentAuthorityColumnV1) -> bool {
        column.accepts(self.kind)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentAuthorityColumnV1 {
    Intent(IntentFieldV1),
    AgentId,
    LocationId,
    Active,
    AuthorityRevision,
    EffectiveAt,
}
impl AgentAuthorityColumnV1 {
    pub const ALL: [Self; 27] = [
        Self::Intent(IntentFieldV1::SchemaVersion),
        Self::Intent(IntentFieldV1::AgentId),
        Self::Intent(IntentFieldV1::IntentId),
        Self::Intent(IntentFieldV1::Kind),
        Self::Intent(IntentFieldV1::Summary),
        Self::Intent(IntentFieldV1::TargetId),
        Self::Intent(IntentFieldV1::EffectIntentId),
        Self::Intent(IntentFieldV1::IntentTick),
        Self::Intent(IntentFieldV1::WorldId),
        Self::Intent(IntentFieldV1::ReorgEpoch),
        Self::Intent(IntentFieldV1::AuthorityScope),
        Self::Intent(IntentFieldV1::Status),
        Self::Intent(IntentFieldV1::Source),
        Self::Intent(IntentFieldV1::LogicalTime),
        Self::Intent(IntentFieldV1::EventSeq),
        Self::Intent(IntentFieldV1::UpdatedAt),
        Self::Intent(IntentFieldV1::ReceiptRef),
        Self::Intent(IntentFieldV1::ReasonCode),
        Self::Intent(IntentFieldV1::ReasonSummary),
        Self::Intent(IntentFieldV1::ReplacedBy),
        Self::Intent(IntentFieldV1::ActorId),
        Self::Intent(IntentFieldV1::RequestDigest),
        Self::AgentId,
        Self::LocationId,
        Self::Active,
        Self::AuthorityRevision,
        Self::EffectiveAt,
    ];
    pub fn path(self) -> (&'static str, &'static str) {
        (
            "record",
            match self {
                Self::Intent(f) => f.name(),
                Self::AgentId => "agent_id",
                Self::LocationId => "location_id",
                Self::Active => "active",
                Self::AuthorityRevision => "authority_revision",
                Self::EffectiveAt => "effective_at",
            },
        )
    }
    pub fn accepts(self, kind: AgentAuthorityRecordKindV1) -> bool {
        match self {
            Self::Intent(_) => kind == AgentAuthorityRecordKindV1::IntentLedger,
            Self::AgentId => kind == AgentAuthorityRecordKindV1::AgentLocation,
            _ => kind != AgentAuthorityRecordKindV1::IntentLedger,
        }
    }
    pub fn profile(self) -> (MemberPayloadCodecV1, bool) {
        use MemberPayloadCodecV1 as C;
        match self {
            Self::Intent(f) => match f {
                IntentFieldV1::SchemaVersion => (C::U64, f.is_optional()),
                IntentFieldV1::AgentId => (C::Text, f.is_optional()),
                IntentFieldV1::IntentId => (C::Text, f.is_optional()),
                IntentFieldV1::Kind => (C::Text, f.is_optional()),
                IntentFieldV1::Summary => (C::Text, f.is_optional()),
                IntentFieldV1::TargetId => (C::Text, f.is_optional()),
                IntentFieldV1::EffectIntentId => (C::Text, f.is_optional()),
                IntentFieldV1::IntentTick => (C::U64, f.is_optional()),
                IntentFieldV1::WorldId => (C::Text, f.is_optional()),
                IntentFieldV1::ReorgEpoch => (C::U64, f.is_optional()),
                IntentFieldV1::AuthorityScope => (C::Text, f.is_optional()),
                IntentFieldV1::Status => (C::Text, f.is_optional()),
                IntentFieldV1::Source => (C::Text, f.is_optional()),
                IntentFieldV1::LogicalTime => (C::U64, f.is_optional()),
                IntentFieldV1::EventSeq => (C::U64, f.is_optional()),
                IntentFieldV1::UpdatedAt => (C::U64, f.is_optional()),
                IntentFieldV1::ReceiptRef => (C::Text, f.is_optional()),
                IntentFieldV1::ReasonCode => (C::Text, f.is_optional()),
                IntentFieldV1::ReasonSummary => (C::Text, f.is_optional()),
                IntentFieldV1::ReplacedBy => (C::Text, f.is_optional()),
                IntentFieldV1::ActorId => (C::Text, f.is_optional()),
                IntentFieldV1::RequestDigest => (C::Text, f.is_optional()),
            },
            Self::Active => (C::Bool, false),
            Self::AuthorityRevision | Self::EffectiveAt => (C::U64, false),
            Self::AgentId | Self::LocationId => (C::Text, false),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentAuthorityCellV1 {
    Record,
    Column(AgentAuthorityColumnV1),
}
impl AgentAuthorityCellV1 {
    pub const ALL: [Self; 28] = [
        Self::Record,
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::SchemaVersion)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::AgentId)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::IntentId)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::Kind)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::Summary)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::TargetId)),
        Self::Column(AgentAuthorityColumnV1::Intent(
            IntentFieldV1::EffectIntentId,
        )),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::IntentTick)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::WorldId)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::ReorgEpoch)),
        Self::Column(AgentAuthorityColumnV1::Intent(
            IntentFieldV1::AuthorityScope,
        )),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::Status)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::Source)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::LogicalTime)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::EventSeq)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::UpdatedAt)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::ReceiptRef)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::ReasonCode)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::ReasonSummary)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::ReplacedBy)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::ActorId)),
        Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::RequestDigest)),
        Self::Column(AgentAuthorityColumnV1::AgentId),
        Self::Column(AgentAuthorityColumnV1::LocationId),
        Self::Column(AgentAuthorityColumnV1::Active),
        Self::Column(AgentAuthorityColumnV1::AuthorityRevision),
        Self::Column(AgentAuthorityColumnV1::EffectiveAt),
    ];
    pub(crate) fn accepts(self, kind: AgentAuthorityRecordKindV1) -> bool {
        match self {
            Self::Record => true,
            Self::Column(column) => kind.sample().accepts(column),
        }
    }
    pub fn profile(self) -> (MemberPayloadCodecV1, bool) {
        match self {
            Self::Record => (MemberPayloadCodecV1::Unit, false),
            Self::Column(c) => c.profile(),
        }
    }
    pub fn integer_max(self) -> u64 {
        if self == Self::Column(AgentAuthorityColumnV1::Intent(IntentFieldV1::SchemaVersion)) {
            u64::from(u32::MAX)
        } else {
            u64::MAX
        }
    }
}
