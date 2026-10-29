//! A provider envelope authenticates transport, not an arbitrary validator DTO.
use super::super::successor_public_archive::BftSuccessorWindowOriginalSignedResultV1 as Original;
use super::super::successor_window::BftSuccessorWindowArchiveIdentityV1 as Identity;
use super::*;
use oasis7_consensus::tendermint_v3::BftSigningJournalEventV3 as Event;
use std::time::Instant;
impl RemoteBftSignerLeaseV3 {
    fn signed_result_attempt<T>(
        &mut self,
        run: impl FnOnce(&mut Self, Instant) -> Result<T, BftSignerErrorV3>,
    ) -> Result<T, BftSignerErrorV3> {
        let deadline = Instant::now()
            .checked_add(self.archive_attempt_timeout)
            .ok_or_else(|| invalid("signed result deadline overflow"))?;
        let read = self
            .stream
            .read_timeout()
            .map_err(|e| invalid(e.to_string()))?;
        let write = self
            .stream
            .write_timeout()
            .map_err(|e| invalid(e.to_string()))?;
        let previous = self.signing_result_deadline.replace(deadline);
        let result = run(self, deadline).and_then(|value| {
            self.archive_timeout(deadline)?;
            Ok(value)
        });
        self.signing_result_deadline = previous;
        let restored = self
            .stream
            .set_read_timeout(read)
            .and_then(|_| self.stream.set_write_timeout(write));
        match (result, restored) {
            (Err(e), _) => Err(e),
            (Ok(_), Err(e)) => Err(invalid(format!("restore signed result timeout: {e}"))),
            (Ok(value), Ok(())) => Ok(value),
        }
    }
    pub(super) fn checked_sign_proposal(
        &mut self,
        expected_revision: u64,
        intent: &BftProposalIntentV3,
    ) -> Result<(BftPublicProposalV3, BftSignerCheckpointV3), BftSignerErrorV3> {
        self.signed_result_attempt(|lease, deadline| {
            let (request, reply) = lease.command(BftSignerCommandV3::SignProposal {
                expected_revision,
                intent: Box::new(intent.clone()),
            })?;
            RemoteBftSignerProviderV3::checked_result(&reply)?;
            let BftSignerReplyKindV3::ProposalSigned { proposal } = &reply.body.result else {
                return Err(invalid("BFT V3 proposal response has the wrong kind"));
            };
            let checkpoint = lease.result_checkpoint(&request, &reply, |kind| {
                matches!(kind, BftSignerReplyKindV3::ProposalSigned { .. })
            })?;
            // Retain the validated checkpoint/floor even if this DTO is corrupt.
            proposal.validate_binding().map_err(invalid)?;
            proposal.header.verify_signature().map_err(invalid)?;
            let mut header = proposal.header.clone();
            header.signature_hex.clear();
            if header != intent.header
                || proposal.block != intent.block
                || proposal.valid_round_evidence != intent.valid_round_evidence
                || proposal.header.scope != lease.binding.scope
                || proposal.header.proposer_id != lease.binding.local_validator_id
            {
                return Err(invalid(
                    "signed proposal differs from exact local request intent",
                ));
            }
            let identity = Identity::SigningSlot {
                public_key_hex: proposal.header.public_key_hex.clone(),
                height: proposal.header.height,
                round: proposal.header.round,
                phase: "proposal".into(),
                preimage_digest: blake3::hash(&proposal.header.signing_payload().map_err(invalid)?)
                    .to_hex()
                    .to_string(),
            };
            lease.prove_signed_result(
                &checkpoint,
                &identity,
                &Original::Proposal(proposal.clone()),
                deadline,
            )?;
            lease.archive_checkpoint_floor(&checkpoint)?;
            Ok((proposal.as_ref().clone(), checkpoint))
        })
    }
    pub(super) fn checked_sign_vote(
        &mut self,
        expected_revision: u64,
        intent: &BftVoteIntentV3,
    ) -> Result<(BftVoteV3, BftSignerCheckpointV3), BftSignerErrorV3> {
        self.signed_result_attempt(|lease, deadline| {
            let (request, reply) = lease.command(BftSignerCommandV3::SignVote {
                expected_revision,
                intent: Box::new(intent.clone()),
            })?;
            RemoteBftSignerProviderV3::checked_result(&reply)?;
            let BftSignerReplyKindV3::VoteSigned { vote } = &reply.body.result else {
                return Err(invalid("BFT V3 vote response has the wrong kind"));
            };
            let checkpoint = lease.result_checkpoint(&request, &reply, |kind| {
                matches!(kind, BftSignerReplyKindV3::VoteSigned { .. })
            })?;
            vote.verify_signature().map_err(invalid)?;
            if vote.height != intent.height
                || vote.round != intent.round
                || vote.phase != intent.phase
                || vote.value != intent.value
                || vote.validator_id != intent.validator_id
                || vote.schedule_policy_hash != intent.schedule_policy_hash
                || vote.scope != lease.binding.scope
                || vote.validator_id != lease.binding.local_validator_id
            {
                return Err(invalid(
                    "signed vote differs from exact local request intent",
                ));
            }
            let identity = Identity::SigningSlot {
                public_key_hex: vote.public_key_hex.clone(),
                height: vote.height,
                round: vote.round,
                phase: match vote.phase {
                    oasis7_proto::distributed_bft::v3::BftVotePhaseV3::Prevote => "prevote",
                    oasis7_proto::distributed_bft::v3::BftVotePhaseV3::Precommit => "precommit",
                }
                .into(),
                preimage_digest: blake3::hash(&vote.signing_payload().map_err(invalid)?)
                    .to_hex()
                    .to_string(),
            };
            lease.prove_signed_result(
                &checkpoint,
                &identity,
                &Original::Vote(vote.clone()),
                deadline,
            )?;
            lease.archive_checkpoint_floor(&checkpoint)?;
            Ok((vote.as_ref().clone(), checkpoint))
        })
    }
    fn prove_signed_result(
        &mut self,
        checkpoint: &BftSignerCheckpointV3,
        identity: &Identity,
        expected: &Original,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3> {
        let hot = checkpoint
            .state
            .journal
            .iter()
            .chain(checkpoint.state.completed_journals.iter().flatten())
            .any(|journal| {
                journal.events.iter().any(|event| match (event, expected) {
                    (
                        Event::Proposal {
                            proposal,
                            local: true,
                            ..
                        },
                        Original::Proposal(expected),
                    ) => proposal == expected,
                    (
                        Event::Vote {
                            vote, local: true, ..
                        },
                        Original::Vote(expected),
                    ) => vote == expected,
                    _ => false,
                })
            });
        if hot {
            return Ok(());
        }
        let height = match identity {
            Identity::SigningSlot { height, .. } => *height,
            _ => {
                return Err(invalid(
                    "signed response requires a complete signing identity",
                ));
            }
        };
        let digest = identity.digest()?;
        if let Some(window) = &checkpoint.state.successor_window {
            for record in &window.requests {
                if height < record.manifest.first_height || height > record.manifest.last_height {
                    continue;
                }
                let indexed = record.manifest.public_index.is_some();
                if indexed
                    && !record
                        .manifest
                        .entries
                        .iter()
                        .any(|e| e.identity_digest == digest)
                {
                    continue;
                }
                let original = self.archive_original(
                    &record.request.intent.body.checkpoint.request_id,
                    &record.request.intent.body.checkpoint.digest()?,
                    indexed.then_some(identity),
                    deadline,
                )?;
                if original.resolve(identity)?.as_ref() == Some(expected) {
                    return Ok(());
                }
                return Err(invalid(
                    "signed response differs from authenticated archived local result",
                ));
            }
        }
        Err(invalid(
            "signed response absent from authenticated local journal/archive",
        ))
    }
}
