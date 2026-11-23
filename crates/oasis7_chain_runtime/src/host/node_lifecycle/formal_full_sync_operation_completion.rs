//! Installation and terminal transitions share the original ledger CAS.
use super::super::installation::VerifiedInstallation;
use super::super::operation_terminal::Installation;
use super::*;
use std::collections::BTreeSet;

impl Ledger {
    pub(in super::super) fn resume_installation_repair(
        &mut self,
        owner: &super::super::super::process::ProcessOwnership,
        data: &Path,
        original: &super::super::installation::VerifiedRepair,
        deadline: Instant,
    ) -> Result<AttemptPermit, String> {
        original.require_snapshot(&self.state)?;
        let target = self.state.verified_target_json.clone();
        self.resume_operation(
            owner,
            data,
            &original.request,
            target.as_deref(),
            deadline,
            true,
        )
    }

    pub(in super::super) fn prepare_installation(
        &mut self,
        permit: &AttemptPermit,
        manifest: &serde_json::Value,
        retained_cas_hashes: &BTreeSet<String>,
    ) -> Result<(), String> {
        self.check_attempt(permit)?;
        if self.state.publication.is_some()
            || self.state.active_request.is_some()
            || self.state.reservation.is_some()
            || self.state.terminal_index.is_none()
        {
            return Err("operation_recovery_required: installation preparation unavailable".into());
        }
        let installation = Installation {
            manifest: manifest.clone(),
            retained_cas_hashes: retained_cas_hashes.clone(),
            completed: false,
            history_manifest_digest: None,
        };
        if let Some(original) = &self.state.installation {
            if original != &installation {
                return Err("operation_recovery_required: original installation changed".into());
            }
            return Ok(());
        }
        let mut next = self.state.clone();
        next.installation = Some(installation);
        self.save(next)?;
        self.check_attempt(permit)
    }

