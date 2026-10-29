//! Lease-authenticated, charged immutable public archive chunks.
use super::*;
use crate::protected_bft_v3::archive_query::*;
use std::time::Instant;

impl BftSignerServiceV3 {
    pub(super) fn process_archive_query(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        request: &SignedBftSignerRequestV3,
        digest: &str,
        snapshot: BftSignerBackendSnapshotV3,
        read: &BftSuccessorArchiveReadV1,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        read.validate()?;
        let credential = request
            .body
            .lease
            .as_ref()
            .ok_or_else(|| invalid("archive read requires current lease"))?;
        let token_digest = lease_digest(&credential.token)?;
        read.target
            .validate(&self.binding, credential.generation, &token_digest)?;
        if let BftSuccessorArchiveReadTargetV1::SigningIdentity { query } = &read.target {
            if query.challenge != request.body.challenge {
                return Err(invalid("archive inner query challenge conflict"));
            }
        }
        // Legacy namespaces cannot implicitly initialize a fresh transfer budget.
        if snapshot.successor_archive_transfer.is_none() {
            return Err(invalid("archive transfer ledger unsupported"));
        }
        let support = self
            .successor_support
            .as_ref()
            .ok_or_else(|| invalid("archive read support unavailable"))?;
        let deadline = runtime
            .successor_deadline
            .ok_or_else(|| invalid("archive read deadline unavailable"))?;
        archive_live(deadline)?;
        let provider = self.provider_key.public_key_hex();
        support.verify_archives(&self.binding, &snapshot.state, &provider, deadline)?;
        let window = snapshot
            .state
            .successor_window
            .as_ref()
            .ok_or_else(|| invalid("archive window missing"))?;
        let minimum = window
            .minimum
            .as_ref()
            .ok_or_else(|| invalid("archive current minimum missing"))?;
        let (id, checkpoint, segment, lower) = read.target.coordinates();
        minimum.validate()?;
        if minimum.source != lower.source
            || minimum.height < lower.height
            || minimum.sequence < lower.sequence
            || (minimum.height == lower.height && minimum != lower)
        {
            return Err(invalid(
                "archive current minimum is below requested bound or forked",
            ));
        }
        let record = window
            .request(id)
            .ok_or_else(|| invalid("archive original request unavailable"))?;
        if record.request.intent.body.checkpoint.digest()? != checkpoint
            || record.manifest.digest()? != segment
            || record.original_public_state != read.object
        {
            return Err(invalid(
                "archive original request/checkpoint/segment/object conflict",
            ));
        }
        if matches!(
            &read.target,
            BftSuccessorArchiveReadTargetV1::SigningIdentity { .. }
        ) && record.manifest.public_index.is_none()
        {
            return Err(invalid(
                "legacy archive requires explicit original State read",
            ));
        }
        let original = support.archive.load_public_state(&read.object)?;
        let bytes = encode_bounded(&original, BFT_SIGNER_MAX_FRAME_BYTES_V3)?;
        read.object.validate_bytes(&bytes)?;
        archive_live(deadline)?;
        let start =
            usize::try_from(read.offset).map_err(|_| invalid("archive offset conversion"))?;
        let end = start
            .checked_add(read.length as usize)
            .ok_or_else(|| invalid("archive range overflow"))?;
        let payload = bytes
            .get(start..end)
            .ok_or_else(|| invalid("archive payload range"))?
            .to_vec();
        let mut body = BftSuccessorArchiveChunkBodyV1 {
            schema_version: 1,
            binding_digest: self.binding.digest()?,
            provider_public_key: provider,
            request_digest: digest.into(),
            challenge: request.body.challenge.clone(),
            lease_generation: credential.generation,
            lease_token_digest: token_digest,
            actual_revision: snapshot.state.revision,
            actual_state_digest: snapshot.state.state_digest.clone(),
            minimum: minimum.clone(),
            read: read.clone(),
            reserved_response_bytes: u64::MAX,
            bytes: payload,
        };
        // Charge an exact encoded full-frame upper bound before any payload release.
        // Maximum-width counter and fixed-size Ed25519 signatures avoid self reference.
        let upper = SignedBftSignerReplyV3 {
            body: BftSignerReplyBodyV3 {
                schema_version: 3,
                binding_digest: body.binding_digest.clone(),
                request_digest: digest.into(),
                challenge: request.body.challenge.clone(),
                lease_generation: credential.generation,
                lease_token: None,
                state_revision: snapshot.state.revision,
                state_digest: snapshot.state.state_digest.clone(),
                checkpoint: None,
                result: BftSignerReplyKindV3::ArchiveChunk {
                    chunk: Box::new(BftSuccessorArchiveChunkV1 {
                        body: body.clone(),
                        provider_signature_hex: "0".repeat(128),
                    }),
                },
            },
            provider_signature_hex: "0".repeat(128),
        };
        let reserve = u64::try_from(encode_frame(&upper)?.len())
            .map_err(|_| invalid("archive response size conversion"))?;
        archive_live(deadline)?;
        let charged = self.reserve_archive_transfer(runtime, &snapshot, reserve)?;
        body.reserved_response_bytes = charged
            .successor_archive_transfer
            .as_ref()
            .ok_or_else(|| invalid("archive charged ledger missing"))?
            .reserved_response_bytes;
        archive_live(deadline)?;
        let chunk = BftSuccessorArchiveChunkV1::sign(body, self.provider_key.as_ref())?;
        archive_live(deadline)?;
        Ok(ReplyDataV3 {
            result: BftSignerReplyKindV3::ArchiveChunk {
                chunk: Box::new(chunk),
            },
            checkpoint: None,
            lease_token: None,
            generation: snapshot.lease_generation,
            revision: snapshot.state.revision,
            state_digest: snapshot.state.state_digest,
        })
    }
}
fn archive_live(deadline: Instant) -> Result<(), BftSignerErrorV3> {
    if Instant::now() >= deadline {
        Err(invalid("archive read attempt expired"))
    } else {
        Ok(())
    }
}

pub(super) fn write_response<S: std::io::Write>(
    stream: &mut S,
    response: &[u8],
    deadline: Option<Instant>,
    deadline_policy: &mut Option<&mut dyn FnMut(&mut S, Option<Instant>) -> std::io::Result<()>>,
) -> Result<(), BftSignerErrorV3> {
    let Some(deadline) = deadline else {
        return stream
            .write_all(response)
            .map_err(|e| invalid(format!("BFT V3 TCP response write: {e}")));
    };
    let policy = deadline_policy
        .as_mut()
        .ok_or_else(|| invalid("archive payload requires a deadline-capable TCP transport"))?;
    let result = (|| {
        let mut offset = 0;
        while offset < response.len() {
            archive_live(deadline)?;
            policy(stream, Some(deadline))
                .map_err(|e| invalid(format!("archive write deadline: {e}")))?;
            let written = match stream.write(&response[offset..]) {
                Ok(0) => return Err(invalid("archive response write returned zero")),
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(invalid(format!("archive response write: {e}"))),
            };
            archive_live(deadline)?;
            offset += written;
        }
        Ok(())
    })();
    let restored =
        policy(stream, None).map_err(|e| invalid(format!("restore provider write timeout: {e}")));
    match (result, restored) {
        (Err(error), _) => Err(error),
        (Ok(()), other) => other,
    }
}

#[cfg(test)]
#[path = "archive_query_write_tests.rs"]
mod write_tests;
