//! Actual crisis spawning as one caller-paid gameplay-cycle mutation.
use super::*;

pub(in crate::world) fn spawn_crisis<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    event: &crate::DomainEvent,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<(), GameplayCycleErrorV1> {
    let crate::DomainEvent::CrisisSpawned {
        crisis_id, kind, ..
    } = event
    else {
        return Err(bad().into());
    };
    tail.charge(crisis_id.len() + kind.len())?;
    let fee = tail.reserve(
        std::mem::size_of::<crate::CrisisState>()
            .checked_add(crisis_id.len())
            .and_then(|n| n.checked_add(kind.len()))
            .ok_or_else(bad)?,
        MemoryClass::Ordinary,
    )?;
    let after = fee.retain(
        oasis7_world_state::incremental_normal_gameplay_cycles_selected::selected_crisis_spawn_v1(
            event, now,
        ),
    );
    let field = DiplomacyFieldV1::Crises;
    let col = |v| DiplomacyScalarV1::Crisis(v);
    let Some(mut proof) =
        selected::open_diplomacy(field, crisis_id, sources, ctx, parent, work, tail, live)?
    else {
        let guard = ctx.transaction.guard();
        create_qualified_diplomacy_empty_v1(
            DiplomacyEmptyCreateV1 {
                id: crisis_id,
                source: DiplomacySourceV1::Crisis(&after),
                family: sources.diplomacy,
                max_pending_node_bytes: work.max_pending_node_bytes,
            },
            ctx,
            write,
            &guard,
            live,
            &mut work.cells_remaining,
        )?;
        return Ok(());
    };
    let inner = selected::text(
        crisis_id,
        field,
        col(CrisisScalarV1::CrisisId),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?
    .ok_or_else(bad)?;
    let oldkind = selected::text(
        crisis_id,
        field,
        col(CrisisScalarV1::Kind),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?
    .ok_or_else(bad)?;
    let severity = unsigned(selected::scalar(
        crisis_id,
        field,
        col(CrisisScalarV1::Severity),
        &proof,
        state,
        ctx,
        tail,
    )?)?
    .ok_or_else(bad)?;
    let status = selected::text(
        crisis_id,
        field,
        col(CrisisScalarV1::Status),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?
    .ok_or_else(bad)?;
    let status = selected::crisis_status(&status)?;
    let opened = unsigned(selected::scalar(
        crisis_id,
        field,
        col(CrisisScalarV1::OpenedAt),
        &proof,
        state,
        ctx,
        tail,
    )?)?
    .ok_or_else(bad)?;
    let expires = unsigned(selected::scalar(
        crisis_id,
        field,
        col(CrisisScalarV1::ExpiresAt),
        &proof,
        state,
        ctx,
        tail,
    )?)?
    .ok_or_else(bad)?;
    let resolver = selected::text(
        crisis_id,
        field,
        col(CrisisScalarV1::ResolverAgentId),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?;
    let strategy = selected::text(
        crisis_id,
        field,
        col(CrisisScalarV1::Strategy),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?;
    let success = boolean(selected::scalar(
        crisis_id,
        field,
        col(CrisisScalarV1::Success),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let impact = signed(selected::scalar(
        crisis_id,
        field,
        col(CrisisScalarV1::Impact),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    let resolved = unsigned(selected::scalar(
        crisis_id,
        field,
        col(CrisisScalarV1::ResolvedAt),
        &proof,
        state,
        ctx,
        tail,
    )?)?;
    use DiplomacyScalarValueV1 as D;
    for (column, before, next) in [
        (
            CrisisScalarV1::CrisisId,
            Some(D::Text(&inner)),
            Some(D::Text(&after.crisis_id)),
        ),
        (
            CrisisScalarV1::Kind,
            Some(D::Text(&oldkind)),
            Some(D::Text(&after.kind)),
        ),
        (
            CrisisScalarV1::Severity,
            Some(D::U32(u32::try_from(severity).map_err(|_| bad())?)),
            Some(D::U32(after.severity)),
        ),
        (
            CrisisScalarV1::Status,
            Some(D::CrisisStatus(status)),
            Some(D::CrisisStatus(after.status)),
        ),
        (
            CrisisScalarV1::OpenedAt,
            Some(D::U64(opened)),
            Some(D::U64(after.opened_at)),
        ),
        (
            CrisisScalarV1::ExpiresAt,
            Some(D::U64(expires)),
            Some(D::U64(after.expires_at)),
        ),
        (
            CrisisScalarV1::ResolverAgentId,
            resolver.as_ref().map(|s| D::Text(s.as_str())),
            None,
        ),
        (
            CrisisScalarV1::Strategy,
            strategy.as_ref().map(|s| D::Text(s.as_str())),
            None,
        ),
        (CrisisScalarV1::Success, success.map(D::Bool), None),
        (
            CrisisScalarV1::Impact,
            Some(D::I64(impact)),
            Some(D::I64(0)),
        ),
        (CrisisScalarV1::ResolvedAt, resolved.map(D::U64), None),
    ] {
        selected::change_diplomacy(
            crisis_id,
            field,
            col(column),
            &mut proof,
            before,
            next,
            ctx,
            write,
            work,
            tail,
            live,
        )?;
    }
    Ok(())
}
