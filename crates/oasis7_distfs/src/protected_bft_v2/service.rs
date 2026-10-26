use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::sync::Mutex;

use oasis7_consensus::tendermint_v2::{
    BftProposalIntentV2, BftVoteIntentV2, complete_reserved_bft_proposal_v2,
    complete_reserved_bft_vote_v2, verify_commit_certificate_token_v2,
};
use oasis7_proto::distributed_bft::v2::{
    BFT_PROTOCOL_VERSION_V2, BftProposalHeaderV2, BftProposalV2, BftVoteV2,
};

use super::backend::{BftSignerAcquireOutcomeV2, BftSignerBackendSnapshotV2, BftSignerBackendV2};
use super::binding::{
    BftSignerBindingV2, BftSignerErrorV2, BftSigningKeyHandleV2, decode_key, invalid,
};
use super::crypto::{
    BFT_SIGNER_MAX_CERTIFICATE_BYTES_V2, BFT_SIGNER_MAX_FRAME_BYTES_V2,
    BFT_SIGNER_MAX_INSTALL_RECORD_BYTES_V2, digest_hex, encode_bounded, sign_preimage,
};
use super::install::{
    BftSignedTransitionInstallAckV2, BftTransitionInstallIntentBodyV2,
    BftTransitionInstallIntentV2, BftTransitionInstallReceiptBodyV2, BftTransitionInstallReceiptV2,
};
use super::state::{
    BftSignerCandidateV2, BftSignerCheckpointV2, BftSignerStateV2, VerifiedBftSignerStateV2,
    install_contexts_for_state, validate_candidate_transition, validate_signing_install_gate,
    validate_state,
};
use super::wire::{
    BFT_SIGNER_LEASE_DOMAIN_V2, BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V2, BftSignerCommandV2,
    BftSignerLeaseCredentialV2, BftSignerReplyBodyV2, BftSignerReplyKindV2, SignedBftSignerReplyV2,
    SignedBftSignerRequestV2, encode_frame, request_digest, validate_request,
};

mod operations;

pub(super) const BFT_SIGNER_CHECKPOINT_DOMAIN_V2: &str =
    "oasis7.bft.protected-signer.v2.checkpoint";
const MAX_SESSION_OPERATIONS_V2: u64 = 65_536;

/// Provider endpoint for the independent dynamic-set V2 signer protocol.
/// Backends must supply atomic generation fencing and durable monotonic time;
/// this service makes no hardware or nonrollback claim about an implementation.
pub struct BftSignerServiceV2 {
    pub(super) binding: BftSignerBindingV2,
    pub(super) provider_key: Box<dyn BftSigningKeyHandleV2>,
    pub(super) validator_keys: BTreeMap<String, Box<dyn BftSigningKeyHandleV2>>,
    pub(super) runtime: Mutex<BftSignerServiceRuntimeV2>,
}

