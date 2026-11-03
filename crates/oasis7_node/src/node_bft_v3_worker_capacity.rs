//! Capacity admission preserves the same parent, input and protected journal.
use super::*;
use crate::{
    NodeBftCapacityAdmissionV3, NodeBftPrivateCandidatePackageV3, NodeBftPrivateCandidateV3,
    NodeBftProtectedCandidateAdmissionV3, NodeBftRollPurposeV3,
};

impl Worker {
    pub(super) fn select_next_build(&self) -> Result<NodeBftBuildContextV3, NodeError> {
        let expected = NodeBftExecutionContextV3::from_schedule(&self.schedule);
        let mut hook = lock_hook(&self.hook)?;
        if hook
            .current_bft_v3_execution_context(&self.schedule)
            .map_err(error)?
            != expected
        {
            return Err(error("wire-3 build parent mismatch"));
        }
        if let Some(original) = hook
            .pending_bft_v3_private_candidate(&expected)
            .map_err(error)?
        {
            return Ok(NodeBftBuildContextV3 {
                execution: expected,
                actions: oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3(
                    &original.actions_cbor,
                )
                .map_err(error)?,
            });
        }
        if let Some(next) = &self.admitted_build {
            if next.execution != expected {
                return Err(error("wire-3 admitted batch lost its original parent"));
            }
            return Ok(next.clone());
        }
        let actions = super::super::actions::select_current(
            &mut *self
                .pending
                .lock()
                .map_err(|_| error("wire-3 pending queue poisoned"))?,
            &self.pending_bytes,
            &expected,
            self.config.max_engine_pending_consensus_actions,
        )?;
        Ok(NodeBftBuildContextV3 {
            execution: expected,
            actions,
        })
    }

    pub(super) fn admit_build(
        &mut self,
        next: &mut NodeBftBuildContextV3,
    ) -> Result<(), NodeError> {
        self.admit_build_before(next, Instant::now() + Duration::from_secs(120))
    }

    fn admit_build_before(
        &mut self,
        next: &mut NodeBftBuildContextV3,
        deadline: Instant,
    ) -> Result<(), NodeError> {
        let admission = lock_hook(&self.hook)?
            .preflight_bft_v3_capacity(next)
            .map_err(error)?;
        if !roll_required(admission, self.checkpoint.state.history.len())? {
            return Ok(());
        }
        self.roll_build_before(next, NodeBftRollPurposeV3::LocalBuild, deadline)
    }

