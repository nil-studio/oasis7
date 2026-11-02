//! New-profile chronology only; historical rows cannot recreate caller tokens.
use super::*;
impl Replay<'_> {
    pub(super) fn apply_term_five(
        &mut self,
        event: &SessionEventKindV2,
        at: UnixMillis,
        ordinal: u64,
    ) -> Result<()> {
        if !self.active || !self.machine_ready || self.closing.is_none() {
            return Err(invalid());
        }
        let (id, port) = match event {
            SessionEventKindV2::TermFiveReserved { round_id } => (round_id, None),
            SessionEventKindV2::TermFivePortClaimed { round_id, port } => (round_id, Some(*port)),
            _ => return Err(invalid()),
        };
        let r = self
            .s
            .termination_state
            .term_five_funding
            .as_ref()
            .ok_or_else(invalid)?
            .iter()
            .find(|r| r.round_id() == id)
            .ok_or_else(invalid)?;
        if let Some(port) = port {
            if !self.term_five_reserved.contains(id) {
                return Err(invalid());
            }
            r.replay_claim(self.s, port, at, ordinal)?;
            let key = digest(&("oasis7.custody.term-five-journal-claim.v3", id, port))?;
            Self::unique(&mut self.term_five_claims, &key)?;
        } else {
            r.replay_reserved(self.s, at, ordinal)?;
            Self::unique(&mut self.term_five_reserved, id)?;
        }
        Ok(())
    }
}
