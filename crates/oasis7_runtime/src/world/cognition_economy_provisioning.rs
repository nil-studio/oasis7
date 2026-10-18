use super::*;
use serde::{Deserialize, Serialize};

/// A Runtime-authorized, one-time allowance for one capability owner.
///
/// The request is intentionally independent of provider transport. Runtime
/// fills the owner binding and world identity from its live authority view;
/// the caller contributes only the durable authority context and allowance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionProvisioningRequestV1 {
    pub schema_version: String,
    pub provision_id: String,
    pub account_id: String,
    pub owner_binding: String,
    pub owner_generation: u64,
    pub world_id: String,
    pub branch_id: String,
    pub reorg_epoch: u64,
    pub resource: String,
    pub resource_version: String,
    pub policy_revision: String,
    pub allowance: u64,
    pub authority_context: String,
    pub authority_digest: String,
    pub provisioning_digest: String,
}

type CognitionAuthorityDigestPayload<'a> = (
    &'a str,
    &'a str,
    &'a str,
    u64,
    &'a str,
    &'a str,
    u64,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
);

type CognitionProvisioningDigestPayload<'a> = (
    &'a str,
    &'a str,
    &'a str,
    u64,
    &'a str,
    &'a str,
    u64,
    &'a str,
    &'a str,
    u64,
    &'a str,
    &'a str,
);

impl CognitionProvisioningRequestV1 {
    #[expect(
        clippy::too_many_arguments,
        reason = "Provisioning identity fields define the stable digest input and are kept explicit for compatibility."
    )]
    pub fn new(
        provision_id: impl Into<String>,
        account_id: impl Into<String>,
        owner_binding: impl Into<String>,
        owner_generation: u64,
        world_id: impl Into<String>,
        branch_id: impl Into<String>,
        reorg_epoch: u64,
        allowance: u64,
        authority_context: impl Into<String>,
    ) -> Self {
        let mut request = Self {
            schema_version: COGNITION_PROVISIONING_SCHEMA_VERSION.to_string(),
            provision_id: provision_id.into(),
            account_id: account_id.into(),
            owner_binding: owner_binding.into(),
            owner_generation,
            world_id: world_id.into(),
            branch_id: branch_id.into(),
            reorg_epoch,
            resource: "cognition_units".to_string(),
            resource_version: COGNITION_RESOURCE_VERSION_V1.to_string(),
            policy_revision: COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION.to_string(),
            allowance,
            authority_context: authority_context.into(),
            authority_digest: String::new(),
            provisioning_digest: String::new(),
        };
        request.refresh_digests();
        request
    }

    fn authority_digest_payload(&self) -> CognitionAuthorityDigestPayload<'_> {
        (
            &self.account_id,
            &self.owner_binding,
            &self.world_id,
            self.owner_generation,
            &self.branch_id,
            &self.resource,
            self.reorg_epoch,
            &self.resource_version,
            &self.policy_revision,
            &self.authority_context,
            &self.provision_id,
        )
    }

    fn provisioning_digest_payload(&self) -> CognitionProvisioningDigestPayload<'_> {
        (
            &self.provision_id,
            &self.account_id,
            &self.owner_binding,
            self.owner_generation,
            &self.world_id,
            &self.branch_id,
            self.reorg_epoch,
            &self.resource,
            &self.resource_version,
            self.allowance,
            &self.policy_revision,
            &self.authority_digest,
        )
    }

    pub fn recompute_authority_digest(&self) -> String {
        economy_digest(
            COGNITION_PROVISIONING_AUTHORITY_DOMAIN,
            &self.authority_digest_payload(),
        )
    }

    pub fn recompute_provisioning_digest(&self) -> String {
        economy_digest(
            COGNITION_PROVISIONING_DIGEST_DOMAIN,
            &self.provisioning_digest_payload(),
        )
    }

    pub fn refresh_digests(&mut self) {
        self.authority_digest = self.recompute_authority_digest();
        self.provisioning_digest = self.recompute_provisioning_digest();
    }

    pub fn validate(&self) -> Result<(), CognitionEconomyError> {
        if self.schema_version != COGNITION_PROVISIONING_SCHEMA_VERSION
            || !bounded_identity(&self.provision_id)
            || !bounded_identity(&self.account_id)
            || !bounded_identity(&self.owner_binding)
            || self.owner_generation == 0
            || self.account_id != self.owner_binding
            || !bounded_identity(&self.world_id)
            || !bounded_identity(&self.branch_id)
            || self.resource != "cognition_units"
            || self.resource_version != COGNITION_RESOURCE_VERSION_V1
            || self.policy_revision != COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION
            || self.allowance == 0
            || !bounded_identity(&self.authority_context)
            || !valid_digest(&self.authority_digest)
            || self.authority_digest != self.recompute_authority_digest()
            || !valid_digest(&self.provisioning_digest)
            || self.provisioning_digest != self.recompute_provisioning_digest()
        {
            return Err(CognitionEconomyError::InvalidInput(
                "cognition_provisioning_request_invalid",
            ));
        }
        Ok(())
    }

    pub(crate) fn binding_key(&self) -> String {
        economy_digest(
            COGNITION_PROVISIONING_DIGEST_DOMAIN,
            &(
                &self.account_id,
                &self.owner_binding,
                self.owner_generation,
                &self.world_id,
                &self.branch_id,
                self.reorg_epoch,
                &self.resource,
                &self.resource_version,
                &self.policy_revision,
            ),
        )
    }
}

