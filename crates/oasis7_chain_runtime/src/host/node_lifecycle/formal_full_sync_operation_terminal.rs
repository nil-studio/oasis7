//! Bounded terminal diagnostics are part of the same atomic ledger revision.
//! The ring never supplies authority or replaces independently pinned history.
use super::super::local_io;
use super::operation_failure::{LocalBlocker, OperationFailure};
use super::operation_snapshot::{Identity, Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub(super) const MAX_TERMINALS: usize = 16;
pub(super) const MAX_TERMINAL_BYTES: usize = 64 * 1024;
pub(super) const MAX_INDEX_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AttemptOwner {
    pub generation: u64,
    pub incarnation: String,
}

/// Exact local originals needed to finish an already decided installation.
/// The manifest contains bounded references, not an alternate trust anchor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Installation {
    pub manifest: Value,
    pub retained_cas_hashes: BTreeSet<String>,
    pub completed: bool,
    pub history_manifest_digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TerminalSummary {
    pub identity: Identity,
    pub target: Value,
    pub result: String,
    pub encoded_tx_actual: u64,
    pub encoded_rx_actual: u64,
    pub unknown_final_charged: u64,
    pub exchanges_started: u64,
    pub attempts_used: u64,
    pub proof_pages: u64,
    pub proof_bytes: u64,
    pub material_bytes: u64,
    pub installation_digest: String,
    pub history_manifest_digest: Option<String>,
    pub predecessor: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TerminalIndex {
    pub completed_operations: u64,
    pub encoded_tx_actual: u64,
    pub encoded_rx_actual: u64,
    pub unknown_final_charged: u64,
    pub exchanges_started: u64,
    pub attempts_used: u64,
    pub chain_head: Option<String>,
    pub summaries: Vec<TerminalSummary>,
}

impl TerminalIndex {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.summaries.len() > MAX_TERMINALS
            || self.completed_operations < self.summaries.len() as u64
            || (self.completed_operations == 0) != self.summaries.is_empty()
            || serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_INDEX_BYTES
        {
            return Err("operation_recovery_required: invalid terminal index capacity".into());
        }
        let mut previous = None;
        let mut identities = BTreeSet::new();
        for (number, summary) in self.summaries.iter().enumerate() {
            let valid_hex = |value: &str, length: usize| {
                value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            };
            let target: super::FormalFullSyncCutV1 =
                serde_json::from_value(summary.target.clone()).map_err(|e| e.to_string())?;
            target.validate()?;
            if serde_json::to_vec(summary)
                .map_err(|e| e.to_string())?
                .len()
                > MAX_TERMINAL_BYTES
                || summary.result != "installed_read_only"
                || summary.attempts_used == 0
                || summary.attempts_used > 4
                || !valid_hex(&summary.identity.operation_id, 32)
                || [
                    &summary.identity.request_digest,
                    &summary.identity.policy_digest,
                    &summary.identity.binding_digest,
                    &summary.installation_digest,
                ]
                .into_iter()
                .any(|digest| !valid_hex(digest, 64))
                || summary
                    .history_manifest_digest
                    .as_ref()
                    .is_some_and(|digest| !valid_hex(digest, 64))
                || summary
                    .predecessor
                    .as_ref()
                    .is_some_and(|digest| !valid_hex(digest, 64))
                || !identities.insert(&summary.identity.operation_id)
                || (number == 0
                    && summary.predecessor.is_some()
                        != (self.completed_operations > self.summaries.len() as u64))
                || (number != 0 && summary.predecessor != previous)
            {
                return Err("operation_recovery_required: invalid terminal summary".into());
            }
            previous = Some(local_io::digest(summary)?);
        }
        if previous != self.chain_head {
            return Err("operation_recovery_required: terminal predecessor mismatch".into());
        }
        for (total, values) in [
            (
                self.encoded_tx_actual,
                self.summaries
                    .iter()
                    .map(|s| s.encoded_tx_actual)
                    .collect::<Vec<_>>(),
            ),
            (
                self.encoded_rx_actual,
                self.summaries.iter().map(|s| s.encoded_rx_actual).collect(),
            ),
            (
                self.unknown_final_charged,
                self.summaries
                    .iter()
                    .map(|s| s.unknown_final_charged)
                    .collect(),
            ),
            (
                self.exchanges_started,
                self.summaries.iter().map(|s| s.exchanges_started).collect(),
            ),
            (
                self.attempts_used,
                self.summaries.iter().map(|s| s.attempts_used).collect(),
            ),
        ] {
            let retained = values
                .into_iter()
                .try_fold(0u64, u64::checked_add)
                .ok_or("terminal aggregate overflow")?;
            if total < retained
                || (self.completed_operations == self.summaries.len() as u64 && total != retained)
            {
                return Err(
                    "operation_recovery_required: terminal cumulative total decreased".into(),
                );
            }
        }
        Ok(())
    }

    /// Reserve diagnostic headroom before the next operation obtains any I/O
    /// authority. Its worst permitted charge must fit all cumulative counters.
    pub(super) fn require_next_capacity(
        &self,
        policy: &super::operation_config::FormalFullSyncTransferPolicyV1,
    ) -> Result<(), String> {
        for (total, maximum) in [
            (self.completed_operations, 1),
            (self.encoded_tx_actual, policy.max_encoded_io_bytes),
            (self.encoded_rx_actual, policy.max_encoded_io_bytes),
            (self.unknown_final_charged, policy.max_encoded_io_bytes),
            (self.exchanges_started, policy.max_exchanges),
            (self.attempts_used, policy.max_attempts),
        ] {
            total
                .checked_add(maximum)
                .ok_or("operation_budget_exhausted: terminal cumulative capacity")?;
        }
        Ok(())
    }

    /// Called only after local original World/readback and independent pins
    /// establish that no private operation journal is an installation input.
    pub(super) fn append(&mut self, snapshot: &Snapshot) -> Result<(), OperationFailure> {
        let installation = snapshot
            .installation
            .as_ref()
            .filter(|i| i.completed)
            .ok_or("operation_recovery_required: installation is not complete")?;
        if snapshot.active_attempt_nonce.is_some()
            || snapshot.active_request.is_some()
            || snapshot.unresolved_reserved()? != 0
            || !snapshot.publication.as_ref().is_some_and(|p| p.completed)
        {
            return Err(
                "operation_recovery_required: unresolved operation cannot terminate".into(),
            );
        }
        let target: Value = serde_json::from_slice(
            snapshot
                .verified_target_json
                .as_deref()
                .ok_or("terminal operation has no original target")?,
        )
        .map_err(|e| e.to_string())?;
        let summary = TerminalSummary {
            identity: snapshot.identity.clone(),
            target: target[0].clone(),
            result: "installed_read_only".into(),
            encoded_tx_actual: snapshot.encoded_tx_actual,
            encoded_rx_actual: snapshot.encoded_rx_actual,
            unknown_final_charged: snapshot.unknown_final_charged,
            exchanges_started: snapshot.exchanges_started,
            attempts_used: snapshot.attempts_used,
            proof_pages: snapshot.proof_pages,
            proof_bytes: snapshot.proof_bytes,
            material_bytes: snapshot.material_bytes,
            installation_digest: local_io::digest(installation)?,
            history_manifest_digest: installation.history_manifest_digest.clone(),
            predecessor: self.chain_head.clone(),
        };
        if self
            .summaries
            .iter()
            .any(|s| s.identity.operation_id == snapshot.identity.operation_id)
        {
            return Err("operation_recovery_required: duplicate terminal operation".into());
        }
        if serde_json::to_vec(&summary)
            .map_err(|e| e.to_string())?
            .len()
            > MAX_TERMINAL_BYTES
        {
            return Err(OperationFailure::new(
                LocalBlocker::BudgetExhausted,
                "operation_budget_exhausted: terminal summary capacity",
            ));
        }
        self.completed_operations = self.completed_operations.checked_add(1).ok_or_else(|| {
            OperationFailure::new(LocalBlocker::BudgetExhausted, "terminal count overflow")
        })?;
        self.encoded_tx_actual = self
            .encoded_tx_actual
            .checked_add(snapshot.encoded_tx_actual)
            .ok_or_else(|| {
                OperationFailure::new(LocalBlocker::BudgetExhausted, "terminal tx overflow")
            })?;
        self.encoded_rx_actual = self
            .encoded_rx_actual
            .checked_add(snapshot.encoded_rx_actual)
            .ok_or_else(|| {
                OperationFailure::new(LocalBlocker::BudgetExhausted, "terminal rx overflow")
            })?;
        self.unknown_final_charged = self
            .unknown_final_charged
            .checked_add(snapshot.unknown_final_charged)
            .ok_or_else(|| {
                OperationFailure::new(LocalBlocker::BudgetExhausted, "terminal unknown overflow")
            })?;
        self.exchanges_started = self
            .exchanges_started
            .checked_add(snapshot.exchanges_started)
            .ok_or_else(|| {
                OperationFailure::new(LocalBlocker::BudgetExhausted, "terminal exchange overflow")
            })?;
        self.attempts_used = self
            .attempts_used
            .checked_add(snapshot.attempts_used)
            .ok_or_else(|| {
                OperationFailure::new(LocalBlocker::BudgetExhausted, "terminal attempt overflow")
            })?;
        self.chain_head = Some(local_io::digest(&summary)?);
        self.summaries.push(summary);
        if self.summaries.len() > MAX_TERMINALS {
            self.summaries.remove(0);
        }
        self.validate().map_err(Into::into)
    }
}
