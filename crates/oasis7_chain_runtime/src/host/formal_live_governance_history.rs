//! Semantic admission-history checks after each externally rooted proof step.
use super::*;
#[path = "formal_live_governance_read_source.rs"]
mod read_source;
use read_source::{GovernanceProjection, GovernanceProjectionReader};

#[cfg(all(test, unix))]
#[path = "formal_live_governance_history_tests.rs"]
mod tests;

pub(super) struct GovernanceHistoryCursor {
    checkpoint: FormalReadCheckpointV1,
    authority_context: String,
    projection: GovernanceProjection,
}

fn remaining(deadline: Instant) -> Result<(), FormalReadError> {
    if Instant::now() >= deadline {
        return Err(FormalReadError::HistoryUnavailable(
            "HistoryUnavailable: governance history deadline exceeded".into(),
        ));
    }
    Ok(())
}

fn verify_governance_successor(
    before: &GovernanceProjection,
    next: &GovernanceProjection,
    mut check_budget: impl FnMut() -> Result<(), FormalReadError>,
) -> Result<(), FormalReadError> {
    use oasis7_runtime::GovernanceHistoryVerificationError;
    // Projections belong only to this invocation's private replay cursor. Reset
    // both captures before each step and drain them immediately afterward.
    before.reader.take_error()?;
    next.reader.take_error()?;
    let mut budget_error = None;
    let verified = before.snapshot.verify_successor_typed(&next.snapshot, || {
        check_budget().map_err(|error| {
            let diagnostic = error.to_string();
            budget_error = Some(error);
            diagnostic
        })
    });
    let source_error = before.reader.take_error()?.or(next.reader.take_error()?);
    verified.map_err(|error| match error {
        GovernanceHistoryVerificationError::Budget(diagnostic) => {
            budget_error.unwrap_or(FormalReadError::HistoryUnavailable(diagnostic))
        }
        GovernanceHistoryVerificationError::Capacity(diagnostic) => {
            FormalReadError::HistoryUnavailable(diagnostic)
        }
        GovernanceHistoryVerificationError::Invalid(diagnostic) => {
            source_error.unwrap_or_else(|| {
                FormalReadError::Integrity(format!(
                    "HistoryUnavailable: governance history authentication: {diagnostic}"
                ))
            })
        }
    })
}

impl NativeHistoryResolver {
    pub(super) fn verify_governance_history_step(
        &self,
        cursor: &mut Option<GovernanceHistoryCursor>,
        predecessor: &VerifiedFormalReadAnchorV1,
        anchor: &VerifiedFormalReadAnchorV1,
        proof: &FormalQualifiedCommitProofV1,
        candidate: Option<&FormalCandidateMaterialV1>,
        deadline: Instant,
    ) -> Result<(), FormalReadError> {
        remaining(deadline)?;
        let before_version = predecessor.authority().definition().profile_version;
        let after_version = anchor.authority().definition().profile_version;
        if before_version != 2 && after_version != 2 {
            return Ok(());
        }
        if before_version != 2 || after_version != 2 {
            return Err(FormalReadError::Integrity(
                "HistoryUnavailable: governance history profile transition unsupported".into(),
            ));
        }
        if anchor.head().record().parent_record_id != predecessor.head().head().record_id {
            return Err(FormalReadError::Integrity(
                "HistoryUnavailable: governance history predecessor mismatch".into(),
            ));
        }
        let context = predecessor
            .authority()
            .context_digest()
            .map_err(FormalReadError::from)?;
        if cursor.as_ref().is_none_or(|cached| {
            cached.checkpoint != predecessor.checkpoint() || cached.authority_context != context
        }) {
            *cursor = Some(GovernanceHistoryCursor {
                checkpoint: predecessor.checkpoint(),
                authority_context: context,
                projection: self.governance_projection_at(predecessor, None, None, deadline)?,
            });
        }
        let next = self.governance_projection_at(anchor, Some(proof), candidate, deadline)?;
        let before = &cursor
            .as_ref()
            .ok_or_else(|| {
                FormalReadError::Integrity(
                    "HistoryUnavailable: governance predecessor unavailable".into(),
                )
            })?
            .projection;
        verify_governance_successor(before, &next, || remaining(deadline))?;
        remaining(deadline)?;
        *cursor = Some(GovernanceHistoryCursor {
            checkpoint: anchor.checkpoint(),
            authority_context: anchor
                .authority()
                .context_digest()
                .map_err(FormalReadError::from)?,
            projection: next,
        });
        Ok(())
    }

    fn governance_projection_at(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
        proof: Option<&FormalQualifiedCommitProofV1>,
        candidate: Option<&FormalCandidateMaterialV1>,
        deadline: Instant,
    ) -> Result<GovernanceProjection, FormalReadError> {
        remaining(deadline)?;
        if let Some(candidate) = candidate {
            let proof = proof.ok_or_else(|| {
                FormalReadError::Integrity(
                    "HistoryUnavailable: governance candidate lacks verified proof".into(),
                )
            })?;
            let cut = self.governance_cut(anchor, Some(proof.clone()))?;
            let expanded = self.validate_material_for_replayed_cut(&cut, candidate, proof)?;
            return self.governance_projection_from_material(
                anchor,
                &candidate.material,
                expanded,
                deadline,
            );
        }
        let (material, expanded) =
            self.governance_material_with_snapshot(anchor, proof, deadline)?;
        self.governance_projection_from_material(anchor, &material, expanded, deadline)
    }