/// Durable record proving that a capability owner received one allowance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionProvisioningRecordV1 {
    pub request: CognitionProvisioningRequestV1,
    pub receipt_id: String,
}

impl CognitionProvisioningRecordV1 {
    pub(super) fn validate(&self) -> Result<(), CognitionEconomyError> {
        self.request.validate()?;
        let expected_receipt_id = economy_digest(
            COGNITION_PROVISIONING_RECEIPT_ID_DOMAIN,
            &(
                &self.request.provision_id,
                &self.request.provisioning_digest,
            ),
        );
        if !valid_digest(&self.receipt_id) || self.receipt_id != expected_receipt_id {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_provisioning_receipt_id_invalid",
            ));
        }
        Ok(())
    }
}

/// Durable receipt for the one-time provisioning operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionProvisioningReceiptV1 {
    pub schema_version: String,
    pub receipt_id: String,
    pub receipt_digest: String,
    pub request: CognitionProvisioningRequestV1,
    pub issued_at_tick: u64,
}

impl CognitionProvisioningReceiptV1 {
    fn recompute_receipt_id(&self) -> String {
        economy_digest(
            COGNITION_PROVISIONING_RECEIPT_ID_DOMAIN,
            &(
                &self.request.provision_id,
                &self.request.provisioning_digest,
            ),
        )
    }

    pub fn recompute_digest(&self) -> String {
        let mut value = serde_json::to_value(self).expect("provisioning receipt is serializable");
        value
            .as_object_mut()
            .expect("provisioning receipt is an object")
            .remove("receipt_digest");
        economy_digest(COGNITION_PROVISIONING_RECEIPT_DOMAIN, &value)
    }

    pub(super) fn validate(&self) -> Result<(), CognitionEconomyError> {
        if self.schema_version != COGNITION_PROVISIONING_RECEIPT_SCHEMA_VERSION
            || !valid_digest(&self.receipt_id)
            || !valid_digest(&self.receipt_digest)
            || self.receipt_id != self.recompute_receipt_id()
            || self.receipt_digest != self.recompute_digest()
        {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_provisioning_receipt_invalid",
            ));
        }
        self.request.validate()
    }
}

/// Hash-chained journal evidence for one provisioning operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionProvisioningEventV1 {
    pub schema_version: String,
    pub journal_seq: u64,
    pub parent_event_digest: String,
    pub event_kind: String,
    pub request: CognitionProvisioningRequestV1,
    pub receipt_id: String,
    pub event_digest: String,
}

impl CognitionProvisioningEventV1 {
    fn recompute_digest(&self) -> String {
        let mut value = serde_json::to_value(self).expect("provisioning event is serializable");
        value
            .as_object_mut()
            .expect("provisioning event is an object")
            .remove("event_digest");
        economy_digest(COGNITION_PROVISIONING_EVENT_DOMAIN, &value)
    }

    pub(super) fn validate(&self) -> Result<(), CognitionEconomyError> {
        if self.schema_version != COGNITION_PROVISIONING_EVENT_SCHEMA_VERSION
            || self.journal_seq == 0
            || (self.journal_seq > 1 && !valid_digest(&self.parent_event_digest))
            || self.event_kind != "provision"
            || self.receipt_id
                != economy_digest(
                    COGNITION_PROVISIONING_RECEIPT_ID_DOMAIN,
                    &(
                        &self.request.provision_id,
                        &self.request.provisioning_digest,
                    ),
                )
            || !valid_digest(&self.event_digest)
            || self.event_digest != self.recompute_digest()
        {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_provisioning_event_invalid",
            ));
        }
        self.request.validate()
    }
}

impl CognitionEconomyStateV1 {
    /// Install one explicit allowance for a capability owner. Replaying the
    /// exact authority request returns its original receipt without changing
    /// the balance or appending another journal event.
    pub fn provision(
        &mut self,
        request: CognitionProvisioningRequestV1,
        tick: u64,
    ) -> Result<CognitionProvisioningReceiptV1, CognitionEconomyError> {
        request.validate()?;
        let mut next = self.clone();
        let receipt = next.provision_inner(request, tick)?;
        next.validate()?;
        *self = next;
        Ok(receipt)
    }

