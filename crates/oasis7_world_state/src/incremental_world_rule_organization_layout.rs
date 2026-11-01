use super::*;
pub(super) const MAX: usize = 17;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Profile {
    Case,
    Estate,
    Grant,
    Recovery,
}
#[derive(Clone, Copy)]
pub(super) enum Kind {
    Unit,
    Text,
    U64,
    U32,
    I64,
    Bool,
    Phase,
    Disposition,
}
#[derive(Clone, Copy)]
pub(super) struct Spec {
    pub path: RecordPathV1<'static>,
    pub kind: Kind,
    pub flag: Option<usize>,
    pub scalar: bool,
}
const CASE: [Spec; 17] = [
    Spec {
        path: RecordPathV1::Record,
        kind: Kind::Unit,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("id"),
        kind: Kind::Text,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("organization_id"),
        kind: Kind::Text,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("founder_id"),
        kind: Kind::Text,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("policy_version"),
        kind: Kind::U32,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("revision"),
        kind: Kind::U64,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("phase"),
        kind: Kind::Phase,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("disposition"),
        kind: Kind::Disposition,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionPresence("notified_at"),
        kind: Kind::Bool,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionValue("notified_at"),
        kind: Kind::U64,
        flag: Some(8),
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionPresence("protected_at"),
        kind: Kind::Bool,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionValue("protected_at"),
        kind: Kind::U64,
        flag: Some(10),
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Collection("members"),
        kind: Kind::Unit,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Collection("grants"),
        kind: Kind::Unit,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Collection("requests"),
        kind: Kind::Unit,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Collection("stage_receipts"),
        kind: Kind::Unit,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Nested("stage_receipts", "length"),
        kind: Kind::U64,
        flag: None,
        scalar: false,
    },
];
const ESTATE: [Spec; 10] = [
    Spec {
        path: RecordPathV1::Record,
        kind: Kind::Unit,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("id"),
        kind: Kind::Text,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("organization_id"),
        kind: Kind::Text,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("contributor_id"),
        kind: Kind::Text,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("material"),
        kind: Kind::Text,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("amount"),
        kind: Kind::I64,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("held_in_trust"),
        kind: Kind::Bool,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("disposed"),
        kind: Kind::Bool,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionPresence("effect_receipt"),
        kind: Kind::Bool,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionValue("effect_receipt"),
        kind: Kind::Text,
        flag: Some(8),
        scalar: true,
    },
];
const GRANT: [Spec; 9] = [
    Spec {
        path: RecordPathV1::Record,
        kind: Kind::Unit,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("id"),
        kind: Kind::Text,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("delegate_id"),
        kind: Kind::Text,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("revision"),
        kind: Kind::U64,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("expires_at"),
        kind: Kind::U64,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("source_proposal"),
        kind: Kind::Text,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionPresence("replaces"),
        kind: Kind::Bool,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionValue("replaces"),
        kind: Kind::Text,
        flag: Some(6),
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Collection("scopes"),
        kind: Kind::Unit,
        flag: None,
        scalar: true,
    },
];
const RECOVERY: [Spec; 7] = [
    Spec {
        path: RecordPathV1::Record,
        kind: Kind::Unit,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("requester_id"),
        kind: Kind::Text,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::Field("appeal"),
        kind: Kind::Bool,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionPresence("approved"),
        kind: Kind::Bool,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionValue("approved"),
        kind: Kind::Bool,
        flag: Some(3),
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionPresence("resolved_by"),
        kind: Kind::Bool,
        flag: None,
        scalar: true,
    },
    Spec {
        path: RecordPathV1::FieldOptionValue("resolved_by"),
        kind: Kind::Text,
        flag: Some(5),
        scalar: true,
    },
];
impl Profile {
    pub(super) fn specs(self) -> &'static [Spec] {
        match self {
            Self::Case => &CASE,
            Self::Estate => &ESTATE,
            Self::Grant => &GRANT,
            Self::Recovery => &RECOVERY,
        }
    }
}
pub(super) fn phase_name(v: OrganizationProtectionPhaseV1) -> &'static str {
    match v {
        OrganizationProtectionPhaseV1::Active => "active",
        OrganizationProtectionPhaseV1::Notice => "notice",
        OrganizationProtectionPhaseV1::Protected => "protected",
        OrganizationProtectionPhaseV1::Limited => "limited",
        OrganizationProtectionPhaseV1::Resolved => "resolved",
    }
}
pub(super) fn disposition_name(v: OrganizationDispositionPhaseV1) -> &'static str {
    match v {
        OrganizationDispositionPhaseV1::None => "none",
        OrganizationDispositionPhaseV1::Frozen => "frozen",
        OrganizationDispositionPhaseV1::Obligations => "obligations",
        OrganizationDispositionPhaseV1::Business => "business",
        OrganizationDispositionPhaseV1::Residual => "residual",
        OrganizationDispositionPhaseV1::Complete => "complete",
    }
}
pub(super) fn phase(bytes: &[u8]) -> Result<OrganizationProtectionPhaseV1, ValueTreeError> {
    Ok(match bytes {
        b"active" => OrganizationProtectionPhaseV1::Active,
        b"notice" => OrganizationProtectionPhaseV1::Notice,
        b"protected" => OrganizationProtectionPhaseV1::Protected,
        b"limited" => OrganizationProtectionPhaseV1::Limited,
        b"resolved" => OrganizationProtectionPhaseV1::Resolved,
        _ => return Err(ValueTreeError::Invalid("organization phase")),
    })
}
pub(super) fn disposition(bytes: &[u8]) -> Result<OrganizationDispositionPhaseV1, ValueTreeError> {
    Ok(match bytes {
        b"none" => OrganizationDispositionPhaseV1::None,
        b"frozen" => OrganizationDispositionPhaseV1::Frozen,
        b"obligations" => OrganizationDispositionPhaseV1::Obligations,
        b"business" => OrganizationDispositionPhaseV1::Business,
        b"residual" => OrganizationDispositionPhaseV1::Residual,
        b"complete" => OrganizationDispositionPhaseV1::Complete,
        _ => return Err(ValueTreeError::Invalid("organization disposition")),
    })
}
pub(super) fn payloads(
    source: WorldRuleOrganizationScalarsRefV1<'_>,
) -> Result<[Option<MemberPayloadRefV1<'_>>; MAX], ValueTreeError> {
    use MemberPayloadRefV1 as P;
    let mut out = [None; MAX];
    let values: &[Option<P<'_>>] = match source {
        WorldRuleOrganizationScalarsRefV1::Case(value) => {
            let OrganizationCaseV1 {
                id,
                organization_id,
                founder_id,
                members: _,
                policy_version,
                revision,
                phase,
                disposition,
                notified_at,
                protected_at,
                grants: _,
                requests: _,
                stage_receipts,
            } = value;
            &[
                Some(P::Unit),
                Some(P::Text(id)),
                Some(P::Text(organization_id)),
                Some(P::Text(founder_id)),
                Some(P::U64(u64::from(*policy_version))),
                Some(P::U64(*revision)),
                Some(P::Text(phase_name(*phase))),
                Some(P::Text(disposition_name(*disposition))),
                Some(P::Bool(notified_at.is_some())),
                notified_at.map(P::U64),
                Some(P::Bool(protected_at.is_some())),
                protected_at.map(P::U64),
                Some(P::Unit),
                Some(P::Unit),
                Some(P::Unit),
                Some(P::Unit),
                Some(P::U64(u64::try_from(stage_receipts.len()).map_err(
                    |_| ValueTreeError::ResourceLimited("organization receipt length"),
                )?)),
            ]
        }
        WorldRuleOrganizationScalarsRefV1::Estate(value) => {
            let OrganizationEstateDepositV1 {
                id,
                organization_id,
                contributor_id,
                material,
                amount,
                held_in_trust,
                disposed,
                effect_receipt,
            } = value;
            &[
                Some(P::Unit),
                Some(P::Text(id)),
                Some(P::Text(organization_id)),
                Some(P::Text(contributor_id)),
                Some(P::Text(material)),
                Some(P::I64(*amount)),
                Some(P::Bool(*held_in_trust)),
                Some(P::Bool(*disposed)),
                Some(P::Bool(effect_receipt.is_some())),
                effect_receipt.as_deref().map(P::Text),
            ]
        }
        WorldRuleOrganizationScalarsRefV1::Grant(value) => {
            let OrganizationGrantV1 {
                id,
                delegate_id,
                scopes: _,
                revision,
                expires_at,
                source_proposal,
                replaces,
            } = value;
            &[
                Some(P::Unit),
                Some(P::Text(id)),
                Some(P::Text(delegate_id)),
                Some(P::U64(*revision)),
                Some(P::U64(*expires_at)),
                Some(P::Text(source_proposal)),
                Some(P::Bool(replaces.is_some())),
                replaces.as_deref().map(P::Text),
                Some(P::Unit),
            ]
        }
        WorldRuleOrganizationScalarsRefV1::Recovery(value) => {
            let OrganizationRecoveryRequestV1 {
                requester_id,
                appeal,
                approved,
                resolved_by,
            } = value;
            &[
                Some(P::Unit),
                Some(P::Text(requester_id)),
                Some(P::Bool(*appeal)),
                Some(P::Bool(approved.is_some())),
                approved.map(P::Bool),
                Some(P::Bool(resolved_by.is_some())),
                resolved_by.as_deref().map(P::Text),
            ]
        }
    };
    out[..values.len()].copy_from_slice(values);
    Ok(out)
}
