//! Complete backend-only sealed namespace view. Public Current replies carry
//! only a bounded latest anchor; this full image never enters the signer wire.
use super::binding::{BftSignerErrorV3, invalid};
use super::private_retention::{
    BftPrivateRetentionAttestorV1, BftPrivateRetentionEntryV1, BftPrivateRetentionFactV1,
    BftPrivateRetentionKeyV1,
};
use super::private_retention_backend::{
    BftPrivateRetentionCurrentV1, BftPrivateRetentionInputInventoryV1, validate_input_inventory,
};
use super::private_retention_seal::{
    BftPrivateRetentionSealV1, ValidatedBftPrivateRetentionSealChainV1,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq)]
pub struct BftPrivateRetentionSealedCurrentV1 {
    pub current: BftPrivateRetentionCurrentV1,
    pub seals: BTreeMap<String, Arc<BftPrivateRetentionSealV1>>,
    /// One immutable canonical original per admission digest. Carried active
    /// inputs retain this same original; retired inputs are never removed.
    pub sealed_inputs: BftPrivateRetentionInputInventoryV1,
    /// Every original Reserve/Ready/Seal receipt, keyed by original receipt
    /// digest. Values share the backend's original-operation snapshot ownership.
    pub history: super::private_retention_history::BftPrivateRetentionHistoryV1,
}
impl BftPrivateRetentionSealedCurrentV1 {
    pub fn latest_seal(&self) -> Result<Option<&Arc<BftPrivateRetentionSealV1>>, BftSignerErrorV3> {
        let digest = self
            .current
            .retention
            .as_ref()
            .and_then(|ledger| ledger.map.previous_seal_receipt_digest.as_ref());
        digest
            .map(|digest| {
                self.seals
                    .get(digest)
                    .ok_or_else(|| invalid("current original seal anchor missing"))
            })
            .transpose()
    }
    fn ordered_chain(&self) -> Result<Vec<Arc<BftPrivateRetentionSealV1>>, BftSignerErrorV3> {
        let mut digest = self
            .current
            .retention
            .as_ref()
            .and_then(|ledger| ledger.map.previous_seal_receipt_digest.clone());
        let mut visited = BTreeSet::new();
        let mut chain = Vec::new();
        while let Some(id) = digest {
            if !visited.insert(id.clone()) {
                return Err(invalid("original sealed chain cycle"));
            }
            let seal = self
                .seals
                .get(&id)
                .ok_or_else(|| invalid("original sealed chain gap"))?;
            if seal.body.digest()? != id {
                return Err(invalid("original sealed index identity differs"));
            }
            digest = seal.body.original.map.previous_seal_receipt_digest.clone();
            chain.push(seal.clone());
        }
        if visited.len() != self.seals.len() {
            return Err(invalid(
                "original sealed index contains unenumerated segment",
            ));
        }
        chain.reverse();
        Ok(chain)
    }
    /// Must follow the same-backend exact current-lease read. This validates the
    /// complete historical inventory; it supplies no World or private ReadGrant.
    pub fn validate_inventory(
        &self,
        binding: &super::binding::BftSignerBindingV3,
        trusted: &BftPrivateRetentionAttestorV1,
    ) -> Result<(), BftSignerErrorV3> {
        self.validate_inventory_with_verified_state(binding, trusted, None)
    }
    pub(crate) fn validate_inventory_with_verified_state(
        &self,
        binding: &super::binding::BftSignerBindingV3,
        trusted: &BftPrivateRetentionAttestorV1,
        verified: Option<&super::state::VerifiedBftSignerStateV3>,
    ) -> Result<(), BftSignerErrorV3> {
        if verified.is_some_and(|verified| verified.state != self.current.signer.state) {
            return Err(invalid(
                "sealed inventory verified State differs from current snapshot",
            ));
        }
        let Some(current) = &self.current.retention else {
            if self.current.signer.state.private_retention.is_some()
                || !self.current.inputs.is_empty()
                || !self.seals.is_empty()
                || !self.sealed_inputs.is_empty()
                || !self.history.is_empty()
            {
                return Err(invalid("uncovered namespace has orphan sealed material"));
            }
            return Ok(());
        };
        let receipt = &current.receipt.body;
        if self.current.signer.state.private_retention.as_ref() != Some(&receipt.head)
            || self.current.signer.state.revision < receipt.after_revision
            || self.current.signer.lease_generation < receipt.original_lease_generation
            || (self.current.signer.state.revision == receipt.after_revision
                && self.current.signer.state.state_digest != receipt.after_state_digest)
        {
            return Err(invalid("current original sealed State/head mismatch"));
        }
        validate_input_inventory(current, &self.current.inputs)?;
        let chain = self.ordered_chain()?;
        ValidatedBftPrivateRetentionSealChainV1::verify(trusted, chain, current)?;
        super::private_retention_history::validate_original_history_with_active_context(
            binding,
            trusted,
            current,
            &self.history,
            &self.seals,
            verified.and_then(|v| v.active_context.clone()),
        )?;
        let mut originals = BTreeMap::<&str, &BftPrivateRetentionEntryV1>::new();
        for seal in self.seals.values() {
            for entry in &seal.body.original.map.entries {
                if let BftPrivateRetentionFactV1::Some {
                    admission_digest, ..
                } = &entry.fact
                {
                    if let Some(previous) = originals.insert(admission_digest, entry) {
                        if previous != entry {
                            return Err(invalid("sealed original entry changed across carry"));
                        }
                    }
                    let input = self
                        .sealed_inputs
                        .get(admission_digest)
                        .ok_or_else(|| invalid("sealed original input missing"))?;
                    if input.source != seal.body.original.map.source
                        || input.key != entry.key
                        || input.fact()? != entry.fact
                    {
                        return Err(invalid("sealed input differs from original map fact"));
                    }
                }
            }
        }
        if originals.len() != self.sealed_inputs.len() {
            return Err(invalid(
                "sealed input inventory contains unenumerated original",
            ));
        }
        for (digest, input) in &self.current.inputs {
            if let Some(original) = self.sealed_inputs.get(digest) {
                if input.as_ref() != original.as_ref() {
                    return Err(invalid("active carry original input changed"));
                }
            }
        }
        Ok(())
    }
    pub fn original_entry(
        &self,
        key: &BftPrivateRetentionKeyV1,
    ) -> Result<Option<&BftPrivateRetentionEntryV1>, BftSignerErrorV3> {
        let mut found = None;
        for ledger in self
            .current
            .retention
            .iter()
            .chain(self.seals.values().map(|seal| &seal.body.original))
        {
            if let Ok(index) = ledger
                .map
                .entries
                .binary_search_by(|entry| entry.key.cmp(key))
            {
                let entry = &ledger.map.entries[index];
                if found.is_some_and(|previous| previous != entry) {
                    return Err(invalid("original entry changed in sealed namespace union"));
                }
                found = Some(entry);
            }
        }
        Ok(found)
    }
    pub fn original_input(
        &self,
        digest: &str,
    ) -> Option<&Arc<super::private_retention_input::BftPrivateRetentionAdmissionInputV1>> {
        self.current
            .inputs
            .get(digest)
            .or_else(|| self.sealed_inputs.get(digest))
    }
}
