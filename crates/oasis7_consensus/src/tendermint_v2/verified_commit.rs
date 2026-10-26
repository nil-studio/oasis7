use oasis7_proto::distributed_bft::v2::{
    BftBlockV2, BftCommitCertificateV2, BftScopeV2, BftValidatorTransitionRefV2,
};
use serde::Serialize;

use super::engine::verify_commit_certificate_v2;
use super::proposer::BftProposerPriorityV2;
use super::validator_set::{BftSchedulePolicyV2, BftValidatorSetV2};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PendingTransitionV2 {
    reference: BftValidatorTransitionRefV2,
    new_set: BftValidatorSetV2,
    new_height_start_priority: BftProposerPriorityV2,
}

/// Opaque read/replay schedule. It contains protocol membership only and is
/// independent of whether this process currently has a signing key in the set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BftHeightProposerScheduleV2 {
    scope: BftScopeV2,
    authority_epoch: u64,
    policy: BftSchedulePolicyV2,
    policy_hash: String,
    active_set: BftValidatorSetV2,
    height: u64,
    parent_commit_ref: String,
    parent_tick: u64,
    parent_state_root: String,
    parent_manifest_hash: String,
    starting_priority: BftProposerPriorityV2,
    pending_transition: Option<PendingTransitionV2>,
    last_activation_height: u64,
}

impl BftHeightProposerScheduleV2 {
    /// The only initial schedule constructor. Genesis config, semantic state
    /// root, and manifest hash must be pinned by the Node/operator; this
    /// capability does not authenticate its arguments.
    pub fn genesis(
        active_set: BftValidatorSetV2,
        policy: BftSchedulePolicyV2,
        authority_epoch: u64,
        genesis_state_root: impl Into<String>,
        genesis_manifest_hash: impl Into<String>,
    ) -> Result<Self, String> {
        policy.validate()?;
        let genesis_state_root = genesis_state_root.into();
        validate_hash("genesis state root", &genesis_state_root)?;
        let genesis_manifest_hash = genesis_manifest_hash.into();
        validate_hash("genesis manifest hash", &genesis_manifest_hash)?;
        let policy_hash = policy.hash()?;
        let starting_priority = BftProposerPriorityV2::new(&active_set)?;
        Ok(Self {
            scope: active_set.scope().clone(),
            authority_epoch,
            policy,
            policy_hash,
            parent_commit_ref: active_set.scope().genesis_hash.clone(),
            parent_tick: 0,
            parent_state_root: genesis_state_root,
            parent_manifest_hash: genesis_manifest_hash,
            active_set,
            height: 1,
            starting_priority,
            pending_transition: None,
            last_activation_height: 0,
        })
    }

    pub fn scope(&self) -> &BftScopeV2 {
        &self.scope
    }

    pub fn authority_epoch(&self) -> u64 {
        self.authority_epoch
    }

    pub fn policy(&self) -> &BftSchedulePolicyV2 {
        &self.policy
    }

    pub fn policy_hash(&self) -> &str {
        &self.policy_hash
    }

    pub fn active_set(&self) -> &BftValidatorSetV2 {
        &self.active_set
    }

    pub fn height(&self) -> u64 {
        self.height
    }

    pub fn parent_commit_ref(&self) -> &str {
        &self.parent_commit_ref
    }

    pub fn parent_tick(&self) -> u64 {
        self.parent_tick
    }

    pub fn parent_state_root(&self) -> &str {
        &self.parent_state_root
    }

    pub fn parent_manifest_hash(&self) -> &str {
        &self.parent_manifest_hash
    }

    pub fn starting_priority(&self) -> &BftProposerPriorityV2 {
        &self.starting_priority
    }

    pub fn pending_transition(&self) -> Option<&BftValidatorTransitionRefV2> {
        self.pending_transition
            .as_ref()
            .map(|pending| &pending.reference)
    }

    pub fn last_activation_height(&self) -> u64 {
        self.last_activation_height
    }

    /// Canonical identity for a complete verified V2 schedule, including any
    /// h+1 pending membership activation and priority seed.
    pub fn digest(&self) -> Result<String, String> {
        let bytes = serde_cbor::to_vec(self)
            .map_err(|error| format!("encode BFT V2 schedule digest: {error}"))?;
        if bytes.len() > oasis7_proto::distributed_bft::v2::BFT_MAX_SIGNED_MATERIAL_BYTES_V2 {
            return Err("BFT V2 schedule exceeds its encoded bound".into());
        }
        let mut framed = b"oasis7.bft.height_schedule.v2.digest|".to_vec();
        framed.extend_from_slice(&bytes);
        Ok(blake3::hash(&framed).to_hex().to_string())
    }

