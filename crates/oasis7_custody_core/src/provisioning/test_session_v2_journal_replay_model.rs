//! Closed model settlement/pending replay. No provider dispatch is reconstructed.
use super::*;
impl Replay<'_> {
    pub(super) fn apply_model(&mut self, event: &SessionEventKindV2, at: UnixMillis) -> Result<()> {
        let model = self.s.native_model_budget.as_ref().ok_or_else(invalid)?;
        self.live(SessionPurposeV1::Execution, at)?;
        let units = match event {
            SessionEventKindV2::NativeModelHttpSettled { ingress_attempt_id } => {
                let record = model
                    .http_admissions
                    .get(ingress_attempt_id)
                    .ok_or_else(invalid)?;
                if record.charged_at != at
                    || self.generations.get(&SessionPurposeV1::Execution)
                        != Some(&record.credential_generation.0)
                {
                    return Err(invalid());
                }
                super::super::super::native_model_entry_fee_v2::validate_entry_fees(self.s, model)?;
                Self::unique(&mut self.model_fees, ingress_attempt_id)?;
                super::super::super::native_model_entry_fee_v2::entry_units()?
            }
            SessionEventKindV2::NativeModelGraphReserved { reservation_id } => {
                let record = model.reservations.get(reservation_id).ok_or_else(invalid)?;
                if record.claimed_at != at
                    || !self.model_fees.contains(&record.host_ingress_attempt_id)
                    || self.generations.get(&SessionPurposeV1::Execution)
                        != Some(&record.credential_generation.0)
                {
                    return Err(invalid());
                }
                Self::unique(&mut self.model_reservations, reservation_id)?;
                record.physical_units
            }
            _ => return Err(invalid()),
        };
        self.root_transport
            .get_mut(&SessionPurposeV1::Execution)
            .ok_or_else(invalid)?
            .charge(units)
            .map_err(|_| invalid())?;
        Ok(())
    }
}
