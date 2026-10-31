//! Qualified record-local member successor, including same-Tx fresh creation.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::StatePageWriter,
};
use oasis7_proto::memory_budget::ReservedValue;
pub struct OrganizationRootMemberChangeV1<'a> {
    pub case: &'a str,
    pub member: &'a str,
    pub expected: bool,
    pub after: bool,
    pub qualification: &'a ReservedValue<OrganizationRecordQualificationV1>,
    pub max_pending_node_bytes: usize,
}
pub fn change_qualified_organization_member_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: OrganizationRootMemberChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<OrganizationRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    change_qualified_organization_set_v1(
        OrganizationSetChangeV1 {
            request: OrganizationSetRequestV1 {
                case: change.case,
                key: OrganizationRelativeSetKeyV1::Member(change.member),
                qualification: change.qualification,
            },
            expected: if change.expected {
                OrganizationSetReadV1::Present
            } else {
                OrganizationSetReadV1::Absent
            },
            after: change.after,
            max_pending_node_bytes: change.max_pending_node_bytes,
        },
        context,
        write,
        guard,
        live,
    )
}