    pub fn expected_proposer_for_round(&self, round: u64) -> Result<String, String> {
        if round > super::BFT_MAX_ROUNDS_PER_HEIGHT_V2 {
            return Err("BFT V2 proposer round capacity exceeded".into());
        }
        let mut priority = self.starting_priority.clone();
        let mut proposer = None;
        for _ in 0..=round {
            proposer = Some(priority.next_proposer(&self.active_set)?.validator_id);
        }
        proposer.ok_or_else(|| "BFT V2 proposer schedule is empty".into())
    }

    pub(super) fn validate_for_commit(&self, block: &BftBlockV2) -> Result<(), String> {
        if self.height == 0
            || self.scope != *self.active_set.scope()
            || self.policy.hash()? != self.policy_hash
            || block.scope != self.scope
            || block.height != self.height
            || block.parent_commit_ref != self.parent_commit_ref
            || self.parent_tick.checked_add(1) != Some(self.height)
            || block.authority_epoch != self.authority_epoch
            || block.active_validator_set_hash != self.active_set.set_hash()
            || block.schedule_policy_hash != self.policy_hash
            || block.manifest_hash != self.parent_manifest_hash
        {
            return Err("BFT V2 commit does not extend its exact schedule".into());
        }
        match (
            &self.pending_transition,
            &block.transition_ref,
            &block.transition,
        ) {
            (Some(pending), Some(reference), None) if reference == &pending.reference => {
                if self.height != pending.reference.scheduled_height + 1
                    || block.parent_commit_ref != pending.reference.source_block_id
                {
                    return Err("BFT V2 h+1 transition reference is at the wrong height".into());
                }
            }
            (Some(_), _, _) => {
                return Err("BFT V2 pending transition must continue at h+1".into());
            }
            (None, None, Some(transition)) => {
                transition.validate()?;
                let activation = transition
                    .target_epoch
                    .checked_mul(self.policy.epoch_length_ticks)
                    .ok_or_else(|| "BFT V2 activation height overflow".to_string())?;
                let scheduled = activation
                    .checked_sub(2)
                    .filter(|height| *height > 0)
                    .ok_or_else(|| "BFT V2 transition height is not positive".to_string())?;
                if transition.source_commit_ref != self.parent_commit_ref
                    || transition.source_state_root != self.parent_state_root
                    || transition.source_epoch != self.parent_tick / self.policy.epoch_length_ticks
                    || transition.source_profile != self.policy.profile_id
                    || transition.epoch_length_ticks != self.policy.epoch_length_ticks
                    || transition.mapping_ruleset_hash != self.policy_hash
                    || transition.old_validator_set_hash != self.active_set.set_hash()
                    || transition.activation_height != activation
                    || transition.scheduled_height != scheduled
                    || transition.scheduled_height != self.height
                    || (self.last_activation_height > 0 && scheduled < self.last_activation_height)
                {
                    return Err("BFT V2 transition is late, overlapping, or mismatched".into());
                }
                self.active_set.verify_transition_set(transition)?;
            }
            (None, None, None) => {}
            (None, Some(_), _) => {
                return Err("BFT V2 block has an unsolicited transition reference".into());
            }
        }
        Ok(())
    }

