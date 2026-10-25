use oasis7_consensus::tendermint_v1::{
    BftHeightProposerScheduleV1, verify_bft_signing_journal_transition_v1,
};

use super::super::backend::BftSignerBackendSnapshotV1;
use super::super::binding::{BftSignerBindingV1, BftSignerError, invalid};
use super::super::validation::{VerifiedBftSignerStateV1, validate_state};

pub(super) fn validate_acquired_snapshot(
    binding: &BftSignerBindingV1,
    previous: &BftSignerBackendSnapshotV1,
    acquired: &BftSignerBackendSnapshotV1,
    expected_generation: u64,
    expected_token_digest: &str,
) -> Result<(), BftSignerError> {
    if acquired.lease_generation != expected_generation
        || acquired.lease_token_digest != expected_token_digest
        || acquired.protected_time_floor_ms < previous.protected_time_floor_ms
        || acquired.state.revision < previous.state.revision
        || (acquired.state.revision == previous.state.revision && acquired.state != previous.state)
    {
        return Err(invalid(
            "acquired lease does not extend the pre-CAS backend snapshot",
        ));
    }
    if acquired.state.binding_digest != binding.digest()? {
        return Err(invalid("acquired BFT signer state binding mismatch"));
    }
    Ok(())
}

pub(super) fn validate_backend_snapshot(
    binding: &BftSignerBindingV1,
    snapshot: &BftSignerBackendSnapshotV1,
) -> Result<BftHeightProposerScheduleV1, BftSignerError> {
    validate_backend_snapshot_header(binding, snapshot)?;
    validate_state(binding, &snapshot.state)
}

pub(super) fn validate_backend_snapshot_against_cache(
    binding: &BftSignerBindingV1,
    snapshot: &BftSignerBackendSnapshotV1,
    verified: &VerifiedBftSignerStateV1,
    expected_generation: u64,
    expected_token_digest: &str,
    observed_clock_floor_ms: i64,
) -> Result<(), BftSignerError> {
    validate_backend_snapshot_header(binding, snapshot)?;
    if snapshot.lease_generation != expected_generation
        || snapshot.lease_token_digest != expected_token_digest
    {
        return Err(invalid("BFT signer lease was fenced"));
    }
    if snapshot.protected_time_floor_ms < observed_clock_floor_ms {
        return Err(invalid("BFT signer backend clock floor rollback"));
    }
    if snapshot.state != verified.state {
        return Err(invalid(
            "BFT signer backend state differs from verified cache at this lease revision",
        ));
    }
    Ok(())
}

pub(super) fn validate_monotonic_state_extension(
    binding: &BftSignerBindingV1,
    previous: &VerifiedBftSignerStateV1,
    next: &VerifiedBftSignerStateV1,
) -> Result<(), BftSignerError> {
    if next.state.revision < previous.state.revision {
        return Err(invalid("BFT signer backend revision rollback"));
    }
    if next.state.revision == previous.state.revision {
        return if next.state == previous.state {
            Ok(())
        } else {
            Err(invalid(
                "BFT signer state changed at an already observed revision",
            ))
        };
    }
    let expected_revision = previous
        .state
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("BFT signer revision overflow"))?;
    if next.state.revision != expected_revision {
        return Err(invalid(
            "BFT signer state extension skipped an unobserved revision",
        ));
    }
    if next.state.history.len() < previous.state.history.len()
        || next.state.history.get(..previous.state.history.len())
            != Some(previous.state.history.as_slice())
    {
        return Err(invalid("BFT signer committed history prefix changed"));
    }

    let history_delta = next.state.history.len() - previous.state.history.len();
    if history_delta > 1 {
        return Err(invalid(
            "BFT signer extension skips unobserved committed heights",
        ));
    }
    if history_delta == 0 && next.state.journal.height != previous.state.journal.height {
        return Err(invalid("BFT signer current-height journal changed height"));
    }
    if history_delta == 0 && next.state.journal == previous.state.journal {
        return Err(invalid(
            "BFT signer revision advanced without a journal transition",
        ));
    }
    if history_delta == 1
        && previous.state.journal.pending_commit()
            != next.state.history.get(previous.state.history.len())
    {
        return Err(invalid(
            "BFT signer committed history did not consume the exact observed pending QC",
        ));
    }

    let set = binding.validate()?;
    let next_schedule = verify_bft_signing_journal_transition_v1(
        &previous.state.journal,
        &next.state.journal,
        &set,
        &previous.schedule,
        binding.timeouts,
    )
    .map_err(|error| invalid(format!("BFT signer cached-state transition: {error}")))?;
    if next_schedule != next.schedule {
        return Err(invalid(
            "BFT signer cached-state transition produced a different schedule",
        ));
    }
    Ok(())
}

fn validate_backend_snapshot_header(
    binding: &BftSignerBindingV1,
    snapshot: &BftSignerBackendSnapshotV1,
) -> Result<(), BftSignerError> {
    let expected_digest = binding.digest()?;
    if snapshot.state.binding_digest != expected_digest || snapshot.protected_time_floor_ms < 0 {
        return Err(invalid("BFT signer backend state binding or clock floor"));
    }
    if snapshot.lease_generation == 0 {
        if !snapshot.lease_token_digest.is_empty() {
            return Err(invalid("unleased BFT signer state has a lease token"));
        }
    } else if !snapshot.lease_token_digest.is_empty() {
        super::helpers::validate_hex_digest(&snapshot.lease_token_digest)?;
    }
    let latest_event_time = snapshot
        .state
        .journal
        .events()
        .last()
        .map(|event| event.event_time_ms())
        .into_iter()
        .chain(snapshot.state.journal.height_started_at_ms())
        .max()
        .unwrap_or(0);
    if snapshot.protected_time_floor_ms < latest_event_time {
        return Err(invalid(
            "BFT signer clock floor is behind durable journal events",
        ));
    }
    Ok(())
}
