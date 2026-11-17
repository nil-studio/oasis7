//! Separate consumer-installation journal. A receipt is historical evidence,
//! never authority to replay an installation or to claim a live World on restart.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct Installation {
    pub publication_digest: String,
    pub attempt_no: u64,
    pub completed: bool,
}

pub(super) fn validate(state: &Snapshot) -> Result<(), String> {
    if state.installation_journal_version.is_some_and(|v| v != 1) {
        return Err("unsupported installation journal version".into());
    }
    if let Some(installation) = &state.installation {
        let publication = state
            .publication
            .as_ref()
            .ok_or("installation lacks publication")?;
        if state.installation_journal_version != Some(1)
            || !publication.completed
            || publication
                .candidate
                .as_ref()
                .and_then(|c| c.manifest.as_ref())
                .is_none()
            || installation.publication_digest != publication.intent_digest
            || installation.attempt_no != publication.attempt_no
            || (installation.completed && state.active_attempt_nonce.is_some())
        {
            return Err("installation journal differs from published candidate".into());
        }
    }
    Ok(())
}

impl Ledger {
    /// Persist before passing this candidate to its real retaining consumer.
    /// Existing pending/completed intent is never permission for another call.
    pub fn begin_installation(
        &mut self,
        permit: &AttemptPermit,
        cut: &serde_json::Value,
        roots: &std::collections::BTreeSet<String>,
        manifest: &FullSyncManifestReference,
    ) -> Result<(), String> {
        self.check_attempt(permit)?;
        let digest = local_io::digest(&(cut, local_io::digest(roots)?, manifest))?;
        let publication = self
            .state
            .publication
            .as_ref()
            .ok_or("installation lacks publication")?;
        if self.state.installation_journal_version != Some(1)
            || self.state.installation.is_some()
            || !publication.completed
            || publication.attempt_no != permit.number
            || publication.intent_digest != digest
            || self.state.active_request.is_some()
            || self.state.reservation.is_some()
        {
            return Err("operation_recovery_required: installation unavailable".into());
        }
        let mut next = self.state.clone();
        next.installation = Some(Installation {
            publication_digest: digest,
            attempt_no: permit.number,
            completed: false,
        });
        self.save(next)
    }

    /// Caller must hold the exact verified World, owner and stop fence. Record
    /// that acknowledgement and consume the sole permit in one durable write.
    pub fn complete_installation(
        &mut self,
        permit: &AttemptPermit,
        cut: &serde_json::Value,
        roots: &std::collections::BTreeSet<String>,
        manifest: &FullSyncManifestReference,
    ) -> Result<(), String> {
        self.check_attempt(permit)?;
        let digest = local_io::digest(&(cut, local_io::digest(roots)?, manifest))?;
        if self.state.active_request.is_some() || self.state.reservation.is_some() {
            return Err("installation still has active I/O".into());
        }
        let mut next = self.state.clone();
        let installation = next
            .installation
            .as_mut()
            .ok_or("missing installation intent")?;
        if installation.completed
            || installation.attempt_no != permit.number
            || installation.publication_digest != digest
        {
            return Err("installation completion differs from original intent".into());
        }
        installation.completed = true;
        next.active_attempt_nonce = None;
        self.save(next)?;
        if Instant::now() >= permit.deadline {
            return Err("attempt_expired_after_installation_ack".into());
        }
        Ok(())
    }
}