    /// A receipt is produced by restoring the original World from verified
    /// local originals and reading back the independent pin shard/history.
    /// Native ownership and this handle's writer lock prove the old I/O ended.
    /// No transport permit is created here, including after an expired attempt.
    pub(in super::super) fn complete_installation(
        &mut self,
        owner: &super::super::super::process::ProcessOwnership,
        data: &Path,
        receipt: &VerifiedInstallation,
        deadline: Instant,
    ) -> Result<(), String> {
        owner.require_directory(data)?;
        let expired = if self.recovered {
            "operation_reopen_expired"
        } else {
            "attempt_expired"
        };
        self.check_cancellation()?;
        if Instant::now() >= deadline {
            return Err(self.local_error(
                if self.recovered {
                    LocalBlocker::RecoveryRequired
                } else {
                    LocalBlocker::AttemptExpired
                },
                expired,
            ));
        }
        if self.path != data.join("state/full-sync/operation.json")
            || owner.binding_digest()? != self.state.identity.binding_digest
            || self.poisoned
        {
            return Err("operation_recovery_required: installation completion unavailable".into());
        }
        receipt.require_snapshot(&self.state)?;
        if self
            .state
            .installation
            .as_ref()
            .is_some_and(|i| i.completed)
        {
            return Ok(());
        }
        // A live caller must have ended its exact request and attempt first.
        // A reopened handle obtained the writer lock only after old work ended.
        if !self.recovered
            && (self.state.active_attempt_nonce.is_some()
                || self.state.active_request.is_some()
                || self.state.reservation.is_some())
        {
            return Err("operation_recovery_required: installation still has live I/O".into());
        }
        let mut next = self.state.clone();
        let original_charge = next.charged()?;
        next.unknown_final_charged = next
            .unknown_final_charged
            .checked_add(next.unresolved_reserved()?)
            .ok_or("unknown final accounting overflow")?;
        next.active_attempt_nonce = None;
        next.repair_attempt = false;
        next.active_request = None;
        next.active_request_nonce = None;
        next.tx_credit = 0;
        next.rx_credit = 0;
        next.reservation = None;
        for old in &mut next.interrupted_attempts {
            old.request = None;
            old.request_nonce = None;
            old.tx_credit = 0;
            old.rx_credit = 0;
            old.reservation = None;
        }
        if next.charged()? != original_charge {
            return Err("operation_recovery_required: finalization changed the paid charge".into());
        }
        next.publication
            .as_mut()
            .ok_or("installation has no publication intent")?
            .completed = true;
        let installation = next
            .installation
            .as_mut()
            .ok_or("installation originals missing")?;
        installation.completed = true;
        installation.history_manifest_digest = receipt.history_manifest_digest.clone();
        let mut terminal = next
            .terminal_index
            .take()
            .ok_or("terminal continuity unavailable")?;
        terminal
            .append(&next)
            .map_err(|error| self.local_error(error.blocker, &error.message))?;
        next.terminal_index = Some(terminal);
        owner.require_directory(data)?;
        self.check_cancellation()?;
        if Instant::now() >= deadline {
            return Err(self.local_error(
                if self.recovered {
                    LocalBlocker::RecoveryRequired
                } else {
                    LocalBlocker::AttemptExpired
                },
                expired,
            ));
        }
        self.save_exact(next)?;
        if Instant::now() >= deadline {
            self.poisoned = true;
            return Err(self.local_error(
                if self.recovered {
                    LocalBlocker::RecoveryRequired
                } else {
                    LocalBlocker::AttemptExpired
                },
                expired,
            ));
        }
        if let Err(error) = self.check_cancellation() {
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }

    /// Atomically replace a completed active record while preserving the bounded
    /// terminal chain and cumulative paid totals. No delete/create crash gap.
    pub(in super::super) fn begin_next_operation(
        &mut self,
        owner: &super::super::super::process::ProcessOwnership,
        data: &Path,
        receipt: &VerifiedInstallation,
        request: &serde_json::Value,
        deadline: Instant,
    ) -> Result<(), String> {
        owner.require_directory(data)?;
        receipt.require_snapshot(&self.state)?;
        if owner.binding_digest()? != self.state.identity.binding_digest
            || !self
                .state
                .installation
                .as_ref()
                .is_some_and(|i| i.completed)
            || self.state.active_attempt_nonce.is_some()
            || self.state.active_request.is_some()
            || self.state.unresolved_reserved()? != 0
            || self.state.terminal_index.is_none()
            || request["native_owner_binding"].as_str()
                != Some(self.state.identity.binding_digest.as_str())
            || Instant::now() >= deadline
        {
            return Err("operation_recovery_required: previous operation cannot rotate".into());
        }
        let original: serde_json::Value = serde_json::from_slice(
            self.state
                .original_request_json
                .as_deref()
                .ok_or("original operation request missing")?,
        )
        .map_err(|e| e.to_string())?;
        for field in [
            "api_version",
            "transfer_policy",
            "native_owner_binding",
            "node_public_key",
            "transport_binding",
        ] {
            if original[field] != request[field] {
                return Err(
                    "operation_recovery_required: next cycle changed immutable identity".into(),
                );
            }
        }
        if !super::super::grant_transition::permits(
            &self.state,
            &original,
            &request["node_identity"],
            &request["config"],
        )? {
            return Err(
                "operation_recovery_required: next cycle changed immutable grant binding".into(),
            );
        }
        let mut next = self.state.clone();
        next.identity.operation_id = local_io::random_id()?;
        next.identity.request_digest = local_io::digest(request)?;
        next.original_request_json = Some(serde_json::to_vec(request).map_err(|e| e.to_string())?);
        next.verified_target_json = None;
        next.publication = None;
        next.installation = None;
        next.attempt_owner = None;
        next.repair_attempt = false;
        next.encoded_tx_actual = 0;
        next.encoded_rx_actual = 0;
        next.unknown_final_charged = 0;
        next.exchanges_started = 0;
        next.proof_pages = 0;
        next.proof_bytes = 0;
        next.material_bytes = 0;
        next.materials.clear();
        next.window_seq = 0;
        next.attempts_used = 0;
        next.interrupted_attempts.clear();
        // Overflow must block before another operation can start, not after its
        // next full transfer has consumed resources.
        next.terminal_index
            .as_ref()
            .unwrap()
            .require_next_capacity(&next.policy)
            .map_err(|error| self.local_error(LocalBlocker::BudgetExhausted, &error))?;
        self.save_exact(next)?;
        if Instant::now() >= deadline {
            self.poisoned = true;
            return Err("operation_reopen_expired".into());
        }
        self.recovered = false;
        self.reopen_deadline = None;
        Ok(())
    }
}