    fn roll_build_before(
        &mut self,
        next: &mut NodeBftBuildContextV3,
        purpose: NodeBftRollPurposeV3<'_>,
        deadline: Instant,
    ) -> Result<(), NodeError> {
        let original_commit = match purpose {
            NodeBftRollPurposeV3::OriginalPendingCommit(certificate) => Some(certificate),
            _ => None,
        };
        let journal = self.checkpoint.state.journal.as_ref();
        if self.checkpoint.state.pending_driver_apply.is_some()
            || journal.is_some_and(|journal| {
                journal.pending_commit.as_ref() != original_commit
                    || journal.pending_proposal_intent.is_some()
                    || journal.pending_vote_intent.is_some()
            })
            || (original_commit.is_none() && !self.effects.is_empty())
            || (original_commit.is_some() && journal.is_none())
        {
            return Err(error(
                "wire-3 capacity roll precedes original pending recovery",
            ));
        }
        if let Some(network) = &self.selected.network {
            network.execution.invalidate_parent().map_err(error)?;
        }
        let (checkpoint, schedule, base_schedule) = {
            let mut hook = lock_hook(&self.hook)?;
            let mut lease = self.signer()?;
            let checkpoint = hook
                .roll_bft_v3_execution(
                    &self.selected.binding,
                    &mut **lease,
                    next,
                    purpose,
                    deadline,
                )
                .map_err(error)?;
            let authenticated = lease.checkpoint().map_err(signer_error)?;
            if checkpoint != authenticated {
                return Err(error(
                    "wire-3 roll returned a different protected checkpoint",
                ));
            }
            let schedule = lease.current_verified_schedule().map_err(signer_error)?;
            let active = lease
                .verified_active_window()
                .map_err(signer_error)?
                .ok_or_else(|| error("wire-3 roll omitted verified active base"))?;
            (checkpoint, schedule, active.base_schedule().clone())
        };
        if schedule != self.schedule
            || checkpoint.state.journal != self.checkpoint.state.journal
            || checkpoint.state.pending_driver_apply.is_some()
            || checkpoint.state.history.len() >= 64
            || base_schedule.height() <= self.base_schedule.height()
            || base_schedule.height() > schedule.height()
        {
            return Err(error(
                "wire-3 roll changed original journal/parent or failed to advance base",
            ));
        }
        startup::validate_schedule(&schedule, &self.selected)?;
        let (machine, effects) = startup::restore(
            &self.selected,
            &schedule,
            &checkpoint,
            &self.hook,
            &self.config,
        )?;
        self.checkpoint = checkpoint;
        self.base_schedule = base_schedule;
        self.schedule = schedule;
        self.machine = machine;
        self.effects = effects.into();
        next.execution = NodeBftExecutionContextV3::from_schedule(&self.schedule);
        let fits = {
            let mut hook = lock_hook(&self.hook)?;
            hook.current_bft_v3_execution_context(&self.schedule)
                .map_err(error)?
                == next.execution
                && (!matches!(purpose, NodeBftRollPurposeV3::LocalBuild)
                    || hook.preflight_bft_v3_capacity(next).map_err(error)?
                        == NodeBftCapacityAdmissionV3::Fits)
        };
        if !fits {
            return Err(error(
                "wire-3 original batch does not fit installed successor",
            ));
        }
        self.selected
            .capacity_exhausted
            .store(false, Ordering::Release);
        if super::super::membership::active_key(&self.selected, &self.schedule).is_some()
            && self
                .checkpoint
                .state
                .journal
                .as_ref()
                .is_some_and(|journal| journal.height_started_at_ms.is_none())
        {
            // A previously unstarted height may start once. An existing
            // original timer, including its spent budget, is never restarted.
            let checkpoint = self
                .signer()?
                .start_height_timer(self.checkpoint.state.revision)
                .map_err(signer_error)?;
            let (machine, effects) = startup::restore(
                &self.selected,
                &self.schedule,
                &checkpoint,
                &self.hook,
                &self.config,
            )?;
            self.checkpoint = checkpoint;
            self.machine = machine;
            self.effects = effects.into();
        }
        self.publish_peer_view()?;
        self.publish_status();
        Ok(())
    }

    pub(super) fn admit_original_commit(
        &mut self,
        certificate: &FormalBftCertifiedHeaderV3,
        actions: &[NodeConsensusAction],
    ) -> Result<(), NodeError> {
        // The QC, original input hashes and static admission are checked before
        // this point. A World build in the old layer could itself exceed its
        // capacity, so dynamic execution is repeated only after admission.
        let mut next = NodeBftBuildContextV3 {
            execution: NodeBftExecutionContextV3::from_schedule(&self.schedule),
            actions: actions.to_vec(),
        };
        let deadline = Instant::now() + Duration::from_secs(120);
        if self.checkpoint.state.history.len() < 64 {
            match lock_hook(&self.hook)?
                .preflight_bft_v3_capacity(&next)
                .map_err(error)?
            {
                NodeBftCapacityAdmissionV3::Fits => return Ok(()),
                NodeBftCapacityAdmissionV3::NeedsRoll => {}
                NodeBftCapacityAdmissionV3::Blocked => {
                    return Err(error("wire-3 original commit capacity blocked"));
                }
            }
        }
        self.roll_build_before(
            &mut next,
            NodeBftRollPurposeV3::OriginalPendingCommit(certificate),
            deadline,
        )
    }

    /// The new retention head advances State, but cannot rewrite consensus work.
    fn refresh_retention_checkpoint(&mut self) -> Result<(), NodeError> {
        let (checkpoint, schedule) = {
            let mut lease = self.signer()?;
            (
                lease.checkpoint().map_err(signer_error)?,
                lease.current_verified_schedule().map_err(signer_error)?,
            )
        };
        let mut expected = self.checkpoint.state.candidate();
        let actual = checkpoint.state.candidate();
        expected.private_retention = actual.private_retention.clone();
        if schedule != self.schedule
            || checkpoint.state.journal != self.checkpoint.state.journal
            || checkpoint.state.pending_driver_apply != self.checkpoint.state.pending_driver_apply
            || checkpoint.state.history != self.checkpoint.state.history
            || checkpoint.lease_generation != self.checkpoint.lease_generation
            || checkpoint.state.revision < self.checkpoint.state.revision
            || expected != actual
        {
            return Err(error("wire-3 retention changed original consensus state"));
        }
        self.checkpoint = checkpoint;
        Ok(())
    }

