use super::*;
use oasis7_agent_api::world_action::{OrganizationEstateRightV1, OrganizationEstateScopeV1};
pub(super) fn right_name(right: OrganizationEstateRightV1) -> &'static str {
    match right {
        OrganizationEstateRightV1::Preserve => "preserve",
        OrganizationEstateRightV1::ReturnTrust => "return_trust",
        OrganizationEstateRightV1::DistributeResidual => "distribute_residual",
        OrganizationEstateRightV1::FinalizeCase => "finalize_case",
        OrganizationEstateRightV1::AdvanceCase => "advance_case",
    }
}
pub(super) fn scope_path(scope: &OrganizationEstateScopeV1) -> RecordPathV1<'_> {
    let OrganizationEstateScopeV1 { object_id, right } = scope;
    RecordPathV1::MemberPair("scopes", object_id, right_name(*right))
}