    fn provision_inner(
        &mut self,
        request: CognitionProvisioningRequestV1,
        tick: u64,
    ) -> Result<CognitionProvisioningReceiptV1, CognitionEconomyError> {
        if let Some(existing) = self.provisions.get(&request.provision_id) {
            if existing.request != request {
                return Err(CognitionEconomyError::Conflict(
                    "cognition_provisioning_idempotency_conflict",
                ));
            }
            return self
                .provision_receipts
                .get(existing.receipt_id.as_str())
                .cloned()
                .ok_or(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_receipt_missing",
                ));
        }

        // A capability owner receives one allowance for one world identity.
        // This also makes two agents sharing an owner account converge on the
        // same provision instead of silently refilling the account.
        let binding_key = request.binding_key();
        if self
            .provisions
            .values()
            .any(|existing| existing.request.binding_key() == binding_key)
        {
            return Err(CognitionEconomyError::Conflict(
                "cognition_provisioning_binding_conflict",
            ));
        }

        if self
            .balances
            .get(request.account_id.as_str())
            .and_then(|resources| resources.get(request.resource.as_str()))
            .is_some()
        {
            // Snapshots written before versioned provisioning stored the
            // allowance in the account-wide legacy map. Move that allowance
            // to its immutable historical binding before installing the new
            // generation/reorg binding. Existing leases are moved with it so
            // terminal transitions still settle against their original fund.
            let legacy_binding = self
                .provisions
                .values()
                .find(|existing| {
                    existing.request.account_id == request.account_id
                        && existing.request.resource == request.resource
                })
                .map(|existing| existing.request.binding_key());
            let Some(legacy_binding) = legacy_binding else {
                return Err(CognitionEconomyError::Conflict(
                    "cognition_provisioning_balance_already_initialized",
                ));
            };
            let legacy_balance = self
                .balances
                .get_mut(request.account_id.as_str())
                .and_then(|resources| resources.remove(request.resource.as_str()))
                .expect("legacy balance checked above");
            if self
                .balances
                .get(request.account_id.as_str())
                .is_some_and(BTreeMap::is_empty)
            {
                self.balances.remove(request.account_id.as_str());
            }
            self.provisioned_balances
                .entry(legacy_binding.clone())
                .or_default()
                .insert(request.resource.clone(), legacy_balance);
            for lease in self.leases.values() {
                if lease.account_id == request.account_id
                    && lease.quote.resource == request.resource
                    && !self.lease_binding_keys.contains_key(&lease.lease_id)
                {
                    self.lease_binding_keys
                        .insert(lease.lease_id.clone(), legacy_binding.clone());
                }
            }
        }

        self.provisioned_balances
            .entry(binding_key)
            .or_default()
            .insert(
                request.resource.clone(),
                CognitionResourceBalanceV1 {
                    available: request.allowance,
                    reserved: 0,
                },
            );
        let receipt_id = economy_digest(
            COGNITION_PROVISIONING_RECEIPT_ID_DOMAIN,
            &(&request.provision_id, &request.provisioning_digest),
        );
        let mut receipt = CognitionProvisioningReceiptV1 {
            schema_version: COGNITION_PROVISIONING_RECEIPT_SCHEMA_VERSION.to_string(),
            receipt_id: receipt_id.clone(),
            receipt_digest: String::new(),
            request: request.clone(),
            issued_at_tick: tick,
        };
        receipt.receipt_digest = receipt.recompute_digest();
        receipt.validate()?;
        self.provisions.insert(
            request.provision_id.clone(),
            CognitionProvisioningRecordV1 {
                request: request.clone(),
                receipt_id: receipt_id.clone(),
            },
        );
        self.provision_receipts
            .insert(receipt_id.clone(), receipt.clone());
        let journal_seq =
            self.provision_head_seq
                .checked_add(1)
                .ok_or(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_journal_overflow",
                ))?;
        let mut event = CognitionProvisioningEventV1 {
            schema_version: COGNITION_PROVISIONING_EVENT_SCHEMA_VERSION.to_string(),
            journal_seq,
            parent_event_digest: if journal_seq == 1 {
                String::new()
            } else {
                self.provision_journal
                    .last()
                    .map(|event| event.event_digest.clone())
                    .unwrap_or_default()
            },
            event_kind: "provision".to_string(),
            request,
            receipt_id,
            event_digest: String::new(),
        };
        event.event_digest = event.recompute_digest();
        event.validate()?;
        self.provision_head_seq = journal_seq;
        self.provision_journal.push(event);
        self.provision_head_digest = economy_digest(
            COGNITION_PROVISIONING_JOURNAL_DOMAIN,
            &(self.provision_head_seq, &self.provision_journal),
        );
        Ok(receipt)
    }
}
