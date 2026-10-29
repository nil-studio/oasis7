//! Protocol-only signed histories from the existing qualified-H fixture.
//! These synthetic commitments do not claim actual World execution.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_consensus::tendermint_v3::{
    BftHeaderValueV3, BftSigningJournalV3,
};
use oasis7_formal_protocol::{
    bft::*,
    controlled_authority::formal::{self, test_fixtures as f},
};
use oasis7_proto::distributed_bft::{
    v2::{BftScopeV2, BftValidatorV2},
    v3::*,
};
use std::sync::{Arc, Mutex};

pub(super) fn binding() -> BftSignerBindingV3 {
    let (certificate, authority, proof, initial) = f::initial_with_profile(2);
    let trust = formal::InitialActivationTrustV1 {
        issuer_public_key: f::public(9),
        world: certificate.body.authority.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
    };
    let anchor =
        formal::VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &proof).unwrap();
    let target = BftProfileTargetV2::new(
        BftScopeV2 {
            world_id: "world".into(),
            chain_id: "chain".into(),
            genesis_hash: f::h(8),
        },
        1,
        vec![BftValidatorV2 {
            validator_id: "validator-a".into(),
            public_key_hex: f::public(31),
            voting_power: 1,
        }],
        BftSchedulePolicyV2::new(4, "bft").unwrap(),
    )
    .unwrap();
    let handoff = formal::ProfileHandoffV1 {
        next_profile: target.profile.clone(),
        next_profile_version: target.profile_version,
        next_authority_digest: target.target_digest().unwrap(),
        valid_from_position: 3,
    };
    let frozen = f::frozen(&authority, &initial, Some(handoff));
    let record = f::execution(&authority, &frozen);
    let proof = f::proof(
        record.clone(),
        &authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    let committed =
        formal::verify_formal_qualified(&proof, &authority, Some(&initial), None).unwrap();
    let anchor = anchor.after_verified_commit(authority, committed).unwrap();
    let handoff = verify_formal_bft_handoff_v2(&anchor, target).unwrap();
    let schedule = handoff
        .initial_schedule_after_handoff_replay(&f::h(101), &f::h(102))
        .unwrap();
    BftSignerBindingV3::new(
        handoff,
        schedule,
        f::h(103),
        "validator-a".into(),
        vec![crate::protected_bft_v3::BftSignerKeyIdentityV3 {
            validator_id: "validator-a".into(),
            public_key_hex: f::public(31),
        }],
        oasis7_consensus::tendermint_v1::BftTimeoutsV1 {
            propose_ms: 2000,
            prevote_ms: 2000,
            precommit_ms: 2000,
            round_delta_ms: 100,
        },
        "bounded-measurement".into(),
        f::public(32),
    )
    .unwrap()
}

pub(super) fn value(schedule: &BftHeaderHeightScheduleV3) -> BftHeaderValueV3 {
    let commitment = FormalBftExecutionCommitmentV1 {
        domain: BFT_FORMAL_EXECUTION_DOMAIN_V1.into(),
        schema_version: 1,
        profile: "bft".into(),
        profile_version: 1,
        activation_digest: schedule.activation_digest().into(),
        scope: schedule.scope().clone(),
        height: schedule.height(),
        parent_commit_ref: schedule.parent_commit_ref().into(),
        parent_tick: schedule.parent_tick(),
        parent_semantic_state_root: schedule.parent_state_root().into(),
        authority_epoch: schedule.authority_epoch(),
        active_validator_set_hash: schedule.active_set().set_hash().into(),
        schedule_policy_hash: schedule.policy_hash().into(),
        manifest_hash: schedule.parent_manifest_hash().into(),
        post_manifest_hash: f::h(104),
        action_root: f::h(105),
        event_root: f::h(106),
        execution_state_root: f::h(107),
        state_root: f::h(108),
        projection_root: f::h(109),
        result_root: f::h(110),
        transition: None,
        transition_ref: None,
    };
    BftHeaderValueV3 {
        header: BftBlockHeaderV3 {
            protocol_version: 3,
            execution_commitment_hash: commitment.digest().unwrap(),
            actions_cbor_digest: actions_cbor_digest_v3(&[0x80]).unwrap(),
        },
        execution_commitment: commitment,
    }
}

