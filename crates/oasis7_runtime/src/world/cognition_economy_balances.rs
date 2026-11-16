use super::*;

impl CognitionEconomyStateV1 {
    pub fn available_balance(&self, account_id: &str, resource: &str) -> u64 {
        let legacy = self
            .balances
            .get(account_id)
            .and_then(|resources| resources.get(resource))
            .map(|balance| balance.available)
            .unwrap_or_default();
        self.provisioned_balances
            .iter()
            .filter_map(|(binding_key, resources)| {
                self.provision_for_binding(binding_key).and_then(|request| {
                    (request.account_id == account_id && request.resource == resource)
                        .then(|| resources.get(resource))
                        .flatten()
                })
            })
            .fold(legacy, |total, balance| {
                total.saturating_add(balance.available)
            })
    }

    pub fn reserved_balance(&self, account_id: &str, resource: &str) -> u64 {
        let legacy = self
            .balances
            .get(account_id)
            .and_then(|resources| resources.get(resource))
            .map(|balance| balance.reserved)
            .unwrap_or_default();
        self.provisioned_balances
            .iter()
            .filter_map(|(binding_key, resources)| {
                self.provision_for_binding(binding_key).and_then(|request| {
                    (request.account_id == account_id && request.resource == resource)
                        .then(|| resources.get(resource))
                        .flatten()
                })
            })
            .fold(legacy, |total, balance| {
                total.saturating_add(balance.reserved)
            })
    }

    pub(super) fn balance_mut(
        &mut self,
        account_id: &str,
        resource: &str,
    ) -> &mut CognitionResourceBalanceV1 {
        self.balances
            .entry(account_id.to_string())
            .or_default()
            .entry(resource.to_string())
            .or_default()
    }

    pub(super) fn provision_for_binding(&self,binding_key:&str)->Option<&CognitionProvisioningRequestV1>{
        self.provision_for_binding_with_work_v1(binding_key,&mut LegacyEconomyWorkV1)
            .expect("cognition payload must be canonically encodable")
    }
    pub(super) fn provision_for_binding_with_work_v1<O:CognitionEconomyWorkOpsV1>(
        &self,binding_key:&str,ops:&mut O,
    )->Result<Option<&CognitionProvisioningRequestV1>,CognitionEconomyError>{
        for record in self.provisions.values(){
            ops.work(1,binding_key.len())?;
            if record.request.binding_key_with_work_v1(ops)?==binding_key{return Ok(Some(&record.request))}
        }
        Ok(None)
    }

    pub(super) fn provisioned_balance_mut(
        &mut self,
        binding_key: &str,
        account_id: &str,
        resource: &str,
    ) -> Result<&mut CognitionResourceBalanceV1, CognitionEconomyError> {
        let request =
            self.provision_for_binding(binding_key)
                .ok_or(CognitionEconomyError::InvalidState(
                    "cognition_provisioning_binding_missing",
                ))?;
        if request.account_id != account_id || request.resource != resource {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_provisioning_binding_mismatch",
            ));
        }
        self.provisioned_balances
            .get_mut(binding_key)
            .and_then(|resources| resources.get_mut(resource))
            .ok_or(CognitionEconomyError::InvalidState(
                "cognition_provisioning_balance_missing",
            ))
    }

    pub(super) fn migrate_legacy_balance_for_binding(
        &mut self,
        binding_key: &str,
    ) -> Result<(), CognitionEconomyError> {
        let Some(request) = self.provision_for_binding(binding_key).cloned() else {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_provisioning_binding_missing",
            ));
        };
        if self
            .provisioned_balances
            .get(binding_key)
            .and_then(|resources| resources.get(request.resource.as_str()))
            .is_some()
        {
            return Ok(());
        }
        let Some(legacy_balance) = self
            .balances
            .get_mut(request.account_id.as_str())
            .and_then(|resources| resources.remove(request.resource.as_str()))
        else {
            return Ok(());
        };
        if self
            .balances
            .get(request.account_id.as_str())
            .is_some_and(BTreeMap::is_empty)
        {
            self.balances.remove(request.account_id.as_str());
        }
        self.provisioned_balances
            .entry(binding_key.to_string())
            .or_default()
            .insert(request.resource.clone(), legacy_balance);
        let lease_ids: Vec<String> = self
            .leases
            .values()
            .filter(|lease| {
                lease.account_id == request.account_id
                    && lease.quote.resource == request.resource
                    && !self.lease_binding_keys.contains_key(&lease.lease_id)
            })
            .map(|lease| lease.lease_id.clone())
            .collect();
        for lease_id in lease_ids {
            self.lease_binding_keys
                .insert(lease_id, binding_key.to_string());
        }
        Ok(())
    }

    pub(super) fn lease_balance_mut(
        &mut self,
        lease: &CognitionLeaseV1,
    ) -> Result<&mut CognitionResourceBalanceV1, CognitionEconomyError> {
        if let Some(binding_key) = self.lease_binding_keys.get(&lease.lease_id).cloned() {
            return self.provisioned_balance_mut(
                binding_key.as_str(),
                lease.account_id.as_str(),
                lease.quote.resource.as_str(),
            );
        }
        Ok(self.balance_mut(&lease.account_id, &lease.quote.resource))
    }
}