    fn governance_projection_from_material(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
        material: &ClosedRecord,
        expanded: Option<Vec<u8>>,
        deadline: Instant,
    ) -> Result<GovernanceProjection, FormalReadError> {
        remaining(deadline)?;
        let bytes = match expanded {
            Some(bytes) => bytes,
            None => material
                .snapshot_bytes()
                .map_err(|e| FormalReadError::Integrity(e.to_string()))?,
        };
        let parent = oasis7_runtime::VerifiedFormalRetainedHistoryParentV1::verify(anchor, &bytes)
            .map_err(FormalReadError::Integrity)?;
        let journal = replay::decode_journal_role(
            material,
            oasis7_distfs::controlled_authority::replicated_protocol::ArtifactRole::Journal,
        )?;
        let reader = GovernanceProjectionReader::new(self.business_reader.clone());
        let projection =
            oasis7_runtime::VerifiedGovernanceHistorySnapshot::from_verified_snapshot_bytes(
                &bytes,
                journal,
                parent,
                reader.clone(),
            )
            .map_err(FormalReadError::Integrity)?;
        remaining(deadline)?;
        Ok(GovernanceProjection {
            snapshot: projection,
            reader,
        })
    }

    pub(super) fn governance_material_at(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
        known_proof: Option<&FormalQualifiedCommitProofV1>,
        deadline: Instant,
    ) -> Result<ClosedRecord, FormalReadError> {
        self.governance_material_with_snapshot(anchor, known_proof, deadline)
            .map(|(material, _)| material)
    }

    fn governance_material_with_snapshot(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
        known_proof: Option<&FormalQualifiedCommitProofV1>,
        deadline: Instant,
    ) -> Result<(ClosedRecord, Option<Vec<u8>>), FormalReadError> {
        remaining(deadline)?;
        let position = anchor.head().record().position;
        let local = super::super::execution_bridge::formal_writer::read_published_candidate_at(
            &self.writer_directory,
            position,
        )
        .map_err(FormalReadError::Internal)?;
        let proof = known_proof
            .cloned()
            .or_else(|| local.as_ref().map(|(_, proof)| proof.clone()));
        let cut = self.governance_cut(anchor, proof)?;
        // Invalid present material is fatal. Only an absent publication may
        // fall back to both authenticated native endpoints at this exact cut.
        let (material, expanded) = if let Some((candidate, cached_proof)) = local {
            let expanded =
                self.validate_material_for_replayed_cut(&cut, &candidate, &cached_proof)?;
            (candidate.material, expanded)
        } else {
            let restored = self.restore_exact_history_cut(&cut, deadline)?;
            let material = restored.material().clone();
            let expanded = oasis7_distfs::controlled_authority::formal::verify_record_material_retaining_snapshot(anchor.head().record(), &material).map_err(FormalReadError::from)?;
            (material, expanded)
        };
        remaining(deadline)?;
        Ok((material, expanded))
    }

    fn governance_cut(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
        proof: Option<FormalQualifiedCommitProofV1>,
    ) -> Result<ReplayedHistoryCut, FormalReadError> {
        let position = anchor.head().record().position;
        let recovery_digest = proof.as_ref().and_then(|p| {
            p.decision
                .primary
                .body
                .recovery_certificate_digest
                .as_deref()
        });
        let needs_recovery = proof
            .as_ref()
            .is_some_and(|p| p.decision.branch != FormalCompletionBranchV1::Normal)
            || anchor.head().finalizing_authority_id()
                != anchor.head().record().original_authority_id;
        let matches = self
            .permissions
            .iter()
            .enumerate()
            .filter_map(|(index, permission)| {
                let recovery = permission.recovery();
                let matches = if let Some(digest) = recovery_digest {
                    recovery.certificate_digest() == digest
                } else {
                    recovery.successor().authority_id().ok().as_deref()
                        == Some(anchor.head().finalizing_authority_id())
                        && permission.cut().record().position.checked_add(1) == Some(position)
                };
                matches.then_some(index)
            })
            .collect::<Vec<_>>();
        let target_recovery = if needs_recovery {
            if matches.len() != 1 {
                return Err(FormalReadError::Integrity(
                    "HistoryUnavailable: governance cut lacks unique recovery context".into(),
                ));
            }
            matches.first().copied()
        } else {
            None
        };
        Ok(ReplayedHistoryCut {
            anchor: anchor.clone(),
            target_proof: proof,
            target_recovery,
            activation: None,
            activation_predecessor: None,
            activation_step: None,
        })
    }
}
