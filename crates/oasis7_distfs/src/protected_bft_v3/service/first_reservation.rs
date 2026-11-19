//! First original-byte reservation over the authenticated V3 lease.
use super::*;
use crate::protected_bft_v3::first_reservation::{
    original_activation_state, prepare_first_reservation, verify_first_reservation,
};
use crate::protected_bft_v3::fresh_coverage::check_namespace_budget;
use crate::protected_bft_v3::{
    BftAuthorizedOriginalReserveInputV1, BftFirstReservationPriceV1,
    BftPrivateRetentionAdmissionInputRefV1, BftPrivateRetentionAdmissionInputV1,
    BftPrivateRetentionCreationMarkerV1,
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
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
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
        let observed = runtime
            .backend
            .read_first_reserve_for_lease(generation, token)?;
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
                let prepared = prepare_first_reservation(
                    &before,
                    activation,
                    reference,
                    read.input.clone(),
                    generation,
                    self.provider_key.as_ref(),
                )?;
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
            .checked_add(self.bounded_retention_reply_bytes(
                &next.current.signer,
                &BftRetentionReplyLedgerV1::FirstReserve(original.ledger.clone()),
                request,
                request_digest,
            )?)
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
        let committed = runtime
            .backend
            .compare_and_commit_first_reserve(&observed, &next)
            .and_then(|()| {
                runtime
                    .backend
                    .read_first_reserve_for_lease(generation, token)
            });
        match committed {
            Ok(readback) if readback == next => {}
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
}

fn check_deadline(deadline: Instant) -> Result<(), BftSignerErrorV3> {
    if Instant::now() >= deadline {
        return Err(invalid("first Reserve operation deadline elapsed"));
    }
    Ok(())
}
