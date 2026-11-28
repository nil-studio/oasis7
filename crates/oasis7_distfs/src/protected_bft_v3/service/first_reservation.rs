//! First original-byte reservation over the authenticated V3 lease.
use super::*;
use crate::protected_bft_v3::first_reservation::{
    original_activation_state, prepare_first_reservation, prepare_first_reservation_checked,
    verify_first_reservation,
};
use crate::protected_bft_v3::fresh_coverage::{ORIGINAL_RECEIPT_RESERVE, check_namespace_budget};
use crate::protected_bft_v3::{
    BftAuthorizedOriginalReserveInputV1, BftFirstReservationPriceV1, BftFirstReservedOriginalV1,
    BftOriginalRetentionTxnSnapshotV1, BftPrivateRetentionAdmissionInputRefV1,
    BftPrivateRetentionAdmissionInputV1, BftPrivateRetentionCreationMarkerV1,
    BftRetainedOriginalOperationV1,
};
use std::time::Instant;

/// Provider-configured authority for encrypted original-byte reads and actual
/// Runtime verification. The normal signer command contains only a reference.
pub trait BftFirstReserveSourceV1: Send + Sync {
    fn read_authorized_original(
        &self,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
        deadline: Instant,
    ) -> Result<BftAuthorizedOriginalReserveInputV1, BftSignerErrorV3>;

    /// The authenticated coverage snapshot is empty at the first reservation.
    /// Verify exact original H's retained inventory, closed candidate replay,
    /// metadata, pin and selection bounds before returning price components.
    fn verify_actual_first_price(
        &self,
        binding: &BftSignerBindingV3,
        marker: &BftPrivateRetentionCreationMarkerV1,
        covered: &BftSignerStateV3,
        input: &BftPrivateRetentionAdmissionInputV1,
        deadline: Instant,
    ) -> Result<BftFirstReservationPriceV1, BftSignerErrorV3>;
}

