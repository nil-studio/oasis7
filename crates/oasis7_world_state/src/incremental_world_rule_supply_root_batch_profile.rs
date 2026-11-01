use super::*;
pub(super) const N: usize = 10;
#[derive(Clone, Copy)]
pub(super) struct Spec {
    pub path: RecordPathV1<'static>,
}
const fn field(name: &'static str) -> Spec {
    Spec {
        path: RecordPathV1::Field(name),
    }
}
pub(super) const SPECS: [Spec; N] = [
    Spec {
        path: RecordPathV1::Record,
    },
    field("id"),
    field("authorization_id"),
    field("frozen_at"),
    field("authorization_revision"),
    field("frozen_escrow"),
    field("allocated"),
    Spec {
        path: RecordPathV1::Collection("frozen_lineages"),
    },
    Spec {
        path: RecordPathV1::Collection("demands"),
    },
    Spec {
        path: RecordPathV1::Collection("allocation"),
    },
];
pub(super) fn payloads(value: &SupplyBatchV1) -> [Option<MemberPayloadRefV1<'_>>; N] {
    let SupplyBatchV1 {
        id,
        authorization_id,
        frozen_at,
        authorization_revision,
        frozen_escrow,
        frozen_lineages: _,
        demands: _,
        allocation: _,
        allocated,
    } = value;
    use MemberPayloadRefV1 as P;
    [
        Some(P::Unit),
        Some(P::Text(id)),
        Some(P::Text(authorization_id)),
        Some(P::U64(*frozen_at)),
        Some(P::U64(*authorization_revision)),
        Some(P::I64(*frozen_escrow)),
        Some(P::Bool(*allocated)),
        Some(P::Unit),
        Some(P::Unit),
        Some(P::Unit),
    ]
}
