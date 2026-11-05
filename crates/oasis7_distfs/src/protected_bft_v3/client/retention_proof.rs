//! Charged metadata reads. One client absolute deadline spans all chunks;
//! the service retains the existing per-request archive timeout semantics.
use super::*;
use crate::protected_bft_v3::retention_proof_query::*;
use crate::protected_bft_v3::{BftPrivateRetentionOperationProofV1, BftPrivateRetentionSealV1};
use std::time::Instant;

impl RemoteBftSignerLeaseV3 {
    pub(super) fn selection_operation_proof(
        &mut self,
        current: &super::super::BftPrivateRetentionLedgerV1,
        operation: String,
        deadline: Instant,
    ) -> Result<BftPrivateRetentionOperationProofV1, BftSignerErrorV3> {
        let role = self
            .private_retention_attestor
            .as_ref()
            .ok_or_else(|| invalid("selection fixed role missing"))?
            .clone();
        super::super::private_retention_current_anchor::verify_current_anchor(
            &role,
            current,
            self.private_retention_latest_seal.as_ref(),
        )?;
        if self.checkpoint.state.private_retention.as_ref() != Some(&current.receipt.body.head) {
            return Err(invalid(
                "selection proof requires exact validated current ledger",
            ));
        }
        let selected = current
            .selection_index
            .as_ref()
            .and_then(|i| {
                i.selections
                    .iter()
                    .find(|s| s.operation_digest == operation)
            })
            .ok_or_else(|| invalid("current selected operation absent"))?
            .clone();
        let revision = self.checkpoint.state.revision;
        let state_digest = self.checkpoint.state.state_digest.clone();
        let bytes = self.retention_proof_bytes(
            RetentionProofTargetV1::SelectionOperation(operation.clone()),
            deadline,
        )?;
        let proof: BftPrivateRetentionOperationProofV1 = super::super::crypto::decode_bounded(
            &bytes,
            super::super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
        )?;
        super::super::private_retention_selection::validate_original_selection(
            &self.binding,
            &role,
            current,
            &proof.state,
            &proof.ledger,
            &selected,
            self.active_context.clone(),
        )?;
        if proof.ledger.receipt.body.operation_digest != operation
            || proof.ledger.receipt.body.original_lease_generation > self.credential.generation
            || self.checkpoint.state.revision != revision
            || self.checkpoint.state.state_digest != state_digest
            || self.checkpoint.state.private_retention.as_ref() != Some(&current.receipt.body.head)
        {
            return Err(invalid("original selection proof/current lease changed"));
        }
        self.archive_timeout(deadline)?;
        Ok(proof)
    }
    pub(super) fn operation_proof(
        &mut self,
        digest: String,
        deadline: Instant,
    ) -> Result<BftPrivateRetentionOperationProofV1, BftSignerErrorV3> {
        let bytes = self
            .retention_proof_bytes(RetentionProofTargetV1::Operation(digest.clone()), deadline)?;
        let proof: BftPrivateRetentionOperationProofV1 = super::super::crypto::decode_bounded(
            &bytes,
            super::super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
        )?;
        let role = self
            .private_retention_attestor
            .as_ref()
            .ok_or_else(|| invalid("retention proof fixed role missing"))?;
        super::super::private_retention_seal::verify_map_at_floor(
            role,
            &proof.ledger,
            proof.ledger.map.coverage_start_height,
        )?;
        proof.state.validate_digest()?;
        let body = &proof.ledger.receipt.body;
        if super::super::crypto::digest_hex(
            "oasis7.bft.private-retention.v1.receipt",
            &proof.ledger.receipt,
        )? != digest
            || proof.state.revision != body.after_revision
            || proof.state.state_digest != body.after_state_digest
            || proof.state.private_retention.as_ref() != Some(&body.head)
            || body.original_lease_generation > self.credential.generation
        {
            return Err(invalid("original operation receipt/State differs"));
        }
        self.archive_timeout(deadline)?;
        Ok(proof)
    }
    pub(super) fn seal_proof(
        &mut self,
        digest: String,
        deadline: Instant,
    ) -> Result<BftPrivateRetentionSealV1, BftSignerErrorV3> {
        let bytes =
            self.retention_proof_bytes(RetentionProofTargetV1::Seal(digest.clone()), deadline)?;
        let proof: BftPrivateRetentionSealV1 = super::super::crypto::decode_bounded(
            &bytes,
            super::super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3,
        )?;
        let role = self
            .private_retention_attestor
            .as_ref()
            .ok_or_else(|| invalid("retention proof fixed role missing"))?;
        proof.body.validate_shape()?;
        proof.anchor()?.verify(role)?;
        super::super::private_retention_seal::verify_map_at_floor(
            role,
            &proof.body.original,
            proof.body.original.map.coverage_start_height,
        )?;
        if proof.body.digest()? != digest {
            return Err(invalid("original seal identity differs"));
        }
        self.archive_timeout(deadline)?;
        Ok(proof)
    }
    fn retention_proof_bytes(
        &mut self,
        target: RetentionProofTargetV1,
        deadline: Instant,
    ) -> Result<Vec<u8>, BftSignerErrorV3> {
        if self.private_retention_attestor.is_none() {
            return Err(invalid("retention proof fixed role missing"));
        }
        let prior = self.signing_result_deadline;
        let read_timeout = self
            .stream
            .read_timeout()
            .map_err(|e| invalid(e.to_string()))?;
        let write_timeout = self
            .stream
            .write_timeout()
            .map_err(|e| invalid(e.to_string()))?;
        self.signing_result_deadline = Some(prior.map_or(deadline, |old| old.min(deadline)));
        let result = self.retention_proof_bytes_inner(target, deadline);
        self.signing_result_deadline = prior;
        let restored = self
            .stream
            .set_read_timeout(read_timeout)
            .and_then(|_| self.stream.set_write_timeout(write_timeout));
        match (result, restored) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => {
                Err(invalid(format!("retention proof restore timeout: {error}")))
            }
        }
    }
    fn retention_proof_bytes_inner(
        &mut self,
        target: RetentionProofTargetV1,
        deadline: Instant,
    ) -> Result<Vec<u8>, BftSignerErrorV3> {
        let current = self.checkpoint.clone();
        let mut read = RetentionProofReadV1 {
            target,
            revision: current.state.revision,
            state_digest: current.state.state_digest.clone(),
            object: None,
            offset: 0,
        };
        let mut output = Vec::new();
        loop {
            read.validate()?;
            self.archive_timeout(deadline)?;
            let (_, reply) = self.command(BftSignerCommandV3::ReadPrivateRetentionProof {
                read: Box::new(read.clone()),
            })?;
            RemoteBftSignerProviderV3::checked_result(&reply)?;
            let BftSignerReplyKindV3::PrivateRetentionProofChunk { chunk } = &reply.body.result
            else {
                return Err(invalid("retention proof reply kind"));
            };
            // The original proof is not a new current checkpoint. Authenticate
            // every reply against the cached live State, never overwrite it.
            if reply.body.checkpoint.is_some()
                || reply.body.state_revision != current.state.revision
                || reply.body.state_digest != current.state.state_digest
                || chunk.read != read
                || read
                    .object
                    .as_ref()
                    .is_some_and(|object| *object != chunk.object)
                || chunk.object.bytes == 0
                || chunk.object.bytes > super::super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3 as u64
                || chunk.bytes.is_empty()
                || chunk.bytes.len()
                    > super::super::archive_query::BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1
                || chunk.reserved_response_bytes <= self.archive_reserved_response_bytes()
            {
                return Err(invalid("retention proof chunk/current binding differs"));
            }
            let expected_length = chunk
                .object
                .bytes
                .checked_sub(read.offset)
                .ok_or_else(|| invalid("retention proof offset beyond object"))?
                .min(super::super::archive_query::BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1 as u64);
            if chunk.bytes.len() as u64 != expected_length {
                return Err(invalid("retention proof incomplete canonical chunk"));
            }
            super::super::state::validate_digest(&chunk.object.digest)?;
            super::super::archive_transfer::validate_successor_archive_budget(
                &current.state,
                Some(
                    &super::super::archive_transfer::BftSuccessorArchiveTransferLedgerV1 {
                        schema_version: 1,
                        reserved_response_bytes: chunk.reserved_response_bytes,
                    },
                ),
            )?;
            // Charge observation survives subsequent malformed/missing chunks.
            self.private_retention_transfer_floor = chunk.reserved_response_bytes;
            let end = read
                .offset
                .checked_add(chunk.bytes.len() as u64)
                .filter(|end| *end <= chunk.object.bytes)
                .ok_or_else(|| invalid("retention proof chunk range"))?;
            if output.is_empty() {
                output
                    .try_reserve_exact(chunk.object.bytes as usize)
                    .map_err(|e| invalid(format!("retention proof allocation: {e}")))?;
            }
            output.extend_from_slice(&chunk.bytes);
            if end == chunk.object.bytes {
                if blake3::hash(&output).to_hex().as_str() != chunk.object.digest {
                    return Err(invalid("retention proof original hash differs"));
                }
                self.archive_timeout(deadline)?;
                return Ok(output);
            }
            read.offset = end;
            read.object = Some(chunk.object.clone());
        }
    }
}
