use super::*;
pub(super) const N: usize = 29;
#[derive(Clone, Copy)]
pub(super) enum Kind {
    Unit,
    Text,
    U64,
    U32,
    I64,
    Bool,
    Status,
}
#[derive(Clone, Copy)]
pub(super) struct Spec {
    pub path: RecordPathV1<'static>,
    pub kind: Kind,
    pub option_flag: Option<usize>,
}
const fn field(name: &'static str, kind: Kind) -> Spec {
    Spec {
        path: RecordPathV1::Field(name),
        kind,
        option_flag: None,
    }
}
const fn crisis(name: &'static str, kind: Kind) -> Spec {
    Spec {
        path: RecordPathV1::Nested("crisis_evidence", name),
        kind,
        option_flag: None,
    }
}
const fn flag(name: &'static str) -> Spec {
    Spec {
        path: RecordPathV1::OptionPresence("crisis_evidence", name),
        kind: Kind::Bool,
        option_flag: None,
    }
}
const fn option(name: &'static str, kind: Kind, flag: usize) -> Spec {
    Spec {
        path: RecordPathV1::OptionValue("crisis_evidence", name),
        kind,
        option_flag: Some(flag),
    }
}
pub(super) const SPECS: [Spec; N] = [
    Spec {
        path: RecordPathV1::Record,
        kind: Kind::Unit,
        option_flag: None,
    },
    field("id", Kind::Text),
    field("provider_id", Kind::Text),
    field("crisis_id", Kind::Text),
    field("material", Kind::Text),
    field("review_proposal", Kind::Text),
    field("policy_version", Kind::U32),
    field("starts_at", Kind::U64),
    field("expires_at", Kind::U64),
    field("remaining_escrow", Kind::I64),
    field("revision", Kind::U64),
    field("active", Kind::Bool),
    crisis("crisis_id", Kind::Text),
    crisis("kind", Kind::Text),
    crisis("severity", Kind::U32),
    crisis("status", Kind::Status),
    crisis("opened_at", Kind::U64),
    crisis("expires_at", Kind::U64),
    flag("resolver_agent_id"),
    option("resolver_agent_id", Kind::Text, 18),
    flag("strategy"),
    option("strategy", Kind::Text, 20),
    flag("success"),
    option("success", Kind::Bool, 22),
    crisis("impact", Kind::I64),
    flag("resolved_at"),
    option("resolved_at", Kind::U64, 25),
    Spec {
        path: RecordPathV1::Collection("beneficiaries"),
        kind: Kind::Unit,
        option_flag: None,
    },
    Spec {
        path: RecordPathV1::Collection("requests"),
        kind: Kind::Unit,
        option_flag: None,
    },
];
pub(super) fn status_name(status: CrisisStatus) -> &'static str {
    match status {
        CrisisStatus::Active => "active",
        CrisisStatus::Resolved => "resolved",
        CrisisStatus::TimedOut => "timed_out",
    }
}
pub(super) fn payloads(value: &SupplyAuthorizationV1) -> [Option<MemberPayloadRefV1<'_>>; N] {
    let SupplyAuthorizationV1 {
        id,
        provider_id,
        crisis_id,
        crisis_evidence,
        material,
        beneficiaries: _,
        policy_version,
        starts_at,
        expires_at,
        review_proposal,
        remaining_escrow,
        requests: _,
        revision,
        active,
    } = value;
    let CrisisState {
        crisis_id: evidence_id,
        kind,
        severity,
        status,
        opened_at,
        expires_at: evidence_expires,
        resolver_agent_id,
        strategy,
        success,
        impact,
        resolved_at,
    } = crisis_evidence;
    use MemberPayloadRefV1 as P;
    [
        Some(P::Unit),
        Some(P::Text(id)),
        Some(P::Text(provider_id)),
        Some(P::Text(crisis_id)),
        Some(P::Text(material)),
        Some(P::Text(review_proposal)),
        Some(P::U64(u64::from(*policy_version))),
        Some(P::U64(*starts_at)),
        Some(P::U64(*expires_at)),
        Some(P::I64(*remaining_escrow)),
        Some(P::U64(*revision)),
        Some(P::Bool(*active)),
        Some(P::Text(evidence_id)),
        Some(P::Text(kind)),
        Some(P::U64(u64::from(*severity))),
        Some(P::Text(status_name(*status))),
        Some(P::U64(*opened_at)),
        Some(P::U64(*evidence_expires)),
        Some(P::Bool(resolver_agent_id.is_some())),
        resolver_agent_id.as_deref().map(P::Text),
        Some(P::Bool(strategy.is_some())),
        strategy.as_deref().map(P::Text),
        Some(P::Bool(success.is_some())),
        success.map(P::Bool),
        Some(P::I64(*impact)),
        Some(P::Bool(resolved_at.is_some())),
        resolved_at.map(P::U64),
        Some(P::Unit),
        Some(P::Unit),
    ]
}