    fn successor_after(&self, block: &BftBlockV2, block_id: String) -> Result<Self, String> {
        let height = self
            .height
            .checked_add(1)
            .ok_or_else(|| "BFT V2 height overflow".to_string())?;
        let parent_state_root = block.state_root.clone();
        let (active_set, starting_priority, pending_transition, last_activation_height) =
            if let Some(pending) = &self.pending_transition {
                if block.transition_ref.as_ref() != Some(&pending.reference) {
                    return Err("BFT V2 transition h+1 proof mismatch".into());
                }
                (
                    pending.new_set.clone(),
                    pending.new_height_start_priority.clone(),
                    None,
                    pending.reference.activation_height,
                )
            } else if let Some(transition) = &block.transition {
                let new_set = self.active_set.verify_transition_set(transition)?;
                let h_plus_one_priority = BftProposerPriorityV2::after_round(
                    &self.active_set,
                    &self.starting_priority,
                    0,
                )?;
                let new_h_plus_two_priority = BftProposerPriorityV2::after_validator_change(
                    &self.active_set,
                    &new_set,
                    &h_plus_one_priority,
                )?;
                let reference = transition.committed_reference(block_id.clone())?;
                (
                    self.active_set.clone(),
                    h_plus_one_priority,
                    Some(PendingTransitionV2 {
                        reference,
                        new_set,
                        new_height_start_priority: new_h_plus_two_priority,
                    }),
                    self.last_activation_height,
                )
            } else {
                (
                    self.active_set.clone(),
                    BftProposerPriorityV2::after_round(
                        &self.active_set,
                        &self.starting_priority,
                        0,
                    )?,
                    None,
                    self.last_activation_height,
                )
            };
        Ok(Self {
            scope: self.scope.clone(),
            authority_epoch: self.authority_epoch,
            policy: self.policy.clone(),
            policy_hash: self.policy_hash.clone(),
            active_set,
            height,
            parent_commit_ref: block_id,
            parent_tick: self.height,
            parent_state_root,
            parent_manifest_hash: block.post_manifest_hash.clone(),
            starting_priority,
            pending_transition,
            last_activation_height,
        })
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedBftCommitCertificateV2 {
    certificate: BftCommitCertificateV2,
    schedule: BftHeightProposerScheduleV2,
    next_height_schedule: BftHeightProposerScheduleV2,
    block_id: String,
}

impl VerifiedBftCommitCertificateV2 {
    pub fn certificate(&self) -> &BftCommitCertificateV2 {
        &self.certificate
    }

    pub fn schedule(&self) -> &BftHeightProposerScheduleV2 {
        &self.schedule
    }

    pub fn next_height_schedule(&self) -> &BftHeightProposerScheduleV2 {
        &self.next_height_schedule
    }

    pub fn block(&self) -> &BftBlockV2 {
        &self.certificate.proposal.block
    }

    pub fn block_id(&self) -> &str {
        &self.block_id
    }

    pub fn active_validator_set(&self) -> &BftValidatorSetV2 {
        self.schedule.active_set()
    }
}

/// Verify the full V2 QC against its schedule and return a protocol-only
/// capability. The token does not prove execution or governance semantics;
/// the driver must independently replay the block before applying a set change.
pub fn verify_commit_certificate_token_v2(
    schedule: &BftHeightProposerScheduleV2,
    certificate: &BftCommitCertificateV2,
) -> Result<VerifiedBftCommitCertificateV2, String> {
    certificate.validate_wire_bounds()?;
    let block = &certificate.proposal.block;
    schedule.validate_for_commit(block)?;
    verify_commit_certificate_v2(schedule, certificate)?;
    let block_id = block.block_id()?;
    let next_height_schedule = schedule.successor_after(block, block_id.clone())?;
    Ok(VerifiedBftCommitCertificateV2 {
        certificate: certificate.clone(),
        schedule: schedule.clone(),
        next_height_schedule,
        block_id,
    })
}

fn validate_hash(label: &str, value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("BFT V2 {label} must be canonical 32-byte hex"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "verified_commit_tests.rs"]
mod certificate_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use oasis7_proto::distributed_bft::v2::{BftValidatorTransitionV2, BftValidatorV2};

    fn hash(byte: &str) -> String {
        byte.repeat(64)
    }

    fn scope() -> BftScopeV2 {
        BftScopeV2 {
            world_id: "schedule-v2-world".into(),
            chain_id: "schedule-v2-chain".into(),
            genesis_hash: hash("1"),
        }
    }

    fn validator(seed: u8) -> (BftValidatorV2, SigningKey) {
        let key = SigningKey::from_bytes(&[seed; 32]);
        (
            BftValidatorV2 {
                validator_id: format!("v{seed}"),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                voting_power: u64::from(seed),
            },
            key,
        )
    }

    fn fixture(
        epoch_length: u64,
    ) -> (
        BftHeightProposerScheduleV2,
        BftValidatorSetV2,
        Vec<BftValidatorV2>,
    ) {
        let old = (1..=4).map(validator).collect::<Vec<_>>();
        let active = BftValidatorSetV2::new(
            scope(),
            old.iter().map(|(value, _)| value.clone()).collect(),
        )
        .expect("active set");
        let policy =
            BftSchedulePolicyV2::new(epoch_length, "controlled-bft").expect("schedule policy");
        let schedule =
            BftHeightProposerScheduleV2::genesis(active.clone(), policy, 7, hash("a"), hash("b"))
                .expect("genesis schedule");
        let next = (5..=8).map(|seed| validator(seed).0).collect();
        (schedule, active, next)
    }

    fn transition_for(
        schedule: &BftHeightProposerScheduleV2,
        next: Vec<BftValidatorV2>,
        target_epoch: u64,
    ) -> BftValidatorTransitionV2 {
        let activation_height = target_epoch * schedule.policy().epoch_length_ticks;
        let next_set =
            BftValidatorSetV2::new(schedule.scope().clone(), next.clone()).expect("next set");
        BftValidatorTransitionV2 {
            source_commit_ref: schedule.parent_commit_ref().into(),
            source_state_root: schedule.parent_state_root().into(),
            source_profile: schedule.policy().profile_id.clone(),
            source_epoch: schedule.parent_tick() / schedule.policy().epoch_length_ticks,
            target_epoch,
            epoch_length_ticks: schedule.policy().epoch_length_ticks,
            mapping_ruleset_hash: schedule.policy_hash().into(),
            scheduled_height: activation_height - 2,
            activation_height,
            old_validator_set_hash: schedule.active_set().set_hash().into(),
            new_validators: next,
            new_validator_set_hash: next_set.set_hash().into(),
            governance_action_digest: hash("c"),
            projected_registry_digest: hash("d"),
        }
    }

    fn block_for(
        schedule: &BftHeightProposerScheduleV2,
        transition: Option<BftValidatorTransitionV2>,
    ) -> BftBlockV2 {
        BftBlockV2 {
            protocol_version: oasis7_proto::distributed_bft::v2::BFT_PROTOCOL_VERSION_V2,
            scope: schedule.scope().clone(),
            height: schedule.height(),
            parent_commit_ref: schedule.parent_commit_ref().into(),
            authority_epoch: schedule.authority_epoch(),
            active_validator_set_hash: schedule.active_set().set_hash().into(),
            schedule_policy_hash: schedule.policy_hash().into(),
            manifest_hash: schedule.parent_manifest_hash().into(),
            post_manifest_hash: hash("e"),
            actions_cbor: vec![0x80],
            action_root: hash("f"),
            event_root: hash("a"),
            execution_block_hash: hash("b"),
            execution_state_root: hash("c"),
            state_root: hash("d"),
            transition,
            transition_ref: None,
        }
    }

    #[test]
    fn genesis_pins_state_and_manifest_and_transition_uses_parent_epoch_at_length_one() {
        let (schedule, _, next) = fixture(1);
        assert_eq!(schedule.height(), 1);
        assert_eq!(schedule.parent_tick(), 0);
        assert_eq!(schedule.parent_state_root(), hash("a"));
        assert_eq!(schedule.parent_manifest_hash(), hash("b"));

        let transition = transition_for(&schedule, next, 3);
        assert_eq!(transition.scheduled_height, 1);
        assert_eq!(transition.activation_height, 3);
        assert_eq!(transition.source_epoch, 0);
        schedule
            .validate_for_commit(&block_for(&schedule, Some(transition.clone())))
            .expect("exact h=1 transition binds parent epoch, root, and profile");

        let mut wrong_epoch = transition;
        wrong_epoch.source_epoch = 1;
        assert!(
            schedule
                .validate_for_commit(&block_for(&schedule, Some(wrong_epoch)))
                .is_err()
        );
    }

    #[test]
    fn schedule_chains_post_manifest_and_rejects_wrong_parent_manifest_or_state_root() {
        let (schedule, _, next) = fixture(10);
        let block = block_for(&schedule, None);
        let next_schedule = schedule
            .successor_after(&block, block.block_id().expect("block id"))
            .expect("successor");
        assert_eq!(next_schedule.parent_tick(), 1);
        assert_eq!(
            next_schedule.parent_manifest_hash(),
            block.post_manifest_hash
        );
        assert_eq!(next_schedule.parent_state_root(), block.state_root);

        let mut wrong_manifest = block_for(&next_schedule, None);
        wrong_manifest.manifest_hash = hash("9");
        assert!(next_schedule.validate_for_commit(&wrong_manifest).is_err());

        let mut schedule_at_h = next_schedule;
        schedule_at_h.height = 28;
        schedule_at_h.parent_tick = 27;
        schedule_at_h.parent_commit_ref = hash("6");
        schedule_at_h.parent_state_root = hash("7");
        let transition = transition_for(&schedule_at_h, next, 3);
        schedule_at_h
            .validate_for_commit(&block_for(&schedule_at_h, Some(transition.clone())))
            .expect("transition source binds exact parent state");
        let mut wrong_root = transition;
        wrong_root.source_state_root = hash("8");
        assert!(
            schedule_at_h
                .validate_for_commit(&block_for(&schedule_at_h, Some(wrong_root)))
                .is_err()
        );
    }
}
