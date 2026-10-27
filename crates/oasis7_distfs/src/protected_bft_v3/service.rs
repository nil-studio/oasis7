use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::sync::Mutex;

use oasis7_consensus::tendermint_v3::{
    BftProposalIntentV3, BftPublicProposalV3, BftVoteIntentV3, complete_reserved_bft_proposal_v3,
    complete_reserved_bft_vote_v3,
};
use oasis7_proto::distributed_bft::v3::{BFT_PROTOCOL_VERSION_V3, BftProposalHeaderV3, BftVoteV3};

use oasis7_formal_protocol::bft::verify_formal_bft_header_certificate_v3;

use super::backend::{BftSignerAcquireOutcomeV3, BftSignerBackendSnapshotV3, BftSignerBackendV3};
use super::binding::{
    BftSignerBindingV3, BftSignerErrorV3, BftSigningKeyHandleV3, decode_key, invalid,
};
use super::crypto::{
    BFT_SIGNER_MAX_CERTIFICATE_BYTES_V3, BFT_SIGNER_MAX_FRAME_BYTES_V3, digest_hex, encode_bounded,
    sign_preimage,
};
use super::state::{
    BftSignerCandidateV3, BftSignerCheckpointV3, BftSignerStateV3, VerifiedBftSignerStateV3,
    validate_candidate_transition, validate_signing_install_gate, validate_state,
};
use super::wire::{
    BFT_SIGNER_LEASE_DOMAIN_V3, BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V3, BftSignerCommandV3,
    BftSignerLeaseCredentialV3, BftSignerReplyBodyV3, BftSignerReplyKindV3, SignedBftSignerReplyV3,
    SignedBftSignerRequestV3, encode_frame, request_digest, validate_request,
};

mod operations;

pub(super) const BFT_SIGNER_CHECKPOINT_DOMAIN_V3: &str =
    "oasis7.bft.protected-signer.v3.checkpoint";
const MAX_SESSION_OPERATIONS_V3: u64 = 65_536;

/// Provider endpoint for the independent dynamic-set V3 signer protocol.
/// Backends must supply atomic generation fencing and durable monotonic time;
/// this service makes no hardware or nonrollback claim about an implementation.
pub struct BftSignerServiceV3 {
    pub(super) binding: BftSignerBindingV3,
    pub(super) provider_key: Box<dyn BftSigningKeyHandleV3>,
    pub(super) validator_keys: BTreeMap<String, Box<dyn BftSigningKeyHandleV3>>,
    pub(super) runtime: Mutex<BftSignerServiceRuntimeV3>,
}

