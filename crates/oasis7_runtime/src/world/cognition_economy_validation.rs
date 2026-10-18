use std::collections::BTreeSet;

use super::*;

impl CognitionEconomyStateV1 {
    pub fn validate(&self) -> Result<(), CognitionEconomyError> {
        if self.schema_version != COGNITION_ECONOMY_SCHEMA_VERSION
            || self.head_seq != self.journal.len() as u64
        {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_economy_head_invalid",
            ));
        }
        let expected_head = economy_digest(
            COGNITION_ECONOMY_JOURNAL_DOMAIN,
            &(self.head_seq, &self.journal),
        );
        if self.head_digest != expected_head {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_economy_head_digest_mismatch",
            ));
        }
        for (account_id, resources) in &self.balances {
            if !bounded_identity(account_id) {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_balance_identity_invalid",
                ));
            }
            for resource in resources.keys() {
                if !bounded_identity(resource) {
                    return Err(CognitionEconomyError::InvalidState(
                        "cognition_economy_balance_identity_invalid",
                    ));
                }
            }
        }
        for (binding_key, resources) in &self.provisioned_balances {
            if !valid_digest(binding_key) || self.provision_for_binding(binding_key).is_none() {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_balance_binding_invalid",
                ));
            }
            let request = self
                .provision_for_binding(binding_key)
                .expect("checked provisioning balance binding");
            for resource in resources.keys() {
                if !bounded_identity(resource) || resource != &request.resource {
                    return Err(CognitionEconomyError::InvalidState(
                        "cognition_provisioning_balance_identity_invalid",
                    ));
                }
            }
        }
        if self.provision_head_seq != self.provision_journal.len() as u64 {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_provisioning_head_invalid",
            ));
        }
        let expected_provision_head = economy_digest(
            COGNITION_PROVISIONING_JOURNAL_DOMAIN,
            &(self.provision_head_seq, &self.provision_journal),
        );
        if self.provision_head_digest != expected_provision_head {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_provisioning_head_digest_mismatch",
            ));
        }
        let mut previous_provision_digest = String::new();
        let mut provision_event_ids = BTreeSet::new();
        let mut provision_bindings = BTreeMap::new();
        for (provision_id, record) in &self.provisions {
            if provision_id != &record.request.provision_id {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_index_invalid",
                ));
            }
            record.validate()?;
            if provision_bindings
                .insert(record.request.binding_key(), provision_id)
                .is_some()
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_binding_duplicate",
                ));
            }
            let binding_key = record.request.binding_key();
            let versioned_balance = self
                .provisioned_balances
                .get(binding_key.as_str())
                .and_then(|resources| resources.get(record.request.resource.as_str()));
            let legacy_balance = self
                .balances
                .get(record.request.account_id.as_str())
                .and_then(|resources| resources.get(record.request.resource.as_str()));
            let matching_provisions = self
                .provisions
                .values()
                .filter(|candidate| {
                    candidate.request.account_id == record.request.account_id
                        && candidate.request.resource == record.request.resource
                })
                .count();
            if versioned_balance.is_some() && legacy_balance.is_some() {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_balance_ambiguous",
                ));
            }
            if legacy_balance.is_some() && matching_provisions != 1 {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_balance_ambiguous",
                ));
            }
            let balance = versioned_balance.or(legacy_balance);
            let Some(balance) = balance else {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_balance_missing",
                ));
            };
            if !matches!(
                balance.available.checked_add(balance.reserved),
                Some(total) if total <= record.request.allowance
            ) {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_balance_mismatch",
                ));
            }
            let Some(receipt) = self.provision_receipts.get(record.receipt_id.as_str()) else {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_receipt_missing",
                ));
            };
            if receipt.request != record.request || receipt.receipt_id != record.receipt_id {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_receipt_mismatch",
                ));
            }
        }
        for (lease_id, binding_key) in &self.lease_binding_keys {
            let Some(lease) = self.leases.get(lease_id) else {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_lease_binding_index_invalid",
                ));
            };
            let Some(request) = self.provision_for_binding(binding_key) else {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_lease_binding_index_invalid",
                ));
            };
            if request.account_id != lease.account_id
                || request.resource != lease.quote.resource
                || self
                    .provisioned_balances
                    .get(binding_key)
                    .and_then(|resources| resources.get(&lease.quote.resource))
                    .is_none()
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_lease_binding_index_invalid",
                ));
            }
        }
        for (receipt_id, receipt) in &self.provision_receipts {
            receipt.validate()?;
            let Some(record) = self.provisions.get(&receipt.request.provision_id) else {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_receipt_index_invalid",
                ));
            };
            if receipt_id != &receipt.receipt_id || record.receipt_id != receipt.receipt_id {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_receipt_index_invalid",
                ));
            }
        }
        for (index, event) in self.provision_journal.iter().enumerate() {
            event.validate()?;
            if event.journal_seq != index as u64 + 1
                || event.parent_event_digest != previous_provision_digest
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_journal_invalid",
                ));
            }
            let Some(record) = self.provisions.get(&event.request.provision_id) else {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_journal_record_missing",
                ));
            };
            if record.request != event.request || record.receipt_id != event.receipt_id {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_journal_record_mismatch",
                ));
            }
            if !provision_event_ids.insert(event.request.provision_id.clone()) {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_journal_cardinality_invalid",
                ));
            }
            previous_provision_digest.clone_from(&event.event_digest);
        }
        if self.provisions.len() != self.provision_journal.len()
            || provision_event_ids.len() != self.provisions.len()
            || self
                .provisions
                .keys()
                .any(|provision_id| !provision_event_ids.contains(provision_id))
        {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_provisioning_journal_cardinality_invalid",
            ));
        }
        let mut previous_digest = String::new();
        let mut event_counts: BTreeMap<String, (u8, u8, u8, u8, u8)> = BTreeMap::new();
        for (index, event) in self.journal.iter().enumerate() {
            if event.schema_version != COGNITION_ECONOMY_EVENT_SCHEMA_VERSION
                || event.journal_seq != index as u64 + 1
                || event.parent_event_digest != previous_digest
                || !valid_digest(&event.event_digest)
                || event.event_digest != event.recompute_digest()
                || !bounded_identity(&event.lease_id)
                || !bounded_identity(&event.operation_key)
                || !bounded_identity(&event.idempotency_key)
                || !bounded_identity(&event.resource)
                || ((event.event_kind != "reserve") && !valid_digest(&event.receipt_id))
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_journal_invalid",
                ));
            }
            let Some(lease) = self.leases.get(&event.lease_id) else {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_journal_lease_missing",
                ));
            };
            let counts = event_counts.entry(event.lease_id.clone()).or_default();
            match event.event_kind.as_str() {
                "reserve" => {
                    if counts.0 != 0
                        || counts.1 != 0
                        || counts.2 != 0
                        || counts.3 != 0
                        || counts.4 != 0
                    {
                        return Err(CognitionEconomyError::InvalidState(
                            "cognition_economy_journal_cardinality_invalid",
                        ));
                    }
                    counts.0 = 1;
                }
                "settle" => {
                    if counts.0 != 1
                        || counts.1 != 0
                        || counts.2 != 0
                        || counts.3 != 0
                        || counts.4 != 0
                    {
                        return Err(CognitionEconomyError::InvalidState(
                            "cognition_economy_journal_cardinality_invalid",
                        ));
                    }
                    counts.1 = 1;
                }
                "release" => {
                    if counts.0 != 1
                        || counts.1 != 0
                        || counts.2 != 0
                        || counts.3 != 0
                        || counts.4 != 0
                    {
                        return Err(CognitionEconomyError::InvalidState(
                            "cognition_economy_journal_cardinality_invalid",
                        ));
                    }
                    counts.2 = 1;
                }
                "expire" => {
                    if counts.0 != 1
                        || counts.1 != 0
                        || counts.2 != 0
                        || counts.3 != 0
                        || counts.4 != 0
                    {
                        return Err(CognitionEconomyError::InvalidState(
                            "cognition_economy_journal_cardinality_invalid",
                        ));
                    }
                    counts.3 = 1;
                }
                "refund" => {
                    if counts.0 != 1
                        || counts.1 != 1
                        || counts.2 != 0
                        || counts.3 != 0
                        || counts.4 != 0
                    {
                        return Err(CognitionEconomyError::InvalidState(
                            "cognition_economy_journal_cardinality_invalid",
                        ));
                    }
                    counts.4 = 1;
                }
                _ => {}
            }
            if event.idempotency_key != lease.idempotency_key
                || event.resource != lease.quote.resource
                || event.reserved_amount != lease.reserved_amount
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_journal_identity_invalid",
                ));
            }
            let (expected_status, expected_operation, expected_receipt) =
                match event.event_kind.as_str() {
                    "reserve" => (
                        CognitionLeaseStatusV1::Reserved,
                        operation_key(&lease.lease_id, "reserve"),
                        if event.receipt_id.is_empty() {
                            None
                        } else {
                            Some(event.receipt_id.as_str())
                        },
                    ),
                    "settle" => (
                        CognitionLeaseStatusV1::Settled,
                        operation_key(&lease.lease_id, "settle"),
                        Some(event.receipt_id.as_str()),
                    ),
                    "release" => (
                        CognitionLeaseStatusV1::Released,
                        operation_key(&lease.lease_id, "release"),
                        Some(event.receipt_id.as_str()),
                    ),
                    "expire" => (
                        CognitionLeaseStatusV1::Expired,
                        operation_key(&lease.lease_id, "expire"),
                        Some(event.receipt_id.as_str()),
                    ),
                    "refund" => (
                        CognitionLeaseStatusV1::Refunded,
                        operation_key(&lease.lease_id, "refund"),
                        Some(event.receipt_id.as_str()),
                    ),
                    _ => {
                        return Err(CognitionEconomyError::InvalidState(
                            "cognition_economy_journal_kind_invalid",
                        ));
                    }
                };
            if event.operation_key != expected_operation
                || (expected_receipt != Some(event.receipt_id.as_str())
                    && !event.receipt_id.is_empty())
                || event.status != expected_status
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_journal_transition_invalid",
                ));
            }
            if let Some(receipt_id) = expected_receipt {
                let Some(receipt) = self.receipts.get(receipt_id) else {
                    return Err(CognitionEconomyError::InvalidState(
                        "cognition_economy_journal_receipt_missing",
                    ));
                };
                if receipt.status != expected_status
                    || (!receipt.operation.is_empty() && receipt.operation != event.event_kind)
                    || receipt.consumed_amount != event.consumed_amount
                    || receipt.released_amount != event.released_amount
                    || receipt.refunded_amount != event.refunded_amount
                    || receipt.net_amount != event.net_amount
                    || receipt.parent_receipt_id != event.parent_receipt_id
                    || receipt.reason != event.reason
                    || !receipt_matches_lease(receipt, lease)
                {
                    return Err(CognitionEconomyError::InvalidState(
                        "cognition_economy_journal_receipt_invalid",
                    ));
                }
            } else if event.consumed_amount != 0
                || event.released_amount != 0
                || event.refunded_amount != 0
                || event.net_amount != 0
                || event.parent_receipt_id.is_some()
                || event.reason.is_some()
                || !event.receipt_id.is_empty()
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_journal_reserve_invalid",
                ));
            } else if self.operations.contains_key(&expected_operation)
                || self.receipts.values().any(|receipt| {
                    receipt.lease_id == lease.lease_id
                        && receipt_operation_name(receipt) == Some("reserve")
                })
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_journal_reserve_receipt_missing",
                ));
            }
            previous_digest.clone_from(&event.event_digest);
        }
        for (lease_id, lease) in &self.leases {
            let (reserve_count, settle_count, release_count, expire_count, refund_count) =
                event_counts.get(lease_id).copied().unwrap_or_default();
            let valid_cardinality = match lease.status {
                CognitionLeaseStatusV1::Reserved => {
                    (
                        reserve_count,
                        settle_count,
                        release_count,
                        expire_count,
                        refund_count,
                    ) == (1, 0, 0, 0, 0)
                }
                CognitionLeaseStatusV1::Settled => {
                    (
                        reserve_count,
                        settle_count,
                        release_count,
                        expire_count,
                        refund_count,
                    ) == (1, 1, 0, 0, 0)
                }
                CognitionLeaseStatusV1::Released => {
                    (
                        reserve_count,
                        settle_count,
                        release_count,
                        expire_count,
                        refund_count,
                    ) == (1, 0, 1, 0, 0)
                }
                CognitionLeaseStatusV1::Expired => {
                    (
                        reserve_count,
                        settle_count,
                        release_count,
                        expire_count,
                        refund_count,
                    ) == (1, 0, 0, 1, 0)
                }
                CognitionLeaseStatusV1::Refunded => {
                    reserve_count == 1
                        && settle_count == 1
                        && release_count == 0
                        && expire_count == 0
                        && refund_count == 1
                }
            };
            if !valid_cardinality {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_journal_cardinality_invalid",
                ));
            }
            lease.validate()?;
            let has_lease_balance =
                if let Some(binding_key) = self.lease_binding_keys.get(&lease.lease_id) {
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
            if !has_lease_balance {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_lease_balance_missing",
                ));
            }
            let Some(record) = self.idempotency.get(&lease.idempotency_key) else {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_lease_index_invalid",
                ));
            };
            if lease_id != &lease.lease_id
                || record.lease_id != lease.lease_id
                || record.request_fingerprint != lease_fingerprint(lease)
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_lease_index_invalid",
                ));
            }
            let reserve_operation_key = operation_key(&lease.lease_id, "reserve");
            let has_reserve_operation = self.operations.contains_key(&reserve_operation_key);
            let has_reserve_receipt = self.receipts.values().any(|receipt| {
                receipt.lease_id == lease.lease_id
                    && receipt_operation_name(receipt) == Some("reserve")
            });
            if lease.status == CognitionLeaseStatusV1::Reserved
                && ((has_reserve_operation || has_reserve_receipt) != lease.receipt_id.is_some())
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_lease_receipt_invalid",
                ));
            }
            if lease.status == CognitionLeaseStatusV1::Reserved
                && (has_reserve_operation || has_reserve_receipt)
            {
                let expected_reserve_receipt_id = economy_digest(
                    COGNITION_ECONOMY_RECEIPT_ID_DOMAIN,
                    &(lease.lease_id.as_str(), reserve_operation_key.as_str()),
                );
                if lease.receipt_id.as_deref() != Some(expected_reserve_receipt_id.as_str()) {
                    return Err(CognitionEconomyError::InvalidState(
                        "cognition_economy_lease_receipt_invalid",
                    ));
                }
            }
            if lease.status != CognitionLeaseStatusV1::Reserved {
                let Some(receipt_id) = lease.receipt_id.as_deref() else {
                    return Err(CognitionEconomyError::InvalidState(
                        "cognition_economy_lease_receipt_missing",
                    ));
                };
                if self.receipts.get(receipt_id).is_none_or(|receipt| {
                    receipt.status != lease.status || !receipt_matches_lease(receipt, lease)
                }) {
                    return Err(CognitionEconomyError::InvalidState(
                        "cognition_economy_lease_receipt_invalid",
                    ));
                }
            }
        }
        for (idempotency_key, record) in &self.idempotency {
            let Some(lease) = self.leases.get(&record.lease_id) else {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_idempotency_index_invalid",
                ));
            };
            if !bounded_identity(idempotency_key)
                || !valid_digest(&record.request_fingerprint)
                || idempotency_key != &lease.idempotency_key
                || record.request_fingerprint != lease_fingerprint(lease)
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_idempotency_index_invalid",
                ));
            }
        }
        for (receipt_id, receipt) in &self.receipts {
            receipt.validate()?;
            let Some(lease) = self.leases.get(&receipt.lease_id) else {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_receipt_index_invalid",
                ));
            };
            let operation_name = receipt_operation_name(receipt).ok_or(
                CognitionEconomyError::InvalidState("cognition_economy_receipt_status_invalid"),
            )?;
            let operation = operation_key(&receipt.lease_id, operation_name);
            let expected_receipt_id = economy_digest(
                COGNITION_ECONOMY_RECEIPT_ID_DOMAIN,
                &(receipt.lease_id.as_str(), operation.as_str()),
            );
            if receipt_id != &receipt.receipt_id
                || receipt.receipt_id != expected_receipt_id
                || !receipt_matches_lease(receipt, lease)
                || self.operations.get(&operation).is_none_or(|record| {
                    record.receipt_id != receipt.receipt_id || record.lease_id != receipt.lease_id
                })
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_receipt_index_invalid",
                ));
            }
        }
        for (stored_operation_key, operation) in &self.operations {
            let Some(receipt) = self.receipts.get(&operation.receipt_id) else {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_operation_index_invalid",
                ));
            };
            let operation_name = receipt_operation_name(receipt).ok_or(
                CognitionEconomyError::InvalidState("cognition_economy_receipt_status_invalid"),
            )?;
            let expected_digest = match operation_name {
                "settle" => economy_digest(
                    COGNITION_ECONOMY_OPERATION_DOMAIN,
                    &(stored_operation_key.as_str(), receipt.consumed_amount),
                ),
                "refund" if receipt.operation.is_empty() => {
                    economy_digest(COGNITION_ECONOMY_OPERATION_DOMAIN, stored_operation_key)
                }
                "refund" => economy_digest(
                    COGNITION_ECONOMY_OPERATION_DOMAIN,
                    &(
                        stored_operation_key.as_str(),
                        receipt.refunded_amount,
                        receipt.parent_receipt_id.as_deref(),
                        receipt.reason.as_deref(),
                    ),
                ),
                _ => economy_digest(COGNITION_ECONOMY_OPERATION_DOMAIN, stored_operation_key),
            };
            if stored_operation_key != &operation.operation_key
                || !valid_digest(stored_operation_key)
                || !valid_digest(&operation.operation_digest)
                || operation.operation_digest != expected_digest
                || operation.lease_id != receipt.lease_id
                || operation.receipt_id != receipt.receipt_id
                || stored_operation_key != &operation_key(&receipt.lease_id, operation_name)
            {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_operation_index_invalid",
                ));
            }
        }
        let mut expected_reserved: BTreeMap<(Option<String>, String, String), u64> =
            BTreeMap::new();
        for lease in self.leases.values() {
            if lease.status == CognitionLeaseStatusV1::Reserved {
                let key = (
                    self.lease_binding_keys.get(&lease.lease_id).cloned(),
                    lease.account_id.clone(),
                    lease.quote.resource.clone(),
                );
                let value = expected_reserved.entry(key).or_default();
                *value = value.checked_add(lease.reserved_amount).ok_or(
                    CognitionEconomyError::InvalidState("cognition_economy_reserved_overflow"),
                )?;
            }
        }
        for (account_id, resources) in &self.balances {
            for (resource, balance) in resources {
                if expected_reserved
                    .get(&(None, account_id.clone(), resource.clone()))
                    .copied()
                    .unwrap_or_default()
                    != balance.reserved
                {
                    return Err(CognitionEconomyError::InvalidState(
                        "cognition_economy_balance_reservation_mismatch",
                    ));
                }
            }
        }
        for ((binding_key, account_id, resource), reserved) in expected_reserved {
            let actual = if let Some(binding_key) = binding_key {
                self.provisioned_balances
                    .get(&binding_key)
                    .and_then(|resources| resources.get(&resource))
                    .map(|balance| balance.reserved)
                    .unwrap_or_default()
            } else {
                self.balances
                    .get(&account_id)
                    .and_then(|resources| resources.get(&resource))
                    .map(|balance| balance.reserved)
                    .unwrap_or_default()
            };
            if actual != reserved {
                return Err(CognitionEconomyError::InvalidState(
                    "cognition_economy_balance_reservation_missing",
                ));
            }
        }
        Ok(())
    }
}

