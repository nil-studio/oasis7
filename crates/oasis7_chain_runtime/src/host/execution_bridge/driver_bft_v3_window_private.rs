//! Window-local candidate WAL, preserving the original Node instance identity.
use super::super::successor_verifier::VerifiedPrivateSuccessorBaseV1;
use super::super::window_storage::{WindowLedger, WindowRecord};
use super::*;
use std::collections::BTreeMap;

#[path = "driver_bft_v3_window_protected_input.rs"]
mod protected_input;
pub(crate) struct WindowPrivateStore(NodePrivateStore);
impl WindowPrivateStore {
    pub(crate) fn open(
        directory: PathBuf,
        records: &Path,
        store: LocalCasStore,
        base: &VerifiedPrivateSuccessorBaseV1,
        ledger: &mut WindowLedger,
        deadline: std::time::Instant,
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
        private.set_auxiliary_root(records);
        let grants = base
            .publications
            .iter()
            .filter(|grant| {
                base.pending
                    .iter()
                    .any(|(_, package)| grant.matches_candidate(&package.candidate))
            })
            .cloned()
            .collect::<Vec<_>>();
        private.register_publication_ancestors(&grants)?;
        // Parent dependencies remain in the verified archive capability; the
        // new segment's original WAL inventory starts with its own material.
        let nodes = BTreeMap::new();
        private.bind_origin(
            &ledger.origin_digest()?,
            &base.metadata.source.material_identity,
            base.metadata.cut.height,
            &nodes,
            &mut |bytes, future| {
                ledger.reserve_private_material(bytes, future.clone(), &accounting_store)?;
                super::super::successor_archive::live(deadline)
            },
        )?;
        for (roles, package) in &base.pending {
            super::super::successor_archive::live(deadline)?;
            let nodes = package.dependency_nodes.iter().cloned().collect();
            let future = private.publication_promises()?;
            if let Some(grant) = grants
                .iter()
                .find(|grant| grant.matches_candidate(&package.candidate))
            {
                private.retain_migrated_publication(
                    grant,
                    roles.contains(&super::super::successor_archive::PendingRoleV1::RetainedLocal),
                    &nodes,
                    &mut |bytes, promises| {
                        ledger.reserve_private_material(bytes, promises, &accounting_store)?;
                        super::super::successor_archive::live(deadline)
                    },
                )?;
                continue;
            }
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
        records: &Path,
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
        let grants = base
            .publications
            .iter()
            .filter(|grant| {
                base.pending
                    .iter()
                    .any(|(_, package)| grant.matches_candidate(&package.candidate))
            })
            .cloned()
            .collect::<Vec<_>>();
        private.register_publication_ancestors(&grants)?;
        private.set_auxiliary_root(records);
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
            let expected = grants
                .iter()
                .find(|g| g.matches_candidate(&package.candidate))
                .map(|g| g.reference())
                .transpose()?;
            if private.publication_reference(&package.candidate.public_value)? != expected {
                return Err("active original pending publication ref changed".into());
            }
            if private.export(&package.candidate.public_value)?.candidate != package.candidate {
                return Err("active original pending candidate changed".into());
            }
        }
        if initial && private.inventory_count() != base.pending.len() {
            return Err("active preparation contains unproved private candidates".into());
        }
        Ok(())
    }
    pub(in super::super) fn publication_archive_materials(
        &self,
        headers: &std::collections::BTreeSet<String>,
    ) -> Result<Vec<PublicationArchiveMaterialV1>, String> {
        self.0.publication_archive_materials(headers)
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
    pub(in super::super) fn retain_planned(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        plan: &super::super::super::publication_plan::PublicationPlanV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        ledger: &mut WindowLedger,
        deadline: std::time::Instant,
    ) -> Result<String, String> {
        let store = self.0.accounting_store();
        self.0
            .retain_planned(candidate, plan, nodes, &mut |bytes, promises| {
                ledger.reserve_private_material(bytes, promises, &store)?;
                super::super::successor_archive::live(deadline)
            })
    }
    pub(in super::super) fn validate_record_publication(
        &self,
        record: &WindowRecord,
        candidate: &NodeBftPrivateCandidateV3,
        executed: &ExecutedFormalHeaderV3,
        ledger: &WindowLedger,
    ) -> Result<(), String> {
        let accounting = ledger.publication_accounting(&self.0.accounting_store())?;
        let claim = self
            .0
            .publication_claim_for_actual(candidate, executed, &accounting)?;
        if record.publication_reservation_ref() != claim.as_ref().map(|c| c.reference.as_str()) {
            return Err("active Record publication ref differs from original Data".into());
        }
        Ok(())
    }
    pub(in super::super) fn publish_actual(
        &self,
        ledger: &mut WindowLedger,
        record: WindowRecord,
        candidate: &NodeBftPrivateCandidateV3,
        executed: &ExecutedFormalHeaderV3,
        nodes: &BTreeMap<String, Vec<u8>>,
        deadline: std::time::Instant,
    ) -> Result<String, String> {
        let store = self.0.accounting_store();
        ledger.reserve_private_material(self.0.usage()?, self.0.publication_promises()?, &store)?;
        let accounting = ledger.publication_accounting(&store)?;
        let claim = self
            .0
            .publication_claim_for_actual(candidate, executed, &accounting)?;
        let record =
            record.with_publication_reservation(claim.as_ref().map(|c| c.reference.clone()))?;
        let mut entries = ledger
            .records()
            .filter(|r| r.height() < record.height())
            .map(|r| r.input().certificate)
            .collect::<Vec<_>>();
        entries.push(record.input().certificate);
        let proof = oasis7_formal_protocol::bft::FormalBftCertifiedHeaderPrefixV3 {
            schema_version: 1,
            entries,
        }
        .canonical_bytes()?
        .len();
        super::super::successor_archive::live(deadline)?;
        if let Some(retained) = claim {
            ledger.accept_prepared_with_publication(
                record,
                super::super::super::super::formal_ledger::PreparedRecordMaterialV1 {
                    snapshot_bytes: &executed.snapshot_bytes,
                    journal_bytes: &executed.journal_bytes,
                    nodes,
                    store: &store,
                },
                retained.claim.as_ref(),
                Some(proof),
            )
        } else {
            ledger.charge_proof(proof)?;
            ledger.accept_prepared(
                record,
                &executed.snapshot_bytes,
                &executed.journal_bytes,
                nodes,
                &store,
                true,
            )
        }
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
