//! Captures complete original fallback event inputs before the first write in
//! each original substage. Aliases cannot cause later event inputs to drift.
use super::*;
use super::{select, selected};
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueTreeError};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_diplomacy::*, incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1, incremental_governance_records::*,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("gameplay fallback source plan")
}
pub(super) struct OwnedGameplayDomainV1 {
    pub(super) body: crate::DomainEvent,
    pub(super) fee: MemoryReservation,
    pub(super) backing: Option<std::sync::Arc<ReservedValue<()>>>,
}
pub(super) struct FallbackPlanV1 {
    iter: std::vec::IntoIter<OwnedGameplayDomainV1>,
    _heap: ReservedValue<()>,
}
impl Iterator for FallbackPlanV1 {
    type Item = OwnedGameplayDomainV1;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}
pub(super) fn build_stage_plan_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    stage: select::FallbackStageV1,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<FallbackPlanV1, GameplayCycleErrorV1> {
    let rows = select::select_due_rows(stage, sources, ctx, parent, work, tail, now, live)?;
    let count = rows.len();
    let heap = tail
        .reserve(
            count
                .checked_mul(std::mem::size_of::<OwnedGameplayDomainV1>())
                .ok_or_else(bad)?,
            MemoryClass::Ordinary,
        )?
        .retain(());
    let mut events = Vec::new();
    events.try_reserve_exact(count).map_err(|_| bad())?;
    if events.capacity() > count {
        return Err(bad().into());
    }
    for row in rows {
        let (body, fee) = match stage {
            select::FallbackStageV1::Contract => {
                let field = DiplomacyFieldV1::EconomicContracts;
                let col = |v| DiplomacyScalarV1::Contract(v);
                let proof = selected::open_diplomacy(
                    field, &row.outer, sources, ctx, parent, work, tail, live,
                )?
                .ok_or_else(bad)?;
                let creator = selected::text(
                    &row.outer,
                    field,
                    col(ContractScalarV1::CreatorAgentId),
                    &proof,
                    state,
                    ctx,
                    work,
                    tail,
                )?
                .ok_or_else(bad)?;
                let counterparty = selected::text(
                    &row.outer,
                    field,
                    col(ContractScalarV1::CounterpartyAgentId),
                    &proof,
                    state,
                    ctx,
                    work,
                    tail,
                )?
                .ok_or_else(bad)?;
                let status = selected::text(
                    &row.outer,
                    field,
                    col(ContractScalarV1::Status),
                    &proof,
                    state,
                    ctx,
                    work,
                    tail,
                )?
                .ok_or_else(bad)?;
                let status = selected::contract_status(&status)?;
                let stake = match selected::scalar(
                    &row.outer,
                    field,
                    col(ContractScalarV1::ReputationStake),
                    &proof,
                    state,
                    ctx,
                    tail,
                )? {
                    Some(DiplomacyValueV1::Signed(v)) => v,
                    _ => return Err(bad().into()),
                };
                let id = row.inner.as_deref().ok_or_else(bad)?;
                let bytes = id
                    .len()
                    .checked_add(creator.len())
                    .and_then(|n| n.checked_add(counterparty.len()))
                    .ok_or_else(bad)?;
                tail.charge(bytes)?;
                let fee = tail.reserve(
                    bytes
                        .checked_add(std::mem::size_of::<crate::WorldEvent>())
                        .ok_or_else(bad)?,
                    MemoryClass::Ordinary,
                )?;
                (
                    kernel::prepare_contract_expiry_event_v1(
                        id.to_owned(),
                        creator.as_str().to_owned(),
                        counterparty.as_str().to_owned(),
                        status,
                        stake,
                    ),
                    fee,
                )
            }
            select::FallbackStageV1::Governance => {
                let key = GovernanceRecordKeyV1 {
                    kind: GovernanceRecordKindV1::Proposal,
                    id: &row.outer,
                };
                let proof = selected::open_governance(
                    key.kind, key.id, sources, ctx, parent, work, tail, live,
                )?
                .ok_or_else(bad)?;
                let quorum = match selected::governance_scalar(
                    key,
                    GovernanceCellV1::Column(GovernanceColumnV1::QuorumWeight),
                    &proof,
                    ctx,
                    tail,
                    live,
                )? {
                    GovernanceValueV1::Unsigned(v) => v,
                    _ => return Err(bad().into()),
                };
                let threshold = match selected::governance_scalar(
                    key,
                    GovernanceCellV1::Column(GovernanceColumnV1::PassThresholdBps),
                    &proof,
                    ctx,
                    tail,
                    live,
                )? {
                    GovernanceValueV1::Unsigned(v) => u16::try_from(v).map_err(|_| bad())?,
                    _ => return Err(bad().into()),
                };
                let vote = selected::open_governance(
                    GovernanceRecordKindV1::Vote,
                    &row.outer,
                    sources,
                    ctx,
                    parent,
                    work,
                    tail,
                    live,
                )?;
                let (total, tallies) = if let Some(vote) = vote {
                    let vote_key = GovernanceRecordKeyV1 {
                        kind: GovernanceRecordKindV1::Vote,
                        id: &row.outer,
                    };
                    let total = match selected::governance_scalar(
                        vote_key,
                        GovernanceCellV1::Column(GovernanceColumnV1::TotalWeight),
                        &vote,
                        ctx,
                        tail,
                        live,
                    )? {
                        GovernanceValueV1::Unsigned(v) => v,
                        _ => return Err(bad().into()),
                    };
                    let guard = ctx.transaction.guard();
                    let tallies = recover_current_gameplay_tallies_v1(
                        &row.outer,
                        &vote,
                        state,
                        ctx,
                        &mut work.selected_scan,
                        &mut work.directory,
                        &guard,
                        live,
                    )?;
                    (total, Some(tallies))
                } else {
                    (0, None)
                };
                let mut max_winner = 0usize;
                if let Some(tallies) = &tallies {
                    for (id, _) in tallies.iter() {
                        tail.charge_record()?;
                        max_winner = max_winner.max(id.len());
                    }
                }
                let bytes = row.outer.len().checked_add(max_winner).ok_or_else(bad)?;
                let fee = tail.reserve(
                    bytes
                        .checked_add(std::mem::size_of::<crate::WorldEvent>())
                        .ok_or_else(bad)?,
                    MemoryClass::Ordinary,
                )?;
                let iter = tallies.as_ref().into_iter().flat_map(|v| v.iter());
                let body = kernel::prepare_governance_finalization_event_v1(
                    row.outer,
                    quorum,
                    threshold,
                    total,
                    iter,
                    &mut |bytes| tail.charge(bytes),
                )?;
                (body, fee)
            }
            select::FallbackStageV1::CrisisTimeout => {
                tail.charge(row.outer.len())?;
                let fee = tail.reserve(
                    row.outer
                        .len()
                        .checked_add(std::mem::size_of::<crate::WorldEvent>())
                        .ok_or_else(bad)?,
                    MemoryClass::Ordinary,
                )?;
                (
                    kernel::prepare_crisis_timeout_event_v1(row.outer, row.severity),
                    fee,
                )
            }
        };
        events.push(OwnedGameplayDomainV1 {
            body,
            fee,
            backing: None,
        });
    }
    Ok(FallbackPlanV1 {
        iter: events.into_iter(),
        _heap: heap,
    })
}

