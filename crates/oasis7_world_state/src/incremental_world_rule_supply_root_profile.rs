//! Closed shared profile adapter; all original Spec paths remain authoritative.
use super::*;
use crate::{
    CrisisState, CrisisStatus,
    world_rules::{SupplyAuthorizationV1, SupplyBatchV1},
};
#[path = "incremental_world_rule_supply_root_authorization_profile.rs"]
mod a;
#[path = "incremental_world_rule_supply_root_batch_profile.rs"]
mod b;
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
pub(super) struct Spec {
    pub path: RecordPathV1<'static>,
    pub kind: Kind,
    pub flag: Option<usize>,
}
impl SupplyRootKeyV1<'_> {
    pub(super) fn relative(self) -> K<'static> {
        match self {
            Self::Authorization(_) => K::AuthorizationInner,
            Self::Batch(_) => K::BatchInner,
        }
    }
    pub(super) fn count(self) -> usize {
        match self {
            Self::Authorization(_) => a::N,
            Self::Batch(_) => b::N,
        }
    }
    pub(super) fn spec(self, slot: usize) -> Spec {
        match self {
            Self::Authorization(_) => {
                let s = a::SPECS[slot];
                Spec {
                    path: s.path,
                    flag: s.option_flag,
                    kind: match s.kind {
                        a::Kind::Unit => Kind::Unit,
                        a::Kind::Text => Kind::Text,
                        a::Kind::U64 => Kind::U64,
                        a::Kind::U32 => Kind::U32,
                        a::Kind::I64 => Kind::I64,
                        a::Kind::Bool => Kind::Bool,
                        a::Kind::Status => Kind::Status,
                    },
                }
            }
            Self::Batch(_) => Spec {
                path: b::SPECS[slot].path,
                flag: None,
                kind: match slot {
                    0 | 7..=9 => Kind::Unit,
                    1 | 2 => Kind::Text,
                    3 | 4 => Kind::U64,
                    5 => Kind::I64,
                    6 => Kind::Bool,
                    _ => unreachable!(),
                },
            },
        }
    }
}
#[derive(Clone, Copy)]
pub enum SupplyRootSourceV1<'a> {
    Authorization(&'a SupplyAuthorizationV1),
    Batch(&'a SupplyBatchV1),
}
impl SupplyRootSourceV1<'_> {
    pub(super) fn empty(self) -> bool {
        match self {
            Self::Authorization(v) => v.beneficiaries.is_empty() && v.requests.is_empty(),
            Self::Batch(v) => {
                v.frozen_lineages.is_empty() && v.demands.is_empty() && v.allocation.is_empty()
            }
        }
    }
    pub(super) fn matches(self, owner: SupplyRootKeyV1<'_>) -> bool {
        matches!(
            (self, owner),
            (Self::Authorization(_), SupplyRootKeyV1::Authorization(_))
                | (Self::Batch(_), SupplyRootKeyV1::Batch(_))
        )
    }
}
pub(super) fn visit<'a>(
    source: SupplyRootSourceV1<'a>,
    visit: &mut impl FnMut(
        K<'a>,
        RecordPathV1<'a>,
        MemberPayloadRefV1<'a>,
    ) -> Result<(), MemberRecordError>,
) -> Result<(), MemberRecordError> {
    match source {
        SupplyRootSourceV1::Authorization(v) => {
            for (s, p) in a::SPECS.into_iter().zip(a::payloads(v)) {
                if let Some(p) = p {
                    visit(K::AuthorizationInner, s.path, p)?;
                }
            }
            for m in &v.beneficiaries {
                visit(
                    K::AuthorizationInner,
                    RecordPathV1::Member("beneficiaries", m),
                    MemberPayloadRefV1::Unit,
                )?;
            }
            for (m, n) in &v.requests {
                visit(
                    K::AuthorizationInner,
                    RecordPathV1::Member("requests", m),
                    MemberPayloadRefV1::I64(*n),
                )?;
            }
        }
        SupplyRootSourceV1::Batch(v) => {
            for (s, p) in b::SPECS.into_iter().zip(b::payloads(v)) {
                if let Some(p) = p {
                    visit(K::BatchInner, s.path, p)?;
                }
            }
            for (m, amount) in &v.demands {
                visit(
                    K::BatchInner,
                    RecordPathV1::Member("demands", m),
                    MemberPayloadRefV1::I64(*amount),
                )?;
            }
            for (m, amount) in &v.allocation {
                visit(
                    K::BatchInner,
                    RecordPathV1::Member("allocation", m),
                    MemberPayloadRefV1::I64(*amount),
                )?;
            }
            for (m, lineage) in &v.frozen_lineages {
                visit(
                    K::BatchInner,
                    RecordPathV1::Member("frozen_lineages", m),
                    MemberPayloadRefV1::Unit,
                )?;
                let crate::world_rules::SupplyLineageV1 {
                    target,
                    fulfilled,
                    reserved,
                } = lineage;
                for (field, n) in [
                    ("target", target),
                    ("fulfilled", fulfilled),
                    ("reserved", reserved),
                ] {
                    visit(
                        K::BatchInner,
                        RecordPathV1::MemberField("frozen_lineages", m, field),
                        MemberPayloadRefV1::I64(*n),
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn fixed(source: SupplyRootSourceV1<'_>) -> [Option<MemberPayloadRefV1<'_>>; 29] {
    match source {
        SupplyRootSourceV1::Authorization(v) => a::payloads(v),
        SupplyRootSourceV1::Batch(v) => {
            let mut out = [None; 29];
            for (i, p) in b::payloads(v).into_iter().enumerate() {
                out[i] = p;
            }
            out
        }
    }
}
