use super::*;

pub(super) fn read(
    service: &Service<'_>,
    request: SignedReadRequest<MembershipReadEnvelope>,
) -> Result<Value, String> {
    verify_read_request(MEMBERSHIP_PATH, &request)?;
    request.request.validate().map_err(|e| e.to_string())?;
    let query = &request.request.query;
    // Only the engineering history path currently supplies an authenticated
    // committed membership source. Legacy/unscoped projections cannot opt in.
    if service.guarded.is_none() || query.world != service.identity {
        return Err("authenticated governance membership source is unsupported".into());
    }
    let pinned = service.pin(query.fixed_commit.as_ref())?;
    if let Some(minimum) = &query.min_commit
        && !pinned
            .commit
            .satisfies_minimum(minimum)
            .map_err(|e| e.to_string())?
    {
        return Err("membership read is not caught up".into());
    }
    let root = oasis7_runtime::WorldGovernanceMembershipView::world_state_root_hash(&pinned.world)?;
    let scope = verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE;
    let view = oasis7_runtime::WorldGovernanceMembershipView::read(
        &pinned.world,
        &pinned.commit,
        &root,
        scope,
    )?;
    let response = membership::public_response(view, Some(scope.into()));
    response.validate(query).map_err(|e| e.to_string())?;
    service.signed(MEMBERSHIP_PATH, &request, response)
}