pub(super) fn signed_history(binding: &BftSignerBindingV3, count: usize) -> BftSignerStateV3 {
    let mut state = BftSignerStateV3::from_handoff(binding).unwrap();
    let mut schedule = binding.initial_schedule().clone();
    for _ in 0..count {
        let value = value(&schedule);
        let header = BftProposalHeaderV3::sign(
            schedule.scope().clone(),
            schedule.height(),
            0,
            schedule.active_set().set_hash().into(),
            schedule.policy_hash().into(),
            binding.local_validator_id.clone(),
            value.block_id().unwrap(),
            None,
            &f::key(31),
        )
        .unwrap();
        let proposal = BftPublicProposalV3 {
            header,
            block: value,
            valid_round_evidence: None,
        };
        let prevote = BftVoteV3::sign(
            schedule.scope().clone(),
            schedule.height(),
            0,
            BftVotePhaseV3::Prevote,
            schedule.active_set().set_hash().into(),
            schedule.policy_hash().into(),
            BftVoteValueV3::Block {
                block_id: proposal.header.block_id.clone(),
            },
            "validator-a".into(),
            &f::key(31),
        )
        .unwrap();
        let precommit = BftVoteV3::sign(
            schedule.scope().clone(),
            schedule.height(),
            0,
            BftVotePhaseV3::Precommit,
            schedule.active_set().set_hash().into(),
            schedule.policy_hash().into(),
            BftVoteValueV3::Block {
                block_id: proposal.header.block_id.clone(),
            },
            "validator-a".into(),
            &f::key(31),
        )
        .unwrap();
        let certificate = FormalBftCertifiedHeaderV3::new(
            BftHeaderCommitCertificateV3 {
                proposal: proposal.header.clone(),
                valid_round_evidence: None,
                precommits: vec![precommit.clone()],
            },
            proposal.block.header.clone(),
            proposal.block.execution_commitment.clone(),
        )
        .unwrap();
        let mut journal = state.journal.take().unwrap();
        journal.height_started_at_ms = Some(100);
        journal.events = vec![
            oasis7_consensus::tendermint_v3::BftSigningJournalEventV3::Proposal {
                proposal: Box::new(proposal),
                local: true,
                now_ms: 101,
            },
            oasis7_consensus::tendermint_v3::BftSigningJournalEventV3::Vote {
                vote: Box::new(prevote),
                local: true,
                now_ms: 102,
            },
            oasis7_consensus::tendermint_v3::BftSigningJournalEventV3::Vote {
                vote: Box::new(precommit),
                local: true,
                now_ms: 103,
            },
        ];
        journal.pending_commit = Some(certificate.clone());
        schedule = verify_formal_bft_header_certificate_v3(&schedule, &certificate)
            .unwrap()
            .next_height_schedule()
            .clone();
        let mut candidate = state.candidate();
        candidate.history.push(certificate.clone());
        candidate.completed_journals.push(Some(journal));
        candidate
            .applied_installs
            .push(crate::protected_bft_v3::BftDriverAppliedReceiptV3 {
                height: certificate.execution_commitment.height,
                header_id: certificate.header.block_id().unwrap(),
                driver_install_digest: f::h(111),
            });
        candidate.journal = Some(
            BftSigningJournalV3::for_schedule(
                &schedule,
                binding.local_validator_id.clone(),
                f::public(31),
            )
            .unwrap(),
        );
        state = BftSignerStateV3::from_candidate(binding, state.revision + 1, candidate).unwrap();
    }
    let mut candidate = state.candidate();
    candidate.journal.as_mut().unwrap().height_started_at_ms = Some(1000);
    BftSignerStateV3::from_candidate(binding, state.revision + 1, candidate).unwrap()
}

pub(super) struct Key(pub SigningKey);
impl BftSigningKeyHandleV3 for Key {
    fn public_key_hex(&self) -> String {
        hex::encode(self.0.verifying_key().to_bytes())
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        Ok(self.0.sign(bytes).to_bytes())
    }
}

pub(super) struct Backend(pub Arc<Mutex<BftSignerBackendSnapshotV3>>);
impl Backend {
    fn checked(&self, generation: u64, token: &str) -> BftSignerBackendSnapshotV3 {
        let snapshot = self.0.lock().unwrap().clone();
        assert_eq!(snapshot.lease_generation, generation);
        assert_eq!(snapshot.lease_token_digest, token);
        snapshot
    }
}
impl BftSignerBackendV3 for Backend {
    fn load_provisioned(
        &mut self,
        binding: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let snapshot = self.0.lock().unwrap().clone();
        assert_eq!(snapshot.state.binding_digest, binding);
        Ok(snapshot)
    }
    fn acquire_lease(
        &mut self,
        _: &str,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        let mut s = self.0.lock().unwrap();
        assert_eq!(s.lease_generation, generation);
        assert!(s.lease_token_digest.is_empty());
        s.lease_generation += 1;
        s.lease_token_digest = token.into();
        Ok(BftSignerAcquireOutcomeV3::Acquired(s.clone()))
    }
    fn release_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.checked(generation, token);
        let mut s = self.0.lock().unwrap();
        s.lease_generation += 1;
        s.lease_token_digest.clear();
        Ok(s.clone())
    }
    fn read_for_lease(
        &mut self,
        generation: u64,
        token: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        Ok(self.checked(generation, token))
    }
    fn protected_time_ms(&mut self, generation: u64, token: &str) -> Result<i64, BftSignerErrorV3> {
        self.checked(generation, token);
        let mut s = self.0.lock().unwrap();
        s.protected_time_floor_ms += 1;
        Ok(s.protected_time_floor_ms)
    }
    fn compare_and_swap(
        &mut self,
        generation: u64,
        token: &str,
        revision: u64,
        digest: &str,
        time: Option<i64>,
        next: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let previous = self.checked(generation, token);
        assert_eq!(previous.state.revision, revision);
        assert_eq!(previous.state.state_digest, digest);
        let mut s = self.0.lock().unwrap();
        assert!(time.is_none_or(|v| v <= s.protected_time_floor_ms));
        s.state = next;
        Ok(s.clone())
    }
}