impl BftSignerServiceV3 {
    pub(super) fn reserve_first_original(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        snapshot: BftSignerBackendSnapshotV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
        original_request_bytes: &[u8],
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        if self.first_reserve_records_original && original_request_bytes.len() > 2048 {
            return Err(invalid(
                "first Reserve original request exceeds material bound",
            ));
        }
        let fresh = self
            .fresh_coverage
            .as_ref()
            .ok_or_else(|| invalid("first Reserve requires original Fresh configuration"))?;
        let (support, timeout) = self.first_reserve.as_ref().ok_or_else(|| {
            invalid("authorized first Reserve reader and verifier are unsupported")
        })?;
        let deadline = Instant::now()
            .checked_add(*timeout)
            .ok_or_else(|| invalid("first Reserve deadline overflow"))?;
        let BftSignerCommandV3::ReservePrivateRetentionInput {
            expected_revision,
            expected_state_digest,
            reference,
        } = &request.body.command
        else {
            return Err(invalid("first original Reserve command kind"));
        };
        reference.validate()?;
        if reference.source != fresh.attestor.source
            || reference.key.ledger_origin_digest != fresh.marker.ledger_origin_digest
            || reference.key.height != fresh.marker.initial_height
        {
            return Err(invalid(
                "first Reserve requires exact original H source and height",
            ));
        }
        let generation = snapshot.lease_generation;
        let token = &snapshot.lease_token_digest;
        let envelope = if self.first_reserve_uses_original_envelope {
            let whole = runtime
                .backend
                .read_original_operation_envelope_for_lease(generation, token)?;
            whole.validate()?;
            Some(whole)
        } else {
            None
        };
        let observed = match &envelope {
            Some(whole) => whole.current.clone(),
            None => runtime
                .backend
                .read_first_reserve_for_lease(generation, token)?,
        };
        if observed.current.signer != snapshot || observed.current.marker != fresh.marker {
            return Err(invalid(
                "first Reserve atomic inventory differs from leased source",
            ));
        }
        let activation =
            observed.current.original.as_ref().ok_or_else(|| {
                invalid("first Reserve requires original covered empty inventory")
            })?;
        let before = original_activation_state(
            &self.binding,
            &fresh.attestor,
            &fresh.marker,
            activation,
            generation,
        )?;
        if *expected_revision != before.revision || *expected_state_digest != before.state_digest {
            return Err(invalid(
                "first Reserve expected original covered checkpoint stale",
            ));
        }
        let mut original_read = None;
        let (original, after) = match &observed.reservation {
            Some(original) => {
                verify_first_reservation(
                    &before,
                    activation,
                    reference,
                    original,
                    (&snapshot.state, snapshot.lease_generation),
                    &fresh.attestor,
                )?;
                if self.first_reserve_records_original {
                    verify_recorded_original(
                        envelope
                            .as_ref()
                            .ok_or_else(|| invalid("original envelope absent"))?,
                        original,
                        request,
                        &self.binding,
                    )?;
                }
                (original.clone(), snapshot.state.clone())
            }
            None if snapshot.state == before => {
                let read = support.read_authorized_original(reference, deadline)?;
                reference.validate_input(&read.input)?;
                support
                    .verify_actual_first_price(
                        &self.binding,
                        &fresh.marker,
                        &before,
                        &read.input,
                        deadline,
                    )?
                    .verify_input(&read.input)?;
                let prepared = if self.first_reserve_records_original {
                    prepare_first_reservation_checked(
                        &before,
                        activation,
                        reference,
                        read.input.clone(),
                        generation,
                        self.provider_key.as_ref(),
                        |original, after| {
                            // Pure sizing/transition checks precede the inner signature.
                            // No unsigned candidate is persisted or returned to a peer.
                            let mut candidate = observed.clone();
                            candidate.current.signer.state = after.clone();
                            let amount = self.recorded_reply_bound(
                                &candidate.current.signer,
                                original,
                                request,
                                request_digest,
                            )?;
                            candidate.current.reserved_response_bytes = candidate
                                .current
                                .reserved_response_bytes
                                .checked_add(amount)
                                .ok_or_else(|| invalid("first Reserve response budget overflow"))?;
                            check_namespace_budget(
                                &original.ledger.receipt.body,
                                candidate.current.reserved_response_bytes,
                            )?;
                            candidate.reservation = Some(original.clone());
                            let operation = recorded_operation(
                                original,
                                original_request_bytes,
                                token,
                                candidate.current.reserved_response_bytes,
                            )?;
                            envelope
                                .as_ref()
                                .ok_or_else(|| invalid("original envelope absent"))?
                                .prepare_single_atomic_operation(candidate, operation)?;
                            Ok(())
                        },
                    )?
                } else {
                    prepare_first_reservation(
                        &before,
                        activation,
                        reference,
                        read.input.clone(),
                        generation,
                        self.provider_key.as_ref(),
                    )?
                };
                original_read = Some(read);
                prepared
            }
            None => return Err(invalid("first Reserve original input readback absent")),
        };
        let mut next = observed.clone();
        next.current.signer.state = after;
        next.current.reserved_response_bytes = next
            .current
            .reserved_response_bytes
            .checked_add(if self.first_reserve_records_original {
                self.recorded_reply_bound(&next.current.signer, &original, request, request_digest)?
            } else {
                self.bounded_retention_reply_bytes(
                    &next.current.signer,
                    &BftRetentionReplyLedgerV1::FirstReserve(original.ledger.clone()),
                    request,
                    request_digest,
                )?
            })
            .ok_or_else(|| invalid("first Reserve transfer reservation overflow"))?;
        check_namespace_budget(
            &original.ledger.receipt.body,
            next.current.reserved_response_bytes,
        )?;
        next.reservation = Some(original.clone());

        let verified = validate_state(
            &self.binding,
            Some(&fresh.attestor.provider_public_key),
            &next.current.signer.state,
        )?;
        check_deadline(deadline)?;
        let committed = if let Some(expected) = envelope {
            let whole = if self.first_reserve_records_original && observed.reservation.is_none() {
                let operation = recorded_operation(
                    &original,
                    original_request_bytes,
                    token,
                    next.current.reserved_response_bytes,
                )?;
                expected.prepare_single_atomic_operation(next.clone(), operation)?
            } else {
                // Cold replies retain the immutable original, including its old lease.
                let mut whole = expected.clone();
                whole.current = next.clone();
                whole
            };
            whole.validate()?;
            runtime
                .backend
                .compare_and_commit_original_operation_envelope(&expected, &whole)
                .and_then(|()| {
                    runtime
                        .backend
                        .read_original_operation_envelope_for_lease(generation, token)
                })
                .map(|readback| readback == whole)
        } else {
            runtime
                .backend
                .compare_and_commit_first_reserve(&observed, &next)
                .and_then(|()| {
                    runtime
                        .backend
                        .read_first_reserve_for_lease(generation, token)
                })
                .map(|readback| readback == next)
        };
        match committed {
            Ok(true) => {}
            _ => {
                runtime.poisoned = true;
                return Err(invalid("first Reserve atomic commit/readback uncertain"));
            }
        }
        let _hold_until_readback = original_read;
        runtime.verified = Some(verified);
        check_deadline(deadline)?;
        self.state_reply(
            next.current.signer,
            request,
            request_digest,
            BftSignerReplyKindV3::PrivateRetention {
                ledger: Box::new(BftRetentionReplyLedgerV1::FirstReserve(original.ledger)),
                latest_seal: None,
                reserved_response_bytes: next.current.reserved_response_bytes,
            },
        )
    }

