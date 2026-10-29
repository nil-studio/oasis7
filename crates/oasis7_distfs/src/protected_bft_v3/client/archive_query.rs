//! Reassemble and authenticate full original S before locally answering identities.
use super::*;
use crate::protected_bft_v3::archive_query::*;
use crate::protected_bft_v3::successor_window::*;
use std::time::Instant;

impl RemoteBftSignerLeaseV3 {
    pub(super) fn archive_timeout(&self, deadline: Instant) -> Result<(), BftSignerErrorV3> {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| invalid("archive client attempt expired"))?;
        self.stream
            .set_read_timeout(Some(left))
            .and_then(|_| self.stream.set_write_timeout(Some(left)))
            .map_err(|e| invalid(format!("archive deadline socket timeout: {e}")))
    }
    pub(super) fn archive_original(
        &mut self,
        request_id: &str,
        checkpoint_digest: &str,
        identity: Option<&BftSuccessorWindowArchiveIdentityV1>,
        deadline: Instant,
    ) -> Result<ValidatedBftSuccessorOriginalStateV1, BftSignerErrorV3> {
        let read_timeout = self
            .stream
            .read_timeout()
            .map_err(|e| invalid(format!("archive read timeout snapshot: {e}")))?;
        let write_timeout = self
            .stream
            .write_timeout()
            .map_err(|e| invalid(format!("archive write timeout snapshot: {e}")))?;
        let result = self.archive_original_inner(request_id, checkpoint_digest, identity, deadline);
        let restored = self
            .stream
            .set_read_timeout(read_timeout)
            .and_then(|_| self.stream.set_write_timeout(write_timeout));
        match (result, restored) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(invalid(format!("restore archive socket timeout: {error}"))),
        }
    }
    fn archive_original_inner(
        &mut self,
        request_id: &str,
        checkpoint_digest: &str,
        identity: Option<&BftSuccessorWindowArchiveIdentityV1>,
        deadline: Instant,
    ) -> Result<ValidatedBftSuccessorOriginalStateV1, BftSignerErrorV3> {
        if request_id.is_empty() || request_id.len() > 256 || request_id.trim() != request_id {
            return Err(invalid("archive request identity bounds"));
        }
        super::super::state::validate_digest(checkpoint_digest)?;
        if matches!(
            identity,
            Some(BftSuccessorWindowArchiveIdentityV1::CanonicalRequest { .. })
        ) {
            return Err(invalid(
                "public archive cannot resolve private canonical requests",
            ));
        }
        // Bootstrap exclusively from a previously authenticated control checkpoint.
        // Archive attempts must never obtain unmetered checkpoint/record frames.
        let current = self.checkpoint.clone();
        let refresh = || {
            invalid(
                "archive cached original request unresolved or unavailable; refresh checkpoint explicitly",
            )
        };
        let window = current
            .state
            .successor_window
            .as_ref()
            .ok_or_else(refresh)?;
        let minimum = window.minimum.as_ref().ok_or_else(refresh)?;
        let index = window
            .requests
            .iter()
            .position(|r| r.request.intent.body.checkpoint.request_id == request_id)
            .ok_or_else(refresh)?;
        let record = &window.requests[index];
        if record.request.intent.body.checkpoint.digest()? != checkpoint_digest {
            return Err(refresh());
        }
        record.validate(&self.binding, &self.trust.provider_public_key)?;
        self.archive_timeout(deadline)?;
        let mut predecessor: Option<VerifiedBftSuccessorWindowV1> = None;
        let mut result = None;
        let mut chunks = 0usize;
        let mut counter = self.archive_observation.as_ref().map_or(0, |o| o.counter);
        for (n, entry) in window.requests[..=index].iter().enumerate() {
            let selected_identity = if n == index { identity } else { None };
            let original = self.archive_read_state(
                entry,
                minimum,
                selected_identity,
                &current,
                deadline,
                &mut chunks,
                &mut counter,
            )?;
            self.archive_timeout(deadline)?;
            let expected = &entry.request.intent.body;
            let previous_minimum = predecessor
                .as_ref()
                .map(|p| p.body().minimum())
                .transpose()?;
            if original.revision != expected.expected_revision
                || original.state_digest != expected.expected_state_digest
                || original.successor_window.as_ref().is_some_and(|w| {
                    w.reservation.is_some()
                        || w.requests.as_slice() != &window.requests[..n]
                        || w.minimum != previous_minimum
                })
                || (n != 0 && original.successor_window.is_none())
            {
                return Err(invalid(
                    "archive original State history/revision/binding conflict",
                ));
            }
            let verified = super::super::state::validate_state(
                &self.binding,
                Some(&self.trust.provider_public_key),
                &original,
            )?;
            super::super::successor_public_archive::validate_actual_public_manifest(
                &entry.manifest,
                &original,
            )?;
            let token = expected.checkpoint.bind_verified_state(
                &self.binding,
                &verified,
                predecessor.as_ref(),
            )?;
            if entry.request.status == BftSuccessorWindowStatusV1::Adopted {
                predecessor = Some(token);
            }
            result = Some(ValidatedBftSuccessorOriginalStateV1 { state: original });
            self.archive_timeout(deadline)?;
        }
        result.ok_or_else(|| invalid("archive original reassembly missing"))
    }

    pub(super) fn archive_checkpoint_floor(
        &self,
        checkpoint: &BftSignerCheckpointV3,
    ) -> Result<(), BftSignerErrorV3> {
        if let Some(observed) = &self.archive_observation {
            let minimum = checkpoint
                .state
                .successor_window
                .as_ref()
                .and_then(|w| w.minimum.as_ref());
            if minimum.is_none_or(|m| {
                !archive_state_at_least(
                    checkpoint.state.revision,
                    &checkpoint.state.state_digest,
                    m,
                    observed.revision,
                    &observed.digest,
                    &observed.minimum,
                )
            }) {
                return Err(invalid(
                    "checkpoint regressed below authenticated archive observation",
                ));
            }
        }
        Ok(())
    }
    fn observe_archive_chunk(
        &mut self,
        body: &BftSuccessorArchiveChunkBodyV1,
    ) -> Result<(), BftSignerErrorV3> {
        if let Some(previous) = &self.archive_observation {
            if !archive_state_at_least(
                body.actual_revision,
                &body.actual_state_digest,
                &body.minimum,
                previous.revision,
                &previous.digest,
                &previous.minimum,
            ) || body.reserved_response_bytes <= previous.counter
            {
                return Err(invalid(
                    "archive observation revision/minimum/counter regressed or forked",
                ));
            }
        }
        self.archive_observation = Some(ArchiveObservation {
            revision: body.actual_revision,
            digest: body.actual_state_digest.clone(),
            minimum: body.minimum.clone(),
            counter: body.reserved_response_bytes,
        });
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn archive_read_state(
        &mut self,
        record: &BftSuccessorWindowStoredRequestV1,
        minimum: &BftSuccessorWindowMinimumV1,
        identity: Option<&BftSuccessorWindowArchiveIdentityV1>,
        current: &BftSignerCheckpointV3,
        deadline: Instant,
        chunks: &mut usize,
        counter: &mut u64,
    ) -> Result<super::super::state::BftSignerStateV3, BftSignerErrorV3> {
        let object = &record.original_public_state;
        object.validate()?;
        if object.bytes == 0
            || object.bytes > super::super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3 as u64
        {
            return Err(invalid("archive original total exceeds bound"));
        }
        let total =
            usize::try_from(object.bytes).map_err(|_| invalid("archive total conversion"))?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(total)
            .map_err(|_| invalid("archive original allocation failed"))?;
        while bytes.len() < total {
            self.archive_timeout(deadline)?;
            *chunks = chunks
                .checked_add(1)
                .filter(|n| *n <= BFT_SUCCESSOR_ARCHIVE_MAX_CHUNKS_V1)
                .ok_or_else(|| invalid("archive attempt chunk count exhausted"))?;
            let challenge = random_nonce()?;
            let checkpoint_digest = record.request.intent.body.checkpoint.digest()?;
            let segment_root = record.manifest.digest()?;
            // Legacy manifests have only the raw State row: explicit read plus local derivation.
            let target = if let Some(identity) =
                identity.filter(|_| record.manifest.public_index.is_some())
            {
                BftSuccessorArchiveReadTargetV1::SigningIdentity {
                    query: Box::new(BftSuccessorWindowArchiveQueryV1 {
                        schema_version: 1,
                        source: minimum.source.clone(),
                        request_id: record.request.intent.body.checkpoint.request_id.clone(),
                        checkpoint_digest,
                        segment_root,
                        identity: identity.clone(),
                        challenge: challenge.clone(),
                        lease_generation: self.credential.generation,
                        lease_token_digest: super::super::crypto::digest_hex(
                            super::super::wire::BFT_SIGNER_LEASE_DOMAIN_V3,
                            &self.credential.token,
                        )?,
                        minimum_lower_bound: minimum.clone(),
                    }),
                }
            } else {
                BftSuccessorArchiveReadTargetV1::GetOriginalState {
                    request_id: record.request.intent.body.checkpoint.request_id.clone(),
                    checkpoint_digest,
                    segment_root,
                    minimum_lower_bound: minimum.clone(),
                }
            };
            let read = BftSuccessorArchiveReadV1 {
                target,
                object: object.clone(),
                offset: bytes.len() as u64,
                length: (total - bytes.len()).min(BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1) as u32,
            };
            read.validate()?;
            let request = SignedBftSignerRequestV3::sign(
                &self.binding,
                &self.requester_key,
                Some(self.credential.clone()),
                BftSignerCommandV3::ReadSuccessorArchive {
                    read: Box::new(read.clone()),
                },
                challenge,
            )?;
            let frame = encode_frame(&request)?;
            self.archive_timeout(deadline)?;
            std::io::Write::write_all(
                &mut ArchiveDeadlineStream {
                    stream: &mut self.stream,
                    deadline,
                },
                &frame,
            )
            .map_err(|e| invalid(format!("archive write: {e}")))?;
            self.archive_timeout(deadline)?;
            let reply: SignedBftSignerReplyV3 = read_frame(&mut ArchiveDeadlineStream {
                stream: &mut self.stream,
                deadline,
            })?;
            self.archive_timeout(deadline)?;
            validate_reply(&self.binding, &self.trust, &request, &reply)?;
            RemoteBftSignerProviderV3::checked_result(&reply)?;
            let BftSignerReplyKindV3::ArchiveChunk { chunk } = &reply.body.result else {
                return Err(invalid("archive reply kind mismatch"));
            };
            chunk.verify(&self.trust.provider_public_key)?;
            let body = &chunk.body;
            let token_digest = super::super::crypto::digest_hex(
                super::super::wire::BFT_SIGNER_LEASE_DOMAIN_V3,
                &self.credential.token,
            )?;
            if body.read != read
                || body.binding_digest != self.binding.digest()?
                || body.request_digest != super::super::wire::request_digest(&request)?
                || body.challenge != request.body.challenge
                || body.lease_generation != self.credential.generation
                || body.lease_token_digest != token_digest
                || reply.body.lease_generation != self.credential.generation
                || reply.body.lease_token.is_some()
                || reply.body.checkpoint.is_some()
                || !archive_state_at_least(
                    body.actual_revision,
                    &body.actual_state_digest,
                    &body.minimum,
                    current.state.revision,
                    &current.state.state_digest,
                    minimum,
                )
                || reply.body.state_revision != body.actual_revision
                || reply.body.state_digest != body.actual_state_digest
                || body.reserved_response_bytes <= *counter
            {
                return Err(invalid(
                    "archive chunk request/lease/current state/minimum/counter conflict",
                ));
            }
            self.observe_archive_chunk(body)?;
            *counter = body.reserved_response_bytes;
            bytes.extend_from_slice(&body.bytes);
            self.archive_timeout(deadline)?;
        }
        object.validate_bytes(&bytes)?;
        let state = super::super::crypto::decode_bounded(
            &bytes,
            super::super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
        )?;
        self.archive_timeout(deadline)?;
        Ok(state)
    }
}

