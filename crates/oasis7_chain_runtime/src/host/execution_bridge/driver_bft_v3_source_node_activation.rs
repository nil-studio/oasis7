//! Writable source construction after original empty coverage has been verified.
use super::super::source_creation::activation::ValidatedSourceActivationV1;
use super::*;
use std::time::{Duration, Instant};

#[path = "driver_bft_v3_protected_admission_transaction.rs"]
mod protected_admission_transaction;

pub(in super::super) struct CoveredSourceOpenV1<'a> {
    pub(in super::super) records: PathBuf,
    pub(in super::super) store: LocalCasStore,
    pub(in super::super) inputs: Inputs,
    pub(in super::super) origin: storage::Origin,
    pub(in super::super) carriers: Arc<super::super::successor_carriers::InstallationRootV1>,
    pub(in super::super) binding: &'a BftSignerBindingV3,
    pub(in super::super) activation: &'a ValidatedSourceActivationV1,
    pub(in super::super) deadline: Instant,
}

pub(super) struct CoveredSourceContextV1 {
    carriers: Arc<super::super::successor_carriers::InstallationRootV1>,
    origin: storage::Origin,
    private_directory: std::fs::File,
    private_path: PathBuf,
    fixed_attestor: oasis7_distfs::protected_bft_v3::BftPrivateRetentionAttestorV1,
    binding: BftSignerBindingV3,
}
impl CoveredSourceContextV1 {
    pub(super) fn archive_binding(&self) -> &BftSignerBindingV3 {
        &self.binding
    }
    pub(super) fn archive_attestor(
        &self,
    ) -> &oasis7_distfs::protected_bft_v3::BftPrivateRetentionAttestorV1 {
        &self.fixed_attestor
    }

    fn verify_observation(
        &self,
        binding: &BftSignerBindingV3,
        observation: &oasis7_distfs::protected_bft_v3::ValidatedBftPrivateRetentionObservationV1,
    ) -> Result<(), String> {
        let checkpoint = observation.checkpoint();
        let head = checkpoint
            .state
            .private_retention
            .as_ref()
            .ok_or("source current protected head missing")?;
        if checkpoint.provider_public_key != self.fixed_attestor.provider_public_key
            || checkpoint.state.binding_digest != binding.digest().map_err(|e| e.to_string())?
            || head.capability_digest != self.fixed_attestor.digest().map_err(|e| e.to_string())?
            || head.source != self.fixed_attestor.source
            || observation.ledger().map.source != self.fixed_attestor.source
        {
            return Err(
                "current source observation differs from original fixed provider/role".into(),
            );
        }
        Ok(())
    }
    pub(super) fn verify(&self) -> Result<(), String> {
        self.verify_at(
            Instant::now()
                .checked_add(Duration::from_secs(120))
                .ok_or("covered source verification deadline overflow")?,
        )
    }
    pub(super) fn verify_at(&self, deadline: Instant) -> Result<(), String> {
        super::super::successor_archive::live(deadline)?;
        let original =
            super::super::window_storage::directories::DirectoryIdentity::from_descriptor(
                &self.private_directory,
            )?;
        let current =
            super::super::window_storage::directories::DirectoryIdentity::read(&self.private_path)?;
        if original != current {
            return Err("covered private original directory replaced after open".into());
        }

        self.carriers
            .verify_origin_carriers(&self.origin, deadline)
            .map(|_| ())
    }
}

pub(in super::super) fn read_original_activation(
    records: &Path,
    store: LocalCasStore,
    source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
    instance: &str,
) -> Result<Option<Vec<u8>>, String> {
    let (private, _) = NodePrivateStore::open_covered(records, store, source, instance)?;
    private.original_activation_bytes()
}

