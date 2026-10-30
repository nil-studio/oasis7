//! Window-local candidate WAL, preserving the original Node instance identity.
use super::super::successor_verifier::VerifiedPrivateSuccessorBaseV1;
use super::super::window_storage::WindowLedger;
use super::*;
use std::collections::BTreeMap;

pub(crate) struct WindowPrivateStore(NodePrivateStore);
impl WindowPrivateStore {
    pub(crate) fn open(
        directory: PathBuf,
        store: LocalCasStore,
        base: &VerifiedPrivateSuccessorBaseV1,
        ledger: &mut WindowLedger,
    ) -> Result<Self, String> {
        let source: Arc<dyn BusinessIndexNodeReader + Send + Sync> =
            base.execution.retained_nodes.clone();
        let accounting_store = store.clone();
        let (mut private, _) = NodePrivateStore::open_window(
            directory,
            store,
            source,
            &base.metadata.private_instance_id,
        )?;
        // Parent dependencies remain in the verified archive capability; the
        // new segment's original WAL inventory starts with its own material.
        let nodes = BTreeMap::new();
        private.bind_origin(
            &ledger.origin_digest()?,
            &base.metadata.source.material_identity,
            base.metadata.cut.height,
            &nodes,
            &mut |bytes, future| {
                ledger.reserve_private_material(bytes, future.clone(), &accounting_store)
            },
        )?;
        for (roles, package) in &base.pending {
            let nodes = package.dependency_nodes.iter().cloned().collect();
            let future = private.publication_promises()?;
            if roles.contains(&super::super::successor_archive::PendingRoleV1::RetainedLocal) {
                private.retain(&package.candidate, &nodes, &mut |bytes| {
                    ledger.reserve_private_material(bytes, future.clone(), &accounting_store)
                })?;
            } else {
                private.retain_received(&package.candidate, &nodes, &mut |bytes| {
                    ledger.reserve_private_material(bytes, future.clone(), &accounting_store)
                })?;
            }
        }
        ledger.reserve_private_material(
            private.usage()?,
            private.publication_promises()?,
            &accounting_store,
        )?;
        Ok(Self(private))
    }
    pub(crate) fn verify_ready(
        directory: PathBuf,
        store: LocalCasStore,
        base: &VerifiedPrivateSuccessorBaseV1,
        origin: &super::super::window_storage::WindowOrigin,
        initial: bool,
    ) -> Result<(), String> {
        let source: Arc<dyn BusinessIndexNodeReader + Send + Sync> =
            base.execution.retained_nodes.clone();
        let (mut private, _) = NodePrivateStore::open_window(
            directory,
            store,
            source,
            &base.metadata.private_instance_id,
        )?;
        private.verify_original_binding(
            &origin.digest()?,
            &base.metadata.source.material_identity,
            base.metadata.cut.height,
        )?;
        private.load_inventory(
            &origin.digest()?,
            &base.metadata.source.material_identity,
            base.metadata.cut.height,
        )?;
        private.verify_inventory_pins()?;
        if initial {
            private.require_completed_inventory()?;
        }
        for (_, package) in &base.pending {
            if private.export(&package.candidate.public_value)?.candidate != package.candidate {
                return Err("active original pending candidate changed".into());
            }
        }
        if initial && private.inventory_count() != base.pending.len() {
            return Err("active preparation contains unproved private candidates".into());
        }
        Ok(())
    }
    pub(crate) fn candidate(
        &self,
        height: u64,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        self.0.candidate(height)
    }
    pub(crate) fn candidate_for_value(
        &self,
        value: &BftHeaderValueV3,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        self.0.candidate_for_value(value)
    }
    pub(crate) fn retain(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        nodes: &BTreeMap<String, Vec<u8>>,
        ledger: &mut WindowLedger,
    ) -> Result<String, String> {
        let future = self.0.publication_promises()?;
        let store = self.0.accounting_store();
        self.0.retain(candidate, nodes, &mut |bytes| {
            ledger.reserve_private_material(bytes, future.clone(), &store)
        })
    }
    pub(crate) fn retain_received(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        nodes: &BTreeMap<String, Vec<u8>>,
        ledger: &mut WindowLedger,
    ) -> Result<String, String> {
        let future = self.0.publication_promises()?;
        let store = self.0.accounting_store();
        self.0.retain_received(candidate, nodes, &mut |bytes| {
            ledger.reserve_private_material(bytes, future.clone(), &store)
        })
    }
    pub(crate) fn public_value(
        &self,
        height: u64,
        id: &str,
    ) -> Result<Option<BftHeaderValueV3>, String> {
        self.0.public_value(height, id)
    }
    pub(crate) fn package_for_header(
        &self,
        height: u64,
        id: &str,
    ) -> Result<NodeBftPrivateCandidatePackageV3, String> {
        let value = self
            .0
            .public_value(height, id)?
            .ok_or("active original candidate header absent")?;
        self.0.export(&value)
    }
    pub(crate) fn export(
        &self,
        value: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidatePackageV3, String> {
        self.0.export(value)
    }
    pub(in super::super) fn publication_promises(
        &self,
    ) -> Result<super::super::super::super::formal_ledger::PublicationPromiseInventoryV1, String>
    {
        self.0.publication_promises()
    }
    pub(crate) fn usage(&self) -> Result<usize, String> {
        self.0.usage()
    }
}
