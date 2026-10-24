//! Semantic admission-history checks after each externally rooted proof step.
use super::*;

#[cfg(all(test, unix))]
#[path = "formal_live_governance_history_tests.rs"]
mod tests;

pub(super) struct GovernanceHistoryCursor {
    checkpoint: FormalReadCheckpointV1,
    authority_context: String,
    world: World,
}

fn remaining(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        return Err("HistoryUnavailable: governance history deadline exceeded".into());
    }
    Ok(())
}

impl NativeHistoryResolver {
    pub(super) fn verify_governance_history_step(
        &self,
        cursor: &mut Option<GovernanceHistoryCursor>,
        predecessor: &VerifiedFormalReadAnchorV1,
        anchor: &VerifiedFormalReadAnchorV1,
        proof: &FormalQualifiedCommitProofV1,
        deadline: Instant,
    ) -> Result<(), String> {
        remaining(deadline)?;
        let before_version = predecessor.authority().definition().profile_version;
        let after_version = anchor.authority().definition().profile_version;
        if before_version != 2 && after_version != 2 {
            return Ok(());
        }
        if before_version != 2 || after_version != 2 {
            return Err(
                "HistoryUnavailable: governance history profile transition unsupported".into(),
            );
        }
        if anchor.head().record().parent_record_id != predecessor.head().head().record_id {
            return Err("HistoryUnavailable: governance history predecessor mismatch".into());
        }
        let context = predecessor
            .authority()
            .context_digest()
            .map_err(|e| e.to_string())?;
        if cursor.as_ref().is_none_or(|cached| {
            cached.checkpoint != predecessor.checkpoint() || cached.authority_context != context
        }) {
            *cursor = Some(GovernanceHistoryCursor {
                checkpoint: predecessor.checkpoint(),
                authority_context: context,
                world: self.governance_world_at(predecessor, None, deadline)?,
            });
        }
        let next = self.governance_world_at(anchor, Some(proof), deadline)?;
        cursor
            .as_ref()
            .ok_or("HistoryUnavailable: governance predecessor unavailable")?
            .world
            .verify_formal_governance_successor(&next, || remaining(deadline))
            .map_err(|e| format!("HistoryUnavailable: governance history authentication: {e}"))?;
        remaining(deadline)?;
        *cursor = Some(GovernanceHistoryCursor {
            checkpoint: anchor.checkpoint(),
            authority_context: anchor
                .authority()
                .context_digest()
                .map_err(|e| e.to_string())?,
            world: next,
        });
        Ok(())
    }

    fn governance_world_at(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
        known_proof: Option<&FormalQualifiedCommitProofV1>,
        deadline: Instant,
    ) -> Result<World, String> {
        let material = self.governance_material_at(anchor, known_proof, deadline)?;
        let world = self.world_for(anchor, &material)?;
        remaining(deadline)?;
        Ok(world)
    }

    pub(super) fn governance_material_at(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
        known_proof: Option<&FormalQualifiedCommitProofV1>,
        deadline: Instant,
    ) -> Result<ClosedRecord, String> {
        remaining(deadline)?;
        let position = anchor.head().record().position;
        let local = super::super::execution_bridge::formal_writer::read_published_candidate_at(
            &self.writer_directory,
            position,
        )?;
        let proof = known_proof
            .cloned()
            .or_else(|| local.as_ref().map(|(_, proof)| proof.clone()));
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
                return Err(
                    "HistoryUnavailable: governance cut lacks unique recovery context".into(),
                );
            }
            matches.first().copied()
        } else {
            None
        };
        let cut = ReplayedHistoryCut {
            anchor: anchor.clone(),
            target_proof: proof,
            target_recovery,
            activation: None,
            activation_predecessor: None,
            activation_step: None,
        };
        // Invalid present material is fatal. Only an absent publication may
        // fall back to both authenticated native endpoints at this exact cut.
        let material = if local.is_some() {
            self.material_for_replayed_cut(&cut)?
        } else {
            self.restore_exact_history_cut(&cut, deadline)?
                .material()
                .clone()
        };
        remaining(deadline)?;
        Ok(material)
    }
}
