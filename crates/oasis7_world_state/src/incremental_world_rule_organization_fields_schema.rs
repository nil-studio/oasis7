//! Closed original scalar fields; collection metadata and Vec length are absent.
//! Draft consumer schema, registered only with its complete reader/writer.
use super::*;
use crate::{
    incremental_world_rule_organization::Kind,
    world_rules::{
        OrganizationDispositionPhaseV1 as Disposition, OrganizationProtectionPhaseV1 as Phase,
    },
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrganizationScalarFieldV1 {
    Id,
    OrganizationId,
    FounderId,
    PolicyVersion,
    Revision,
    Phase,
    Disposition,
    NotifiedAt,
    ProtectedAt,
    ContributorId,
    Material,
    Amount,
    HeldInTrust,
    Disposed,
    EffectReceipt,
    DelegateId,
    ExpiresAt,
    SourceProposal,
    Replaces,
    RequesterId,
    Appeal,
    Approved,
    ResolvedBy,
}
impl OrganizationScalarFieldV1 {
    pub fn name(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::OrganizationId => "organization_id",
            Self::FounderId => "founder_id",
            Self::PolicyVersion => "policy_version",
            Self::Revision => "revision",
            Self::Phase => "phase",
            Self::Disposition => "disposition",
            Self::NotifiedAt => "notified_at",
            Self::ProtectedAt => "protected_at",
            Self::ContributorId => "contributor_id",
            Self::Material => "material",
            Self::Amount => "amount",
            Self::HeldInTrust => "held_in_trust",
            Self::Disposed => "disposed",
            Self::EffectReceipt => "effect_receipt",
            Self::DelegateId => "delegate_id",
            Self::ExpiresAt => "expires_at",
            Self::SourceProposal => "source_proposal",
            Self::Replaces => "replaces",
            Self::RequesterId => "requester_id",
            Self::Appeal => "appeal",
            Self::Approved => "approved",
            Self::ResolvedBy => "resolved_by",
        }
    }
    pub(super) fn span(
        self,
        record: OrganizationRelativeRecordV1<'_>,
    ) -> Result<Span, ValueTreeError> {
        let specs = closure::profile_of(record).specs();
        for (slot, spec) in specs.iter().enumerate() {
            if !spec.scalar {
                continue;
            }
            match spec.path {
                RecordPathV1::Field(name) if name == self.name() => {
                    return Ok(Span {
                        slot,
                        count: 1,
                        kind: spec.kind,
                    });
                }
                RecordPathV1::FieldOptionPresence(name) if name == self.name() => {
                    let value = specs
                        .get(slot + 1)
                        .ok_or(ValueTreeError::Invalid("organization option schema"))?;
                    if value.flag != Some(slot) {
                        return Err(ValueTreeError::Invalid("organization option schema"));
                    }
                    return Ok(Span {
                        slot,
                        count: 2,
                        kind: value.kind,
                    });
                }
                _ => {}
            }
        }
        Err(ValueTreeError::Invalid(
            "organization scalar field/record profile",
        ))
    }
}
pub(super) struct Span {
    pub slot: usize,
    pub count: usize,
    pub kind: Kind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrganizationFieldRefV1<'a> {
    Text(&'a str),
    U32(u32),
    U64(u64),
    I64(i64),
    Bool(bool),
    Phase(Phase),
    Disposition(Disposition),
    OptionalU64(Option<u64>),
    OptionalText(Option<&'a str>),
    OptionalBool(Option<bool>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrganizationFieldValueV1 {
    Text { utf8_bytes: u64 },
    U32(u32),
    U64(u64),
    I64(i64),
    Bool(bool),
    Phase(Phase),
    Disposition(Disposition),
    OptionalU64(Option<u64>),
    OptionalText { utf8_bytes: Option<u64> },
    OptionalBool(Option<bool>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrganizationFieldReadV1 {
    RecordAbsent,
    Value(OrganizationFieldValueV1),
}
fn phase_label(value: Phase) -> &'static str {
    match value {
        Phase::Active => "active",
        Phase::Notice => "notice",
        Phase::Protected => "protected",
        Phase::Limited => "limited",
        Phase::Resolved => "resolved",
    }
}
fn disposition_label(value: Disposition) -> &'static str {
    match value {
        Disposition::None => "none",
        Disposition::Frozen => "frozen",
        Disposition::Obligations => "obligations",
        Disposition::Business => "business",
        Disposition::Residual => "residual",
        Disposition::Complete => "complete",
    }
}
impl<'a> OrganizationFieldRefV1<'a> {
    pub(super) fn payloads(
        self,
        span: &Span,
    ) -> Result<[Option<MemberPayloadRefV1<'a>>; 2], ValueTreeError> {
        use MemberPayloadRefV1 as P;
        let bad = || ValueTreeError::Invalid("organization scalar value type");
        if span.count == 2 {
            let value = match (span.kind, self) {
                (Kind::Text, Self::OptionalText(value)) => value.map(P::Text),
                (Kind::U64, Self::OptionalU64(value)) => value.map(P::U64),
                (Kind::Bool, Self::OptionalBool(value)) => value.map(P::Bool),
                _ => return Err(bad()),
            };
            return Ok([Some(P::Bool(value.is_some())), value]);
        }
        let value = match (span.kind, self) {
            (Kind::Text, Self::Text(value)) => P::Text(value),
            (Kind::U32, Self::U32(value)) => P::U64(u64::from(value)),
            (Kind::U64, Self::U64(value)) => P::U64(value),
            (Kind::I64, Self::I64(value)) => P::I64(value),
            (Kind::Bool, Self::Bool(value)) => P::Bool(value),
            (Kind::Phase, Self::Phase(value)) => P::Text(phase_label(value)),
            (Kind::Disposition, Self::Disposition(value)) => P::Text(disposition_label(value)),
            _ => return Err(bad()),
        };
        Ok([Some(value), None])
    }
    pub(super) fn text(self) -> Option<&'a str> {
        match self {
            Self::Text(value) | Self::OptionalText(Some(value)) => Some(value),
            _ => None,
        }
    }
    pub(super) fn value(self) -> OrganizationFieldValueV1 {
        use OrganizationFieldValueV1 as V;
        match self {
            Self::Text(value) => V::Text {
                utf8_bytes: value.len() as u64,
            },
            Self::U32(value) => V::U32(value),
            Self::U64(value) => V::U64(value),
            Self::I64(value) => V::I64(value),
            Self::Bool(value) => V::Bool(value),
            Self::Phase(value) => V::Phase(value),
            Self::Disposition(value) => V::Disposition(value),
            Self::OptionalU64(value) => V::OptionalU64(value),
            Self::OptionalBool(value) => V::OptionalBool(value),
            Self::OptionalText(value) => V::OptionalText {
                utf8_bytes: value.map(|value| value.len() as u64),
            },
        }
    }
}
