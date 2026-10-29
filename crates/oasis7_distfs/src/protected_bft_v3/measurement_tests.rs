//! Bounded ignored diagnostic: native signed frames, no socket/World latency.
//! No elapsed-time assertion or fabricated production performance claim.
use super::super::wire::validate_reply;
use super::super::{BftSignerTrustV3, measurement_counts};
use super::*;
use oasis7_consensus::tendermint_v3::{
    BftConsensusActionV3, BftConsensusMachineV3, replay_bft_signing_journal_with_effects_v3,
};
use oasis7_formal_protocol::controlled_authority::formal::test_fixtures as f;
use std::time::Instant;
#[path = "measurement_fixture.rs"]
mod fixture;

fn measured<T>(history: usize, phase: &str, side: &str, body: impl FnOnce() -> T) -> T {
    let before = (
        measurement_counts::history_qcs(),
        oasis7_consensus::tendermint_v3::measurement_counts::replay_count(),
        measurement_counts::bindings(),
    );
    let started = Instant::now();
    let value = body();
    let micros = started.elapsed().as_micros();
    println!(
        "BFT_V3_MEASUREMENT {}",
        serde_json::json!({
            "history": history, "phase": phase, "side": side, "elapsed_us": micros,
            "old_history_qc_validations": measurement_counts::history_qcs()-before.0,
            "journal_replays": oasis7_consensus::tendermint_v3::measurement_counts::replay_count()-before.1,
            "binding_validations": measurement_counts::bindings()-before.2,
        })
    );
    value
}

struct Session {
    service: BftSignerServiceV3,
    session: BftSignerSessionV3,
    credential: Option<BftSignerLeaseCredentialV3>,
    verified: Option<super::super::state::VerifiedBftSignerStateV3>,
    nonce: u64,
    history: usize,
}
impl Session {
    fn rpc(
        &mut self,
        phase: &str,
        command: BftSignerCommandV3,
        measure: bool,
    ) -> SignedBftSignerReplyV3 {
        self.nonce += 1;
        let request = SignedBftSignerRequestV3::sign(
            &self.service.binding,
            &f::key(32),
            self.credential.clone(),
            command,
            format!("{:064x}", self.nonce),
        )
        .unwrap();
        let bytes =
            super::super::crypto::encode_bounded(&request, BFT_SIGNER_MAX_FRAME_BYTES_V3).unwrap();
        let mut handle = || {
            self.service
                .handle_frame(&mut self.session, &bytes)
                .unwrap()
        };
        let frame = if measure {
            measured(self.history, phase, "provider", handle)
        } else {
            handle()
        };
        let reply: SignedBftSignerReplyV3 =
            super::super::wire::read_frame(&mut frame.as_slice()).unwrap();
        assert!(
            !matches!(reply.body.result, BftSignerReplyKindV3::Rejected { .. }),
            "{phase}: {:?}",
            reply.body.result
        );
        // The real remote provider retains its trust pin; provisioning it is
        // outside the measured per-response verification boundary.
        let trust = BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: f::public(33),
            binding_digest: self.service.binding.digest().unwrap(),
        };
        let validate = || {
            validate_reply(&self.service.binding, &trust, &request, &reply).unwrap();
            if let Some(checkpoint) = &reply.body.checkpoint {
                let digest = request_digest(&request).unwrap();
                // Acquire validates the complete history once; subsequent
                // replies share the exact helper used by RemoteBftSignerLeaseV3.
                let verified = if let Some(previous) = &self.verified {
                    checkpoint.validate_against_cache(
                        &self.service.binding,
                        previous,
                        &request.body.challenge,
                        &digest,
                        reply.body.lease_generation,
                        &trust.provider_public_key,
                    )
                } else {
                    checkpoint.validate(
                        &self.service.binding,
                        &request.body.challenge,
                        &digest,
                        reply.body.lease_generation,
                        &trust.provider_public_key,
                    )
                }
                .unwrap();
                assert_eq!(verified.state, checkpoint.state);
                assert_eq!(checkpoint.state.revision, reply.body.state_revision);
                assert_eq!(checkpoint.state.state_digest, reply.body.state_digest);
                Some(verified)
            } else {
                None
            }
        };
        let verified = if measure {
            measured(self.history, phase, "client_validation", validate)
        } else {
            validate()
        };
        if let Some(verified) = verified {
            self.verified = Some(verified);
        }
        reply
    }
    fn new(binding: &BftSignerBindingV3, state: BftSignerStateV3, history: usize) -> Self {
        let backend = fixture::Backend(std::sync::Arc::new(std::sync::Mutex::new(
            BftSignerBackendSnapshotV3 {
                state,
                lease_generation: 1,
                lease_token_digest: String::new(),
                protected_time_floor_ms: 1000,
            },
        )));
        let service = BftSignerServiceV3::open(
            binding.clone(),
            Box::new(fixture::Key(f::key(33))),
            vec![Box::new(fixture::Key(f::key(31)))],
            Box::new(backend),
        )
        .unwrap();
        let mut session = Self {
            service,
            session: BftSignerSessionV3::default(),
            credential: None,
            verified: None,
            nonce: 0,
            history,
        };
        let prepared = session.rpc(
            "setup_prepare",
            BftSignerCommandV3::PrepareAcquire {
                binding_digest: binding.digest().unwrap(),
            },
            false,
        );
        let BftSignerReplyKindV3::Prepared {
            expected_generation,
            prepare_nonce,
        } = prepared.body.result
        else {
            panic!("prepared reply")
        };
        let acquired = session.rpc(
            "setup_acquire",
            BftSignerCommandV3::Acquire {
                binding_digest: binding.digest().unwrap(),
                expected_generation,
                prepare_nonce,
            },
            false,
        );
        session.credential = Some(BftSignerLeaseCredentialV3 {
            generation: acquired.body.lease_generation,
            token: acquired.body.lease_token.unwrap(),
        });
        session
    }
}