/// Each partial socket read/write consumes the same deadline, including drip-fed frames.
pub(super) struct ArchiveDeadlineStream<'a> {
    pub(super) stream: &'a mut TcpStream,
    pub(super) deadline: Instant,
}
impl ArchiveDeadlineStream<'_> {
    fn remaining(&self) -> std::io::Result<Duration> {
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "archive attempt expired")
            })
    }
}
impl std::io::Read for ArchiveDeadlineStream<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.stream.set_read_timeout(Some(self.remaining()?))?;
        let n = std::io::Read::read(self.stream, bytes)?;
        self.remaining()?;
        Ok(n)
    }
}
impl std::io::Write for ArchiveDeadlineStream<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        let n = std::io::Write::write(self.stream, bytes)?;
        self.remaining()?;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.set_write_timeout(Some(self.remaining()?))?;
        std::io::Write::flush(self.stream)?;
        self.remaining()?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "archive_query_deadline_tests.rs"]
mod deadline_tests;

/// Only a rejection floor for archive observations, never current signing authority.
#[derive(Clone)]
pub(super) struct ArchiveObservation {
    revision: u64,
    digest: String,
    minimum: BftSuccessorWindowMinimumV1,
    counter: u64,
}
fn archive_minimum_at_least(
    actual: &BftSuccessorWindowMinimumV1,
    lower: &BftSuccessorWindowMinimumV1,
) -> bool {
    actual.validate().is_ok()
        && lower.validate().is_ok()
        && actual.source == lower.source
        && (actual == lower || (actual.height > lower.height && actual.sequence > lower.sequence))
}

fn archive_state_at_least(
    actual_revision: u64,
    actual_digest: &str,
    actual_minimum: &BftSuccessorWindowMinimumV1,
    lower_revision: u64,
    lower_digest: &str,
    lower_minimum: &BftSuccessorWindowMinimumV1,
) -> bool {
    actual_revision >= lower_revision
        && (actual_revision != lower_revision
            || (actual_digest == lower_digest && actual_minimum == lower_minimum))
        && archive_minimum_at_least(actual_minimum, lower_minimum)
}

#[cfg(test)]
#[path = "archive_query_observation_tests.rs"]
mod observation_tests;
