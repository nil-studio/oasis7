use super::*;

impl CognitionEconomyStateV1 {
    pub(super) fn release_inner(
        &mut self,
        lease_id: &str,
        tick: u64,
    ) -> Result<CognitionReceiptV1, CognitionEconomyError> {
        let lease = self
            .leases
            .get(lease_id)
            .cloned()
            .ok_or_else(|| CognitionEconomyError::LeaseNotFound(lease_id.to_string()))?;
        let key = operation_key(lease_id, "release");
        let digest = economy_digest(COGNITION_ECONOMY_OPERATION_DOMAIN, &key);
        if let Some(existing) = self.operations.get(&key) {
            if existing.operation_digest != digest {
                return Err(CognitionEconomyError::Conflict(
                    "cognition_release_idempotency_conflict",
                ));
            }
            return self.receipts.get(&existing.receipt_id).cloned().ok_or(
                CognitionEconomyError::InvalidState("cognition_operation_receipt_missing"),
            );
        }
        if lease.status != CognitionLeaseStatusV1::Reserved {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_lease_already_closed",
            ));
        }
        let balance = self.lease_balance_mut(&lease)?;
        if balance.reserved < lease.reserved_amount {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_reserved_balance_missing",
            ));
        }
        balance.reserved -= lease.reserved_amount;
        balance.available = balance.available.checked_add(lease.reserved_amount).ok_or(
            CognitionEconomyError::InvalidState("cognition_available_balance_overflow"),
        )?;
        let reserved_amount = lease.reserved_amount;
        self.close_lease(
            lease,
            CognitionLeaseStatusV1::Released,
            "release",
            0,
            reserved_amount,
            0,
            tick,
            key,
            digest,
            0,
            None,
            None,
        )
    }

    pub(super) fn settle_inner(
        &mut self,
        lease_id: &str,
        consumed_amount: u64,
        tick: u64,
    ) -> Result<CognitionReceiptV1, CognitionEconomyError> {
        let lease = self
            .leases
            .get(lease_id)
            .cloned()
            .ok_or_else(|| CognitionEconomyError::LeaseNotFound(lease_id.to_string()))?;
        if consumed_amount == 0 {
            return Err(CognitionEconomyError::InvalidInput(
                "cognition_settlement_usage_zero",
            ));
        }
        if consumed_amount > lease.reserved_amount {
            return Err(CognitionEconomyError::InvalidInput(
                "cognition_settlement_usage_exceeds_reservation",
            ));
        }
        let key = operation_key(lease_id, "settle");
        let digest = economy_digest(
            COGNITION_ECONOMY_OPERATION_DOMAIN,
            &(key.as_str(), consumed_amount),
        );
        if let Some(existing) = self.operations.get(&key) {
            if existing.operation_digest != digest {
                return Err(CognitionEconomyError::Conflict(
                    "cognition_settlement_idempotency_conflict",
                ));
            }
            return self.receipts.get(&existing.receipt_id).cloned().ok_or(
                CognitionEconomyError::InvalidState("cognition_operation_receipt_missing"),
            );
        }
        if lease.status != CognitionLeaseStatusV1::Reserved {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_lease_already_closed",
            ));
        }
        if lease
            .quote
            .valid_until_tick
            .is_some_and(|expires| tick > expires)
        {
            // A late provider response cannot consume the reservation. Close
            // it through the canonical expiry transition to return the
            // reservation atomically with its receipt and journal event.
            return self.expire_inner(lease_id, tick);
        }
        let refund = lease.reserved_amount - consumed_amount;
        let balance = self.lease_balance_mut(&lease)?;
        if balance.reserved < lease.reserved_amount {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_reserved_balance_missing",
            ));
        }
        balance.reserved -= lease.reserved_amount;
        balance.available =
            balance
                .available
                .checked_add(refund)
                .ok_or(CognitionEconomyError::InvalidState(
                    "cognition_available_balance_overflow",
                ))?;
        let receipt = self.close_lease(
            lease,
            CognitionLeaseStatusV1::Settled,
            "settle",
            consumed_amount,
            0,
            refund,
            tick,
            key,
            digest,
            0,
            None,
            None,
        )?;
        Ok(receipt)
    }

    pub(super) fn refund_inner(
        &mut self,
        lease_id: &str,
        refunded_amount: u64,
        parent_receipt_id: &str,
        reason: &str,
        tick: u64,
    ) -> Result<CognitionReceiptV1, CognitionEconomyError> {
        let lease = self
            .leases
            .get(lease_id)
            .cloned()
            .ok_or_else(|| CognitionEconomyError::LeaseNotFound(lease_id.to_string()))?;
        let key = operation_key(lease_id, "refund");
        let digest = economy_digest(
            COGNITION_ECONOMY_OPERATION_DOMAIN,
            &(
                key.as_str(),
                refunded_amount,
                Some(parent_receipt_id),
                Some(reason),
            ),
        );
        if let Some(existing) = self.operations.get(&key) {
            if existing.operation_digest != digest {
                return Err(CognitionEconomyError::Conflict(
                    "cognition_refund_idempotency_conflict",
                ));
            }
            return self.receipts.get(&existing.receipt_id).cloned().ok_or(
                CognitionEconomyError::InvalidState("cognition_operation_receipt_missing"),
            );
        }
        lease.validate()?;
        if lease.status != CognitionLeaseStatusV1::Settled {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_refund_requires_settled",
            ));
        }
        if refunded_amount == 0 || refunded_amount > lease.settled_amount {
            return Err(CognitionEconomyError::InvalidInput(
                "cognition_refund_amount_invalid",
            ));
        }
        if !bounded_identity(reason) {
            return Err(CognitionEconomyError::InvalidInput(
                "cognition_refund_reason_invalid",
            ));
        }
        let Some(parent) = self.receipts.get(parent_receipt_id) else {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_refund_parent_receipt_missing",
            ));
        };
        parent.validate()?;
        if parent.operation != "settle"
            || parent.status != CognitionLeaseStatusV1::Settled
            || parent.lease_id != lease.lease_id
            || parent.consumed_amount != lease.settled_amount
            || parent.receipt_id != parent_receipt_id
            || lease.receipt_id.as_deref() != Some(parent_receipt_id)
        {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_refund_parent_receipt_invalid",
            ));
        }

        let settle_operation_key = operation_key(&lease.lease_id, "settle");
        let Some(settle_operation) = self.operations.get(&settle_operation_key) else {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_refund_settlement_provenance_invalid",
            ));
        };
        if settle_operation.operation_key != settle_operation_key
            || settle_operation.lease_id != lease.lease_id
            || settle_operation.receipt_id != parent_receipt_id
            || settle_operation.operation_digest
                != economy_digest(
                    COGNITION_ECONOMY_OPERATION_DOMAIN,
                    &(settle_operation_key.as_str(), lease.settled_amount),
                )
        {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_refund_settlement_provenance_invalid",
            ));
        }
        let settle_events: Vec<_> = self
            .journal
            .iter()
            .filter(|event| event.lease_id == lease.lease_id && event.event_kind == "settle")
            .collect();
        if settle_events.len() != 1 {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_refund_settlement_provenance_invalid",
            ));
        }
        let settle_event = settle_events[0];
        if settle_event.operation_key != settle_operation_key
            || settle_event.idempotency_key != lease.idempotency_key
            || settle_event.resource != lease.quote.resource
            || settle_event.reserved_amount != lease.reserved_amount
            || settle_event.consumed_amount != lease.settled_amount
            || settle_event.released_amount != 0
            || settle_event.refunded_amount != lease.refunded_amount
            || settle_event.net_amount != lease.net_amount
            || settle_event.parent_receipt_id.is_some()
            || settle_event.reason.is_some()
            || settle_event.receipt_id != parent_receipt_id
            || settle_event.status != CognitionLeaseStatusV1::Settled
        {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_refund_settlement_provenance_invalid",
            ));
        }

        let expected_reserved = self
            .leases
            .values()
            .filter(|candidate| {
                candidate.status == CognitionLeaseStatusV1::Reserved
                    && candidate.account_id == lease.account_id
                    && candidate.quote.resource == lease.quote.resource
                    && self.lease_binding_keys.get(&candidate.lease_id)
                        == self.lease_binding_keys.get(&lease.lease_id)
            })
            .try_fold(0_u64, |total, candidate| {
                total.checked_add(candidate.reserved_amount).ok_or(
                    CognitionEconomyError::InvalidState("cognition_economy_reserved_overflow"),
                )
            })?;
        let has_balance = if let Some(binding_key) = self.lease_binding_keys.get(&lease.lease_id) {
            self.provisioned_balances
                .get(binding_key)
                .and_then(|resources| resources.get(&lease.quote.resource))
                .is_some()
        } else {
            self.balances
                .get(&lease.account_id)
                .and_then(|resources| resources.get(&lease.quote.resource))
                .is_some()
        };
        if !has_balance {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_refund_balance_missing",
            ));
        }
        let balance = self.lease_balance_mut(&lease)?;
        if balance.reserved != expected_reserved {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_refund_balance_reservation_mismatch",
            ));
        }
        balance.available = balance.available.checked_add(refunded_amount).ok_or(
            CognitionEconomyError::InvalidState("cognition_available_balance_overflow"),
        )?;
        let settled_amount = lease.settled_amount;
        self.close_lease(
            lease,
            CognitionLeaseStatusV1::Refunded,
            "refund",
            settled_amount,
            0,
            refunded_amount,
            tick,
            key,
            digest,
            refunded_amount,
            Some(parent_receipt_id.to_string()),
            Some(reason.to_string()),
        )
    }

    pub(super) fn expire_inner(
        &mut self,
        lease_id: &str,
        tick: u64,
    ) -> Result<CognitionReceiptV1, CognitionEconomyError> {
        let lease = self
            .leases
            .get(lease_id)
            .cloned()
            .ok_or_else(|| CognitionEconomyError::LeaseNotFound(lease_id.to_string()))?;
        let key = operation_key(lease_id, "expire");
        let digest = economy_digest(COGNITION_ECONOMY_OPERATION_DOMAIN, &key);
        if let Some(existing) = self.operations.get(&key) {
            if existing.operation_digest != digest {
                return Err(CognitionEconomyError::Conflict(
                    "cognition_expiry_idempotency_conflict",
                ));
            }
            return self.receipts.get(&existing.receipt_id).cloned().ok_or(
                CognitionEconomyError::InvalidState("cognition_operation_receipt_missing"),
            );
        }
        if lease.status != CognitionLeaseStatusV1::Reserved {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_lease_already_closed",
            ));
        }
        if lease
            .quote
            .valid_until_tick
            .is_none_or(|expires| tick <= expires)
        {
            return Err(CognitionEconomyError::InvalidInput(
                "cognition_lease_not_expired",
            ));
        }
        let balance = self.lease_balance_mut(&lease)?;
        if balance.reserved < lease.reserved_amount {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_reserved_balance_missing",
            ));
        }
        balance.reserved -= lease.reserved_amount;
        balance.available = balance.available.checked_add(lease.reserved_amount).ok_or(
            CognitionEconomyError::InvalidState("cognition_available_balance_overflow"),
        )?;
        let reserved_amount = lease.reserved_amount;
        self.close_lease(
            lease,
            CognitionLeaseStatusV1::Expired,
            "expire",
            0,
            reserved_amount,
            0,
            tick,
            key,
            digest,
            0,
            None,
            Some("quote_expired".to_string()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn close_lease(
        &mut self,
        mut lease: CognitionLeaseV1,
        status: CognitionLeaseStatusV1,
        operation: &str,
        consumed_amount: u64,
        released_amount: u64,
        refunded_amount: u64,
        tick: u64,
        operation_key: String,
        operation_digest: String,
        compensated_amount: u64,
        parent_receipt_id: Option<String>,
        reason: Option<String>,
    ) -> Result<CognitionReceiptV1, CognitionEconomyError> {
        let receipt_id = economy_digest(
            COGNITION_ECONOMY_RECEIPT_ID_DOMAIN,
            &(lease.lease_id.as_str(), operation_key.as_str()),
        );
        let net_amount = consumed_amount.checked_sub(compensated_amount).ok_or(
            CognitionEconomyError::InvalidState("cognition_net_amount_underflow"),
        )?;
        let mut receipt = CognitionReceiptV1 {
            schema_version: COGNITION_RECEIPT_SCHEMA_VERSION.to_string(),
            receipt_id: receipt_id.clone(),
            receipt_digest: String::new(),
            lease_id: lease.lease_id.clone(),
            idempotency_key: lease.idempotency_key.clone(),
            account_id: lease.account_id.clone(),
            agent_id: lease.agent_id.clone(),
            agent_session_id: lease.agent_session_id.clone(),
            agent_turn_id: lease.agent_turn_id.clone(),
            decision_request_id: lease.decision_request_id.clone(),
            request_digest: lease.request_digest.clone(),
            quote: lease.quote.clone(),
            reserved_amount: lease.reserved_amount,
            operation: operation.to_string(),
            consumed_amount,
            released_amount,
            refunded_amount,
            net_amount,
            parent_receipt_id,
            reason,
            status,
            issued_at_tick: tick,
        };
        receipt.receipt_digest = receipt.recompute_digest();
        lease.status = status;
        lease.settled_amount = if status == CognitionLeaseStatusV1::Settled {
            consumed_amount
        } else {
            lease.settled_amount
        };
        lease.released_amount = released_amount;
        lease.compensated_amount = compensated_amount;
        lease.net_amount = receipt.net_amount;
        lease.refunded_amount = match status {
            CognitionLeaseStatusV1::Settled => refunded_amount,
            CognitionLeaseStatusV1::Refunded => {
                lease.refunded_amount.checked_add(refunded_amount).ok_or(
                    CognitionEconomyError::InvalidState("cognition_refund_overflow"),
                )?
            }
            _ => 0,
        };
        lease.closed_at_tick = Some(tick);
        lease.receipt_id = Some(receipt_id.clone());
        lease.validate()?;
        self.leases.insert(lease.lease_id.clone(), lease.clone());
        self.receipts.insert(receipt_id.clone(), receipt.clone());
        self.operations.insert(
            operation_key.clone(),
            CognitionEconomyOperationRecordV1 {
                operation_key: operation_key.clone(),
                operation_digest,
                lease_id: lease.lease_id.clone(),
                receipt_id: receipt_id.clone(),
            },
        );
        self.append_event(
            operation,
            &lease,
            &operation_key,
            &receipt_id,
            consumed_amount,
            released_amount,
            refunded_amount,
            receipt.net_amount,
            receipt.parent_receipt_id.clone(),
            receipt.reason.clone(),
        )?;
        Ok(receipt)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Economy journal event fields map one-to-one to the persisted transition schema."
    )]
    pub(super) fn append_event(
        &mut self,
        event_kind: &str,
        lease: &CognitionLeaseV1,
        operation_key: &str,
        receipt_id: &str,
        consumed_amount: u64,
        released_amount: u64,
        refunded_amount: u64,
        net_amount: u64,
        parent_receipt_id: Option<String>,
        reason: Option<String>,
    ) -> Result<(), CognitionEconomyError> {
        let journal_seq = self.head_seq.saturating_add(1);
        let parent_event_digest = self
            .journal
            .last()
            .map(|event| event.event_digest.clone())
            .unwrap_or_default();
        let mut event = CognitionEconomyEventV1 {
            schema_version: COGNITION_ECONOMY_EVENT_SCHEMA_VERSION.to_string(),
            journal_seq,
            parent_event_digest,
            event_kind: event_kind.to_string(),
            lease_id: lease.lease_id.clone(),
            operation_key: operation_key.to_string(),
            idempotency_key: lease.idempotency_key.clone(),
            resource: lease.quote.resource.clone(),
            reserved_amount: lease.reserved_amount,
            consumed_amount,
            released_amount,
            refunded_amount,
            net_amount,
            parent_receipt_id,
            reason,
            status: lease.status,
            receipt_id: receipt_id.to_string(),
            event_digest: String::new(),
        };
        event.event_digest = event.recompute_digest();
        self.journal.push(event);
        self.head_seq = journal_seq;
        self.head_digest = economy_digest(
            COGNITION_ECONOMY_JOURNAL_DOMAIN,
            &(self.head_seq, &self.journal),
        );
        Ok(())
    }
}