#[test]
#[ignore = "bounded explicit performance measurement; no wall-clock acceptance gate"]
fn bounded_history_validation_measurement() {
    let binding = fixture::binding();
    for history in [1, 5, 9] {
        let state = fixture::signed_history(&binding, history);
        assert_eq!(state.history.len(), history);
        measured(history, "binding_validate", "isolated", || {
            binding.validate().unwrap()
        });
        let mut session = Session::new(&binding, state, history);
        session.rpc("protected_time", BftSignerCommandV3::ProtectedTime, true);
        let current = session
            .rpc("checkpoint", BftSignerCommandV3::Checkpoint, true)
            .body
            .checkpoint
            .unwrap();
        let verified = session.verified.as_ref().unwrap().clone();
        assert_eq!(verified.state, current.state);
        let machine = BftConsensusMachineV3::new(
            verified.schedule.clone(),
            "validator-a",
            1000,
            binding.timeouts,
        )
        .unwrap();
        let intent = machine
            .proposal_intent(fixture::value(&verified.schedule), "validator-a")
            .unwrap();
        let mut candidate = current.state.candidate();
        candidate.journal.as_mut().unwrap().pending_proposal_intent = Some(intent.clone());
        let reserved = session
            .rpc(
                "advance_proposal",
                BftSignerCommandV3::Advance {
                    expected_revision: current.state.revision,
                    expected_state_digest: current.state.state_digest.clone(),
                    candidate: Box::new(candidate),
                },
                true,
            )
            .body
            .checkpoint
            .unwrap();
        let signed = session
            .rpc(
                "sign_proposal",
                BftSignerCommandV3::SignProposal {
                    expected_revision: reserved.state.revision,
                    intent: Box::new(intent),
                },
                true,
            )
            .body
            .checkpoint
            .unwrap();
        let (_, effects) = replay_bft_signing_journal_with_effects_v3(
            signed.state.journal.as_ref().unwrap(),
            verified.schedule.active_set(),
            &verified.schedule,
            binding.timeouts,
        )
        .unwrap();
        let vote = effects
            .into_iter()
            .find_map(|effect| {
                if let BftConsensusActionV3::SignVote(v) = effect {
                    Some(v)
                } else {
                    None
                }
            })
            .unwrap();
        let mut candidate = signed.state.candidate();
        candidate.journal.as_mut().unwrap().pending_vote_intent = Some(vote.clone());
        let reserved = session
            .rpc(
                "advance_vote",
                BftSignerCommandV3::Advance {
                    expected_revision: signed.state.revision,
                    expected_state_digest: signed.state.state_digest.clone(),
                    candidate: Box::new(candidate),
                },
                true,
            )
            .body
            .checkpoint
            .unwrap();
        session.rpc(
            "sign_vote",
            BftSignerCommandV3::SignVote {
                expected_revision: reserved.state.revision,
                intent: Box::new(vote),
            },
            true,
        );
    }
}

#[path = "validation_cache_tests.rs"]
mod cache_tests;

#[path = "cache_membership_tests.rs"]
mod membership_cache_tests;
