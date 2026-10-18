//! Typed proof and healthy-transition replay from an externally trusted cut.
use super::*;

pub(super) struct ReplayedHistoryCut {
    pub(super) anchor: VerifiedFormalReadAnchorV1,
    pub(super) target_proof: Option<FormalQualifiedCommitProofV1>,
    pub(super) target_recovery: Option<usize>,
    pub(super) activation: Option<VerifiedDurableSetActivationV1>,
    pub(super) activation_predecessor: Option<VerifiedFormalReadAnchorV1>,
    pub(super) activation_step:
        Option<oasis7::world_service::formal_read::FormalNormalDurableSetReadStepV1>,
}

pub(super) fn requested_anchor_matches(
    anchor: &VerifiedFormalReadAnchorV1,
    requested: &FormalReadAnchorRefV1,
) -> Result<bool, String> {
    Ok(anchor.head().record().position == requested.position
        && anchor.head().head().record_id == requested.record_id
        && anchor
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?
            == requested.active_authority_id)
}

/// Requalify a DTO cut from an externally verified origin. `proof_at` may use
/// an exact private proof cache or a native protected restore, but every
/// returned proof is replayed against the typed cursor before it can advance.
/// Scratch state stays within the protocol's page bound; total history length
/// is limited only by the absolute operation deadline.
#[cfg(test)]
pub(super) fn replay_exact_origin_from(
    external: &VerifiedFormalReadAnchorV1,
    requested: &FormalReadAnchorRefV1,
    permissions: &[VerifiedRecoveryBootstrapV1],
    deadline: Instant,
    proof_at: impl FnMut(
        &VerifiedFormalReadAnchorV1,
        u64,
        Option<usize>,
        Instant,
    ) -> Result<FormalQualifiedCommitProofV1, String>,
) -> Result<ReplayedHistoryCut, String> {
    replay_exact_origin_from_with_activations(
        external,
        requested,
        permissions,
        deadline,
        |_| Ok(None),
        proof_at,
    )
}

pub(super) fn replay_exact_origin_from_with_activations(
    external: &VerifiedFormalReadAnchorV1,
    requested: &FormalReadAnchorRefV1,
    permissions: &[VerifiedRecoveryBootstrapV1],
    deadline: Instant,
    mut activation_at: impl FnMut(
        u64,
    ) -> Result<
        Option<oasis7::world_service::formal_read::FormalNormalDurableSetReadStepV1>,
        String,
    >,
    mut proof_at: impl FnMut(
        &VerifiedFormalReadAnchorV1,
        u64,
        Option<usize>,
        Instant,
    ) -> Result<FormalQualifiedCommitProofV1, String>,
) -> Result<ReplayedHistoryCut, String> {
    replay_exact_origin_after_verified_with_activation_policy(
        external,
        requested,
        permissions,
        deadline,
        ReplayTargetPolicy::Exact,
        |cursor, position, pending_permission, operation_deadline| {
            proof_at(cursor, position, pending_permission, operation_deadline)
                .map(|proof| ReplayStepProof {
                    proof,
                    candidate: None,
                })
                .map_err(ReplayFailure::Invalid)
        },
        |position| activation_at(position).map_err(ReplayFailure::Invalid),
        |_, _, _, _| Ok(()),
    )
    .map_err(ReplayFailure::into_string)
}

#[derive(Debug)]
pub(super) enum ReplayFailure {
    LocalPublicationMissing,
    DeadlineExceeded,
    TargetMismatch,
    UnqualifiedPendingRecoveryTarget(Box<FormalReadCheckpointV1>),
    Invalid(String),
}

impl ReplayFailure {
    fn into_string(self) -> String {
        match self {
            Self::LocalPublicationMissing => {
                "HistoryUnavailable: exact local formal publication is missing".into()
            }
            Self::DeadlineExceeded => {
                "HistoryUnavailable: external-origin replay deadline exceeded".into()
            }
            Self::TargetMismatch => {
                "HistoryUnavailable: protected head hint differs from verified replay".into()
            }
            Self::UnqualifiedPendingRecoveryTarget(_) => {
                "HistoryUnavailable: pending recovery target is not a qualified read cut".into()
            }
            Self::Invalid(error) => error,
        }
    }
}

