//! Independent Pending Execution protocol; no Client/Read profile or Current token.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PendingSourceProfileV2 {
    InspectThreeObserveCompleteV1,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reservation {
    ordinal: u64,
    at: UnixMillis,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingSourceObservationV2 {
    profile: PendingSourceProfileV2,
    // Mandatory fixed slots: Inspect, PreDecrypt, PostDecrypt, ClaimLF, Complete.
    // Only Inspect is implemented in this component; later slots stay closed.
    reservations: [Option<Reservation>; 5],
}
impl PendingSourceObservationV2 {
    pub(super) fn new() -> Self {
        Self {
            profile: PendingSourceProfileV2::InspectThreeObserveCompleteV1,
            reservations: std::array::from_fn(|_| None),
        }
    }
}
pub(super) fn reserve_inspect(s: &mut TestSessionAuthorityV2, now: UnixMillis) -> Result<()> {
    let p = s.execution_pending_start.as_ref().ok_or_else(invalid)?;
    if p.source_observation
        .reservations
        .iter()
        .any(Option::is_some)
        || p.claimed_leaf_count() != 0
        || now.get() < p.accepted_at.get()
        || now.get() >= p.absolute_until.get()
        || s.source_oracle_reads.get(&p.source_operation_id) != Some(&0)
        || !pending_funds_source_v2(s, &p.source_operation_id)
    {
        return Err(invalid());
    }
    let source_id = p.source_operation_id.clone();
    let ordinal = next_session_ordinal_v2(s)?;
    s.source_oracle_reads.insert(source_id.clone(), 1);
    s.execution_pending_start
        .as_mut()
        .ok_or_else(invalid)?
        .source_observation
        .reservations[0] = Some(Reservation { ordinal, at: now });
    append_session_event_v2(
        s,
        SessionEventKindV2::SourceReadReserved {
            source_operation_id: source_id,
            ticket: 1,
        },
        now,
    )?;
    Ok(())
}
pub(super) fn validate(s: &TestSessionAuthorityV2) -> Result<()> {
    let Some(p) = &s.execution_pending_start else {
        return Ok(());
    };
    let protocol = &p.source_observation;
    if protocol.profile != PendingSourceProfileV2::InspectThreeObserveCompleteV1
        || protocol.reservations[1..].iter().any(Option::is_some)
    {
        return Err(invalid());
    }
    let reads = s
        .source_oracle_reads
        .get(&p.source_operation_id)
        .ok_or_else(invalid)?;
    let first = fence::first_rf_sequence()[0];
    match &protocol.reservations[0] {
        None if *reads == 0 && p.claimed_leaf_count() == 0 => Ok(()),
        Some(r) if *reads == 1 => {
            let claim = p.claimed_leaf_ordinal(first).ok_or_else(invalid)?;
            let at = p.claimed_leaf_at(first).ok_or_else(invalid)?;
            if r.ordinal <= p.accepted_ordinal
                || r.ordinal >= claim
                || r.at.get() < p.accepted_at.get()
                || r.at.get() >= p.absolute_until.get()
                || r.at != at
                || session_event_ordinal_v2(
                    s,
                    &SessionEventKindV2::SourceReadReserved {
                        source_operation_id: p.source_operation_id.clone(),
                        ticket: 1,
                    },
                )? != Some(r.ordinal)
            {
                return Err(invalid());
            }
            Ok(())
        }
        _ => Err(invalid()),
    }
}
