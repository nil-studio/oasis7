//! Typed cursor transitions for normal commits and verified recovery epochs.
use super::*;
use oasis7_distfs::controlled_authority::formal::RecoveryTailV1;

pub(super) fn advance_normal_history_proof(
    cursor: &VerifiedFormalReadAnchorV1,
    proof: &FormalQualifiedCommitProofV1,
) -> Result<VerifiedFormalReadAnchorV1, String> {
    if proof.decision.branch != FormalCompletionBranchV1::Normal {
        return Err("formal normal history step has a non-Normal completion".into());
    }
    let committed = oasis7_distfs::controlled_authority::formal::verify_formal_qualified(
        proof,
        cursor.authority(),
        Some(cursor.head()),
        None,
    )
    .map_err(|error| error.to_string())?;
    cursor
        .after_verified_commit(cursor.authority().clone(), committed)
        .map_err(|error| error.to_string())
}

fn append_recovery_read_step(
    permission: &VerifiedRecoveryBootstrapV1,
    steps: &mut Vec<FormalReadStepV1>,
    max_steps: usize,
) -> Result<(), String> {
    if steps.len() >= max_steps {
        return Err("HistoryUnavailable: recovery transitions require another bounded page".into());
    }
    let evidence = permission.permission_evidence();
    let fencing = match &evidence.fencing {
        oasis7_distfs::controlled_authority::formal::FencingBootstrapV1::Dual {
            primary,
            replica,
        } => oasis7::world_service::formal_read::FormalReadFencingEvidenceV1::HealthyPair(
            Box::new([*primary.clone(), *replica.clone()]),
        ),
        oasis7_distfs::controlled_authority::formal::FencingBootstrapV1::NecessaryEndpoint {
            seal,
        } => oasis7::world_service::formal_read::FormalReadFencingEvidenceV1::NecessaryEndpoint(
            seal.clone(),
        ),
    };
    steps.push(FormalReadStepV1::Recovery(Box::new(
        oasis7::world_service::formal_read::FormalRecoveryReadStepV1 {
            certificate: evidence.certificate.clone(),
            issuance: evidence.issuer_witness.clone(),
            fencing,
            original_tail: evidence.tail.clone(),
        },
    )));
    Ok(())
}

