use std::collections::BTreeMap;

use crate::viewer::protocol::{PromptControlAck, PromptControlError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PromptControlLedgerReceipt {
    Ack(PromptControlAck),
    Error(PromptControlError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PromptControlResultLedgerEntry {
    pub(super) digest: String,
    pub(super) receipt: PromptControlLedgerReceipt,
}

#[expect(
    clippy::large_enum_variant,
    reason = "Prompt-control replay lookup carries the stable typed receipt envelope"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PromptControlLedgerLookup {
    Missing,
    Replay(PromptControlLedgerReceipt),
    Conflict,
}

#[derive(Debug, Clone)]
pub(super) struct PromptControlResultLedger {
    capacity: usize,
    receipt_max_bytes: usize,
    entries: BTreeMap<(String, String, String), PromptControlResultLedgerEntry>,
}

impl PromptControlResultLedger {
    pub(super) fn new(capacity: usize, receipt_max_bytes: usize) -> Result<Self, String> {
        if capacity == 0 {
            return Err("prompt result cache capacity must be greater than zero".to_string());
        }
        if receipt_max_bytes == 0 {
            return Err("prompt result receipt max bytes must be greater than zero".to_string());
        }
        Ok(Self {
            capacity,
            receipt_max_bytes,
            entries: BTreeMap::new(),
        })
    }

    pub(super) fn lookup(
        &self,
        authority_epoch: &str,
        player_id: &str,
        request_id: &str,
        digest: &str,
    ) -> PromptControlLedgerLookup {
        let key = (
            authority_epoch.to_string(),
            player_id.to_string(),
            request_id.to_string(),
        );
        match self.entries.get(&key) {
            None => PromptControlLedgerLookup::Missing,
            Some(entry) if entry.digest == digest => {
                PromptControlLedgerLookup::Replay(entry.receipt.clone())
            }
            Some(_) => PromptControlLedgerLookup::Conflict,
        }
    }

    pub(super) fn is_full(&self) -> bool {
        self.entries.len() >= self.capacity
    }

    pub(super) fn validate_receipt(
        &self,
        ack: &PromptControlAck,
    ) -> Result<(), PromptControlLedgerInsertError> {
        self.validate_serialized_receipt(ack)
    }

    pub(super) fn validate_error(
        &self,
        error: &PromptControlError,
    ) -> Result<(), PromptControlLedgerInsertError> {
        self.validate_serialized_receipt(error)
    }

    fn validate_serialized_receipt<T: serde::Serialize>(
        &self,
        receipt: &T,
    ) -> Result<(), PromptControlLedgerInsertError> {
        let receipt_size = serde_json::to_vec(receipt)
            .map_err(|error| PromptControlLedgerInsertError::Serialize(error.to_string()))?
            .len();
        if receipt_size > self.receipt_max_bytes {
            return Err(PromptControlLedgerInsertError::ReceiptTooLarge {
                actual: receipt_size,
                limit: self.receipt_max_bytes,
            });
        }
        Ok(())
    }

    pub(super) fn insert(
        &mut self,
        authority_epoch: &str,
        player_id: &str,
        request_id: &str,
        digest: String,
        ack: PromptControlAck,
    ) -> Result<(), PromptControlLedgerInsertError> {
        self.insert_receipt(
            authority_epoch,
            player_id,
            request_id,
            digest,
            PromptControlLedgerReceipt::Ack(ack),
        )
    }

    pub(super) fn insert_error(
        &mut self,
        authority_epoch: &str,
        player_id: &str,
        request_id: &str,
        digest: String,
        error: PromptControlError,
    ) -> Result<(), PromptControlLedgerInsertError> {
        self.insert_receipt(
            authority_epoch,
            player_id,
            request_id,
            digest,
            PromptControlLedgerReceipt::Error(error),
        )
    }

    fn insert_receipt(
        &mut self,
        authority_epoch: &str,
        player_id: &str,
        request_id: &str,
        digest: String,
        receipt: PromptControlLedgerReceipt,
    ) -> Result<(), PromptControlLedgerInsertError> {
        match &receipt {
            PromptControlLedgerReceipt::Ack(ack) => self.validate_receipt(ack)?,
            PromptControlLedgerReceipt::Error(error) => self.validate_error(error)?,
        }
        if self.is_full() {
            return Err(PromptControlLedgerInsertError::Full);
        }
        self.entries.insert(
            (
                authority_epoch.to_string(),
                player_id.to_string(),
                request_id.to_string(),
            ),
            PromptControlResultLedgerEntry { digest, receipt },
        );
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PromptControlLedgerInsertError {
    Full,
    ReceiptTooLarge { actual: usize, limit: usize },
    Serialize(String),
}

#[derive(Debug, Clone)]
pub(super) struct PromptControlRuntimeAuthority {
    pub(super) authority_epoch: String,
    pub(super) binding_epoch_by_agent: BTreeMap<String, u64>,
    pub(super) result_ledger: PromptControlResultLedger,
}

impl PromptControlRuntimeAuthority {
    pub(super) fn new(cache_capacity: usize, receipt_max_bytes: usize) -> Result<Self, String> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes)
            .map_err(|error| format!("generate prompt authority epoch failed: {error}"))?;
        Ok(Self {
            authority_epoch: hex::encode(bytes),
            binding_epoch_by_agent: BTreeMap::new(),
            result_ledger: PromptControlResultLedger::new(cache_capacity, receipt_max_bytes)?,
        })
    }

    pub(super) fn binding_epoch(&self, agent_id: &str) -> u64 {
        self.binding_epoch_by_agent
            .get(agent_id)
            .copied()
            .unwrap_or(0)
    }

    pub(super) fn advance_binding_epoch(&mut self, agent_id: &str) -> u64 {
        let next = self.binding_epoch(agent_id).saturating_add(1);
        self.binding_epoch_by_agent
            .insert(agent_id.to_string(), next);
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ack(request_id: &str) -> PromptControlAck {
        let mut ack = PromptControlAck::default_legacy();
        ack.request_id = Some(request_id.to_string());
        ack.agent_id = "agent-0".to_string();
        ack.digest = "profile-digest".to_string();
        ack
    }

    #[test]
    fn result_ledger_is_fixed_and_distinguishes_replay_from_conflict() {
        let mut ledger = PromptControlResultLedger::new(1, 4096).expect("ledger");
        ledger
            .insert(
                "authority-1",
                "player-a",
                "request-1",
                "operation-1".to_string(),
                ack("request-1"),
            )
            .expect("first receipt");
        assert!(matches!(
            ledger.lookup("authority-1", "player-a", "request-1", "operation-1"),
            PromptControlLedgerLookup::Replay(_)
        ));
        assert!(matches!(
            ledger.lookup("authority-1", "player-a", "request-1", "operation-2"),
            PromptControlLedgerLookup::Conflict
        ));
        assert!(matches!(
            ledger.insert(
                "authority-1",
                "player-a",
                "request-2",
                "operation-2".to_string(),
                ack("request-2"),
            ),
            Err(PromptControlLedgerInsertError::Full)
        ));
    }

    #[test]
    fn result_ledger_rejects_oversize_receipts_before_insert() {
        let mut ledger = PromptControlResultLedger::new(2, 1).expect("ledger");
        let error = ledger
            .insert(
                "authority-1",
                "player-a",
                "request-1",
                "operation-1".to_string(),
                ack("request-1"),
            )
            .expect_err("oversize receipt");
        assert!(matches!(
            error,
            PromptControlLedgerInsertError::ReceiptTooLarge { .. }
        ));
        assert_eq!(ledger.len(), 0);
    }
}