pub(super) struct ReplayStepProof {
    pub(super) proof: FormalQualifiedCommitProofV1,
    pub(super) candidate: Option<FormalCandidateMaterialV1>,
}

/// Replay exact local publications from the external origin. The observer is
/// called only after the proof advances the opaque cursor successfully.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(super) fn replay_exact_origin_after_verified(
    external: &VerifiedFormalReadAnchorV1,
    requested: &FormalReadAnchorRefV1,
    permissions: &[VerifiedRecoveryBootstrapV1],
    deadline: Instant,
    proof_at: impl FnMut(
        &VerifiedFormalReadAnchorV1,
        u64,
        Option<usize>,
        Instant,
    ) -> Result<ReplayStepProof, ReplayFailure>,
    after_verified: impl FnMut(
        &VerifiedFormalReadAnchorV1,
        &VerifiedFormalReadAnchorV1,
        &FormalQualifiedCommitProofV1,
        Option<&FormalCandidateMaterialV1>,
    ) -> Result<(), ReplayFailure>,
) -> Result<ReplayedHistoryCut, ReplayFailure> {
    replay_exact_origin_after_verified_with_activation_policy(
        external,
        requested,
        permissions,
        deadline,
        ReplayTargetPolicy::Exact,
        proof_at,
        |_| Ok(None),
        after_verified,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplayTargetPolicy {
    Exact,
    ProtectedHeadHint,
}

/// The hint mode changes only mismatch handling: a verified chain that reaches
/// a different head is an optional warmup miss. Every returned proof and any
/// local candidate still passes the same canonical replay and closure checks.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(super) fn replay_exact_origin_after_verified_with_policy(
    external: &VerifiedFormalReadAnchorV1,
    requested: &FormalReadAnchorRefV1,
    permissions: &[VerifiedRecoveryBootstrapV1],
    deadline: Instant,
    target_policy: ReplayTargetPolicy,
    proof_at: impl FnMut(
        &VerifiedFormalReadAnchorV1,
        u64,
        Option<usize>,
        Instant,
    ) -> Result<ReplayStepProof, ReplayFailure>,
    after_verified: impl FnMut(
        &VerifiedFormalReadAnchorV1,
        &VerifiedFormalReadAnchorV1,
        &FormalQualifiedCommitProofV1,
        Option<&FormalCandidateMaterialV1>,
    ) -> Result<(), ReplayFailure>,
) -> Result<ReplayedHistoryCut, ReplayFailure> {
    replay_exact_origin_after_verified_with_activation_policy(
        external,
        requested,
        permissions,
        deadline,
        target_policy,
        proof_at,
        |_| Ok(None),
        after_verified,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn replay_exact_origin_after_verified_with_activation_policy(
    external: &VerifiedFormalReadAnchorV1,
    requested: &FormalReadAnchorRefV1,
    permissions: &[VerifiedRecoveryBootstrapV1],
    deadline: Instant,
    target_policy: ReplayTargetPolicy,
    mut proof_at: impl FnMut(
        &VerifiedFormalReadAnchorV1,
        u64,
        Option<usize>,
        Instant,
    ) -> Result<ReplayStepProof, ReplayFailure>,
    mut activation_at: impl FnMut(
        u64,
    ) -> Result<
        Option<oasis7::world_service::formal_read::FormalNormalDurableSetReadStepV1>,
        ReplayFailure,
    >,
    mut after_verified: impl FnMut(
        &VerifiedFormalReadAnchorV1,
        &VerifiedFormalReadAnchorV1,
        &FormalQualifiedCommitProofV1,
        Option<&FormalCandidateMaterialV1>,
    ) -> Result<(), ReplayFailure>,
) -> Result<ReplayedHistoryCut, ReplayFailure> {
    if requested.position < external.head().record().position {
        return Err(match target_policy {
            ReplayTargetPolicy::Exact => ReplayFailure::Invalid(
                "HistoryUnavailable: requested origin precedes external trust".into(),
            ),
            ReplayTargetPolicy::ProtectedHeadHint => ReplayFailure::TargetMismatch,
        });
    }
    let mut cursor = external.clone();
    let mut next_permission = 0;
    let mut previous_permission = None;
    let mut previous_emitted_in_page = false;
    let mut pending_permission = None;
    let mut target_proof = None;
    let mut target_recovery = None;
    let mut last_activation = None;
    let mut last_activation_predecessor = None;
    let mut last_activation_step = None;
    let mut bounded_steps = Vec::with_capacity(MAX_FORMAL_HISTORY_STEPS);

    loop {
        if Instant::now() >= deadline {
            return Err(ReplayFailure::DeadlineExceeded);
        }
        // Stop before a same-position recovery when the DTO names the authority
        // that was current immediately before that transition.
        if requested_anchor_matches(&cursor, requested).map_err(ReplayFailure::Invalid)? {
            return Ok(ReplayedHistoryCut {
                anchor: cursor,
                target_proof,
                target_recovery,
                activation: last_activation,
                activation_predecessor: last_activation_predecessor,
                activation_step: last_activation_step,
            });
        }
        if cursor.head().record().position > requested.position {
            return Err(match target_policy {
                ReplayTargetPolicy::Exact => ReplayFailure::Invalid(
                    "HistoryUnavailable: external-origin replay passed requested cut".into(),
                ),
                ReplayTargetPolicy::ProtectedHeadHint => ReplayFailure::TargetMismatch,
            });
        }

        bounded_steps.clear();
        apply_ready_recoveries_until(
            &mut cursor,
            permissions,
            &mut next_permission,
            &mut previous_permission,
            &mut previous_emitted_in_page,
            &mut pending_permission,
            &mut bounded_steps,
            MAX_FORMAL_HISTORY_STEPS,
            Some(requested),
        )
        .map_err(ReplayFailure::Invalid)?;
        if requested_anchor_matches(&cursor, requested).map_err(ReplayFailure::Invalid)? {
            return Ok(ReplayedHistoryCut {
                anchor: cursor,
                target_proof,
                target_recovery,
                activation: last_activation,
                activation_predecessor: last_activation_predecessor,
                activation_step: last_activation_step,
            });
        }
        let current_position = cursor.head().record().position;
        if current_position >= requested.position {
            if current_position == requested.position
                && requested.record_id == cursor.head().head().record_id
                && let Some(index) = pending_permission
            {
                let permission = &permissions[index];
                if permission.cut().record() == cursor.head().record()
                    && permission.cut().finalizing_authority_id()
                        == cursor.head().finalizing_authority_id()
                    && permission.recovery().body().sealed_cut == cursor.head().head()
                    && permission
                        .recovery()
                        .successor()
                        .authority_id()
                        .map_err(|error| ReplayFailure::Invalid(error.to_string()))?
                        == requested.active_authority_id
                {
                    let pending = cursor
                        .with_pending_recovery(permission.recovery())
                        .map_err(|error| ReplayFailure::Invalid(error.to_string()))?;
                    return Err(ReplayFailure::UnqualifiedPendingRecoveryTarget(Box::new(
                        pending.checkpoint(),
                    )));
                }
            }
            if target_policy == ReplayTargetPolicy::ProtectedHeadHint {
                return Err(ReplayFailure::TargetMismatch);
            }
            return Err(ReplayFailure::Invalid(
                "HistoryUnavailable: requested authority is not qualified at exact cut".into(),
            ));
        }

        let position = current_position.checked_add(1).ok_or_else(|| {
            ReplayFailure::Invalid("HistoryUnavailable: formal position overflow".into())
        })?;
        let completion_permission = pending_permission;
        let step = proof_at(&cursor, position, completion_permission, deadline)?;
        let proof = &step.proof;
        if Instant::now() >= deadline {
            return Err(ReplayFailure::DeadlineExceeded);
        }
        let predecessor = cursor.clone();
        cursor = if let Some(index) = completion_permission {
            advance_history_completion(&cursor, &permissions[index], proof)
        } else {
            advance_normal_history_proof(&cursor, proof)
        }
        .map_err(ReplayFailure::Invalid)?;
        after_verified(&predecessor, &cursor, proof, step.candidate.as_ref())?;
        // A caller may request the pre-activation authority at the exact H
        // boundary. Return before applying the local marker in that case.
        if requested_anchor_matches(&cursor, requested).map_err(ReplayFailure::Invalid)? {
            return Ok(ReplayedHistoryCut {
                anchor: cursor,
                target_proof: Some(proof.clone()),
                target_recovery: completion_permission,
                activation: last_activation,
                activation_predecessor: last_activation_predecessor,
                activation_step: last_activation_step,
            });
        }
        if let Some(marker) = activation_at(position)? {
            if completion_permission.is_some() {
                return Err(ReplayFailure::Invalid(
                    "healthy durable set cannot activate over unresolved recovery completion"
                        .into(),
                ));
            }
            let (activated, token) = advance_durable_set_marker(&predecessor, &cursor, &marker)
                .map_err(ReplayFailure::Invalid)?;
            cursor = activated;
            last_activation = Some(token);
            last_activation_predecessor = Some(predecessor);
            last_activation_step = Some(marker);
        }
        pending_permission = None;
        target_proof = Some(proof.clone());
        target_recovery = completion_permission;
        if bounded_steps.len() >= MAX_FORMAL_HISTORY_STEPS {
            bounded_steps.clear();
        }
        bounded_steps.push(FormalReadStepV1::Commit(Box::new(proof.clone())));
    }
}

pub(super) fn advance_durable_set_marker(
    predecessor: &VerifiedFormalReadAnchorV1,
    boundary: &VerifiedFormalReadAnchorV1,
    marker: &oasis7::world_service::formal_read::FormalNormalDurableSetReadStepV1,
) -> Result<(VerifiedFormalReadAnchorV1, VerifiedDurableSetActivationV1), String> {
    if boundary.head().record().position != marker.transition.body.boundary_position
        || predecessor.head().head() != marker.transition.body.predecessor
        || predecessor
            .authority()
            .authority_id()
            .map_err(|e| e.to_string())?
            != marker.transition.body.predecessor_authority_id
        || boundary.head().record().kind
            != oasis7_distfs::controlled_authority::formal::FormalRecordKindV1::Execution
        || boundary
            .head()
            .record()
            .durable_set_transition_digest
            .as_deref()
            != Some(
                marker
                    .transition
                    .body
                    .digest()
                    .map_err(|error| error.to_string())?
                    .as_str(),
            )
    {
        return Err("formal durable-set marker does not match exact H-1/H commits".into());
    }
    let transition =
        oasis7_distfs::controlled_authority::formal::verify_normal_durable_set_transition(
            &marker.transition,
            boundary.authority(),
            predecessor.head(),
            boundary.head(),
        )
        .map_err(|error| error.to_string())?;
    let activation = oasis7_distfs::controlled_authority::formal::verify_durable_set_activation(
        &transition,
        [&marker.old_seals[0], &marker.old_seals[1]],
        [&marker.new_installs[0], &marker.new_installs[1]],
    )
    .map_err(|error| error.to_string())?;
    let successor = boundary
        .with_durable_set_activation(&activation)
        .map_err(|error| error.to_string())?;
    Ok((successor, activation))
}

pub(super) fn decode_role<T: serde::de::DeserializeOwned>(
    material: &ClosedRecord,
    role: ArtifactRole,
) -> Result<T, String> {
    if role == ArtifactRole::Snapshot {
        let bytes = material.snapshot_bytes().map_err(|e| e.to_string())?;
        return serde_cbor::from_slice(&bytes).map_err(|e| e.to_string());
    }
    let hash = material
        .roots
        .get(&role)
        .ok_or("native publication artifact role missing")?;
    let object = material
        .objects
        .iter()
        .find(|object| &object.content_hash == hash)
        .ok_or("native publication artifact object missing")?;
    if blake3::hash(&object.bytes).to_hex().as_str() != hash {
        return Err("native publication artifact hash mismatch".into());
    }
    serde_cbor::from_slice(&object.bytes).map_err(|error| error.to_string())
}

pub(super) fn require_exact_history_target(
    cursor: &VerifiedFormalReadAnchorV1,
    target: &VerifiedFormalReadAnchorV1,
) -> Result<(), String> {
    if cursor.checkpoint() != target.checkpoint() {
        return Err("formal bounded history target mismatch".into());
    }
    Ok(())
}