pub(super) struct BftSignerServiceRuntimeV3 {
    pub(super) backend: Box<dyn BftSignerBackendV3>,
    pub(super) poisoned: bool,
    pub(super) verified: Option<VerifiedBftSignerStateV3>,
    pub(super) active_lease: Option<ActiveLeaseV3>,
    pub(super) observed_time_floor_ms: i64,
    pub(super) observed_generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ActiveLeaseV3 {
    pub(super) generation: u64,
    pub(super) token_digest: String,
}

#[derive(Default)]
pub(super) struct BftSignerSessionV3 {
    pub(super) prepared: Option<PreparedAcquireV3>,
    pub(super) lease: Option<BftSignerLeaseCredentialV3>,
    pub(super) operations: u64,
    pub(super) challenges: BTreeSet<String>,
}

pub(super) struct PreparedAcquireV3 {
    pub(super) expected_generation: u64,
    pub(super) nonce: String,
}

pub(super) struct ReplyDataV3 {
    pub(super) result: BftSignerReplyKindV3,
    pub(super) checkpoint: Option<Box<BftSignerCheckpointV3>>,
    pub(super) lease_token: Option<String>,
    pub(super) generation: u64,
    pub(super) revision: u64,
    pub(super) state_digest: String,
}

impl BftSignerServiceV3 {
    pub fn open(
        binding: BftSignerBindingV3,
        provider_key: Box<dyn BftSigningKeyHandleV3>,
        validator_keys: Vec<Box<dyn BftSigningKeyHandleV3>>,
        mut backend: Box<dyn BftSignerBackendV3>,
    ) -> Result<Self, BftSignerErrorV3> {
        binding.validate()?;
        let provider_public_key = provider_key.public_key_hex();
        decode_key(&provider_public_key)?;
        if provider_public_key == binding.requester_auth_public_key
            || binding
                .provisioned_signing_keys
                .iter()
                .any(|identity| identity.public_key_hex == provider_public_key)
        {
            return Err(invalid(
                "BFT V3 provider, requester, and validator identities must be distinct",
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
                return Err(invalid("BFT V3 provisioned signing key set is ambiguous"));
            }
        }
        if binding
            .provisioned_signing_keys
            .iter()
            .any(|identity| !keys_by_public.contains_key(&identity.public_key_hex))
            || keys_by_public.len() != binding.provisioned_signing_keys.len()
        {
            return Err(invalid(
                "BFT V3 provider keys differ from the pinned provisioned identities",
            ));
        }
        let binding_digest = binding.digest()?;
        let snapshot = backend.load_provisioned(&binding_digest)?;
        validate_snapshot_shape(&snapshot, None, None)?;
        let verified = validate_state(&binding, Some(&provider_public_key), &snapshot.state)?;
        if verified.schedule.height() < binding.initial_schedule().height()
            || snapshot.lease_generation == 0
            || snapshot.protected_time_floor_ms < 0
        {
            return Err(invalid("BFT V3 provisioned backend snapshot is invalid"));
        }
        Ok(Self {
            binding,
            provider_key,
            validator_keys: keys_by_public,
            runtime: Mutex::new(BftSignerServiceRuntimeV3 {
                backend,
                poisoned: false,
                verified: Some(verified),
                active_lease: (!snapshot.lease_token_digest.is_empty()).then_some(ActiveLeaseV3 {
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
    ) -> Result<(), BftSignerErrorV3> {
        let mut session = BftSignerSessionV3::default();
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
        session: &mut BftSignerSessionV3,
    ) -> Result<(), BftSignerErrorV3> {
        loop {
            let mut header = [0u8; 4];
            match stream.read(&mut header[..1]) {
                Ok(0) => return Ok(()),
                Ok(1) => {}
                Ok(_) => return Err(invalid("BFT V3 frame header over-read")),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(invalid(format!("BFT V3 TCP frame header: {error}"))),
            }
            stream
                .read_exact(&mut header[1..])
                .map_err(|error| invalid(format!("truncated BFT V3 TCP frame header: {error}")))?;
            let length = usize::try_from(u32::from_be_bytes(header))
                .map_err(|_| invalid("BFT V3 TCP frame length conversion"))?;
            if length == 0 || length > BFT_SIGNER_MAX_WIRE_FRAME_BYTES_V3 {
                return Err(invalid("BFT V3 TCP frame length is outside its bound"));
            }
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(length)
                .map_err(|_| invalid("BFT V3 TCP frame allocation failed"))?;
            bytes.resize(length, 0);
            stream
                .read_exact(&mut bytes)
                .map_err(|error| invalid(format!("BFT V3 TCP frame body: {error}")))?;
            let response = self.handle_frame(session, &bytes)?;
            stream
                .write_all(&response)
                .map_err(|error| invalid(format!("BFT V3 TCP response write: {error}")))?;
        }
    }

    fn handle_frame(
        &self,
        session: &mut BftSignerSessionV3,
        bytes: &[u8],
    ) -> Result<Vec<u8>, BftSignerErrorV3> {
        let request: SignedBftSignerRequestV3 =
            super::crypto::decode_bounded(bytes, BFT_SIGNER_MAX_FRAME_BYTES_V3)?;
        validate_request(&self.binding, &request)?;
        let digest = request_digest(&request)?;
        if !session.challenges.insert(request.body.challenge.clone()) {
            return Err(invalid(
                "BFT V3 request challenge was replayed on this connection",
            ));
        }
        session.operations = session
            .operations
            .checked_add(1)
            .filter(|count| *count <= MAX_SESSION_OPERATIONS_V3)
            .ok_or_else(|| invalid("BFT V3 session operation counter overflow"))?;
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| invalid("BFT V3 provider lock poisoned"))?;
        let data = if runtime.poisoned {
            rejected_reply(
                &runtime,
                "BFT V3 provider is fail-closed after an uncertain operation".to_string(),
            )
        } else {
            match self.dispatch(&mut runtime, session, &request, &digest) {
                Ok(data) => data,
                Err(error) => rejected_reply(&runtime, error.0),
            }
        };
        let body = BftSignerReplyBodyV3 {
            schema_version: 3,
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
        encode_frame(&SignedBftSignerReplyV3::sign(
            body,
            self.provider_key.as_ref(),
        )?)
    }

    fn dispatch(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        session: &mut BftSignerSessionV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        match &request.body.command {
            BftSignerCommandV3::PrepareAcquire { binding_digest } => {
                self.prepare_acquire(runtime, session, request, binding_digest)
            }
            BftSignerCommandV3::Acquire {
                binding_digest,
                expected_generation,
                prepare_nonce,
            } => self.acquire(
                runtime,
                session,
                request,
                request_digest,
                binding_digest,
                *expected_generation,
                prepare_nonce,
            ),
            BftSignerCommandV3::ProtectedTime => self.protected_time(runtime, session, request),
            command => {
                let credential = require_session_lease(session, &request.body.lease)?;
                let token_digest = lease_digest(&credential.token)?;
                if runtime.active_lease.as_ref()
                    != Some(&ActiveLeaseV3 {
                        generation: credential.generation,
                        token_digest: token_digest.clone(),
                    })
                {
                    return Err(invalid(
                        "BFT V3 lease is fenced or bound to another connection",
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
        runtime: &mut BftSignerServiceRuntimeV3,
        session: &mut BftSignerSessionV3,
        request: &SignedBftSignerRequestV3,
        binding_digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        if session.lease.is_some()
            || binding_digest != self.binding.digest()?
            || request.body.lease.is_some()
        {
            return Err(invalid(
                "BFT V3 prepare-acquire is not valid for this session",
            ));
        }
        let snapshot = runtime.backend.load_provisioned(&self.binding.digest()?)?;
        self.verify_observed_snapshot(
            runtime,
            &snapshot,
            &BftSignerLeaseCredentialV3 {
                generation: snapshot.lease_generation,
                token: String::new(),
            },
        )?;
        let nonce = random_nonce()?;
        session.prepared = Some(PreparedAcquireV3 {
            expected_generation: snapshot.lease_generation,
            nonce: nonce.clone(),
        });
        let state = runtime
            .verified
            .as_ref()
            .ok_or_else(|| invalid("BFT V3 verified state cache is unavailable"))?;
        Ok(ReplyDataV3 {
            result: BftSignerReplyKindV3::Prepared {
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
        runtime: &mut BftSignerServiceRuntimeV3,
        session: &mut BftSignerSessionV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
        binding_digest: &str,
        expected_generation: u64,
        nonce: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let prepared = session
            .prepared
            .as_ref()
            .ok_or_else(|| invalid("BFT V3 acquire has no connection-bound preparation"))?;
        if session.lease.is_some()
            || request.body.lease.is_some()
            || binding_digest != self.binding.digest()?
            || prepared.expected_generation != expected_generation
            || prepared.nonce != nonce
        {
            return Err(invalid(
                "BFT V3 acquire differs from connection preparation",
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
                    "BFT V3 acquire outcome is uncertain; provider poisoned: {error}"
                )));
            }
        };
        let snapshot = match outcome {
            BftSignerAcquireOutcomeV3::Conflict(snapshot) => {
                if snapshot.lease_token_digest == token_digest {
                    runtime.poisoned = true;
                    return Err(invalid(
                        "BFT V3 acquire conflict installed the attempted token; provider poisoned",
                    ));
                }
                if let Err(error) = validate_snapshot_shape(&snapshot, None, None) {
                    runtime.poisoned = true;
                    return Err(invalid(format!(
                        "BFT V3 acquire conflict snapshot is invalid; provider poisoned: {error}"
                    )));
                }
                let verified = match self.verify_snapshot_against_cache(runtime, &snapshot) {
                    Ok(verified) => verified,
                    Err(error) => {
                        runtime.poisoned = true;
                        return Err(invalid(format!(
                            "BFT V3 acquire conflict snapshot is invalid; provider poisoned: {error}"
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
                    return Err(invalid("BFT V3 acquire conflict snapshot regressed"));
                }
                runtime.verified = Some(verified);
                runtime.observed_generation = snapshot.lease_generation;
                runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
                runtime.active_lease =
                    (!snapshot.lease_token_digest.is_empty()).then(|| ActiveLeaseV3 {
                        generation: snapshot.lease_generation,
                        token_digest: snapshot.lease_token_digest.clone(),
                    });
                let state = runtime
                    .verified
                    .as_ref()
                    .ok_or_else(|| invalid("BFT V3 acquired verified state is unavailable"))?;
                return Ok(ReplyDataV3 {
                    result: BftSignerReplyKindV3::AcquireConflict {
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
            BftSignerAcquireOutcomeV3::Acquired(snapshot) => snapshot,
        };
        let next_generation = expected_generation
            .checked_add(1)
            .ok_or_else(|| invalid("BFT V3 lease generation overflow"))?;
        if snapshot.lease_generation != next_generation
            || snapshot.lease_token_digest != token_digest
            || snapshot.lease_token_digest.is_empty()
        {
            runtime.poisoned = true;
            return Err(invalid("BFT V3 acquire CAS returned an unexpected lease"));
        }
        if snapshot.lease_generation <= runtime.observed_generation
            || snapshot.protected_time_floor_ms < runtime.observed_time_floor_ms
        {
            runtime.poisoned = true;
            return Err(invalid(
                "BFT V3 acquired snapshot regressed; provider poisoned",
            ));
        }
        let verified = match self.verify_snapshot_against_cache(runtime, &snapshot) {
            Ok(verified) => verified,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V3 acquire committed but returned an invalid snapshot; provider poisoned: {error}"
                )));
            }
        };
        runtime.observed_generation = snapshot.lease_generation;
        runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
        runtime.verified = Some(verified);
        runtime.active_lease = Some(ActiveLeaseV3 {
            generation: snapshot.lease_generation,
            token_digest,
        });
        let credential = BftSignerLeaseCredentialV3 {
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
        Ok(ReplyDataV3 {
            result: BftSignerReplyKindV3::Acquired,
            checkpoint: Some(Box::new(checkpoint.clone())),
            lease_token: Some(raw_token),
            generation: checkpoint.lease_generation,
            revision: checkpoint.state.revision,
            state_digest: checkpoint.state.state_digest.clone(),
        })
    }

    fn protected_time(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        session: &BftSignerSessionV3,
        request: &SignedBftSignerRequestV3,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let credential = require_session_lease(session, &request.body.lease)?;
        let token_digest = lease_digest(&credential.token)?;
        if runtime.active_lease.as_ref()
            != Some(&ActiveLeaseV3 {
                generation: credential.generation,
                token_digest: token_digest.clone(),
            })
        {
            return Err(invalid("BFT V3 protected-time lease is fenced"));
        }
        let now = match runtime
            .backend
            .protected_time_ms(credential.generation, &token_digest)
        {
            Ok(now) => now,
            Err(error) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "BFT V3 protected clock is unavailable; provider poisoned: {error}"
                )));
            }
        };
        if now < runtime.observed_time_floor_ms || now < 0 {
            runtime.poisoned = true;
            return Err(invalid("BFT V3 backend protected clock rollback"));
        }
        runtime.observed_time_floor_ms = now;
        let cached = runtime
            .verified
            .as_ref()
            .ok_or_else(|| invalid("BFT V3 verified state cache is unavailable"))?;
        Ok(ReplyDataV3 {
            result: BftSignerReplyKindV3::ProtectedTime { time_ms: now },
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
        runtime: &mut BftSignerServiceRuntimeV3,
        _session: &mut BftSignerSessionV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
        credential: BftSignerLeaseCredentialV3,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV3,
        command: &BftSignerCommandV3,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let cached_schedule = runtime
            .verified
            .as_ref()
            .map(|verified| verified.schedule.clone())
            .ok_or_else(|| invalid("BFT V3 verified state cache is unavailable"))?;
        let state = &snapshot.state;
        if state.pending_driver_apply.is_some()
            && !matches!(
                command,
                BftSignerCommandV3::Checkpoint | BftSignerCommandV3::ConfirmApplied { .. }
            )
        {
            return Err(invalid(
                "BFT V3 original driver installation is not confirmed",
            ));
        }
        match command {
            BftSignerCommandV3::Checkpoint => self.state_reply(
                snapshot,
                request,
                request_digest,
                BftSignerReplyKindV3::Checkpoint,
            ),
            BftSignerCommandV3::StartHeightTimer { expected_revision } => {
                if *expected_revision != state.revision {
                    return Err(invalid("BFT V3 height timer revision is stale"));
                }
                let Some(old_journal) = state.journal.as_ref() else {
                    return self.state_reply(
                        snapshot,
                        request,
                        request_digest,
                        BftSignerReplyKindV3::HeightTimerStarted,
                    );
                };
                if old_journal.height_started_at_ms().is_some() {
                    return self.state_reply(
                        snapshot,
                        request,
                        request_digest,
                        BftSignerReplyKindV3::HeightTimerStarted,
                    );
                }
                let now = self.sample_time(runtime, &credential, token_digest)?;
                let mut candidate = state.candidate();
                let journal = candidate
                    .journal
                    .as_mut()
                    .ok_or_else(|| invalid("BFT V3 active signer journal disappeared"))?;
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
                    BftSignerReplyKindV3::HeightTimerStarted,
                )
            }
            BftSignerCommandV3::Advance {
                expected_revision,
                expected_state_digest,
                candidate,
            } => {
                if *expected_revision != state.revision
                    || expected_state_digest != &state.state_digest
                {
                    return Err(invalid("BFT V3 advance checkpoint is stale"));
                }
                if candidate.pending_driver_apply != state.pending_driver_apply
                    || candidate.applied_installs != state.applied_installs
                {
                    return Err(invalid(
                        "BFT V3 protocol advance cannot change driver confirmation",
                    ));
                }
                let event_time = appended_event_time(state, candidate)?;
                if let Some(event_time) = event_time {
                    let now = self.sample_time(runtime, &credential, token_digest)?;
                    if event_time < 0 || event_time > now {
                        return Err(invalid(
                            "BFT V3 event time is outside protected time bounds",
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
                    BftSignerReplyKindV3::Advanced,
                )
            }
            BftSignerCommandV3::SignProposal {
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
            BftSignerCommandV3::SignVote {
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
            BftSignerCommandV3::ConfirmApplied {
                expected_revision,
                expected_state_digest,
                height,
                header_id,
                driver_install_digest,
            } => {
                super::state::validate_digest(driver_install_digest)?;
                if let Some(receipt) = state.applied_installs.iter().find(|r| r.height == *height) {
                    if receipt.header_id != *header_id
                        || receipt.driver_install_digest != *driver_install_digest
                    {
                        return Err(invalid(
                            "BFT V3 original applied receipt cannot be replaced",
                        ));
                    }
                    return self.state_reply(
                        snapshot,
                        request,
                        request_digest,
                        BftSignerReplyKindV3::AppliedConfirmed,
                    );
                }
                if *expected_revision != state.revision
                    || expected_state_digest != &state.state_digest
                    || state.pending_driver_apply.as_ref()
                        != Some(&super::state::BftDriverApplyIntentV3 {
                            height: *height,
                            header_id: header_id.clone(),
                        })
                {
                    return Err(invalid(
                        "BFT V3 applied confirmation differs from original pending QC",
                    ));
                }
                let mut candidate = state.candidate();
                candidate.pending_driver_apply = None;
                candidate
                    .applied_installs
                    .push(super::state::BftDriverAppliedReceiptV3 {
                        height: *height,
                        header_id: header_id.clone(),
                        driver_install_digest: driver_install_digest.clone(),
                    });
                self.commit_candidate(
                    runtime,
                    &credential,
                    token_digest,
                    state,
                    candidate,
                    None,
                    request,
                    request_digest,
                    BftSignerReplyKindV3::AppliedConfirmed,
                )
            }
            BftSignerCommandV3::AppendCommittedCertificate {
                expected_revision,
                expected_state_digest,
                certificate,
            } => {
                if *expected_revision != state.revision
                    || expected_state_digest != &state.state_digest
                {
                    return Err(invalid("BFT V3 certificate append checkpoint is stale"));
                }
                encode_bounded(certificate.as_ref(), BFT_SIGNER_MAX_CERTIFICATE_BYTES_V3)?;
                let mut candidate = state.candidate();
                candidate.history.push(certificate.as_ref().clone());
                candidate.pending_driver_apply = Some(super::state::BftDriverApplyIntentV3 {
                    height: certificate.execution_commitment.height,
                    header_id: certificate.header.block_id().map_err(invalid)?,
                });
                let token = verify_formal_bft_header_certificate_v3(&cached_schedule, certificate)
                    .map_err(|error| invalid(format!("BFT V3 commit certificate: {error}")))?;
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
                    BftSignerReplyKindV3::CertificateAppended,
                )
            }
            _ => Err(invalid("BFT V3 command is not valid as a leased operation")),
        }
    }
}

fn appended_event_time(
    previous: &BftSignerStateV3,
    candidate: &BftSignerCandidateV3,
) -> Result<Option<i64>, BftSignerErrorV3> {
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
    snapshot: &BftSignerBackendSnapshotV3,
    credential: Option<&BftSignerLeaseCredentialV3>,
    expected_token_digest: Option<&str>,
) -> Result<(), BftSignerErrorV3> {
    if snapshot.lease_generation == 0 || snapshot.protected_time_floor_ms < 0 {
        return Err(invalid("BFT V3 backend snapshot generation or time floor"));
    }
    if !snapshot.lease_token_digest.is_empty() && !is_lower_hex_32(&snapshot.lease_token_digest) {
        return Err(invalid("BFT V3 backend lease token digest is noncanonical"));
    }
    if let Some(expected) = expected_token_digest
        && snapshot.lease_token_digest != expected
    {
        return Err(invalid("BFT V3 backend snapshot lease token mismatch"));
    }
    if let Some(credential) = credential
        && credential.generation != snapshot.lease_generation
    {
        return Err(invalid(
            "BFT V3 backend snapshot generation differs from lease",
        ));
    }
    Ok(())
}

fn require_session_lease<'a>(
    session: &'a BftSignerSessionV3,
    request_lease: &'a Option<BftSignerLeaseCredentialV3>,
) -> Result<&'a BftSignerLeaseCredentialV3, BftSignerErrorV3> {
    let lease = request_lease
        .as_ref()
        .ok_or_else(|| invalid("BFT V3 command requires an active lease"))?;
    if session.lease.as_ref() != Some(lease) {
        return Err(invalid("BFT V3 lease is not bound to this TCP session"));
    }
    Ok(lease)
}

fn lease_digest(token: &str) -> Result<String, BftSignerErrorV3> {
    if !is_lower_hex_32(token) {
        return Err(invalid("BFT V3 lease token is noncanonical"));
    }
    digest_hex(BFT_SIGNER_LEASE_DOMAIN_V3, &token)
}

fn rejected_reply(runtime: &BftSignerServiceRuntimeV3, message: String) -> ReplyDataV3 {
    let (revision, state_digest) = runtime
        .verified
        .as_ref()
        .map(|verified| (verified.state.revision, verified.state.state_digest.clone()))
        .unwrap_or_default();
    ReplyDataV3 {
        result: BftSignerReplyKindV3::Rejected {
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

fn random_nonce() -> Result<String, BftSignerErrorV3> {
    #[cfg(unix)]
    {
        use std::io::Read;
        let mut bytes = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(&mut bytes))
            .map_err(|error| invalid(format!("BFT V3 CSPRNG unavailable: {error}")))?;
        Ok(hex::encode(bytes))
    }
    #[cfg(not(unix))]
    {
        Err(invalid(
            "BFT V3 provider currently requires a Unix CSPRNG implementation",
        ))
    }
}
