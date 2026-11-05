//! Exact immutable public metadata, charged before every chunk release.
use super::*;
use crate::protected_bft_v3::retention_proof_query::*;
use serde::Serialize;

#[derive(Serialize)]
struct OperationBorrow<'a> {
    state: &'a BftSignerStateV3,
    ledger: &'a crate::protected_bft_v3::BftPrivateRetentionLedgerV1,
}
impl BftSignerServiceV3 {
    pub(super) fn process_retention_proof(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        request: &SignedBftSignerRequestV3,
        digest: &str,
        observed: ObservedSnapshotV3,
        read: &RetentionProofReadV1,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        read.validate()?;
        let ObservedSnapshotV3(snapshot) = observed;
        if read.revision != snapshot.state.revision
            || read.state_digest != snapshot.state.state_digest
        {
            return Err(invalid(
                "retention proof current State changed; refresh explicitly",
            ));
        }
        let support = self
            .private_retention_support
            .as_ref()
            .ok_or_else(|| invalid("retention proof requires fixed role"))?;
        let credential = request
            .body
            .lease
            .as_ref()
            .ok_or_else(|| invalid("retention proof requires lease"))?;
        let token = lease_digest(&credential.token)?;
        let deadline = runtime
            .successor_deadline
            .ok_or_else(|| invalid("retention proof deadline unavailable"))?;
        let live = || {
            if std::time::Instant::now() < deadline {
                Ok(())
            } else {
                Err(invalid("retention proof attempt expired"))
            }
        };
        live()?;
        let sealed = match runtime
            .backend
            .read_private_retention_sealed_current(credential.generation, &token)
        {
            Ok(sealed) => sealed,
            Err(error) => {
                runtime.input_view_epoch.invalidate()?;
                return Err(error);
            }
        };
        if sealed.current.signer != snapshot {
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid("retention proof atomic current changed"));
        }
        if let Err(error) = sealed.validate_inventory_with_verified_state(
            &self.binding,
            &support.attestor,
            runtime.verified.as_ref(),
        ) {
            runtime.input_view_epoch.invalidate()?;
            return Err(error);
        }
        let (object, bytes) = match &read.target {
            RetentionProofTargetV1::SelectionOperation(operation) => {
                let current = sealed
                    .current
                    .retention
                    .as_ref()
                    .ok_or_else(|| invalid("current selection ledger absent"))?;
                let selected = current
                    .selection_index
                    .as_ref()
                    .and_then(|i| {
                        i.selections
                            .iter()
                            .find(|s| &s.operation_digest == operation)
                    })
                    .ok_or_else(|| invalid("current selected original operation absent"))?;
                let original = runtime.backend.read_private_retention_operation(
                    credential.generation,
                    &token,
                    operation,
                )?;
                crate::protected_bft_v3::private_retention_selection::validate_original_selection(
                    &self.binding,
                    &support.attestor,
                    current,
                    &original.signer.state,
                    &original.retention,
                    selected,
                    runtime
                        .verified
                        .as_ref()
                        .and_then(|verified| verified.active_context.clone()),
                )?;
                canonical_chunk(
                    &OperationBorrow {
                        state: &original.signer.state,
                        ledger: &original.retention,
                    },
                    read.offset,
                )?
            }
            RetentionProofTargetV1::Operation(key) => {
                let original = sealed
                    .history
                    .get(key)
                    .ok_or_else(|| invalid("original retention operation unavailable"))?;
                if crate::protected_bft_v3::crypto::digest_hex(
                    "oasis7.bft.private-retention.v1.receipt",
                    &original.retention.receipt,
                )? != *key
                {
                    return Err(invalid("original retention operation index differs"));
                }
                canonical_chunk(
                    &OperationBorrow {
                        state: &original.signer.state,
                        ledger: &original.retention,
                    },
                    read.offset,
                )?
            }
            RetentionProofTargetV1::Seal(key) => {
                let original = sealed
                    .seals
                    .get(key)
                    .ok_or_else(|| invalid("original retention seal unavailable"))?;
                if original.body.digest()? != *key {
                    return Err(invalid("original retention seal index differs"));
                }
                canonical_chunk(original.as_ref(), read.offset)?
            }
        };
        if read
            .object
            .as_ref()
            .is_some_and(|expected| *expected != object)
        {
            return Err(invalid("retention proof object changed"));
        }
        live()?;
        // A fresh fence check precedes reservation. The reservation itself also
        // verifies that only the transfer counter changed, including on retry.
        let fresh = match runtime
            .backend
            .read_for_lease(credential.generation, &token)
        {
            Ok(fresh) => fresh,
            Err(error) => {
                runtime.input_view_epoch.invalidate()?;
                return Err(error);
            }
        };
        if fresh != snapshot {
            runtime.input_view_epoch.invalidate()?;
            return Err(invalid(
                "retention proof lease/head changed during encoding",
            ));
        }
        let mut chunk = RetentionProofChunkV1 {
            read: read.clone(),
            object,
            bytes,
            reserved_response_bytes: 0,
        };
        let mut frame = SignedBftSignerReplyV3 {
            body: BftSignerReplyBodyV3 {
                schema_version: 3,
                binding_digest: self.binding.digest()?,
                request_digest: digest.into(),
                challenge: request.body.challenge.clone(),
                lease_generation: credential.generation,
                lease_token: None,
                state_revision: snapshot.state.revision,
                state_digest: snapshot.state.state_digest.clone(),
                checkpoint: None,
                result: BftSignerReplyKindV3::PrivateRetentionProofChunk {
                    chunk: Box::new(chunk.clone()),
                },
            },
            provider_signature_hex: "0".repeat(128),
        };
        let current_counter = snapshot
            .successor_archive_transfer
            .as_ref()
            .ok_or_else(|| invalid("retention proof transfer ledger absent"))?
            .reserved_response_bytes;
        // Ed25519 hex signatures have fixed encoded width. Solve the counter's
        // CBOR integer width against the final complete signed frame, rather
        // than charging the old archive path's u64::MAX upper bound.
        let amount = exact_response_reservation(current_counter, |counter| {
            let BftSignerReplyKindV3::PrivateRetentionProofChunk { chunk } = &mut frame.body.result
            else {
                return Err(invalid("retention proof frame kind"));
            };
            chunk.reserved_response_bytes = counter;
            Ok(encode_frame(&frame)?.len())
        })?;
        drop(frame);
        live()?;
        let charged = self.reserve_archive_transfer(runtime, &snapshot, amount)?;
        chunk.reserved_response_bytes = charged
            .successor_archive_transfer
            .as_ref()
            .ok_or_else(|| invalid("retention proof transfer ledger absent"))?
            .reserved_response_bytes;
        live()?;
        Ok(ReplyDataV3 {
            result: BftSignerReplyKindV3::PrivateRetentionProofChunk {
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
