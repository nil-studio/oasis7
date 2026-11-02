//! Exact nonfungible two-read allocation; no balance can be borrowed elsewhere.
use super::*;
impl Replay<'_> {
    pub(super) fn apply_world_preflight(
        &mut self,
        event: &SessionEventKindV2,
        at: UnixMillis,
    ) -> Result<()> {
        match event {
            SessionEventKindV2::PreflightWorldAccessReserved => {
                let w = self.s.preflight_world_access.as_ref().ok_or_else(invalid)?;
                if !self.approval_consumed
                    || self.active
                    || self.world_preflight_reserved
                    || self.access
                    || self.closing.is_some()
                    || self.preflight_reads != 2
                    || !self
                        .steps
                        .contains(&SessionPreflightStepV1::FreshAccessPair)
                    || w.certificate.issued_at.0 != at.get()
                    || self
                        .reads
                        .get(&w.certificate.descriptor.source_operation_id)
                        != Some(&3)
                {
                    return Err(invalid());
                }
                self.source_ready(&w.certificate.descriptor.source_operation_id, at)?;
                // Exactly the two typed ports recorded in the approved World3
                // profile were reserved, not Native160 or an extensible cap.
                self.world_preflight_reserved = true;
            }
            SessionEventKindV2::PreflightWorldAccessAdmitted => {
                let w = self.s.preflight_world_access.as_ref().ok_or_else(invalid)?;
                if !self.world_preflight_reserved
                    || self.world_preflight_admitted
                    || self.active
                    || self.access
                    || self.closing.is_some()
                    || self.preflight_reads != 3
                    || w.admitted_at != Some(at)
                    || !w.admitted()
                    || self
                        .reads
                        .get(&w.certificate.descriptor.source_operation_id)
                        != Some(&4)
                {
                    return Err(invalid());
                }
                self.source_ready(&w.certificate.descriptor.source_operation_id, at)?;
                self.world_preflight_admitted = true;
            }
            _ => return self.apply_purpose(event, at),
        }
        Ok(())
    }
}