pub(super) struct BftSignerServiceRuntimeV2 {
    pub(super) backend: Box<dyn BftSignerBackendV2>,
    pub(super) poisoned: bool,
    pub(super) verified: Option<VerifiedBftSignerStateV2>,
    pub(super) active_lease: Option<ActiveLeaseV2>,
    pub(super) observed_time_floor_ms: i64,
    pub(super) observed_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ActiveLeaseV2 {
    pub(super) generation: u64,
    pub(super) token_digest: String,
}

#[derive(Default)]
pub(super) struct BftSignerSessionV2 {
    pub(super) prepared: Option<PreparedAcquireV2>,
    pub(super) lease: Option<BftSignerLeaseCredentialV2>,
    pub(super) operations: u64,
    pub(super) challenges: BTreeSet<String>,
}

pub(super) struct PreparedAcquireV2 {
    pub(super) expected_generation: u64,
    pub(super) nonce: String,
}

pub(super) struct ReplyDataV2 {
    pub(super) result: BftSignerReplyKindV2,
    pub(super) checkpoint: Option<Box<BftSignerCheckpointV2>>,
    pub(super) lease_token: Option<String>,
    pub(super) generation: u64,
    pub(super) revision: u64,
    pub(super) state_digest: String,
}

impl BftSignerServiceV2 {
    pub fn open(
        binding: BftSignerBindingV2,
        provider_key: Box<dyn BftSigningKeyHandleV2>,
        validator_keys: Vec<Box<dyn BftSigningKeyHandleV2>>,
        mut backend: Box<dyn BftSignerBackendV2>,
    ) -> Result<Self, BftSignerErrorV2> {
        let genesis_set = binding.validate()?;
        let provider_public_key = provider_key.public_key_hex();
        decode_key(&provider_public_key)?;
        if provider_public_key == binding.requester_auth_public_key
            || binding
                .provisioned_signing_keys
                .iter()
                .any(|identity| identity.public_key_hex == provider_public_key)
        {
            return Err(invalid(
                "BFT V2 provider, requester, and validator identities must be distinct",
            ));
        }
        let mut keys_by_public = BTreeMap::new();
        for key in validator_keys {
            let public_key = key.public_key_hex();
            decode_key(&public_key)?;
            if public_key == provider_public_key
                || public_key == binding.requester_auth_public_key
                || keys_by_public.insert(public_key, key).is_some()
            {
                return Err(invalid("BFT V2 provisioned signing key set is ambiguous"));
            }
        }
        if binding
            .provisioned_signing_keys
            .iter()
            .any(|identity| !keys_by_public.contains_key(&identity.public_key_hex))
            || keys_by_public.len() != binding.provisioned_signing_keys.len()
        {
            return Err(invalid(
                "BFT V2 provider keys differ from the pinned provisioned identities",
            ));
        }
        let binding_digest = binding.digest()?;
        let snapshot = backend.load_provisioned(&binding_digest)?;
        validate_snapshot_shape(&snapshot, None, None)?;
        let verified = validate_state(&binding, Some(&provider_public_key), &snapshot.state)?;
        let genesis_schedule =
            oasis7_consensus::tendermint_v2::BftHeightProposerScheduleV2::genesis(
                genesis_set,
                binding.schedule_policy.clone(),
                binding.genesis_authority_epoch,
                binding.genesis_state_root.clone(),
                binding.genesis_manifest_hash.clone(),
            )
            .map_err(|error| invalid(format!("BFT V2 provider genesis schedule: {error}")))?;
        if verified.schedule.height() < genesis_schedule.height()
            || snapshot.lease_generation == 0
            || snapshot.protected_time_floor_ms < 0
        {
            return Err(invalid("BFT V2 provisioned backend snapshot is invalid"));
        }
        Ok(Self {
            binding,
            provider_key,
            validator_keys: keys_by_public,
            runtime: Mutex::new(BftSignerServiceRuntimeV2 {
                backend,
                poisoned: false,
                verified: Some(verified),
                active_lease: (!snapshot.lease_token_digest.is_empty()).then_some(ActiveLeaseV2 {
                    generation: snapshot.lease_generation,
                    token_digest: snapshot.lease_token_digest,
                }),
                observed_time_floor_ms: snapshot.protected_time_floor_ms,
                observed_generation: snapshot.lease_generation,
            }),
        })
    }

