//! First-only Read callback producer; no historical reconstruction of its
//! original Source opaque and no Source/Current grant from a JSON continuation.
use super::rf_sender::{accepted_until, reservation_ordinal};
use super::*;
use crate::read_directory_v3::*;
use crate::{
    native_cleanup_v2::{
        AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
    },
    read_claim_source_v3::*,
};
pub struct ReadClaimSourceRouteV3 {
    pub issuer: AccountMachineIssuerV2,
    pub account_object_id: String,
}
pub struct ReadClaimSourceSigningAuthorityV3 {
    claim: ReadClaimSourceV3,
}
impl ReadClaimSourceSigningAuthorityV3 {
    pub fn claim(&self) -> &ReadClaimSourceV3 {
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
    ) -> core::result::Result<SignedReadClaimSourceV3, String> {
        let proof = SignedReadClaimSourceV3 {
            claim: self.claim,
            signature_hex,
        };
        proof.verify(pin, floor, &proof.claim.account_object_id, now)?;
        Ok(proof)
    }
}
/// Only the fresh actual Account callback SQL issues this Directory-send handle.
/// It is distinct from the first HTTP/Source opaque, and never deserializable.
pub struct ReadClaimSourceCallbackV3 {
    pub(super) session: String,
    pub(super) ingress: String,
    pub(super) input: String,
    pub(super) source: String,
    pub(super) graph: String,
    coordinator: SignedReadClaimSourceV3,
    query: ReadDirectoryQueryV3,
    directory_ordinal: u64,
    claimed_at: UnixMillis,
}
pub struct ReadClaimSourceDirectoryClaimedV3 {
    pub(super) inner: ReadClaimSourceCallbackV3,
}
pub struct ReadDirectorySigningAuthorityV3 {
    claim: ReadDirectoryClaimV3,
}
impl ReadDirectorySigningAuthorityV3 {
    pub fn claim(&self) -> &ReadDirectoryClaimV3 {
        &self.claim
    }
    pub fn signing_bytes(&self) -> core::result::Result<Vec<u8>, String> {
        self.claim.signing_bytes()
    }
    pub fn finish(
        self,
        signature_hex: String,
        verifier: &ReadDirectoryVerifierV3,
        now: u64,
    ) -> core::result::Result<SignedReadDirectoryV3, String> {
        let proof = SignedReadDirectoryV3 {
            claim: self.claim,
            signature_hex,
        };
        proof.verify(
            verifier,
            &proof.claim.directory_object_id,
            &proof.claim.coordinator.claim.account_object_id,
            now,
        )?;
        proof.wire_bytes()?;
        Ok(proof)
    }
}
impl ReadClaimSourceCallbackV3 {
    pub fn directory_query(&self) -> &ReadDirectoryQueryV3 {
        &self.query
    }
    pub fn issue_directory(
        self,
        directory_object_id: String,
    ) -> Result<(
        ReadClaimSourceDirectoryClaimedV3,
        ReadDirectorySigningAuthorityV3,
    )> {
        let claim = ReadDirectoryClaimV3 {
            api_version: 3,
            physical_attempt_id: read_directory_attempt_id_v3(&self.coordinator)
                .map_err(|_| invalid())?,
            directory_object_id,
            query: self.query.clone(),
            coordinator: self.coordinator.clone(),
            claim_ordinal: DecimalU64(self.directory_ordinal),
            claimed_at: DecimalU64(self.claimed_at.get()),
            expires_at: self.coordinator.claim.expires_at,
        };
        claim.validate_shape().map_err(|_| invalid())?;
        Ok((
            ReadClaimSourceDirectoryClaimedV3 { inner: self },
            ReadDirectorySigningAuthorityV3 { claim },
        ))
    }
}
impl AccountProvisionApprovalState {
    pub fn claim_test_session_v2_read_claim_source_callback(
        &mut self,
        mut source: ReadCurrentSourceV3,
        route: ReadClaimSourceRouteV3,
        now: UnixMillis,
    ) -> Result<ReadClaimSourceSigningAuthorityV3> {
        let s =
            self.test_session_v2_local(&source.session, SessionPurposeV1::ReconcileRead, now)?;
        let r = record(s, &source.ingress)?;
        let p = r.reservation.as_ref().ok_or_else(invalid)?;
        if p.tickets != 0
            || source.tickets != 0
            || p.source_id != source.source
            || p.graph_digest != source.graph
        {
            return Err(invalid());
        }
        let until = accepted_until(s, r, p)?;
        if now.get() >= until {
            return Err(invalid());
        }
        let ordinal = next_session_ordinal_v2(s)?;
        let attempt = |port| {
            digest(&(
                "oasis7.custody.read-claim-source-physical.v3",
                &source.session,
                &source.ingress,
                &source.graph,
                port,
            ))
        };
        let claim = ReadClaimSourceV3 {
            api_version: 3,
            issuer: route.issuer,
            scope: s.session.scope.role_scope.clone(),
            session_id: source.session.clone(),
            ingress_attempt_id: source.ingress.clone(),
            approved_input_digest: s.input_digest()?,
            original_digest: r.original_digest.clone().ok_or_else(invalid)?,
            read_request: r.request.clone(),
            signed_attempt_digest: s
                .purpose_http_admissions
                .get(&source.ingress)
                .ok_or_else(invalid)?
                .signed_attempt_digest
                .clone(),
            credential_generation: r.credential_generation,
            read_graph_digest: p.graph_digest.clone(),
            source_operation_id: p.source_id.clone(),
            source_nonce: p.nonce.clone(),
            account_object_id: route.account_object_id,
            owner_attempt_id: attempt("owner_service")?,
            account_attempt_id: attempt("account_actor")?,
            reservation_ordinal: DecimalU64(reservation_ordinal(s, &source.ingress)?),
            owner_claim_ordinal: DecimalU64(ordinal),
            account_claim_ordinal: DecimalU64(ordinal.checked_add(1).ok_or_else(invalid)?),
            settled_at: DecimalU64(r.settled_at.get()),
            root_read_deadline: DecimalU64(
                s.roots
                    .get(&SessionPurposeV1::ReconcileRead)
                    .ok_or_else(invalid)?
                    .deadline
                    .get(),
            ),
            request_until: DecimalU64(r.request_until.get()),
            issued_at: DecimalU64(now.get()),
            expires_at: DecimalU64(until),
        };
        claim.validate_shape().map_err(|_| invalid())?;
        let mut next = self.clone();
        for port in [
            ReadCurrentPortV3::ClaimSourceCallbackOwner,
            ReadCurrentPortV3::ClaimSourceCallbackAccount,
        ] {
            next.claim_test_session_v2_read_current_prepaid_port(&mut source, port, now)?;
        }
        *self = next;
        Ok(ReadClaimSourceSigningAuthorityV3 { claim })
    }
    pub fn accept_test_session_v2_read_claim_source_callback(
        &mut self,
        proof: &SignedReadClaimSourceV3,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        actual_account: &str,
        now: UnixMillis,
    ) -> Result<ReadClaimSourceCallbackV3> {
        proof
            .verify(pin, floor, actual_account, now.get())
            .map_err(|_| invalid())?;
        let b = &proof.claim;
        let s = self.test_session_v2_local(&b.session_id, SessionPurposeV1::ReconcileRead, now)?;
        let r = record(s, &b.ingress_attempt_id)?;
        let p = r.reservation.as_ref().ok_or_else(invalid)?;
        let owner = p
            .claims
            .get(&ReadCurrentPortV3::ClaimSourceCallbackOwner)
            .ok_or_else(invalid)?;
        let account = p
            .claims
            .get(&ReadCurrentPortV3::ClaimSourceCallbackAccount)
            .ok_or_else(invalid)?;
        let attempt = |port| {
            digest(&(
                "oasis7.custody.read-claim-source-physical.v3",
                &b.session_id,
                &b.ingress_attempt_id,
                &b.read_graph_digest,
                port,
            ))
        };
        if p.tickets != 0
            || p.claims
                .contains_key(&ReadCurrentPortV3::ClaimSourceDirectory)
            || b.approved_input_digest != s.input_digest()?
            || b.scope != s.session.scope.role_scope
            || b.original_digest != *r.original_digest.as_ref().ok_or_else(invalid)?
            || b.read_request != r.request
            || b.credential_generation != r.credential_generation
            || b.signed_attempt_digest
                != s.purpose_http_admissions
                    .get(&b.ingress_attempt_id)
                    .ok_or_else(invalid)?
                    .signed_attempt_digest
            || b.read_graph_digest != p.graph_digest
            || b.source_operation_id != p.source_id
            || b.source_nonce != p.nonce
            || b.owner_attempt_id != attempt("owner_service")?
            || b.account_attempt_id != attempt("account_actor")?
            || b.owner_claim_ordinal.0 != owner.ordinal
            || b.account_claim_ordinal.0 != account.ordinal
            || b.issued_at.0 != owner.at.get()
            || b.issued_at.0 != account.at.get()
            || b.reservation_ordinal.0 != reservation_ordinal(s, &b.ingress_attempt_id)?
            || b.settled_at.0 != r.settled_at.get()
            || b.expires_at.0 != accepted_until(s, r, p)?
            || b.request_until.0 != r.request_until.get()
            || b.root_read_deadline.0
                != s.roots
                    .get(&SessionPurposeV1::ReconcileRead)
                    .ok_or_else(invalid)?
                    .deadline
                    .get()
        {
            return Err(invalid());
        }
        let original = ReadDirectoryOriginalV3::from_record(
            s.book.original(&b.session_id).ok_or_else(invalid)?,
        )
        .map_err(|_| invalid())?;
        let query = ReadDirectoryQueryV3 {
            api_version: 3,
            phase: ReadDirectoryPhaseV3::ClaimSource,
            manager_reference: ReadDirectoryManagerReferenceV3::from_original(&original),
            provision_id: b.scope.provision_id.clone(),
            custody_account_id: b.scope.context.identity.account_id.clone(),
            original,
        };
        query.validate_against(proof).map_err(|_| invalid())?;
        let mut next = self.clone();
        record_mut(
            next.test_session_v2_record_mut(&b.session_id)?,
            &b.ingress_attempt_id,
        )?
        .reservation
        .as_mut()
        .ok_or_else(invalid)?
        .tickets = 1;
        if next.reserve_test_session_v2_source_read(&b.session_id, &b.source_operation_id, now)?
            != 1
        {
            return Err(invalid());
        }
        let s = next.test_session_v2_record_mut(&b.session_id)?;
        let port = ReadCurrentPortV3::ClaimSourceDirectory;
        let ordinal = next_session_ordinal_v2(s)?;
        record_mut(s, &b.ingress_attempt_id)?
            .reservation
            .as_mut()
            .ok_or_else(invalid)?
            .claims
            .insert(port, ReadCurrentPortClaimV3 { at: now, ordinal });
        append_session_event_v2(
            s,
            SessionEventKindV2::ReadCurrentPortPrepaidClaimed {
                ingress_attempt_id: b.ingress_attempt_id.clone(),
                port,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(ReadClaimSourceCallbackV3 {
            session: b.session_id.clone(),
            ingress: b.ingress_attempt_id.clone(),
            input: b.approved_input_digest.clone(),
            source: b.source_operation_id.clone(),
            graph: b.read_graph_digest.clone(),
            coordinator: proof.clone(),
            query,
            directory_ordinal: ordinal,
            claimed_at: now,
        })
    }
}