impl FormalBftHeaderNodeDriverV3 {
    pub(in super::super) fn open_covered_source(
        request: CoveredSourceOpenV1<'_>,
    ) -> Result<Box<Self>, String> {
        super::super::successor_archive::live(request.deadline)?;
        validate_storage_paths(&request.records, request.store.root())?;
        request
            .carriers
            .verify_origin_carriers(&request.origin, request.deadline)?;
        let instance = &request.activation.marker().creation_identity;
        let (mut private, retained) = NodePrivateStore::open_covered(
            &request.records,
            request.store.clone(),
            request.inputs.source.clone(),
            instance,
        )?;
        let mut inputs = request.inputs;
        inputs.source = retained;
        let mut execution = FormalBftHeaderLedgerExecutionV3::open_for_fresh_node(
            &request.records,
            request.store,
            inputs,
            instance,
        )?;
        execution.ledger.require_completed()?;
        if execution.ledger.origin()? != &request.origin {
            return Err("empty source activation differs from original applied H".into());
        }
        let initial_schedule = execution.origin.schedule.clone();
        if initial_schedule.active_set().validators().len() != 1
            || initial_schedule.pending_transition().is_some()
        {
            return Err(
                "covered source activation requires explicit single-validator initial profile"
                    .into(),
            );
        }
        let cursor_height = execution.origin.handoff.handoff_position();
        let ledger_origin = execution.ledger.origin_digest()?;
        let material_identity = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_material.v3",
                &ledger_origin,
                private.instance_id(),
                execution.ledger.material_pin_digest()?,
            ))
            .map_err(|_| "encode covered source material identity")?,
        );
        if material_identity != request.binding.material_identity
            || initial_schedule.digest()? != request.binding.initial_schedule().digest()?
        {
            return Err("covered source actual H differs from fixed signer binding".into());
        }
        capture_h_index(&execution.origin)?;
        let nodes = execution.origin.retained_nodes.accessed_nodes()?;
        request
            .carriers
            .verify_origin_carriers(&request.origin, request.deadline)?;
        let mut verify_owner = || {
            super::super::successor_archive::live(request.deadline)?;
            request
                .carriers
                .verify_origin_carriers(&request.origin, request.deadline)
                .map(|_| ())
        };
        private.activate_original_source(
            private_store::OriginalSourcePublicationV1 {
                ledger_origin: &ledger_origin,
                material: &material_identity,
                height: cursor_height,
                nodes: &nodes,
                activation: request.activation,
                deadline: request.deadline,
                verify_owner: &mut verify_owner,
            },
            &mut |bytes, future| {
                execution
                    .ledger
                    .reserve_private_material(bytes, future.clone(), &execution.store)
            },
        )?;
        request
            .carriers
            .verify_origin_carriers(&request.origin, request.deadline)?;
        private.verify_original_binding(&ledger_origin, &material_identity, cursor_height)?;
        private.load_inventory(&ledger_origin, &material_identity, cursor_height)?;
        super::super::successor_archive::live(request.deadline)?;
        let private_path = request
            .records
            .join(storage::DIRECTORY)
            .join(storage::NODE_PRIVATE_DIRECTORY);
        let private_directory =
            super::super::successor_carriers::open_absolute_directory(&private_path)?;
        Ok(Box::new(Self {
            records_root: request.records,
            installed_world: execution.origin.parent_world.clone(),
            execution,
            private,
            initial_schedule,
            material_identity,
            configured_binding: Some(request.binding.digest().map_err(|e| e.to_string())?),
            cursor_height,
            peer_network: false,
            membership_enabled: false,
            parent_memo: None,
            admission_input_io: None,
            covered_source: Some(CoveredSourceContextV1 {
                binding: request.binding.clone(),
                carriers: request.carriers,
                origin: request.origin,
                private_directory,
                private_path,
                fixed_attestor: request.activation.fixed_attestor().clone(),
            }),
        }))
    }
}

