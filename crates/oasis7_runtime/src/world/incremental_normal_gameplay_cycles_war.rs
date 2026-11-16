//! Complete selected war sources; resource/reputation reads preserve original
//! member sequence and duplicate outcomes. No full World/Alliance is restored.
use super::*;
use super::{plan, select, selected};
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueTreeError};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_diplomacy::*, incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Current gameplay selected war source")
}
struct AllianceSnapshotV1 {
    members: Vec<ReservedValue<String>>,
    reputation: i64,
    _heap: ReservedValue<()>,
}
fn alliance_snapshot_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    id: &str,
    sources: &NormalGameplayCycleSourcesV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<AllianceSnapshotV1, GameplayCycleErrorV1> {
    let proof = selected::open_diplomacy(
        DiplomacyFieldV1::Alliances,
        id,
        sources,
        ctx,
        parent,
        work,
        tail,
        live,
    )?;
    let count = if let Some(proof) = &proof {
        usize::try_from(selected::member_length(
            id,
            DiplomacyFieldV1::Alliances,
            proof,
            state,
            ctx,
            tail,
        )?)
        .map_err(|_| bad())?
    } else {
        0
    };
    let heap = tail
        .reserve(
            count
                .checked_mul(std::mem::size_of::<ReservedValue<String>>())
                .ok_or_else(bad)?,
            MemoryClass::Ordinary,
        )?
        .retain(());
    let mut members = Vec::new();
    members.try_reserve_exact(count).map_err(|_| bad())?;
    if members.capacity() > count {
        return Err(bad().into());
    }
    let mut reputation = 0i64;
    if let Some(proof) = proof {
        for ordinal in 0..count {
            let member = selected::member_text(
                id,
                DiplomacyFieldV1::Alliances,
                ordinal as u64,
                DiplomacyMemberColumnV1::AllianceMember,
                &proof,
                state,
                ctx,
                work,
                tail,
            )?
            .ok_or_else(bad)?;
            let (_, amount) = selected::reputation(&member, ctx, tail)?;
            // The old Iterator<i64>::sum has profile overflow behavior. Do not replace
            // it with checked/saturating arithmetic and add a new business rejection.
            reputation += amount;
            members.push(member);
        }
    }
    Ok(AllianceSnapshotV1 {
        members,
        reputation,
        _heap: heap,
    })
}
pub(super) fn prepare_next_war_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    sources: &NormalGameplayCycleSourcesV1<'_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut NormalGameplayCycleWorkV1,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<Option<plan::OwnedGameplayDomainV1>, GameplayCycleErrorV1> {
    let Some(row) = select::select_next_war_row_v1(sources, ctx, parent, work, tail, now, live)?
    else {
        return Ok(None);
    };
    let field = DiplomacyFieldV1::Wars;
    let col = |v| DiplomacyScalarV1::War(v);
    let proof = selected::open_diplomacy(field, &row.0, sources, ctx, parent, work, tail, live)?
        .ok_or_else(bad)?;
    let aggressor = selected::text(
        &row.0,
        field,
        col(WarScalarV1::AggressorAllianceId),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?
    .ok_or_else(bad)?;
    let defender = selected::text(
        &row.0,
        field,
        col(WarScalarV1::DefenderAllianceId),
        &proof,
        state,
        ctx,
        work,
        tail,
    )?
    .ok_or_else(bad)?;
    let intensity = match selected::scalar(
        &row.0,
        field,
        col(WarScalarV1::Intensity),
        &proof,
        state,
        ctx,
        tail,
    )? {
        Some(DiplomacyValueV1::Unsigned(n)) => u32::try_from(n).map_err(|_| bad())?,
        _ => return Err(bad().into()),
    };
    let aggressor_members = alliance_snapshot_v1(
        &aggressor, sources, state, ctx, parent, work, tail, now, live,
    )?;
    let defender_members = alliance_snapshot_v1(
        &defender, sources, state, ctx, parent, work, tail, now, live,
    )?;
    let (aggressor_score, defender_score) = kernel::prepare_war_scores_v1(
        aggressor_members.members.len() as i64,
        defender_members.members.len() as i64,
        aggressor_members.reputation,
        defender_members.reputation,
        intensity,
    );
    let (winner, loser, winner_id, loser_id) = if aggressor_score >= defender_score {
        (
            &aggressor_members,
            &defender_members,
            aggressor.as_str(),
            defender.as_str(),
        )
    } else {
        (
            &defender_members,
            &aggressor_members,
            defender.as_str(),
            aggressor.as_str(),
        )
    };
    let count = winner
        .members
        .len()
        .checked_add(loser.members.len())
        .ok_or_else(bad)?;
    let mut strings = row
        .1
        .len()
        .checked_add(winner_id.len())
        .and_then(|n| n.checked_add(loser_id.len()))
        .ok_or_else(bad)?;
    for member in winner.members.iter().chain(loser.members.iter()) {
        tail.charge_record()?;
        strings = strings.checked_add(member.len()).ok_or_else(bad)?;
    }
    // Input String ownership is moved into each outcome. Both selected input
    // vectors and the exact-capacity output Vec are alive at once; all metadata
    // and every cloned string are admitted before either allocation/copy.
    let metadata = loser
        .members
        .len()
        .checked_mul(std::mem::size_of::<(String, i64, i64)>())
        .and_then(|n| {
            winner
                .members
                .len()
                .checked_mul(std::mem::size_of::<String>())
                .and_then(|m| n.checked_add(m))
        })
        .and_then(|n| {
            count
                .checked_mul(std::mem::size_of::<crate::WarParticipantOutcome>())
                .and_then(|m| n.checked_add(m))
        })
        .and_then(|n| n.checked_add(std::mem::size_of::<crate::WorldEvent>() + 512))
        .and_then(|n| n.checked_add(strings))
        .ok_or_else(bad)?;
    let fee = tail.reserve(metadata, MemoryClass::Ordinary)?;
    let mut losers = Vec::new();
    losers
        .try_reserve_exact(loser.members.len())
        .map_err(|_| bad())?;
    if losers.capacity() > loser.members.len() {
        return Err(bad().into());
    }
    let mut winners = Vec::new();
    winners
        .try_reserve_exact(winner.members.len())
        .map_err(|_| bad())?;
    if winners.capacity() > winner.members.len() {
        return Err(bad().into());
    }
    for member in &loser.members {
        let electricity =
            selected::electricity_or_data(member, 0, sources, ctx, tail, live)?.unwrap_or(0);
        let data = selected::electricity_or_data(member, 1, sources, ctx, tail, live)?.unwrap_or(0);
        tail.charge(member.len())?;
        losers.push((String::clone(&**member), electricity, data));
    }
    for member in &winner.members {
        tail.charge(member.len())?;
        winners.push(String::clone(&**member));
    }
    let mut output = Vec::new();
    output.try_reserve_exact(count).map_err(|_| bad())?;
    if output.capacity() > count {
        return Err(bad().into());
    }
    let outcomes = kernel::prepare_war_participant_outcomes_v1(
        losers,
        winners,
        intensity,
        output,
        &mut |bytes| tail.charge(bytes),
    )?;
    tail.charge(row.1.len() + winner_id.len() + loser_id.len() + 512)?;
    let summary = format!(
        "auto settlement: aggressor_score={} defender_score={} aggressor_reputation={} defender_reputation={} outcome_count={}",
        aggressor_score,
        defender_score,
        aggressor_members.reputation,
        defender_members.reputation,
        outcomes.len()
    );
    let body = crate::DomainEvent::WarConcluded {
        war_id: row.1.clone(),
        winner_alliance_id: winner_id.to_owned(),
        loser_alliance_id: loser_id.to_owned(),
        aggressor_score,
        defender_score,
        summary,
        participant_outcomes: outcomes,
    };
    Ok(Some(plan::OwnedGameplayDomainV1 {
        body,
        fee,
        backing: None,
    }))
}