    fn recorded_reply_bound(
        &self,
        signer: &BftSignerBackendSnapshotV3,
        original: &BftFirstReservedOriginalV1,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
    ) -> Result<u64, BftSignerErrorV3> {
        let upper = SignedBftSignerReplyV3 {
            body: BftSignerReplyBodyV3 {
                schema_version: 3,
                binding_digest: self.binding.digest()?,
                request_digest: request_digest.into(),
                challenge: request.body.challenge.clone(),
                lease_generation: signer.lease_generation,
                lease_token: None,
                state_revision: signer.state.revision,
                state_digest: signer.state.state_digest.clone(),
                checkpoint: Some(Box::new(BftSignerCheckpointV3 {
                    state: signer.state.clone(),
                    lease_generation: signer.lease_generation,
                    challenge: request.body.challenge.clone(),
                    request_digest: request_digest.into(),
                    provider_public_key: self.provider_key.public_key_hex(),
                    provider_signature_hex: "0".repeat(128),
                })),
                result: BftSignerReplyKindV3::PrivateRetention {
                    ledger: Box::new(BftRetentionReplyLedgerV1::FirstReserve(
                        original.ledger.clone(),
                    )),
                    latest_seal: None,
                    reserved_response_bytes: u64::MAX,
                },
            },
            provider_signature_hex: "0".repeat(128),
        };
        Ok(encode_frame(&upper)?.len() as u64)
    }
}

fn recorded_operation(
    original: &BftFirstReservedOriginalV1,
    raw_request: &[u8],
    original_token_digest: &str,
    cumulative_response_bytes: u64,
) -> Result<BftRetainedOriginalOperationV1, BftSignerErrorV3> {
    let receipt = &original.ledger.receipt;
    let body = &receipt.body;
    let raw_receipt = encode_bounded(receipt, ORIGINAL_RECEIPT_RESERVE as usize)?;
    let operation =
        BftRetainedOriginalOperationV1 {
            schema_version: 1,
            operation_id: format!("first-reserve:{}", body.operation_digest),
            source: body.head.source.clone(),
            ledger_origin_digest: original.input.key.ledger_origin_digest.clone(),
            signed_request_bytes: raw_request.to_vec(),
            original_request_digest:
                BftRetainedOriginalOperationV1::request_digest_from_original_bytes(raw_request),
            before_revision: body.before_revision,
            before_state_digest: body.before_state_digest.clone(),
            after_revision: body.after_revision,
            after_state_digest: body.after_state_digest.clone(),
            original_lease_generation: body.original_lease_generation,
            original_lease_token_digest: original_token_digest.into(),
            input_refs: vec![original.input.descriptor()?],
            original_receipt_digest:
                BftRetainedOriginalOperationV1::receipt_digest_from_original_bytes(&raw_receipt),
            signed_receipt_bytes: raw_receipt,
            cumulative_reserved_response_bytes: cumulative_response_bytes,
        };
    // The ONE existing 16 KiB reserve covers the receipt already in the ledger,
    // the complete serialized index record (including duplicate receipt bytes),
    // its key and 256 bytes for container framing. No extra unpriced allocation.
    let mut widest = operation.clone();
    widest.cumulative_reserved_response_bytes = u64::MAX;
    encode_bounded(
        &(receipt, &widest.operation_id, &widest),
        (ORIGINAL_RECEIPT_RESERVE - 256) as usize,
    )?;
    operation.validate()?;
    Ok(operation)
}

fn verify_recorded_original(
    envelope: &BftOriginalRetentionTxnSnapshotV1,
    original: &BftFirstReservedOriginalV1,
    retry: &SignedBftSignerRequestV3,
    binding: &BftSignerBindingV3,
) -> Result<(), BftSignerErrorV3> {
    let id = format!(
        "first-reserve:{}",
        original.ledger.receipt.body.operation_digest
    );
    let stored = envelope
        .original_operations
        .get(&id)
        .ok_or_else(|| invalid("first Reserve original operation record absent"))?;
    let request: SignedBftSignerRequestV3 =
        crate::protected_bft_v3::crypto::decode_bounded(&stored.signed_request_bytes, 2048)?;
    validate_request(binding, &request)?;
    let lease = request
        .body
        .lease
        .as_ref()
        .ok_or_else(|| invalid("first Reserve original request lease absent"))?;
    if request.body.command != retry.body.command
        || lease.generation != original.ledger.receipt.body.original_lease_generation
        || lease_digest(&lease.token)? != stored.original_lease_token_digest
        || recorded_operation(
            original,
            &stored.signed_request_bytes,
            &stored.original_lease_token_digest,
            stored.cumulative_reserved_response_bytes,
        )? != *stored
    {
        return Err(invalid(
            "first Reserve original authenticated record differs",
        ));
    }
    Ok(())
}

fn check_deadline(deadline: Instant) -> Result<(), BftSignerErrorV3> {
    if Instant::now() >= deadline {
        return Err(invalid("first Reserve operation deadline elapsed"));
    }
    Ok(())
}