impl FormalBftHeaderNodeDriverV3 {
    pub(super) fn prepare_source_original(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        deadline: Instant,
    ) -> Result<(), String> {
        use oasis7_distfs::protected_bft_v3::{
            BftPrivateRetentionAdmissionInputRefV1, BftPrivateRetentionLifecycleV1,
        };
        self.check_binding(binding)?;
        let checkpoint = lease.checkpoint().map_err(|e| e.to_string())?;
        if checkpoint.state.active_window.is_some() || checkpoint.state.successor_window.as_ref().is_some_and(|window| window.reservation.is_some() || window.requests.iter().any(|r| matches!(r.request.status, oasis7_distfs::protected_bft_v3::BftSuccessorWindowStatusV1::Prepared | oasis7_distfs::protected_bft_v3::BftSuccessorWindowStatusV1::InstalledPending))) { return Err("source startup requires authenticated successor recovery".into()); }
        if self.covered_source.is_none() {
            if checkpoint.state.private_retention.is_some() {
                return Err("protected source startup lacks original covered owner".into());
            }
            return Ok(());
        }
        let observation = lease
            .observe_private_retention(deadline)
            .map_err(|e| e.to_string())?;
        self.covered_source
            .as_ref()
            .ok_or("original covered source owner absent")?
            .verify_observation(binding, &observation)?;
        let schedule = lease
            .current_verified_schedule()
            .map_err(|e| e.to_string())?;
        let origin = self.execution.ledger.origin_digest()?;
        for entry in &observation.ledger().map.entries {
            super::super::successor_archive::live(deadline)?;
            if entry.key.ledger_origin_digest != origin {
                return Err(
                    "covered source startup requires authenticated foreign-origin carry support"
                        .into(),
                );
            }
            if entry.lifecycle == BftPrivateRetentionLifecycleV1::Ready {
                let reference = BftPrivateRetentionAdmissionInputRefV1::from_entry(
                    &observation.ledger().map.source,
                    entry,
                )
                .map_err(|e| e.to_string())?;
                self.private
                    .verify_ready_source_entry(&reference, &entry.fact)?;
            }
        }
        let mut selected_keys = observation
            .ledger()
            .selection_index
            .as_ref()
            .map(|index| {
                index
                    .selections
                    .iter()
                    .map(|selected| (selected.key.height, selected.key.header_id.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (height, id) in self.private.source_selection_keys()? {
            if observation.ledger().receipt.body.head.schema_version == 3 {
                if !selected_keys.contains(&(height, id.clone())) {
                    return Err("local selection5 lacks original protected selection".into());
                }
            } else if height == schedule.height() {
                selected_keys.push((height, id)); // Explicit current-lease legacy migration, never export.
            }
        }
        if observation.ledger().receipt.body.head.schema_version == 2 {
            if let Some(local) = self.private.candidate(schedule.height())? {
                let key = (schedule.height(), local.public_value.block_id()?);
                if !selected_keys.contains(&key) {
                    selected_keys.push(key);
                }
            }
        }
        for (height, id) in selected_keys {
            let entry = observation
                .ledger()
                .map
                .entries
                .iter()
                .find(|entry| {
                    entry.key.ledger_origin_digest == origin
                        && entry.key.height == height
                        && entry.key.header_id == id
                })
                .ok_or("source local selection lacks original protected fact")?;
            let reference = BftPrivateRetentionAdmissionInputRefV1::from_entry(
                &observation.ledger().map.source,
                entry,
            )
            .map_err(|e| e.to_string())?;
            let proof = private_store::ReservedSourceInputV1::from_observation(
                binding,
                &self
                    .covered_source
                    .as_ref()
                    .ok_or("source selection owner absent")?
                    .fixed_attestor,
                &reference,
                &entry.fact,
                &observation,
            )?;
            proof.require_ready()?;
            let value = self
                .private
                .public_value(height, &id)?
                .ok_or("source selection value missing")?;
            let candidate = self
                .private
                .candidate_for_value(&value)?
                .ok_or("source selection candidate missing")?;
            self.select_source_ready(
                binding,
                lease,
                &candidate,
                &proof,
                observation.ledger(),
                deadline,
            )?;
        }
        let pending = observation
            .ledger()
            .map
            .entries
            .iter()
            .filter(|entry| entry.lifecycle == BftPrivateRetentionLifecycleV1::ReservedPending)
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return Ok(());
        }
        if pending.iter().any(|entry| {
            entry.key.height != schedule.height() || entry.key.ledger_origin_digest != origin
        }) {
            return Err(
                "source startup unresolved originals differ from current exact local slot".into(),
            );
        }
        for entry in pending {
            let reference = BftPrivateRetentionAdmissionInputRefV1::from_entry(
                &observation.ledger().map.source,
                entry,
            )
            .map_err(|e| e.to_string())?;
            let io = self
                .admission_input_io
                .as_ref()
                .ok_or("source startup original input IO not configured")?;
            let read = io.read_original(&reference, deadline)?;
            reference
                .validate_input(read.input())
                .map_err(|e| e.to_string())?;
            let kind = private_store::source_admission_slot_kind(read.input())?;
            let candidate = private_store::source_admission_candidate(read.input())?;
            if candidate.public_value.block_id()? != reference.key.header_id
                || candidate.public_value.execution_commitment.height != reference.key.height
            {
                return Err("source startup original header/height changed".into());
            }
            // Select the original protected parent, never queued replacement actions.
            // This temporary execution cursor is reset by ordinary startup replay.
            self.cursor_height = schedule
                .height()
                .checked_sub(1)
                .ok_or("source startup parent height underflow")?;
            self.reset_parent()?;
            let expected = oasis7_node::NodeBftExecutionContextV3::from_schedule(&schedule);
            if self.expected() != expected {
                return Err(
                    "source startup original parent differs from protected schedule".into(),
                );
            }
            self.retain_source_input_with_original(
                binding,
                lease,
                &expected,
                &candidate,
                deadline,
                kind,
                Some(read),
            )?;
        }
        Ok(())
    }
    fn select_source_ready(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        candidate: &oasis7_node::NodeBftPrivateCandidateV3,
        proof: &private_store::ReservedSourceInputV1,
        current: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionLedgerV1,
        deadline: Instant,
    ) -> Result<String, String> {
        let owner = self
            .covered_source
            .as_ref()
            .ok_or("protected selection owner absent")?;
        self.private.select_protected_ready(
            binding,
            &owner.fixed_attestor,
            lease,
            candidate,
            proof,
            current,
            deadline,
            &mut |bytes, future| {
                self.execution
                    .ledger
                    .reserve_private_material(bytes, future, &self.execution.store)
            },
            &mut || owner.verify_at(deadline),
            false,
        )
    }
    pub(super) fn retain_source_input(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        expected: &oasis7_node::NodeBftExecutionContextV3,
        candidate: &oasis7_node::NodeBftPrivateCandidateV3,
        deadline: Instant,
    ) -> Result<String, String> {
        self.retain_source_input_with_original(
            binding,
            lease,
            expected,
            candidate,
            deadline,
            private_store::SourceSlotKindV1::Local,
            None,
        )
    }
    pub(super) fn retain_received_source_input(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        expected: &oasis7_node::NodeBftExecutionContextV3,
        candidate: &oasis7_node::NodeBftPrivateCandidateV3,
        deadline: Instant,
    ) -> Result<String, String> {
        self.retain_source_input_with_original(
            binding,
            lease,
            expected,
            candidate,
            deadline,
            private_store::SourceSlotKindV1::Received,
            None,
        )
    }
}
