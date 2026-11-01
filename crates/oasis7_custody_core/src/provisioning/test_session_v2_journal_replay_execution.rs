//! The pending/LF phase only; no signer or World dispatch follows from replay.
use super::*;
impl Replay<'_> {
    pub(super) fn apply_execution(
        &mut self,
        event: &SessionEventKindV2,
        at: UnixMillis,
    ) -> Result<()> {
        let b = self
            .s
            .execution_book
            .as_ref()
            .ok_or_else(invalid)?
            .journal_record();
        match event {
            SessionEventKindV2::ExecutionPrepareFactoryPaid { request_id } => {
                self.live(SessionPurposeV1::Execution, at)?;
                let book = self.s.execution_book.as_ref().ok_or_else(invalid)?;
                let (original, _, claimed) = book
                    .prepare_paid_journal_record(self.s)
                    .ok_or_else(invalid)?;
                if !self.execution_lifetime_admitted
                    || self.execution_prepare_paid
                    || request_id != original
                    || claimed != at
                    || self
                        .reads
                        .get(b.source_operation_id)
                        .is_none_or(|n| *n < 4 || *n > 8)
                    || self.generations.get(&SessionPurposeV1::Execution)
                        != Some(&b.credential_generation.0)
                {
                    return Err(invalid());
                }
                self.root_transport
                    .get_mut(&SessionPurposeV1::Execution)
                    .ok_or_else(invalid)?
                    .charge(book.prepare_factory_units(self.s)?)
                    .map_err(|_| invalid())?;
                self.execution_prepare_paid = true;
            }
            SessionEventKindV2::ExecutionPrepareCurrentPortsClaimed { request_id } => {
                self.live(SessionPurposeV1::Execution, at)?;
                let book = self.s.execution_book.as_ref().ok_or_else(invalid)?;
                let read = book.prepare_current_read(self.s).ok_or_else(invalid)?;
                if !self.execution_prepare_paid
                    || self.execution_prepare_current_ports
                    || self.execution_prepare_admitted
                    || request_id != b.request_id
                    || read.claimed_at != at
                    || self.reads.get(b.source_operation_id) != Some(&5)
                    || self.generations.get(&SessionPurposeV1::Execution)
                        != Some(&b.credential_generation.0)
                {
                    return Err(invalid());
                }
                self.source_ready(b.source_operation_id, at)?;
                self.root_transport
                    .get_mut(&SessionPurposeV1::Execution)
                    .ok_or_else(invalid)?
                    .charge(3)
                    .map_err(|_| invalid())?;
                self.execution_prepare_current_ports = true;
            }
            SessionEventKindV2::ExecutionPrivateCurrentClaimed { request_id } => {
                self.live(SessionPurposeV1::Execution, at)?;
                let book = self.s.execution_book.as_ref().ok_or_else(invalid)?;
                let r = book.private_current_read(self.s).ok_or_else(invalid)?;
                if !self.execution_prepare_current_ports
                    || self.execution_prepare_admitted
                    || self.execution_private_current_claimed
                    || r.claimed_at != at
                    || request_id != b.request_id
                    || self.reads.get(b.source_operation_id) != Some(&5)
                    || r.expires_at.get() > self.lease.ok_or_else(invalid)?.get()
                    || self.generations.get(&SessionPurposeV1::Execution)
                        != Some(&b.credential_generation.0)
                {
                    return Err(invalid());
                }
                self.source_ready(b.source_operation_id, at)?;
                self.root_transport
                    .get_mut(&SessionPurposeV1::Execution)
                    .ok_or_else(invalid)?
                    .charge(2)
                    .map_err(|_| invalid())?;
                self.execution_private_current_claimed = true;
            }
            SessionEventKindV2::ExecutionPrivateCurrentObserved { request_id } => {
                let book = self.s.execution_book.as_ref().ok_or_else(invalid)?;
                let r = book.private_current_read(self.s).ok_or_else(invalid)?;
                if !self.execution_private_current_claimed
                    || self.execution_private_current_observed
                    || r.completed_at != Some(at)
                    || r.result.is_none()
                    || request_id != b.request_id
                {
                    return Err(invalid());
                }
                self.execution_private_current_observed = true;
            }
            SessionEventKindV2::ExecutionNativePrepareClaimed { request_id } => {
                self.live(SessionPurposeV1::Execution, at)?;
                let book = self.s.execution_book.as_ref().ok_or_else(invalid)?;
                let record = book.native_prepare_record().ok_or_else(invalid)?;
                if !self.execution_prepare_original_archived
                    || self.execution_native_prepare_claimed
                    || request_id != b.request_id
                    || record.claimed_at != at
                    || self.reads.get(b.source_operation_id) != Some(&5)
                    || record.proof.certificate.expires_at.0 > self.lease.ok_or_else(invalid)?.get()
                    || self.generations.get(&SessionPurposeV1::Execution)
                        != Some(&b.credential_generation.0)
                {
                    return Err(invalid());
                }
                self.source_ready(b.source_operation_id, at)?;
                self.root_transport
                    .get_mut(&SessionPurposeV1::Execution)
                    .ok_or_else(invalid)?
                    .charge(10)
                    .map_err(|_| invalid())?;
                self.execution_native_prepare_claimed = true;
            }
            SessionEventKindV2::ExecutionPreparedPublicArchived { request_id } => {
                let book = self.s.execution_book.as_ref().ok_or_else(invalid)?;
                let record = book.native_prepare_record().ok_or_else(invalid)?;
                if !self.execution_native_prepare_claimed
                    || self.execution_prepared_public_archived
                    || request_id != b.request_id
                    || record.public_recorded_at != Some(at)
                    || record.public_original_cbor_hex.is_none()
                {
                    return Err(invalid());
                }
                // Immutable fact arrival can be late. No live/fee/dispatch grant.
                self.execution_prepared_public_archived = true;
            }
            SessionEventKindV2::ExecutionPrepareOriginalArchived { request_id } => {
                let book = self.s.execution_book.as_ref().ok_or_else(invalid)?;
                if !self.execution_prepare_permits_recorded
                    || self.execution_prepare_original_archived
                    || request_id != b.request_id
                    || book.prepare_original_recorded_at() != Some(at)
                {
                    return Err(invalid());
                }
                // A late historical signature is not a live admission.
                self.execution_prepare_original_archived = true;
            }
            SessionEventKindV2::ExecutionPreparePermitsRecorded { request_id } => {
                self.live(SessionPurposeV1::Execution, at)?;
                let book = self.s.execution_book.as_ref().ok_or_else(invalid)?;
                let record = book.prepare_service_record().ok_or_else(invalid)?;
                if !self.execution_prepare_current_ports
                    || !self.execution_prepare_admitted
                    || self.execution_prepare_permits_recorded
                    || record.recorded_at != at
                    || request_id != b.request_id
                    || self.reads.get(b.source_operation_id) != Some(&5)
                    || self.generations.get(&SessionPurposeV1::Execution)
                        != Some(&b.credential_generation.0)
                {
                    return Err(invalid());
                }
                self.source_ready(b.source_operation_id, at)?;
                self.root_transport
                    .get_mut(&SessionPurposeV1::Execution)
                    .ok_or_else(invalid)?
                    .charge(4)
                    .map_err(|_| invalid())?;
                self.execution_prepare_permits_recorded = true;
            }
            SessionEventKindV2::ExecutionPrepareAllocated { request_id } => {
                self.live(SessionPurposeV1::Execution, at)?;
                let (original_request, paid_key, admitted_at) = self
                    .s
                    .execution_book
                    .as_ref()
                    .ok_or_else(invalid)?
                    .prepare_journal_record()
                    .ok_or_else(invalid)?;
                if !self.execution_lifetime_admitted
                    || self.execution_prepare_admitted
                    || !self.execution_prepare_paid
                    || request_id != original_request
                    || admitted_at != at
                    || !matches!(
                        self.s.purpose_transports.get(paid_key),
                        Some(SessionPurposeTransportV1::ExecutionFactoryCallbackV2(_))
                    )
                    || self.reads.get(b.source_operation_id) != Some(&5)
                    || self.generations.get(&SessionPurposeV1::Execution)
                        != Some(&b.credential_generation.0)
                {
                    return Err(invalid());
                }
                self.source_ready(b.source_operation_id, at)?;
                if self
                    .s
                    .execution_book
                    .as_ref()
                    .and_then(|book| book.private_current_read(self.s))
                    .is_some()
                    && !self.execution_private_current_observed
                {
                    return Err(invalid());
                }
                self.execution_prepare_admitted = true;
            }
            SessionEventKindV2::ExecutionBookStarted { request_id } => {
                self.live(SessionPurposeV1::Execution, at)?;
                if self.execution_started
                    || request_id != b.request_id
                    || b.started_at != at
                    || !self.purpose_fees.contains(b.ingress_attempt_id)
                    || self.generations.get(&SessionPurposeV1::Execution)
                        != Some(&b.credential_generation.0)
                {
                    return Err(invalid());
                }
                self.consume(
                    b.challenge_request_id,
                    request_id,
                    SessionPurposeV1::Execution,
                    at,
                )?;
                self.execution_requests.charge(1).map_err(|_| invalid())?;
                self.execution_started = true;
            }
            SessionEventKindV2::ExecutionLifetimeClaimed {
                request_id,
                source_operation_id,
            } => {
                self.live(SessionPurposeV1::Execution, at)?;
                if !self.execution_started
                    || self.execution_lifetime_claimed
                    || request_id != b.request_id
                    || source_operation_id != b.source_operation_id
                    || b.lifetime_claimed_at != at
                    || self.reads.get(source_operation_id) != Some(&3)
                    || !matches!(
                        self.s.source_funding.get(source_operation_id),
                        Some(SessionSourceFundingV1::ExecutionFactoryV2 { .. })
                    )
                {
                    return Err(invalid());
                }
                self.source_ready(source_operation_id, at)?;
                self.root_transport
                    .get_mut(&SessionPurposeV1::Execution)
                    .ok_or_else(invalid)?
                    .charge(1)
                    .map_err(|_| invalid())?;
                self.execution_lifetime_claimed = true;
            }
            SessionEventKindV2::ExecutionLifetimeArchived {
                request_id,
                source_operation_id,
            } => {
                // Archive is historical, with no live/current/time admission.
                if !self.execution_lifetime_claimed
                    || self.execution_lifetime_archived
                    || request_id != b.request_id
                    || source_operation_id != b.source_operation_id
                    || b.lifetime_archived_at != Some(at)
                {
                    return Err(invalid());
                }
                self.execution_lifetime_archived = true;
            }
            SessionEventKindV2::ExecutionLifetimeAdmitted {
                request_id,
                source_operation_id,
            } => {
                self.live(SessionPurposeV1::Execution, at)?;
                if !self.execution_lifetime_archived
                    || self.execution_lifetime_admitted
                    || request_id != b.request_id
                    || source_operation_id != b.source_operation_id
                    || b.lifetime_admitted_at != Some(at)
                    || self.reads.get(source_operation_id) != Some(&4)
                    || self.generations.get(&SessionPurposeV1::Execution)
                        != Some(&b.credential_generation.0)
                {
                    return Err(invalid());
                }
                self.source_ready(source_operation_id, at)?;
                self.execution_lifetime_admitted = true;
            }
            _ => return Err(invalid()),
        }
        Ok(())
    }
}
