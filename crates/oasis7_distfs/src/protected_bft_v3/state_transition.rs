//! Reuse only a locally verified exact prefix; every new durable step is checked.
use super::*;

pub(crate) fn validate_state_against_cache(
    binding: &BftSignerBindingV3,
    previous: &VerifiedBftSignerStateV3,
    next: &BftSignerStateV3,
    provider: &str,
) -> Result<VerifiedBftSignerStateV3, BftSignerErrorV3> {
    if previous.state.binding_digest != binding.digest()? {
        return Err(invalid("BFT V3 verified cache belongs to another binding"));
    }
    if next.revision == previous.state.revision {
        if next != &previous.state {
            return Err(invalid("BFT V3 state changed at the same revision"));
        }
        return Ok(previous.clone());
    }
    if previous.state.revision.checked_add(1) != Some(next.revision) {
        return Err(invalid(
            "BFT V3 state skipped or regressed its observed revision",
        ));
    }
    let verified = validate_candidate_transition(binding, previous, &next.candidate(), provider)?;
    if &verified.state != next {
        return Err(invalid(
            "BFT V3 state differs from its exact legal transition",
        ));
    }
    Ok(verified)
}

pub(crate) fn validate_candidate_transition(
    binding: &BftSignerBindingV3,
    previous: &VerifiedBftSignerStateV3,
    candidate: &BftSignerCandidateV3,
    _provider: &str,
) -> Result<VerifiedBftSignerStateV3, BftSignerErrorV3> {
    let old = &previous.state;
    if old.binding_digest != binding.digest()? {
        return Err(invalid("BFT V3 verified cache belongs to another binding"));
    }
    let mut schedule = previous.schedule.clone();
    if candidate.history.len() < old.history.len()
        || candidate.history.get(..old.history.len()) != Some(old.history.as_slice())
    {
        return Err(invalid("BFT V3 history is not an exact prefix extension"));
    }
    super::super::membership::validate_change(binding, old, candidate, _provider)?;
    let added = candidate.history.len() - old.history.len();
    if added > 1 {
        return Err(invalid("BFT V3 candidate appends more than one QC"));
    }
    let mut expected_archives = old.completed_journals.clone();
    if added == 1 {
        expected_archives.push(old.journal.clone());
    }
    if candidate.completed_journals != expected_archives {
        return Err(invalid(
            "BFT V3 completed journals must retain the exact original signed slots",
        ));
    }
    if added == 1 {
        if old.pending_driver_apply.is_some() || candidate.applied_installs != old.applied_installs
        {
            return Err(invalid("BFT V3 unconfirmed driver install blocks new QC"));
        }
        let certificate = candidate
            .history
            .last()
            .ok_or_else(|| invalid("BFT V3 missing append QC"))?;
        let verified = verify_formal_bft_header_certificate_v3(&previous.schedule, certificate)
            .map_err(invalid)?;
        let expected = journal_for_schedule(binding, verified.next_height_schedule())?;
        schedule = verified.next_height_schedule().clone();
        if candidate.journal != expected
            || candidate.pending_driver_apply
                != Some(BftDriverApplyIntentV3 {
                    height: certificate.execution_commitment.height,
                    header_id: verified.block_id().into(),
                })
        {
            return Err(invalid(
                "BFT V3 QC append did not retain exact pending driver install",
            ));
        }
        if let Some(journal) = &old.journal {
            if journal.pending_commit() != Some(certificate) {
                return Err(invalid("BFT V3 QC differs from exact reserved commit"));
            }
            let next = verify_reserved_bft_commit_transition_v3(
                journal,
                certificate,
                previous.schedule.active_set(),
                &previous.schedule,
                binding.timeouts,
            )
            .map_err(invalid)?;
            if &next != verified.next_height_schedule() {
                return Err(invalid("BFT V3 reserved commit schedule differs"));
            }
        }
    } else if let Some(pending) = &old.pending_driver_apply {
        if candidate.history != old.history
            || candidate.journal != old.journal
            || candidate.pending_driver_apply.is_some()
            || candidate.applied_installs.len() != old.applied_installs.len() + 1
            || !candidate
                .applied_installs
                .starts_with(&old.applied_installs)
            || !candidate
                .applied_installs
                .last()
                .is_some_and(|r| r.height == pending.height && r.header_id == pending.header_id)
        {
            return Err(invalid(
                "BFT V3 pending apply can only finish its exact original receipt",
            ));
        }
    } else {
        if candidate.pending_driver_apply != old.pending_driver_apply
            || candidate.applied_installs != old.applied_installs
            || candidate.history != old.history
        {
            return Err(invalid("BFT V3 protocol advance changed install state"));
        }
        match (&old.journal, &candidate.journal) {
            (Some(a), Some(b)) if a != b => {
                let derived = verify_bft_signing_journal_transition_v3(
                    a,
                    b,
                    previous.schedule.active_set(),
                    &previous.schedule,
                    binding.timeouts,
                )
                .map_err(invalid)?;
                // A protocol event cannot install a successor height without
                // the separately checked QC append and driver confirmation.
                if derived != previous.schedule {
                    return Err(invalid(
                        "BFT V3 journal advance cannot install a new height",
                    ));
                }
            }
            _ if candidate.membership_install != old.membership_install
                && candidate.journal == old.journal => {}
            _ => {
                return Err(invalid(
                    "BFT V3 candidate is not one legal journal transition",
                ));
            }
        }
    }
    let revision = old
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("BFT V3 revision overflow"))?;
    let next = BftSignerStateV3::assemble_candidate(binding, revision, candidate.clone())?;
    validate_state_envelope(binding, &next, Some(_provider))?;
    Ok(VerifiedBftSignerStateV3 {
        state: next,
        schedule,
    })
}
