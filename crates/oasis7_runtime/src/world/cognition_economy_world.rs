//! World persistence boundary for the typed cognition economy projection.

use super::super::error::WorldError;
use super::World;
use super::cognition_economy::{
    CognitionEconomyError, CognitionEconomyStateV1, CognitionLeaseRequestV1, CognitionLeaseV1,
    CognitionProvisioningReceiptV1, CognitionProvisioningRequestV1, CognitionReceiptV1,
};
use super::transition::{RootInstallContext, SubsystemDelta};
use crate::domains::monetary::{CognitionEconomyOutput, StagedCognitionEconomyOp};

fn world_economy_error(error: CognitionEconomyError) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: error.code().to_string(),
    }
}

impl World {
    pub(crate) fn install_cognition_economy_projection(&mut self, projection: serde_json::Value) {
        self.cognition["cognition_economy"] = projection;
    }

    fn execute_staged_cognition_economy(
        &mut self,
        op: StagedCognitionEconomyOp<'_>,
    ) -> Result<CognitionEconomyOutput, WorldError> {
        let (output, delta) = op.prepare_on_world(self, self.state.time)?;
        if self.persistence_dir.borrow().is_some() {
            let previous_cognition = self.cognition.clone();
            delta.apply_infallible(&mut RootInstallContext::new(self));
            if let Err(err) = self.persist_runtime_transaction_if_configured() {
                self.cognition = previous_cognition;
                return Err(err);
            }
        } else {
            delta.apply_infallible(&mut RootInstallContext::new(self));
        }
        Ok(output)
    }

    fn execute_staged_cognition_receipt(
        &mut self,
        op: StagedCognitionEconomyOp<'_>,
    ) -> Result<CognitionReceiptV1, WorldError> {
        match self.execute_staged_cognition_economy(op)? {
            CognitionEconomyOutput::Receipt(receipt) => Ok(receipt),
            _ => unreachable!("lease receipt operation produces Receipt output"),
        }
    }

    /// Read back and validate the Runtime-owned cognition economy projection.
    pub fn cognition_economy(&self) -> Result<CognitionEconomyStateV1, WorldError> {
        let value = self
            .cognition
            .get("cognition_economy")
            .cloned()
            .unwrap_or_else(|| {
                serde_json::to_value(CognitionEconomyStateV1::default())
                    .expect("default economy is serializable")
            });
        CognitionEconomyStateV1::from_snapshot_json(value).map_err(world_economy_error)
    }

    /// Explicit bootstrap balance setup. This is deliberately separate from
    /// scheduler capacity and does not refill an account implicitly.
    pub fn set_cognition_resource_balance(
        &mut self,
        account_id: impl Into<String>,
        resource: impl Into<String>,
        available: u64,
    ) -> Result<(), WorldError> {
        let (account_id, resource) = (account_id.into(), resource.into());
        self.execute_staged_cognition_economy(StagedCognitionEconomyOp::SetBalance(
            &account_id,
            &resource,
            available,
        ))
        .map(|_| ())
    }

    /// Install one Runtime-authorized cognition allowance for a live Agent's
    /// capability owner.  The owner binding and current world identity are
    /// derived from Runtime state so callers cannot fund an arbitrary account
    /// or carry an allowance across a reorg/generation boundary.
    pub fn provision_cognition_for_agent(
        &mut self,
        agent_id: &str,
        provision_id: impl Into<String>,
        authority_context: impl Into<String>,
        allowance: u64,
    ) -> Result<CognitionProvisioningReceiptV1, WorldError> {
        let (provision_id, authority_context) = (provision_id.into(), authority_context.into());
        match self.execute_staged_cognition_economy(StagedCognitionEconomyOp::ProvisionForAgent(
            agent_id,
            &provision_id,
            &authority_context,
            allowance,
        ))? {
            CognitionEconomyOutput::ProvisionReceipt(receipt) => Ok(receipt),
            _ => unreachable!("ProvisionForAgent produces ProvisionReceipt output"),
        }
    }