    pub(super) fn build_protected_candidate(
        &mut self,
    ) -> Result<NodeBftPrivateCandidateV3, NodeError> {
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut next = self.select_next_build()?;
        self.admit_build_before(&mut next, deadline)?;
        let private = {
            let mut hook = lock_hook(&self.hook)?;
            let private = match hook
                .pending_bft_v3_private_candidate(&next.execution)
                .map_err(error)?
            {
                Some(original) => original,
                None => hook.build_bft_v3_candidate(next.clone()).map_err(error)?,
            };
            startup::validate_private(
                &self.config,
                &mut **hook,
                &self.schedule,
                &private,
                &self.selected,
            )?;
            let mut lease = self.signer()?;
            let digest = hook
                .retain_bft_v3_protected_candidate(
                    &self.selected.binding,
                    &mut **lease,
                    &next.execution,
                    &private,
                    deadline,
                )
                .map_err(error)?;
            if !canonical_digest(&digest) {
                return Err(error("wire-3 protected retention receipt malformed"));
            }
            private
        };
        self.refresh_retention_checkpoint()?;
        self.admitted_build = None;
        Ok(private)
    }

    pub(super) fn retain_received_protected(
        &mut self,
        context: &NodeBftExecutionContextV3,
        package: &NodeBftPrivateCandidatePackageV3,
        deadline: Instant,
    ) -> Result<bool, NodeError> {
        super::received_admission::retain(
            self,
            super::received_admission::Original {
                context,
                package,
                deadline,
            },
        )
    }
}

impl super::received_admission::Host for Worker {
    fn retain_original(
        &mut self,
        original: super::received_admission::Original<'_>,
    ) -> Result<NodeBftProtectedCandidateAdmissionV3, NodeError> {
        let mut hook = lock_hook(&self.hook)?;
        let mut lease = self.signer()?;
        hook.retain_received_bft_v3_protected_candidate(
            &self.selected.binding,
            &mut **lease,
            original.context,
            original.package,
            original.deadline,
        )
        .map_err(error)
    }

    fn roll_original(
        &mut self,
        original: super::received_admission::Original<'_>,
    ) -> Result<(), NodeError> {
        let mut next = NodeBftBuildContextV3 {
            execution: original.context.clone(),
            actions: oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3(
                &original.package.candidate.actions_cbor,
            )
            .map_err(error)?,
        };
        self.roll_build_before(
            &mut next,
            NodeBftRollPurposeV3::Received(original.package),
            original.deadline,
        )?;
        if &next.execution != original.context {
            return Err(error("wire-3 received package lost its original parent"));
        }
        Ok(())
    }

    fn refresh_original(
        &mut self,
        _: super::received_admission::Original<'_>,
    ) -> Result<(), NodeError> {
        self.refresh_retention_checkpoint()
    }
}

fn signer_error(reason: oasis7_distfs::protected_bft_v3::BftSignerErrorV3) -> NodeError {
    error(reason.to_string())
}

fn roll_required(
    private: NodeBftCapacityAdmissionV3,
    history_entries: usize,
) -> Result<bool, NodeError> {
    match private {
        NodeBftCapacityAdmissionV3::Fits if history_entries < 64 => Ok(false),
        NodeBftCapacityAdmissionV3::Fits | NodeBftCapacityAdmissionV3::NeedsRoll => Ok(true),
        _ => Err(error("wire-3 private execution capacity blocked")),
    }
}

#[cfg(test)]
mod capacity_boundary_tests {
    use super::*;

    #[test]
    fn authenticated_history_boundary_requires_roll_even_when_private_material_fits() {
        assert!(!roll_required(NodeBftCapacityAdmissionV3::Fits, 63).unwrap());
        assert!(
            roll_required(NodeBftCapacityAdmissionV3::Fits, 64).unwrap(),
            "private material headroom cannot authorize a 65th protected history entry"
        );
    }

    #[test]
    fn history_boundary_cannot_override_private_material_blocker() {
        assert!(roll_required(NodeBftCapacityAdmissionV3::Blocked, 64).is_err());
        assert!(roll_required(NodeBftCapacityAdmissionV3::NeedsRoll, 63).unwrap());
    }
}
