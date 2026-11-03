//! Read RF paid producers. These linear signing handles convey physical
//! dispatch/read claims only, never Directory authority, Source, LF or Current.
use super::*;
use crate::native_cleanup_v2::{
    AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
};
use crate::read_fence_dispatch_v3::*;
/// Deployment-owned route selection. Caller input cannot select a URL or verb.
pub struct ReadFenceRouteV3 {
    pub issuer: AccountMachineIssuerV2,
    pub dispatcher_object_id: String,
    pub fence_object_id: String,
    pub dispatcher_attempt_id: String,
    pub fence_attempt_id: String,
}
pub struct ReadFenceDispatchSigningAuthorityV3 {
    binding: ReadFenceBindingV3,
}
impl ReadFenceDispatchSigningAuthorityV3 {
    pub fn binding(&self) -> &ReadFenceBindingV3 {
        &self.binding
    }
    pub fn signing_bytes(&self) -> core::result::Result<Vec<u8>, String> {
        SignedReadFenceDispatchV3::signing_bytes(&self.binding)
    }
    pub fn finish(
        self,
        signature_hex: String,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> core::result::Result<SignedReadFenceDispatchV3, String> {
        let proof = SignedReadFenceDispatchV3 {
            binding: self.binding,
            signature_hex,
        };
        proof.verify(pin, floor, &proof.binding.dispatcher_object_id, now)?;
        Ok(proof)
    }
}
pub struct ReadFenceReadSigningAuthorityV3 {
    claim: ReadFenceReadV3,
}
impl ReadFenceReadSigningAuthorityV3 {
    pub fn claim(&self) -> &ReadFenceReadV3 {
        &self.claim
    }
    pub fn signing_bytes(&self) -> core::result::Result<Vec<u8>, String> {
        self.claim.signing_bytes()
    }
    pub fn finish(
        self,
        signature_hex: String,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> core::result::Result<SignedReadFenceReadV3, String> {
        let proof = SignedReadFenceReadV3 {
            claim: self.claim,
            signature_hex,
        };
        proof.verify(pin, floor, &proof.claim.binding.fence_object_id, now)?;
        Ok(proof)
    }
}
pub(super) fn accepted_until(
    s: &TestSessionAuthorityV2,
    r: &ReadCurrentPreparationRecordV3,
    p: &ReadCurrentReservationV3,
) -> Result<u64> {
    let root = s
        .roots
        .get(&SessionPurposeV1::ReconcileRead)
        .ok_or_else(invalid)?;
    let challenge = s
        .challenges
        .get(&r.challenge_request_id)
        .ok_or_else(invalid)?;
    if root.credential.as_ref().ok_or_else(invalid)?.generation != r.credential_generation
        || challenge.purpose != SessionPurposeV1::ReconcileRead
        || challenge.logical_request_id != r.request.request_id
        || challenge.body_digest != r.request.digest()?
        || challenge.consumed_by.as_ref() != Some(&r.request.request_id)
    {
        return Err(invalid());
    }
    Ok(r.settled_at
        .get()
        .checked_add(5000)
        .ok_or_else(invalid)?
        .min(r.request_until.get())
        .min(challenge.expires_at.get())
        .min(root.deadline.get())
        .min(p.until.get()))
}
pub(super) fn reservation_ordinal(s: &TestSessionAuthorityV2, ingress: &str) -> Result<u64> {
    let mut matching = s.events.iter().enumerate().filter(|(_, e)| matches!(&e.event, SessionEventKindV2::ReadCurrentReserved { ingress_attempt_id } if ingress_attempt_id == ingress));
    let (index, _) = matching.next().ok_or_else(invalid)?;
    if matching.next().is_some() {
        return Err(invalid());
    }
    u64::try_from(index)
        .ok()
        .and_then(|n| n.checked_add(1))
        .ok_or_else(invalid)
}
impl AccountProvisionApprovalState {
    /// Invoke only after the Worker's actual Directory response passed original
    /// private gate checks. The three earlier slots are payment/order facts,
    /// not a Directory result; this API does not make that result portable.
    pub fn claim_test_session_v2_read_fence_dispatch(
        &mut self,
        token: ReadClaimSourceDirectoryClaimedV3,
        route: ReadFenceRouteV3,
        now: UnixMillis,
    ) -> Result<ReadFenceDispatchSigningAuthorityV3> {
        let token = token.inner;
        let s = self.test_session_v2_local(&token.session, SessionPurposeV1::ReconcileRead, now)?;
        let r = record(s, &token.ingress)?;
        let p = r.reservation.as_ref().ok_or_else(invalid)?;
        if p.tickets != 1
            || token.input != s.input_digest()?
            || token.source != p.source_id
            || token.graph != p.graph_digest
        {
            return Err(invalid());
        }
        for port in [
            ReadCurrentPortV3::ClaimSourceCallbackOwner,
            ReadCurrentPortV3::ClaimSourceCallbackAccount,
            ReadCurrentPortV3::ClaimSourceDirectory,
        ] {
            if !p.claims.contains_key(&port) {
                return Err(invalid());
            }
        }
        let until = accepted_until(s, r, p)?;
        if now.get() >= until {
            return Err(invalid());
        }
        let binding = ReadFenceBindingV3 {
            api_version: 3,
            ingress_attempt_id: token.ingress.clone(),
            original_digest: r.original_digest.clone().ok_or_else(invalid)?,
            source_nonce: p.nonce.clone(),
            root_read_deadline: DecimalU64(
                s.roots
                    .get(&SessionPurposeV1::ReconcileRead)
                    .ok_or_else(invalid)?
                    .deadline
                    .get(),
            ),
            source_until: DecimalU64(until),
            request_until: DecimalU64(r.request_until.get()),
            issuer: route.issuer,
            scope: s.session.scope.role_scope.clone(),
            session_id: token.session.clone(),
            approved_input_digest: s.input_digest()?,
            read_graph_digest: p.graph_digest.clone(),
            source_operation_id: p.source_id.clone(),
            reservation_ordinal: DecimalU64(reservation_ordinal(s, &token.ingress)?),
            dispatcher_claim_ordinal: DecimalU64(next_session_ordinal_v2(s)?),
            phase: ReadFencePhaseV3::ClaimSource,
            dispatcher_attempt_id: route.dispatcher_attempt_id,
            fence_attempt_id: route.fence_attempt_id,
            dispatcher_object_id: route.dispatcher_object_id,
            fence_object_id: route.fence_object_id,
            expected_service_epoch: s.input.request.context.service_epoch,
            business_body_digest: crate::approval::bytes_digest(
                &serde_json::to_vec(&ReadFenceQueryV3 {
                    api_version: 3,
                    expected_service_epoch: s.input.request.context.service_epoch,
                })
                .map_err(|_| invalid())?,
            ),
            issued_at: DecimalU64(now.get()),
            expires_at: DecimalU64(until),
        };
        binding.validate_shape().map_err(|_| invalid())?;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(&token.session)?;
        let port = ReadCurrentPortV3::ClaimSourceRfDispatcher;
        let ordinal = next_session_ordinal_v2(s)?;
        let p = record_mut(s, &token.ingress)?
            .reservation
            .as_mut()
            .ok_or_else(invalid)?;
        if p.claims.contains_key(&port) {
            return Err(invalid());
        }
        p.claims
            .insert(port, ReadCurrentPortClaimV3 { at: now, ordinal });
        append_session_event_v2(
            s,
            SessionEventKindV2::ReadCurrentPortPrepaidClaimed {
                ingress_attempt_id: token.ingress.clone(),
                port,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(ReadFenceDispatchSigningAuthorityV3 { binding })
    }
    /// The actual dedicated Dispatcher already made the fixed Owner/Account
    /// callback. New callback claims are atomic; duplicate/cold cannot reissue
    /// a signing handle even if its signature/response was lost.
    pub fn claim_test_session_v2_read_fence_read(
        &mut self,
        dispatch: &SignedReadFenceDispatchV3,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        actual_dispatcher: &str,
        now: UnixMillis,
    ) -> Result<ReadFenceReadSigningAuthorityV3> {
        dispatch
            .verify(pin, floor, actual_dispatcher, now.get())
            .map_err(|_| invalid())?;
        let b = &dispatch.binding;
        let s = self.test_session_v2_local(&b.session_id, SessionPurposeV1::ReconcileRead, now)?;
        let r = record(s, &b.ingress_attempt_id)?;
        let p = r.reservation.as_ref().ok_or_else(invalid)?;
        if p.tickets != 1 {
            return Err(invalid());
        }
        let prior = p
            .claims
            .get(&ReadCurrentPortV3::ClaimSourceRfDispatcher)
            .ok_or_else(invalid)?;
        if b.original_digest != *r.original_digest.as_ref().ok_or_else(invalid)?
            || b.approved_input_digest != s.input_digest()?
            || b.scope != s.session.scope.role_scope
            || b.read_graph_digest != p.graph_digest
            || b.source_operation_id != p.source_id
            || b.source_nonce != p.nonce
            || b.reservation_ordinal.0 != reservation_ordinal(s, &b.ingress_attempt_id)?
            || b.dispatcher_claim_ordinal.0 != prior.ordinal
            || b.issued_at.0 != prior.at.get()
            || b.expires_at.0 != accepted_until(s, r, p)?
            || b.source_until != b.expires_at
            || b.request_until.0 != r.request_until.get()
            || b.root_read_deadline.0
                != s.roots
                    .get(&SessionPurposeV1::ReconcileRead)
                    .ok_or_else(invalid)?
                    .deadline
                    .get()
            || b.expected_service_epoch != s.input.request.context.service_epoch
        {
            return Err(invalid());
        }
        // A verified signed dispatch permits only this causal receipt. Do not
        // reconstruct the original first-only ReadCurrentSourceV3 opaque.
        let mut next = self.clone();
        for port in [
            ReadCurrentPortV3::ClaimSourceRfCausalOwner,
            ReadCurrentPortV3::ClaimSourceRfCausalAccount,
            ReadCurrentPortV3::ClaimSourceRfFences,
        ] {
            let s = next.test_session_v2_record_mut(&b.session_id)?;
            let ordinal = next_session_ordinal_v2(s)?;
            let p = record_mut(s, &b.ingress_attempt_id)?
                .reservation
                .as_mut()
                .ok_or_else(invalid)?;
            if p.claims.contains_key(&port) {
                return Err(invalid());
            }
            p.claims
                .insert(port, ReadCurrentPortClaimV3 { at: now, ordinal });
            append_session_event_v2(
                s,
                SessionEventKindV2::ReadCurrentPortPrepaidClaimed {
                    ingress_attempt_id: b.ingress_attempt_id.clone(),
                    port,
                },
                now,
            )?;
        }
        let s = next.test_session_v2_record_mut(&b.session_id)?;
        let ordinal = record(s, &b.ingress_attempt_id)?
            .reservation
            .as_ref()
            .ok_or_else(invalid)?
            .claims
            .get(&ReadCurrentPortV3::ClaimSourceRfFences)
            .ok_or_else(invalid)?
            .ordinal;
        let claim = ReadFenceReadV3 {
            binding: b.clone(),
            dispatch_digest: dispatch.digest().map_err(|_| invalid())?,
            claim_ordinal: DecimalU64(ordinal),
            claimed_at: DecimalU64(now.get()),
        };
        claim.validate_shape().map_err(|_| invalid())?;
        next.validate_persistence()?;
        *self = next;
        Ok(ReadFenceReadSigningAuthorityV3 { claim })
    }
}
