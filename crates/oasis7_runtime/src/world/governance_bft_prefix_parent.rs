//! Continue a genuinely H-anchored BFT prefix without replacing its cold
//! controlled-history reader or borrowing H's policy for later authorization.
use super::*;
use oasis7_formal_protocol::bft::VerifiedFormalBftPrefixV1;

impl World {
    /// The prefix proves consensus ancestry, not execution. The caller must
    /// replay it from H before publishing this World; this entry point binds
    /// the actual parent Snapshot and semantic/manifest roots to its QC.
    pub fn attach_formal_bft_prefix_parent_v1(
        &mut self,
        prefix: &VerifiedFormalBftPrefixV1,
    ) -> Result<(), WorldError> {
        if self.pending_actions_len() != 0 || self.pending_effects_len() != 0 {
            return Err(invalid(
                "formal BFT prefix parent contains uncommitted queues",
            ));
        }
        if self.formal_execution_tick.is_some()
            || self.bft_governance_parent.is_some()
            || self.bft_governance_execution_in_progress()
            || self.formal_history_candidate_installed()
        {
            return Err(invalid(
                "formal BFT prefix cannot replace an active execution",
            ));
        }
        let handoff = prefix.handoff();
        let schedule = prefix.next_height_schedule();
        let retained = self
            .formal_history_reader
            .as_ref()
            .and_then(|reader| reader.parent.as_ref())
            .ok_or_else(|| invalid("formal BFT prefix requires its original retained H reader"))?;
        let mut binding = FormalBftHandoffBindingV1::from_capability(handoff);
        if !binding.matches_retained_parent(retained)
            || self.state.retained_history_root.as_ref() != Some(retained.root())
        {
            return Err(invalid(
                "formal BFT prefix differs from the retained H authority",
            ));
        }
        self.attached_formal_history().map_err(|error| {
            invalid(format!(
                "validate formal BFT prefix history reader: {error}"
            ))
        })?;
        self.validate_bft_parent_world(schedule)?;
        let commitment = prefix.commitment();
        let tip = prefix.certificate();
        let snapshot = crate::codec::snapshot_cbor(&self.snapshot())
            .map_err(|error| invalid(format!("encode formal BFT prefix parent: {error}")))?;
        if crate::blake3_hex(&snapshot) != commitment.execution_state_root()
            || schedule.parent_commit_ref() != tip.block_id()
            || schedule.parent_tick() != tip.block().height
            || schedule.parent_state_root() != commitment.state_root()
            || schedule.parent_manifest_hash() != commitment.post_manifest_hash()
        {
            return Err(invalid(
                "formal BFT prefix actual World differs from its certified parent",
            ));
        }
        // The parent Snapshot is QC-bound above. Capture its policy now,
        // before successor actions can install any replacement policy.
        let registry = self
            .governance_main_token_controller_registry()
            .cloned()
            .ok_or_else(|| invalid("formal BFT prefix parent controller policy is missing"))?;
        let registry = Self::validate_governance_main_token_controller_registry(registry)?;
        let governing = self
            .current_cognition_runtime_binding()
            .map_err(|error| {
                invalid(format!(
                    "read formal BFT prefix governing binding: {error:?}"
                ))
            })?
            .runtime_manifest_hash
            .to_string();
        let governing = governing
            .strip_prefix("blake3:")
            .ok_or_else(|| invalid("formal BFT prefix governing binding is not canonical"))?
            .to_owned();
        binding.successor = Some(FormalBftSuccessorBindingV1 {
            schedule_digest: schedule.digest().map_err(invalid)?,
            raw_snapshot_root: commitment.execution_state_root().to_owned(),
            governing_manifest_digest: governing,
        });
        if !binding.matches_schedule(schedule) {
            return Err(invalid("formal BFT prefix is not after its handoff height"));
        }
        self.bft_governance_parent = Some(Box::new(BftGovernanceParentV2 {
            schedule: schedule.clone(),
            controller_registry: Some(registry),
            formal_handoff: Some(binding),
            execution: None,
        }));
        Ok(())
    }
}