    /// Serves one connection-bound lease session. Any exit attempts a durable
    /// release; an uncertain release poisons this service instance.
    pub fn serve_connection<S: Read + Write>(
        &self,
        stream: &mut S,
    ) -> Result<(), BftSignerErrorV2> {
        let mut session = BftSignerSessionV2::default();
        let served = self.serve_loop(stream, &mut session);
        let released = self.release_session(&mut session);
        match (served, released) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) => Err(error),
            (_, Err(error)) => Err(error),
        }
    }

    fn serve_loop<S: Read + Write>(
        &self,
        stream: &mut S,
        session: &mut BftSignerSessionV2,
    ) -> Result<(), BftSignerErrorV2> {
        loop {
            let mut header = [0u8; 4];
            match stream.read(&mut header[..1]) {
                Ok(0) => return Ok(()),
                Ok(1) => {}
                Ok(_) => return Err(invalid("BFT V2 frame header over-read")),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(invalid(format!("BFT V2 TCP frame header: {error}"))),
            }
            stream
                .read_exact(&mut header[1..])
                .map_err(|error| invalid(format!("truncated BFT V2 TCP frame header: {error}")))?;
            let length = usize::try_from(u32::from_be_bytes(header))
                .map_err(|_| invalid("BFT V2 TCP frame length conversion"))?;
            if length == 0 || length > BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V2 {
                return Err(invalid("BFT V2 TCP frame length is outside its bound"));
            }
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(length)
                .map_err(|_| invalid("BFT V2 TCP frame allocation failed"))?;
            bytes.resize(length, 0);
            stream
                .read_exact(&mut bytes)
                .map_err(|error| invalid(format!("BFT V2 TCP frame body: {error}")))?;
            let response = self.handle_frame(session, &bytes)?;
            stream
                .write_all(&response)
                .map_err(|error| invalid(format!("BFT V2 TCP response write: {error}")))?;
        }
    }

    fn handle_frame(
        &self,
        session: &mut BftSignerSessionV2,
        bytes: &[u8],
    ) -> Result<Vec<u8>, BftSignerErrorV2> {
        let request: SignedBftSignerRequestV2 =
            super::crypto::decode_bounded(bytes, BFT_SIGNER_MAX_FRAME_BYTES_V2)?;
        validate_request(&self.binding, &request)?;
        let digest = request_digest(&request)?;
        if !session.challenges.insert(request.body.challenge.clone()) {
            return Err(invalid(
                "BFT V2 request challenge was replayed on this connection",
            ));
        }
        session.operations = session
            .operations
            .checked_add(1)
            .filter(|count| *count <= MAX_SESSION_OPERATIONS_V2)
            .ok_or_else(|| invalid("BFT V2 session operation counter overflow"))?;
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| invalid("BFT V2 provider lock poisoned"))?;
        let data = if runtime.poisoned {
            rejected_reply(
                &runtime,
                "BFT V2 provider is fail-closed after an uncertain operation".to_string(),
            )
        } else {
            match self.dispatch(&mut runtime, session, &request, &digest) {
                Ok(data) => data,
                Err(error) => rejected_reply(&runtime, error.0),
            }
        };
        let body = BftSignerReplyBodyV2 {
            schema_version: 2,
            binding_digest: self.binding.digest()?,
            request_digest: digest,
            challenge: request.body.challenge,
            lease_generation: data.generation,
            lease_token: data.lease_token,
            state_revision: data.revision,
            state_digest: data.state_digest,
            checkpoint: data.checkpoint,
            result: data.result,
        };
        encode_frame(&SignedBftSignerReplyV2::sign(
            body,
            self.provider_key.as_ref(),
        )?)
    }

    fn dispatch(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        session: &mut BftSignerSessionV2,
        request: &SignedBftSignerRequestV2,
        request_digest: &str,
    ) -> Result<ReplyDataV2, BftSignerErrorV2> {
        match &request.body.command {
            BftSignerCommandV2::PrepareAcquire { binding } => {
                self.prepare_acquire(runtime, session, request, binding)
            }
            BftSignerCommandV2::Acquire {
                binding,
                expected_generation,
                prepare_nonce,
            } => self.acquire(
                runtime,
                session,
                request,
                request_digest,
                binding,
                *expected_generation,
                prepare_nonce,
            ),
            BftSignerCommandV2::ProtectedTime => self.protected_time(runtime, session, request),
            command => {
                let credential = require_session_lease(session, &request.body.lease)?;
                let token_digest = lease_digest(&credential.token)?;
                if runtime.active_lease.as_ref()
                    != Some(&ActiveLeaseV2 {
                        generation: credential.generation,
                        token_digest: token_digest.clone(),
                    })
                {
                    return Err(invalid(
                        "BFT V2 lease is fenced or bound to another connection",
                    ));
                }
                let snapshot = runtime
                    .backend
                    .read_for_lease(credential.generation, &token_digest)?;
                self.verify_observed_snapshot(runtime, &snapshot, credential)?;
                self.process_leased_command(
                    runtime,
                    session,
                    request,
                    request_digest,
                    credential.clone(),
                    &token_digest,
                    snapshot,
                    command,
                )
            }
        }
    }

    fn prepare_acquire(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        session: &mut BftSignerSessionV2,
        request: &SignedBftSignerRequestV2,
        binding: &BftSignerBindingV2,
    ) -> Result<ReplyDataV2, BftSignerErrorV2> {
        if session.lease.is_some() || binding != &self.binding || request.body.lease.is_some() {
            return Err(invalid(
                "BFT V2 prepare-acquire is not valid for this session",
            ));
        }
        let snapshot = runtime.backend.load_provisioned(&self.binding.digest()?)?;
        self.verify_observed_snapshot(
            runtime,
            &snapshot,
            &BftSignerLeaseCredentialV2 {
                generation: snapshot.lease_generation,
                token: String::new(),
            },
        )?;
        let nonce = random_nonce()?;
        session.prepared = Some(PreparedAcquireV2 {
            expected_generation: snapshot.lease_generation,
            nonce: nonce.clone(),
        });
        let state = runtime
            .verified
            .as_ref()
            .ok_or_else(|| invalid("BFT V2 verified state cache is unavailable"))?;
        Ok(ReplyDataV2 {
            result: BftSignerReplyKindV2::Prepared {
                expected_generation: snapshot.lease_generation,
                prepare_nonce: nonce,
            },
            checkpoint: None,
            lease_token: None,
            generation: snapshot.lease_generation,
            revision: state.state.revision,
            state_digest: state.state.state_digest.clone(),
        })
    }

    // Preserve the connection-bound preparation and signed-request inputs
    // separately so each can be checked against the active session.
    #[allow(clippy::too_many_arguments)]
    fn acquire(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        session: &mut BftSignerSessionV2,
        request: &SignedBftSignerRequestV2,
        request_digest: &str,
        binding: &BftSignerBindingV2,
        expected_generation: u64,
        nonce: &str,
    ) -> Result<ReplyDataV2, BftSignerErrorV2> {
        let prepared = session
            .prepared
            .as_ref()
            .ok_or_else(|| invalid("BFT V2 acquire has no connection-bound preparation"))?;
        if session.lease.is_some()
            || request.body.lease.is_some()
            || binding != &self.binding
            || prepared.expected_generation != expected_generation
            || prepared.nonce != nonce
        {
            return Err(invalid(
                "BFT V2 acquire differs from connection preparation",
            ));
        }
        session.prepared = None;
        let raw_token = random_nonce()?;
        let token_digest = lease_digest(&raw_token)?;
        let outcome = runtime.backend.acquire_lease(
            &self.binding.digest()?,
            expected_generation,
            &token_digest,
        );
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V2 acquire outcome is uncertain; provider poisoned: {error}"
                )));
            }
        };
        let snapshot = match outcome {
            BftSignerAcquireOutcomeV2::Conflict(snapshot) => {
                if let Err(error) = validate_snapshot_shape(&snapshot, None, None) {
                    runtime.poisoned = true;
                    return Err(invalid(format!(
                        "BFT V2 acquire conflict snapshot is invalid; provider poisoned: {error}"
                    )));
                }
                let verified = match self.verify_snapshot_against_cache(runtime, &snapshot) {
                    Ok(verified) => verified,
                    Err(error) => {
                        runtime.poisoned = true;
                        return Err(invalid(format!(
                            "BFT V2 acquire conflict snapshot is invalid; provider poisoned: {error}"
                        )));
                    }
                };
                let active = !snapshot.lease_token_digest.is_empty();
                if snapshot.lease_generation < expected_generation
                    || (snapshot.lease_generation == expected_generation && !active)
                    || snapshot.lease_generation < runtime.observed_generation
                    || snapshot.protected_time_floor_ms < runtime.observed_time_floor_ms
                {
                    runtime.poisoned = true;
                    return Err(invalid("BFT V2 acquire conflict snapshot regressed"));
                }
                runtime.verified = Some(verified);
                runtime.observed_generation = snapshot.lease_generation;
                runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
                runtime.active_lease =
                    (!snapshot.lease_token_digest.is_empty()).then(|| ActiveLeaseV2 {
                        generation: snapshot.lease_generation,
                        token_digest: snapshot.lease_token_digest.clone(),
                    });
                let state = runtime.verified.as_ref().expect("just installed");
                return Ok(ReplyDataV2 {
                    result: BftSignerReplyKindV2::AcquireConflict {
                        current_generation: snapshot.lease_generation,
                        lease_active: active,
                    },
                    checkpoint: None,
                    lease_token: None,
                    generation: snapshot.lease_generation,
                    revision: state.state.revision,
                    state_digest: state.state.state_digest.clone(),
                });
            }
            BftSignerAcquireOutcomeV2::Acquired(snapshot) => snapshot,
        };
        let next_generation = expected_generation
            .checked_add(1)
            .ok_or_else(|| invalid("BFT V2 lease generation overflow"))?;
        if snapshot.lease_generation != next_generation
            || snapshot.lease_token_digest != token_digest
            || snapshot.lease_token_digest.is_empty()
        {
            runtime.poisoned = true;
            return Err(invalid("BFT V2 acquire CAS returned an unexpected lease"));
        }
        if snapshot.lease_generation <= runtime.observed_generation
            || snapshot.protected_time_floor_ms < runtime.observed_time_floor_ms
        {
            runtime.poisoned = true;
            return Err(invalid(
                "BFT V2 acquired snapshot regressed; provider poisoned",
            ));
        }
        let verified = match self.verify_snapshot_against_cache(runtime, &snapshot) {
            Ok(verified) => verified,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V2 acquire committed but returned an invalid snapshot; provider poisoned: {error}"
                )));
            }
        };
        runtime.observed_generation = snapshot.lease_generation;
        runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
        runtime.verified = Some(verified);
        runtime.active_lease = Some(ActiveLeaseV2 {
            generation: snapshot.lease_generation,
            token_digest,
        });
        let credential = BftSignerLeaseCredentialV2 {
            generation: snapshot.lease_generation,
            token: raw_token.clone(),
        };
        session.lease = Some(credential);
        let checkpoint = self.make_checkpoint(
            snapshot.state,
            snapshot.lease_generation,
            request.body.challenge.clone(),
            request_digest.to_string(),
        )?;
        Ok(ReplyDataV2 {
            result: BftSignerReplyKindV2::Acquired,
            checkpoint: Some(Box::new(checkpoint.clone())),
            lease_token: Some(raw_token),
            generation: checkpoint.lease_generation,
            revision: checkpoint.state.revision,
            state_digest: checkpoint.state.state_digest.clone(),
        })
    }

    fn protected_time(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        session: &BftSignerSessionV2,
        request: &SignedBftSignerRequestV2,
    ) -> Result<ReplyDataV2, BftSignerErrorV2> {
        let credential = require_session_lease(session, &request.body.lease)?;
        let token_digest = lease_digest(&credential.token)?;
        if runtime.active_lease.as_ref()
            != Some(&ActiveLeaseV2 {
                generation: credential.generation,
                token_digest: token_digest.clone(),
            })
        {
            return Err(invalid("BFT V2 protected-time lease is fenced"));
        }
        let now = match runtime
            .backend
            .protected_time_ms(credential.generation, &token_digest)
        {
            Ok(now) => now,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V2 protected clock is unavailable; provider poisoned: {error}"
                )));
            }
        };
        if now < runtime.observed_time_floor_ms || now < 0 {
            runtime.poisoned = true;
            return Err(invalid("BFT V2 backend protected clock rollback"));
        }
        runtime.observed_time_floor_ms = now;
        let cached = runtime
            .verified
            .as_ref()
            .ok_or_else(|| invalid("BFT V2 verified state cache is unavailable"))?;
        Ok(ReplyDataV2 {
            result: BftSignerReplyKindV2::ProtectedTime { time_ms: now },
            checkpoint: None,
            lease_token: None,
            generation: credential.generation,
            revision: cached.state.revision,
            state_digest: cached.state.state_digest.clone(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn process_leased_command(
        &self,
        runtime: &mut BftSignerServiceRuntimeV2,
        session: &mut BftSignerSessionV2,
        request: &SignedBftSignerRequestV2,
        request_digest: &str,
        credential: BftSignerLeaseCredentialV2,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV2,
        command: &BftSignerCommandV2,
    ) -> Result<ReplyDataV2, BftSignerErrorV2> {
        let cached_schedule = runtime
            .verified
            .as_ref()
            .map(|verified| verified.schedule.clone())
            .ok_or_else(|| invalid("BFT V2 verified state cache is unavailable"))?;
        let state = &snapshot.state;
        match command {
            BftSignerCommandV2::Checkpoint => self.state_reply(
                snapshot,
                request,
                request_digest,
                BftSignerReplyKindV2::Checkpoint,
            ),
            BftSignerCommandV2::StartHeightTimer { expected_revision } => {
                if *expected_revision != state.revision {
                    return Err(invalid("BFT V2 height timer revision is stale"));
                }
                let Some(old_journal) = state.journal.as_ref() else {
                    return self.state_reply(
                        snapshot,
                        request,
                        request_digest,
                        BftSignerReplyKindV2::HeightTimerStarted,
                    );
                };
                if old_journal.height_started_at_ms().is_some() {
                    return self.state_reply(
                        snapshot,
                        request,
                        request_digest,
                        BftSignerReplyKindV2::HeightTimerStarted,
                    );
                }
                let now = self.sample_time(runtime, &credential, token_digest)?;
                let mut candidate = state.candidate();
                let journal = candidate
                    .journal
                    .as_mut()
                    .ok_or_else(|| invalid("BFT V2 active signer journal disappeared"))?;
                journal.height_started_at_ms = Some(now);
                self.commit_candidate(
                    runtime,
                    &credential,
                    token_digest,
                    state,
                    candidate,
                    Some(now),
                    request,
                    request_digest,
                    BftSignerReplyKindV2::HeightTimerStarted,
                )
            }
            BftSignerCommandV2::Advance {
                expected_revision,
                expected_state_digest,
                candidate,
            } => {
                if *expected_revision != state.revision
                    || expected_state_digest != &state.state_digest
                {
                    return Err(invalid("BFT V2 advance checkpoint is stale"));
                }
                let event_time = appended_event_time(state, candidate)?;
                if let Some(event_time) = event_time {
                    let now = self.sample_time(runtime, &credential, token_digest)?;
                    if event_time < 0 || event_time > now {
                        return Err(invalid(
                            "BFT V2 event time is outside protected time bounds",
                        ));
                    }
                }
                self.commit_candidate(
                    runtime,
                    &credential,
                    token_digest,
                    state,
                    *candidate.clone(),
                    event_time,
                    request,
                    request_digest,
                    BftSignerReplyKindV2::Advanced,
                )
            }
            BftSignerCommandV2::SignProposal {
                expected_revision,
                intent,
            } => self.sign_proposal(
                runtime,
                &credential,
                token_digest,
                snapshot,
                *expected_revision,
                intent,
                request,
                request_digest,
            ),
            BftSignerCommandV2::SignVote {
                expected_revision,
                intent,
            } => self.sign_vote(
                runtime,
                &credential,
                token_digest,
                snapshot,
                *expected_revision,
                intent,
                request,
                request_digest,
            ),
            BftSignerCommandV2::AppendCommittedCertificate {
                expected_revision,
                expected_state_digest,
                certificate,
            } => {
                if *expected_revision != state.revision
                    || expected_state_digest != &state.state_digest
                {
                    return Err(invalid("BFT V2 certificate append checkpoint is stale"));
                }
                encode_bounded(certificate.as_ref(), BFT_SIGNER_MAX_CERTIFICATE_BYTES_V2)?;
                let mut candidate = state.candidate();
                candidate.history.push(certificate.as_ref().clone());
                let token = verify_commit_certificate_token_v2(&cached_schedule, certificate)
                    .map_err(|error| invalid(format!("BFT V2 commit certificate: {error}")))?;
                candidate.journal = super::state::journal_for_schedule(
                    &self.binding,
                    token.next_height_schedule(),
                )?;
                self.commit_candidate(
                    runtime,
                    &credential,
                    token_digest,
                    state,
                    candidate,
                    None,
                    request,
                    request_digest,
                    BftSignerReplyKindV2::CertificateAppended,
                )
            }
            BftSignerCommandV2::PrepareTransitionInstall {
                expected_revision,
                expected_state_digest,
                request_id,
                transition_source_height,
                runtime_instance_id,
            } => self.prepare_transition_install(
                runtime,
                &credential,
                token_digest,
                snapshot,
                *expected_revision,
                expected_state_digest,
                request_id,
                *transition_source_height,
                runtime_instance_id,
                request,
                request_digest,
            ),
            BftSignerCommandV2::CompleteTransitionInstall {
                intent,
                acknowledgement,
            } => self.complete_transition_install(
                runtime,
                session,
                &credential,
                token_digest,
                snapshot,
                intent,
                acknowledgement,
                request,
                request_digest,
            ),
            BftSignerCommandV2::PrepareAcquire { .. } | BftSignerCommandV2::Acquire { .. } => Err(
                invalid("BFT V2 acquire commands cannot use an active lease"),
            ),
            BftSignerCommandV2::ProtectedTime => Err(invalid(
                "BFT V2 protected time must use its no-snapshot path",
            )),
        }
    }
}

fn appended_event_time(
    previous: &BftSignerStateV2,
    candidate: &BftSignerCandidateV2,
) -> Result<Option<i64>, BftSignerErrorV2> {
    match (&previous.journal, &candidate.journal) {
        (Some(old), Some(next)) if old != next => {
            if next.events.len() == old.events.len() + 1
                && next.events.starts_with(old.events.as_slice())
            {
                Ok(next.events.last().map(|event| event.event_time_ms()))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

fn validate_snapshot_shape(
    snapshot: &BftSignerBackendSnapshotV2,
    credential: Option<&BftSignerLeaseCredentialV2>,
    expected_token_digest: Option<&str>,
) -> Result<(), BftSignerErrorV2> {
    if snapshot.lease_generation == 0 || snapshot.protected_time_floor_ms < 0 {
        return Err(invalid("BFT V2 backend snapshot generation or time floor"));
    }
    if !snapshot.lease_token_digest.is_empty() && !is_lower_hex_32(&snapshot.lease_token_digest) {
        return Err(invalid("BFT V2 backend lease token digest is noncanonical"));
    }
    if let Some(expected) = expected_token_digest
        && snapshot.lease_token_digest != expected
    {
        return Err(invalid("BFT V2 backend snapshot lease token mismatch"));
    }
    if let Some(credential) = credential
        && credential.generation != snapshot.lease_generation
    {
        return Err(invalid(
            "BFT V2 backend snapshot generation differs from lease",
        ));
    }
    Ok(())
}

fn require_session_lease<'a>(
    session: &'a BftSignerSessionV2,
    request_lease: &'a Option<BftSignerLeaseCredentialV2>,
) -> Result<&'a BftSignerLeaseCredentialV2, BftSignerErrorV2> {
    let lease = request_lease
        .as_ref()
        .ok_or_else(|| invalid("BFT V2 command requires an active lease"))?;
    if session.lease.as_ref() != Some(lease) {
        return Err(invalid("BFT V2 lease is not bound to this TCP session"));
    }
    Ok(lease)
}

fn lease_digest(token: &str) -> Result<String, BftSignerErrorV2> {
    if !is_lower_hex_32(token) {
        return Err(invalid("BFT V2 lease token is noncanonical"));
    }
    digest_hex(BFT_SIGNER_LEASE_DOMAIN_V2, &token)
}

fn rejected_reply(runtime: &BftSignerServiceRuntimeV2, message: String) -> ReplyDataV2 {
    let (revision, state_digest) = runtime
        .verified
        .as_ref()
        .map(|verified| (verified.state.revision, verified.state.state_digest.clone()))
        .unwrap_or_default();
    ReplyDataV2 {
        result: BftSignerReplyKindV2::Rejected {
            message: bounded_message(message),
        },
        checkpoint: None,
        lease_token: None,
        generation: runtime.observed_generation,
        revision,
        state_digest,
    }
}

fn is_lower_hex_32(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn bounded_message(message: String) -> String {
    const MAX_ERROR_BYTES: usize = 512;
    if message.len() <= MAX_ERROR_BYTES {
        message
    } else {
        let mut end = MAX_ERROR_BYTES;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message[..end].to_string()
    }
}

fn random_nonce() -> Result<String, BftSignerErrorV2> {
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut bytes = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(|error| invalid(format!("BFT V2 CSPRNG unavailable: {error}")))?;
        Ok(hex::encode(bytes))
    }
    #[cfg(not(unix))]
    {
        Err(invalid(
            "BFT V2 provider currently requires a Unix CSPRNG implementation",
        ))
    }
}
