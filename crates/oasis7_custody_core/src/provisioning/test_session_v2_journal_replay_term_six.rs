//! Term metadata chronology only; no historical Current or transport token.
use super::*;
impl Replay<'_> {
    pub(super) fn apply_term_six(
        &mut self,
        event: &SessionEventKindV2,
        at: UnixMillis,
        ordinal: u64,
    ) -> Result<()> {
        if !self.active || !self.machine_ready || self.closing.is_none() {
            return Err(invalid());
        }
        if let SessionEventKindV2::TermSixNativePaidRecorded { round_id } = event {
            let r = self
                .s
                .termination_state
                .term_six_funding
                .iter()
                .find(|r| r.round_id() == round_id)
                .ok_or_else(invalid)?;
            let native = digest(&(
                "oasis7.custody.term-six-journal-claim.v2",
                round_id,
                TermObservationPortV2::Native,
            ))?;
            if !self.term_six_claims.contains(&native) {
                return Err(invalid());
            }
            r.replay_native_paid(at, ordinal)?;
            Self::unique(&mut self.term_six_native_paid, round_id)?;
            return Ok(());
        }
        let (id, port) = match event {
            SessionEventKindV2::TermSixReserved { round_id } => (round_id, None),
            SessionEventKindV2::TermSixPortClaimed { round_id, port } => (round_id, Some(*port)),
            _ => return Err(invalid()),
        };
        let r = self
            .s
            .termination_state
            .term_six_funding
            .iter()
            .find(|r| r.round_id() == id)
            .ok_or_else(invalid)?;
        if let Some(port) = port {
            if !self.term_six_reserved.contains(id) {
                return Err(invalid());
            }
            r.replay_claim(self.s, port, at, ordinal)?;
            let key = digest(&("oasis7.custody.term-six-journal-claim.v2", id, port))?;
            Self::unique(&mut self.term_six_claims, &key)?;
        } else {
            r.replay_reserved(self.s, at, ordinal)?;
            Self::unique(&mut self.term_six_reserved, id)?;
        }
        Ok(())
    }
}