fn receipt_operation_name(receipt: &CognitionReceiptV1) -> Option<&str> {
    if !receipt.operation.is_empty() {
        return Some(receipt.operation.as_str());
    }
    match receipt.status {
        CognitionLeaseStatusV1::Reserved => Some("reserve"),
        CognitionLeaseStatusV1::Settled => Some("settle"),
        CognitionLeaseStatusV1::Released => Some("release"),
        CognitionLeaseStatusV1::Expired => Some("expire"),
        CognitionLeaseStatusV1::Refunded => Some("refund"),
    }
}

fn lease_fingerprint(lease: &CognitionLeaseV1) -> String {
    CognitionLeaseRequestV1::new(
        lease.idempotency_key.clone(),
        lease.account_id.clone(),
        lease.agent_id.clone(),
        lease.agent_session_id.clone(),
        lease.agent_turn_id.clone(),
        lease.decision_request_id.clone(),
        lease.request_digest.clone(),
        lease.quote.clone(),
    )
    .fingerprint_digest()
}

fn receipt_matches_lease(receipt: &CognitionReceiptV1, lease: &CognitionLeaseV1) -> bool {
    let identity_matches = receipt.lease_id == lease.lease_id
        && receipt.idempotency_key == lease.idempotency_key
        && receipt.account_id == lease.account_id
        && receipt.agent_id == lease.agent_id
        && receipt.agent_session_id == lease.agent_session_id
        && receipt.agent_turn_id == lease.agent_turn_id
        && receipt.decision_request_id == lease.decision_request_id
        && receipt.request_digest == lease.request_digest
        && receipt.quote == lease.quote
        && receipt.reserved_amount == lease.reserved_amount;
    if !identity_matches {
        return false;
    }
    if receipt.operation.is_empty() {
        return match receipt.status {
            CognitionLeaseStatusV1::Settled => {
                matches!(
                    lease.status,
                    CognitionLeaseStatusV1::Settled | CognitionLeaseStatusV1::Refunded
                ) && receipt.consumed_amount == lease.settled_amount
                    && lease.reserved_amount.checked_sub(lease.settled_amount)
                        == Some(receipt.refunded_amount)
            }
            CognitionLeaseStatusV1::Released => {
                lease.status == CognitionLeaseStatusV1::Released
                    && receipt.consumed_amount == 0
                    && receipt.refunded_amount == lease.reserved_amount
            }
            CognitionLeaseStatusV1::Refunded => {
                lease.status == CognitionLeaseStatusV1::Refunded
                    && receipt.consumed_amount == 0
                    && receipt.refunded_amount == lease.settled_amount
            }
            _ => false,
        };
    }
    match receipt_operation_name(receipt) {
        Some("reserve") => {
            receipt.status == CognitionLeaseStatusV1::Reserved
                && receipt.consumed_amount == 0
                && receipt.released_amount == 0
                && receipt.refunded_amount == 0
                && receipt.net_amount == 0
        }
        Some("settle") => {
            matches!(
                lease.status,
                CognitionLeaseStatusV1::Settled | CognitionLeaseStatusV1::Refunded
            ) && receipt.status == CognitionLeaseStatusV1::Settled
                && receipt.consumed_amount == lease.settled_amount
                && receipt.released_amount == 0
                && lease.reserved_amount.checked_sub(lease.settled_amount)
                    == Some(receipt.refunded_amount)
                && receipt.net_amount == receipt.consumed_amount
        }
        Some("release") => {
            lease.status == CognitionLeaseStatusV1::Released
                && receipt.status == CognitionLeaseStatusV1::Released
                && receipt.consumed_amount == 0
                && receipt.released_amount == lease.reserved_amount
                && receipt.refunded_amount == 0
                && receipt.net_amount == 0
        }
        Some("expire") => {
            lease.status == CognitionLeaseStatusV1::Expired
                && receipt.status == CognitionLeaseStatusV1::Expired
                && receipt.consumed_amount == 0
                && receipt.released_amount == lease.reserved_amount
                && receipt.refunded_amount == 0
                && receipt.net_amount == 0
        }
        Some("refund") => {
            lease.status == CognitionLeaseStatusV1::Refunded
                && receipt.status == CognitionLeaseStatusV1::Refunded
                && receipt.consumed_amount == lease.settled_amount
                && receipt.released_amount == 0
                && receipt.refunded_amount == lease.compensated_amount
                && receipt.consumed_amount.checked_sub(receipt.refunded_amount)
                    == Some(receipt.net_amount)
        }
        _ => false,
    }
}