/// Apply only the next checked recovery whose complete sealed cut is the
/// cursor. Schema-1 nonempty tails retain the predecessor cursor until their
/// completion; schema-2 successors use the opaque retained context and the
/// immediately preceding checked permission. A cursor already at a verified
/// successor can carry that predecessor context without repeating its step.
#[expect(
    clippy::too_many_arguments,
    reason = "Recovery replay carries independently checked cursor, permission, and page-bound state."
)]
pub(super) fn apply_ready_recoveries(
    cursor: &mut VerifiedFormalReadAnchorV1,
    permissions: &[VerifiedRecoveryBootstrapV1],
    next_permission: &mut usize,
    previous_permission: &mut Option<usize>,
    previous_emitted_in_page: &mut bool,
    pending_permission: &mut Option<usize>,
    steps: &mut Vec<FormalReadStepV1>,
    max_steps: usize,
) -> Result<(), String> {
    apply_ready_recoveries_until(
        cursor,
        permissions,
        next_permission,
        previous_permission,
        previous_emitted_in_page,
        pending_permission,
        steps,
        max_steps,
        None,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "The bounded page replay keeps each cursor, permission, and stop condition explicit."
)]
pub(super) fn apply_ready_recoveries_until(
    cursor: &mut VerifiedFormalReadAnchorV1,
    permissions: &[VerifiedRecoveryBootstrapV1],
    next_permission: &mut usize,
    previous_permission: &mut Option<usize>,
    previous_emitted_in_page: &mut bool,
    pending_permission: &mut Option<usize>,
    steps: &mut Vec<FormalReadStepV1>,
    max_steps: usize,
    stop_at: Option<&FormalReadAnchorRefV1>,
) -> Result<(), String> {
    while *next_permission < permissions.len() {
        let index = *next_permission;
        let permission = &permissions[index];
        let recovery = permission.recovery();
        let cursor_position = cursor.head().record().position;
        let cut_position = permission.cut().record().position;

        if cut_position < cursor_position {
            // A caller may begin after a recovery already qualified into its
            // externally trusted origin. It is safe to ignore that old step,
            // but a retained successor cannot be recreated without its prior
            // typed recovery in this page.
            if recovery.body().schema_version == 2 {
                return Err(
                    "HistoryUnavailable: retained recovery starts after its exact sealed cut"
                        .into(),
                );
            }
            *next_permission += 1;
            *previous_permission = Some(index);
            *previous_emitted_in_page = recovery
                .successor()
                .authority_id()
                .map_err(|error| error.to_string())?
                == cursor
                    .authority()
                    .authority_id()
                    .map_err(|error| error.to_string())?;
            *pending_permission = None;
            if let Some(target) = stop_at
                && requested_anchor_matches(cursor, target)?
            {
                return Ok(());
            }
            continue;
        }
        if cut_position > cursor_position {
            break;
        }
        if permission.cut().record() != cursor.head().record()
            || permission.cut().finalizing_authority_id() != cursor.head().finalizing_authority_id()
            || recovery.body().sealed_cut != cursor.head().head()
        {
            return Err("HistoryUnavailable: recovery exact sealed cut differs from cursor".into());
        }

        match recovery.body().schema_version {
            1 => {
                if recovery
                    .predecessor()
                    .authority_id()
                    .map_err(|error| error.to_string())?
                    != cursor
                        .authority()
                        .authority_id()
                        .map_err(|error| error.to_string())?
                {
                    // A recovery already present in this typed cursor is not
                    // repeated in history. Its independently checked token is
                    // still the required context for a same-cut schema-2 step.
                    if recovery
                        .successor()
                        .authority_id()
                        .map_err(|error| error.to_string())?
                        == cursor
                            .authority()
                            .authority_id()
                            .map_err(|error| error.to_string())?
                    {
                        *next_permission += 1;
                        *previous_permission = Some(index);
                        // The typed cursor already carries this verified
                        // successor. Its retained recovery token remains the
                        // predecessor context for a same-cut schema-2 step.
                        *previous_emitted_in_page = true;
                        *pending_permission = None;
                        if let Some(target) = stop_at
                            && requested_anchor_matches(cursor, target)?
                        {
                            return Ok(());
                        }
                        continue;
                    }
                    return Err(
                        "HistoryUnavailable: recovery predecessor authority differs at sealed cut"
                            .into(),
                    );
                }
                append_recovery_read_step(permission, steps, max_steps)?;
                if recovery.body().tail == RecoveryTailV1::Empty {
                    *cursor = cursor
                        .with_empty_recovery(recovery)
                        .map_err(|error| error.to_string())?;
                    *pending_permission = None;
                } else {
                    // A schema-1 pending completion is replayed under its
                    // original authority, so the service cursor stays at the
                    // predecessor until that exact completion is verified.
                    *pending_permission = Some(index);
                }
            }
            2 => {
                let previous_index = previous_permission.ok_or_else(|| {
                    "HistoryUnavailable: retained recovery lacks prior permission".to_owned()
                })?;
                if !*previous_emitted_in_page {
                    return Err(
                        "HistoryUnavailable: retained recovery lacks its replayed predecessor"
                            .into(),
                    );
                }
                let previous = &permissions[previous_index];
                let context = permission.retained_context().ok_or_else(|| {
                    "HistoryUnavailable: retained recovery context unavailable".to_owned()
                })?;
                *cursor = cursor
                    .with_verified_retained_recovery(previous.recovery(), context, recovery)
                    .map_err(|error| error.to_string())?;
                append_recovery_read_step(permission, steps, max_steps)?;
                *pending_permission =
                    (recovery.body().tail != RecoveryTailV1::Empty).then_some(index);
            }
            _ => return Err("formal recovery certificate schema".into()),
        }
        *next_permission += 1;
        *previous_permission = Some(index);
        *previous_emitted_in_page = true;
        if let Some(target) = stop_at
            && requested_anchor_matches(cursor, target)?
        {
            return Ok(());
        }
    }
    Ok(())
}

fn recovery_tail_matches_normal(
    permission: &VerifiedRecoveryBootstrapV1,
    proof: &FormalQualifiedCommitProofV1,
) -> Result<bool, String> {
    let record_id = proof
        .decision
        .record
        .record_id()
        .map_err(|error| error.to_string())?;
    Ok(matches!(
        &permission.recovery().body().tail,
        RecoveryTailV1::Prepared { record_id: retained, .. }
            | RecoveryTailV1::Decision { record_id: retained, .. }
            if retained == &record_id
    ))
}

pub(super) fn advance_history_completion(
    cursor: &VerifiedFormalReadAnchorV1,
    permission: &VerifiedRecoveryBootstrapV1,
    proof: &FormalQualifiedCommitProofV1,
) -> Result<VerifiedFormalReadAnchorV1, String> {
    let recovery = permission.recovery();
    let normal = proof.decision.branch == FormalCompletionBranchV1::Normal;
    if normal && !recovery_tail_matches_normal(permission, proof)? {
        return Err("formal recovery cannot replace an unrelated Normal proof".into());
    }
    let committed = oasis7_distfs::controlled_authority::formal::verify_formal_qualified(
        proof,
        recovery.original_authority(),
        Some(cursor.head()),
        (!normal).then_some(recovery),
    )
    .map_err(|error| error.to_string())?;
    if recovery.body().schema_version == 2 {
        let context = permission.retained_context().ok_or_else(|| {
            "HistoryUnavailable: retained completion context unavailable".to_owned()
        })?;
        cursor
            .after_verified_retained_recovery_commit(context, recovery, committed)
            .map_err(|error| error.to_string())
    } else {
        cursor
            .after_verified_recovery_commit(recovery, committed)
            .map_err(|error| error.to_string())
    }
}
