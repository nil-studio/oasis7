//! Native activation uses the same authoritative session event sequence.
use super::*;
impl Replay<'_> {
    pub(super) fn apply_machine(
        &mut self,
        event: &SessionEventKindV2,
        at: UnixMillis,
    ) -> Result<()> {
        match event {
            SessionEventKindV2::NativeMachineActivationClaimed => {
                let m = self
                    .s
                    .native_machine_registration
                    .as_ref()
                    .ok_or_else(invalid)?;
                let a = m.active.as_ref().ok_or_else(invalid)?;
                if !self.active
                    || self.machine_claimed
                    || self.machine_ready
                    || self.closing.is_some()
                    || a.certificate.issued_at.0 != at.get()
                    || at.get() >= a.dispatch_until.get()
                {
                    return Err(invalid());
                }
                self.identity.charge(1).map_err(|_| invalid())?;
                self.machine_claimed = true;
            }
            SessionEventKindV2::NativeMachineActivationAdmitted => {
                let a = self
                    .s
                    .native_machine_registration
                    .as_ref()
                    .and_then(|m| m.active.as_ref())
                    .ok_or_else(invalid)?;
                let ready = a.ready.as_ref().ok_or_else(invalid)?;
                if !self.active
                    || !self.machine_claimed
                    || self.machine_ready
                    || self.closing.is_some()
                    || ready.admitted_at != at
                    || at.get() >= a.dispatch_until.get()
                {
                    return Err(invalid());
                }
                self.machine_ready = true;
            }
            _ => return self.apply_world_preflight(event, at),
        }
        Ok(())
    }
}