    pub(crate) fn build_cognition_provisioning_request(
        &self,
        agent_id: &str,
        provision_id: &str,
        authority_context: &str,
        allowance: u64,
    ) -> Result<CognitionProvisioningRequestV1, WorldError> {
        if !self.state.agents.contains_key(agent_id) {
            return Err(WorldError::DistributedValidationFailed {
                reason: "cognition provisioning requires a live agent".to_string(),
            });
        }
        let identity = self
            .current_agent_identity(agent_id)
            .ok_or_else(|| WorldError::DistributedValidationFailed {
                reason: "cognition provisioning requires a live capability identity".to_string(),
            })?
            .clone();
        let binding = self.current_cognition_runtime_binding()?;
        Ok(CognitionProvisioningRequestV1::new(
            provision_id,
            identity.owner_binding.clone(),
            identity.owner_binding,
            identity.generation,
            binding.world_id,
            binding.branch_id,
            binding.reorg_epoch,
            allowance,
            authority_context,
        ))
    }

    pub fn reserve_cognition_lease(
        &mut self,
        request: CognitionLeaseRequestV1,
    ) -> Result<CognitionLeaseV1, WorldError> {
        match self
            .execute_staged_cognition_economy(StagedCognitionEconomyOp::ReserveLease(&request))?
        {
            CognitionEconomyOutput::Lease(lease) => Ok(lease),
            _ => unreachable!("ReserveLease produces Lease output"),
        }
    }

    pub(crate) fn cognition_provisioning_binding_for_request(
        &self,
        request: &CognitionLeaseRequestV1,
        economy: &CognitionEconomyStateV1,
    ) -> Result<Option<String>, WorldError> {
        if !self
            .cognition
            .get("runtime_binding")
            .is_some_and(serde_json::Value::is_object)
        {
            return Ok(None);
        }
        let binding = self.current_cognition_runtime_binding()?;
        let identity = self
            .current_agent_identity(request.agent_id.as_str())
            .ok_or_else(|| {
                world_economy_error(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_identity_missing",
                ))
            })?;
        let Some(record) = economy.provisions.values().find(|record| {
            let provision = &record.request;
            provision.account_id == request.account_id
                && provision.owner_binding == identity.owner_binding
                && provision.owner_generation == identity.generation
                && provision.world_id == binding.world_id
                && provision.branch_id == binding.branch_id
                && provision.reorg_epoch == binding.reorg_epoch
                && provision.resource == request.quote.resource
        }) else {
            return Err(world_economy_error(CognitionEconomyError::Conflict(
                "cognition_provisioning_binding_required",
            )));
        };
        if request.quote.world_binding != binding.base_world_hash.to_string() {
            return Err(world_economy_error(CognitionEconomyError::Conflict(
                "cognition_provisioning_binding_mismatch",
            )));
        }
        Ok(Some(record.request.binding_key()))
    }

    pub fn settle_cognition_lease(
        &mut self,
        lease_id: &str,
        consumed_amount: u64,
    ) -> Result<CognitionReceiptV1, WorldError> {
        self.execute_staged_cognition_receipt(StagedCognitionEconomyOp::SettleLease(
            lease_id,
            consumed_amount,
        ))
    }

    pub fn release_cognition_lease(
        &mut self,
        lease_id: &str,
    ) -> Result<CognitionReceiptV1, WorldError> {
        self.execute_staged_cognition_receipt(StagedCognitionEconomyOp::ReleaseLease(lease_id))
    }

    pub fn expire_cognition_lease(
        &mut self,
        lease_id: &str,
    ) -> Result<CognitionReceiptV1, WorldError> {
        self.execute_staged_cognition_receipt(StagedCognitionEconomyOp::ExpireLease(lease_id))
    }

    pub fn refund_cognition_lease(
        &mut self,
        lease_id: &str,
    ) -> Result<CognitionReceiptV1, WorldError> {
        self.execute_staged_cognition_receipt(StagedCognitionEconomyOp::RefundLease(lease_id))
    }

    pub fn refund_settled_cognition_lease(
        &mut self,
        lease_id: &str,
        refunded_amount: u64,
        parent_receipt_id: &str,
        reason: &str,
    ) -> Result<CognitionReceiptV1, WorldError> {
        self.execute_staged_cognition_receipt(StagedCognitionEconomyOp::RefundSettledLease(
            lease_id,
            refunded_amount,
            parent_receipt_id,
            reason,
        ))
    }
}