pub(super) fn prepare_directive_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    owned: super::decode::OwnedGameplayDirectiveV1,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<Option<OwnedGameplayDomainV1>, GameplayCycleErrorV1> {
    let super::decode::OwnedGameplayDirectiveV1 { value, heap } = owned;
    let sides = if let kernel::GameplayLifecycleDirective::WarConclude {
        war_id,
        loser_alliance_id: None,
        ..
    } = &value
    {
        if let Some(proof) = selected::open_diplomacy(
            DiplomacyFieldV1::Wars,
            war_id,
            sources,
            ctx,
            parent,
            work,
            tail,
            live,
        )? {
            let aggressor = selected::text(
                war_id,
                DiplomacyFieldV1::Wars,
                DiplomacyScalarV1::War(WarScalarV1::AggressorAllianceId),
                &proof,
                state,
                ctx,
                work,
                tail,
            )?
            .ok_or_else(bad)?;
            let defender = selected::text(
                war_id,
                DiplomacyFieldV1::Wars,
                DiplomacyScalarV1::War(WarScalarV1::DefenderAllianceId),
                &proof,
                state,
                ctx,
                work,
                tail,
            )?
            .ok_or_else(bad)?;
            Some((aggressor, defender))
        } else {
            None
        }
    } else {
        None
    };
    let clone_bytes = sides
        .as_ref()
        .map_or(0, |(a, d)| a.len().saturating_add(d.len()));
    tail.charge(clone_bytes)?;
    let fee = tail.reserve(
        std::mem::size_of::<crate::WorldEvent>()
            .checked_add(clone_bytes)
            .ok_or_else(bad)?,
        MemoryClass::Ordinary,
    )?;
    let body = kernel::prepare_gameplay_directive_event_v1(
        value,
        sides.as_ref().map(|(a, d)| (a.as_str(), d.as_str())),
    );
    Ok(body.map(|body| OwnedGameplayDomainV1 {
        body,
        fee,
        backing: Some(heap),
    }))
}
