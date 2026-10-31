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
