//! Exhaustive borrowed rollback control projection. No authority or acceptance token.
use crate::snapshot::{
    RollbackAuthorityRecord, RollbackAuthorityRole, RollbackCompensationCaseRef,
    RollbackCompensationState, RollbackDispositionStatus, RollbackEventDisposition,
    RollbackNonceOutcome, RollbackReceiptProjection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackValueRefV1<'a> {
    Text(&'a str),
    U64(u64),
    Bool(bool),
}
/// Fixed column names and ordinal components remain separate semantic tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RollbackPathV1 {
    Column(&'static str),
    Ordinal(u64),
}
#[derive(Debug)]
pub enum RollbackVisitErrorV1<E> {
    Sink(E),
    IntegerOverflow,
}
use RollbackPathV1::{Column as C, Ordinal as O};
use RollbackValueRefV1::{Bool as B, Text as T, U64 as U};
fn cell<'a, E>(
    emit: &mut impl FnMut(&[RollbackPathV1], RollbackValueRefV1<'a>) -> Result<(), E>,
    path: &[RollbackPathV1],
    value: RollbackValueRefV1<'a>,
) -> Result<(), RollbackVisitErrorV1<E>> {
    emit(path, value).map_err(RollbackVisitErrorV1::Sink)
}
fn count<E>(value: usize) -> Result<u64, RollbackVisitErrorV1<E>> {
    u64::try_from(value).map_err(|_| RollbackVisitErrorV1::IntegerOverflow)
}
fn strings<'a, E>(
    name: &'static str,
    values: &'a [String],
    emit: &mut impl FnMut(&[RollbackPathV1], RollbackValueRefV1<'a>) -> Result<(), E>,
) -> Result<(), RollbackVisitErrorV1<E>> {
    cell(emit, &[C(name), C("count")], U(count(values.len())?))?;
    for (i, v) in values.iter().enumerate() {
        cell(emit, &[C(name), O(count(i)?)], T(v))?;
    }
    Ok(())
}

pub fn visit_rollback_authority_v1<'a, E>(
    source: &'a RollbackAuthorityRecord,
    mut emit: impl FnMut(&[RollbackPathV1], RollbackValueRefV1<'a>) -> Result<(), E>,
) -> Result<(), RollbackVisitErrorV1<E>> {
    let RollbackAuthorityRecord {
        authority_id,
        role,
        public_key_hex,
        active,
    } = source;
    cell(&mut emit, &[C("authority_id")], T(authority_id))?;
    cell(
        &mut emit,
        &[C("role")],
        U(match role {
            RollbackAuthorityRole::OnCall => 0,
            RollbackAuthorityRole::Governance => 1,
        }),
    )?;
    cell(&mut emit, &[C("public_key_hex")], T(public_key_hex))?;
    cell(&mut emit, &[C("active")], B(*active))
}
pub fn visit_rollback_outcome_v1<'a, E>(
    source: &'a RollbackNonceOutcome,
    mut emit: impl FnMut(&[RollbackPathV1], RollbackValueRefV1<'a>) -> Result<(), E>,
) -> Result<(), RollbackVisitErrorV1<E>> {
    let RollbackNonceOutcome {
        canonical_intent_hash,
        rollback_checkpoint_batch_id,
        rollback_checkpoint_snapshot_hash,
        rollback_ticket,
        target_journal_commitment,
        target_state_root,
        affected_census_digest,
        target_batch_id,
        rollback_checkpoint_journal_len,
        target_journal_len,
        affected_census_count,
        rollback_event_id,
        prior_reorg_epoch,
        committed_reorg_epoch,
        readiness_blockers,
        invalidated_batch_ids,
        dispositions,
        receipt,
    } = source;
    cell(
        &mut emit,
        &[C("canonical_intent_hash")],
        T(canonical_intent_hash),
    )?;
    cell(
        &mut emit,
        &[C("rollback_checkpoint_batch_id")],
        T(rollback_checkpoint_batch_id),
    )?;
    cell(
        &mut emit,
        &[C("rollback_checkpoint_snapshot_hash")],
        T(rollback_checkpoint_snapshot_hash),
    )?;
    cell(&mut emit, &[C("rollback_ticket")], T(rollback_ticket))?;
    cell(
        &mut emit,
        &[C("target_journal_commitment")],
        T(target_journal_commitment),
    )?;
    cell(&mut emit, &[C("target_state_root")], T(target_state_root))?;
    cell(
        &mut emit,
        &[C("affected_census_digest")],
        T(affected_census_digest),
    )?;
    cell(&mut emit, &[C("target_batch_id")], T(target_batch_id))?;
    cell(
        &mut emit,
        &[C("rollback_checkpoint_journal_len")],
        U(count(*rollback_checkpoint_journal_len)?),
    )?;
    cell(
        &mut emit,
        &[C("target_journal_len")],
        U(count(*target_journal_len)?),
    )?;
    cell(
        &mut emit,
        &[C("affected_census_count")],
        U(count(*affected_census_count)?),
    )?;
    cell(&mut emit, &[C("rollback_event_id")], U(*rollback_event_id))?;
    cell(&mut emit, &[C("prior_reorg_epoch")], U(*prior_reorg_epoch))?;
    cell(
        &mut emit,
        &[C("committed_reorg_epoch")],
        U(*committed_reorg_epoch),
    )?;
    strings("readiness_blockers", readiness_blockers, &mut emit)?;
    strings("invalidated_batch_ids", invalidated_batch_ids, &mut emit)?;
    cell(
        &mut emit,
        &[C("dispositions"), C("count")],
        U(count(dispositions.len())?),
    )?;
    for (i, disposition) in dispositions.iter().enumerate() {
        let ordinal = count(i)?;
        visit_disposition(disposition, &mut |path, value| {
            let mut full = [C("dispositions"), O(ordinal), C(""), C("")];
            full[2..2 + path.len()].copy_from_slice(path);
            emit(&full[..2 + path.len()], value)
        })?;
    }
    cell(
        &mut emit,
        &[C("receipt"), C("present")],
        B(receipt.is_some()),
    )?;
    if let Some(receipt) = receipt {
        visit_receipt(receipt, &mut |path, value| {
            let mut full = [C("receipt"), C(""), C(""), C("")];
            full[1..1 + path.len()].copy_from_slice(path);
            emit(&full[..1 + path.len()], value)
        })?;
    }
    Ok(())
}
fn visit_disposition<'a, E>(
    source: &'a RollbackEventDisposition,
    emit: &mut impl FnMut(&[RollbackPathV1], RollbackValueRefV1<'a>) -> Result<(), E>,
) -> Result<(), RollbackVisitErrorV1<E>> {
    let RollbackEventDisposition {
        source_batch_id,
        source_event_id,
        status,
        compensation,
        player_id,
        action_id,
        system_authored,
    } = source;
    cell(emit, &[C("source_batch_id")], T(source_batch_id))?;
    cell(emit, &[C("source_event_id")], U(*source_event_id))?;
    cell(
        emit,
        &[C("status")],
        U(match status {
            RollbackDispositionStatus::PreservedAtTarget => 0,
            RollbackDispositionStatus::Replayed => 1,
            RollbackDispositionStatus::RejectedFork => 2,
            RollbackDispositionStatus::CompensationRequired => 3,
        }),
    )?;
    cell(emit, &[C("system_authored")], B(*system_authored))?;
    for (name, value) in [("player_id", player_id), ("action_id", action_id)] {
        cell(emit, &[C(name), C("present")], B(value.is_some()))?;
        if let Some(value) = value {
            cell(emit, &[C(name), C("value")], T(value))?;
        }
    }
    cell(
        emit,
        &[C("compensation"), C("present")],
        B(compensation.is_some()),
    )?;
    if let Some(RollbackCompensationCaseRef {
        owner_id,
        ticket_id,
        state,
    }) = compensation
    {
        cell(emit, &[C("compensation"), C("owner_id")], T(owner_id))?;
        cell(emit, &[C("compensation"), C("ticket_id")], T(ticket_id))?;
        cell(
            emit,
            &[C("compensation"), C("state")],
            U(match state {
                RollbackCompensationState::PendingAuthorization => 0,
                RollbackCompensationState::Authorized => 1,
                RollbackCompensationState::InProgress => 2,
                RollbackCompensationState::Completed => 3,
                RollbackCompensationState::Rejected => 4,
            }),
        )?;
    }
    Ok(())
}
fn visit_receipt<'a, E>(
    source: &'a RollbackReceiptProjection,
    emit: &mut impl FnMut(&[RollbackPathV1], RollbackValueRefV1<'a>) -> Result<(), E>,
) -> Result<(), RollbackVisitErrorV1<E>> {
    let RollbackReceiptProjection {
        receipt_id,
        canonical_intent_digest,
        rollback_checkpoint_batch_id,
        rollback_checkpoint_snapshot_hash,
        replay_target_batch_id,
        replay_target_state_root,
        replay_target_journal_commitment,
        affected_census_digest,
        snapshot_hash,
        rollback_checkpoint_journal_len,
        replay_target_journal_len,
        affected_census_count,
        snapshot_height,
        log_cursor,
        acknowledged_at_tick,
        readiness_blockers,
    } = source;
    cell(emit, &[C("receipt_id")], T(receipt_id))?;
    cell(
        emit,
        &[C("canonical_intent_digest")],
        T(canonical_intent_digest),
    )?;
    cell(
        emit,
        &[C("rollback_checkpoint_batch_id")],
        T(rollback_checkpoint_batch_id),
    )?;
    cell(
        emit,
        &[C("rollback_checkpoint_snapshot_hash")],
        T(rollback_checkpoint_snapshot_hash),
    )?;
    cell(
        emit,
        &[C("replay_target_batch_id")],
        T(replay_target_batch_id),
    )?;
    cell(
        emit,
        &[C("replay_target_state_root")],
        T(replay_target_state_root),
    )?;
    cell(
        emit,
        &[C("replay_target_journal_commitment")],
        T(replay_target_journal_commitment),
    )?;
    cell(
        emit,
        &[C("affected_census_digest")],
        T(affected_census_digest),
    )?;
    cell(emit, &[C("snapshot_hash")], T(snapshot_hash))?;
    cell(
        emit,
        &[C("rollback_checkpoint_journal_len")],
        U(count(*rollback_checkpoint_journal_len)?),
    )?;
    cell(
        emit,
        &[C("replay_target_journal_len")],
        U(count(*replay_target_journal_len)?),
    )?;
    cell(
        emit,
        &[C("affected_census_count")],
        U(count(*affected_census_count)?),
    )?;
    cell(emit, &[C("snapshot_height")], U(*snapshot_height))?;
    cell(emit, &[C("log_cursor")], U(*log_cursor))?;
    cell(emit, &[C("acknowledged_at_tick")], U(*acknowledged_at_tick))?;
    strings("readiness_blockers", readiness_blockers, emit)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authority_preserves_original_identity_and_stops_on_sink_failure() {
        let row = RollbackAuthorityRecord {
            authority_id: " inner ".into(),
            role: RollbackAuthorityRole::Governance,
            public_key_hex: "ABC".into(),
            active: false,
        };
        let mut n = 0;
        visit_rollback_authority_v1(&row, |path, value| -> Result<(), ()> {
            match n {
                0 => {
                    assert_eq!(path, &[C("authority_id")]);
                    assert_eq!(value, T(" inner "));
                }
                1 => assert_eq!(value, U(1)),
                2 => assert_eq!(value, T("ABC")),
                3 => assert_eq!(value, B(false)),
                _ => panic!("extra column"),
            };
            n += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(n, 4);
        let mut calls = 0;
        let result = visit_rollback_authority_v1(&row, |_, _| {
            calls += 1;
            Err("cancelled")
        });
        assert!(matches!(
            result,
            Err(RollbackVisitErrorV1::Sink("cancelled"))
        ));
        assert_eq!(calls, 1);
    }
    #[test]
    fn disposition_none_and_some_empty_are_distinct() {
        let mut row = RollbackEventDisposition {
            source_batch_id: "original".into(),
            source_event_id: u64::MAX,
            status: RollbackDispositionStatus::RejectedFork,
            compensation: None,
            player_id: None,
            action_id: Some(String::new()),
            system_authored: true,
        };
        let mut player_presence = false;
        let mut action_presence = false;
        let mut action_empty = false;
        visit_disposition(&row, &mut |path, value| -> Result<(), ()> {
            if path == [C("player_id"), C("present")] {
                assert_eq!(value, B(false));
                player_presence = true;
            }
            if path == [C("action_id"), C("present")] {
                assert_eq!(value, B(true));
                action_presence = true;
            }
            if path == [C("action_id"), C("value")] {
                assert_eq!(value, T(""));
                action_empty = true;
            }
            Ok(())
        })
        .unwrap();
        assert!(player_presence && action_presence && action_empty);
        row.compensation = Some(RollbackCompensationCaseRef {
            owner_id: "o".into(),
            ticket_id: "t".into(),
            state: RollbackCompensationState::Completed,
        });
        let mut saw_state = false;
        visit_disposition(&row, &mut |path, value| -> Result<(), ()> {
            if path == [C("compensation"), C("state")] {
                assert_eq!(value, U(3));
                saw_state = true;
            }
            Ok(())
        })
        .unwrap();
        assert!(saw_state);
    }
}
