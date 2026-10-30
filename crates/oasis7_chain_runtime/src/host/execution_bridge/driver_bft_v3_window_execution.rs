//! Actual private successor execution. Every open starts from independently replayed archives.
use super::super::super::super::super::storage as common_storage;
use super::super::super::formal_prefix_files as files;
use super::successor_archive::{MAX_METADATA, encode, err, live};
use super::successor_verifier::VerifiedPrivateSuccessorBaseV1;
use super::window_storage::{DIRECTORY, WindowLedger, WindowRecord};
use super::*;
use oasis7_distfs::protected_bft_v3::{
    ValidatedBftSuccessorInstalledStateV1, VerifiedBftSignerActiveWindowV1,
};
use std::fs::File;
use std::time::Instant;

pub(crate) struct FormalBftHeaderActiveExecutionV1 {
    base: VerifiedPrivateSuccessorBaseV1,
    ledger: WindowLedger,
    private: node_execution::WindowPrivateStore,
    execution: FormalBftHeaderExecutionV3,
    store: LocalCasStore,
    segment: PathBuf,
    records_root: PathBuf,
    installed: oasis7_distfs::protected_bft_v3::BftSignerStateV3,
    carriers: successor_carriers::InstallationRootV1,
    _original_format: File,
    _original_writer: File,
    protected_origin: oasis7_distfs::protected_bft_v3::BftSuccessorWindowArchiveRefV1,
    directory_pins: super::window_storage::directories::WindowDirectoryPins,
}
impl FormalBftHeaderActiveExecutionV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        records: &Path,
        store: LocalCasStore,
        base: VerifiedPrivateSuccessorBaseV1,
        active: &VerifiedBftSignerActiveWindowV1,
        binding: &oasis7_distfs::protected_bft_v3::BftSignerBindingV3,
        provider_pin: &str,
        installed: &ValidatedBftSuccessorInstalledStateV1,
        deadline: Instant,
    ) -> Result<Self, String> {
        live(deadline)?;
        validate_storage_paths(records, store.root())?;
        let original = base
            .original
            .as_ref()
            .ok_or("active private base has no authenticated original request")?;
        if original != active.original_request()
            || active.base_schedule() != &base.execution.schedule
            || active.minimum()
                != &original
                    .request
                    .intent
                    .body
                    .checkpoint
                    .minimum()
                    .map_err(err)?
        {
            return Err(
                "private active base differs from authenticated current provider minimum".into(),
            );
        }
        if active.provider_public_key() != provider_pin
            || active.binding_digest() != binding.digest().map_err(err)?
            || base.metadata.source.binding_digest != active.binding_digest()
        {
            return Err(
                "private active base provider/binding differs from fixed configuration".into(),
            );
        }
        original.validate(binding, provider_pin).map_err(err)?;
        let checkpoint = &original.request.intent.body.checkpoint;
        let installed = installed.installed_state().clone();
        let immutable = installed
            .successor_window
            .as_ref()
            .and_then(|s| s.request(&checkpoint.request_id))
            .ok_or("historical installed original absent")?;
        let records = canonical_storage_path(records)?;
        if !records.join("bft-format.lock").is_file()
            || !records
                .join(storage::DIRECTORY)
                .join("writer.lock")
                .is_file()
        {
            return Err("private active original lock carrier missing".into());
        }
        let original_format = common_storage::lock_format(&records)?;
        let original_writer = files::lock(&records.join(storage::DIRECTORY), false)?;
        let carriers = successor_carriers::InstallationRootV1::new(records.clone())?;
        carriers.read_installed(&base.origin, immutable, &installed, deadline)?;
        let segment = layout::segment_path(&records, &base)?;
        layout::verify_ready(
            &records,
            store.clone(),
            &base,
            active.active_segment_origin(),
            false,
            deadline,
        )?;
        let mut ledger = WindowLedger::open(&segment, false, layout::base_charge(&base)?)?;
        if ledger.material_pin_digest()? != blake3_hex(&base_identity(&base)?)
            || ledger.origin()?.first_height() != base.execution.schedule.height()
        {
            return Err("cold active segment differs from original installed base".into());
        }
        let directory_pins = super::window_storage::directories::WindowDirectoryPins::open(
            &segment,
            ledger.origin()?,
        )?;
        directory_pins.verify()?;
        let private = node_execution::WindowPrivateStore::open(
            segment
                .join(DIRECTORY)
                .join(storage::NODE_PRIVATE_DIRECTORY),
            store.clone(),
            &base,
            &mut ledger,
        )?;
        let execution = base.execution.fork_strict()?;
        let mut owner = Self {
            base,
            ledger,
            private,
            execution,
            store,
            segment,
            records_root: records,
            installed,
            carriers,
            _original_format: original_format,
            _original_writer: original_writer,
            protected_origin: active.active_segment_origin().clone(),
            directory_pins,
        };
        owner.reset(deadline)?;
        Ok(owner)
    }
    fn verify_base(&self, deadline: Instant) -> Result<(), String> {
        self.directory_pins.verify()?;
        live(deadline)?;
        let original = self
            .base
            .original
            .as_ref()
            .ok_or("active base original absent")?;
        let id = &original.request.intent.body.checkpoint.request_id;
        let immutable = self
            .installed
            .successor_window
            .as_ref()
            .and_then(|s| s.request(id))
            .ok_or("active installed original absent")?;
        self.carriers
            .read_installed(&self.base.origin, immutable, &self.installed, deadline)?;
        let bytes =
            common_storage::read_optional_bytes(&self.segment.join("base.cbor"), MAX_METADATA)?
                .ok_or("active private base absent")?;
        if bytes != base_identity(&self.base)?
            || self.ledger.material_pin_digest()? != blake3_hex(&bytes)
        {
            return Err("active private base changed after open".into());
        }
        let reference = self.carriers.read_window_origin(&self.base, deadline)?.1;
        if reference != self.protected_origin {
            return Err("active Origin changed from protected adoption commitment".into());
        }
        self.directory_pins.verify()?;
        self.ledger.verify_carriers()
    }
    fn reset(&mut self, deadline: Instant) -> Result<(), String> {
        self.reset_through(None, deadline)
    }
    fn reset_through(&mut self, through: Option<u64>, deadline: Instant) -> Result<(), String> {
        self.verify_base(deadline)?;
        self.ledger.reload()?;
        let suffix_nodes = self.ledger.preload(&self.store, through)?;
        self.ledger.reserve_private_material(
            self.private.usage()?,
            self.private.publication_promises()?,
            &self.store,
        )?;
        self.execution = self.base.execution.fork_strict_with_seed(suffix_nodes)?;
        let mut previous = self.ledger.origin_digest()?;
        let records: Vec<_> = self
            .ledger
            .records()
            .filter(|record| through.is_none_or(|height| record.height() <= height))
            .cloned()
            .collect();
        for record in records {
            live(deadline)?;
            let input = record.input();
            if self
                .ledger
                .reservation_without_pins(record.height(), &self.store)?
            {
                // A pre-pin crash has fixed the exact QC/actions in the durable
                // reservation but has not persisted its derived result nodes.
                // Admit only its original retained candidate dependencies, then
                // reproduce and compare the entire record before publication.
                let value = oasis7_consensus::tendermint_v3::BftHeaderValueV3 {
                    header: input.certificate.header.clone(),
                    execution_commitment: input.certificate.execution_commitment.clone(),
                };
                let package = self.private.export(&value)?;
                if package.candidate.actions_cbor != input.actions_cbor {
                    return Err("pre-pin reservation original private actions differ".into());
                }
                let mut seed = self
                    .execution
                    .retained_nodes
                    .recorded_nodes()?
                    .into_iter()
                    .collect::<std::collections::BTreeMap<_, _>>();
                for (digest, bytes) in package.dependency_nodes {
                    if seed.get(&digest).is_some_and(|original| original != &bytes) {
                        return Err("pre-pin original candidate dependency bytes conflict".into());
                    }
                    seed.insert(digest, bytes);
                }
                let observed = self.execution.retained_nodes.accessed_nodes()?;
                self.execution = self
                    .execution
                    .fork_replay_current(seed.into_iter().collect())?;
                for (digest, bytes) in observed {
                    if self.execution.retained_nodes.read_node(&digest)?.as_ref() != Some(&bytes) {
                        return Err("pre-pin recovery lost a verified prefix dependency".into());
                    }
                }
                self.execution = self.execution.fork_live_closed()?;
            }
            let executed = self
                .execution
                .replay_next(&input.certificate, &input.actions_cbor)?;
            let nodes = self.execution.retained_nodes.accessed_nodes()?;
            let refs = self
                .ledger
                .retained_business_refs(record.height(), &nodes)?;
            let reproduced = WindowRecord::from_actual(
                self.ledger.origin()?,
                &previous,
                &input,
                &executed,
                refs,
            )?;
            let reproduced = if let Some(reference) = record.publication_reservation_ref() {
                super::super::publication_reservation::PublicationReservationV1::read(
                    &self.store,
                    reference,
                )?
                .validate_execution(
                    record.origin_digest(),
                    record.height(),
                    &input.actions_cbor,
                    &executed,
                )?;
                reproduced.with_publication_reservation(Some(reference.to_owned()))?
            } else {
                reproduced
            };
            if reproduced != record {
                return Err(
                    "active private record differs from actual archive-rooted replay".into(),
                );
            }
            live(deadline)?;
            previous = self.ledger.accept_prepared(
                reproduced,
                &executed.snapshot_bytes,
                &executed.journal_bytes,
                &nodes,
                &self.store,
                false,
            )?;
        }
        self.verify_base(deadline)
    }
    #[cfg(test)]
    pub(crate) fn inject_publication_failure(&mut self, height: u64) {
        self.ledger.inject_publication_failure(
            height,
            super::super::super::formal_ledger::PublicationFault::PinsPublished,
        );
    }

    #[cfg(test)]
    pub(crate) fn inject_pre_pin_failure(&mut self, height: u64) {
        self.ledger.inject_publication_failure(
            height,
            super::super::super::formal_ledger::PublicationFault::ReservationPublished,
        );
    }

    pub(crate) fn schedule(&self) -> &BftHeaderHeightScheduleV3 {
        &self.execution.schedule
    }
    pub(crate) fn world(&self) -> &RuntimeWorld {
        &self.execution.parent_world
    }
    fn execution_error_context(&self, operation: &str, error: String) -> String {
        let count = |result: Result<Vec<(String, Vec<u8>)>, String>| {
            result.map(|nodes| nodes.len()).unwrap_or_default()
        };
        format!(
            "active {operation} height={} base_recorded={} current_recorded={} observed={}: {error}",
            self.execution.schedule.height(),
            count(self.base.execution.retained_nodes.recorded_nodes()),
            count(self.execution.retained_nodes.recorded_nodes()),
            self.execution
                .retained_nodes
                .accessed_nodes()
                .map(|nodes| nodes.len())
                .unwrap_or_default()
        )
    }
    pub(crate) fn current_install_digest(&self) -> Result<String, String> {
        let tip = self
            .execution
            .tip
            .as_ref()
            .ok_or("active private tip absent")?;
        let record = self
            .ledger
            .records()
            .last()
            .ok_or("active private record absent")?;
        super::successor_archive::install_digest(
            self.material_identity(),
            &record.digest()?,
            tip.certified_header(),
        )
    }
    pub(crate) fn current_result_root(&self) -> Result<String, String> {
        self.execution.parent_result_tree.root()
    }
    pub(crate) fn material_identity(&self) -> &str {
        &self.base.metadata.source.material_identity
    }
    pub(crate) fn pending_candidate(
        &self,
    ) -> Result<Option<oasis7_node::NodeBftPrivateCandidateV3>, String> {
        self.private.candidate(self.schedule().height())
    }
    pub(crate) fn build_and_retain(
        &mut self,
        actions_cbor: Vec<u8>,
        deadline: Instant,
    ) -> Result<oasis7_node::NodeBftPrivateCandidateV3, String> {
        self.reset(deadline)?;
        let original = self.pending_candidate()?;
        if original
            .as_ref()
            .is_some_and(|original| original.actions_cbor != actions_cbor)
        {
            return Err("active private height has different original actions".into());
        }
        self.execution = self.execution.fork_live_closed()?;
        let executed = self.execution.execute(&actions_cbor)?;
        let candidate = oasis7_node::NodeBftPrivateCandidateV3 {
            public_value: oasis7_consensus::tendermint_v3::BftHeaderValueV3 {
                header: executed.candidate.header,
                execution_commitment: executed.candidate.commitment,
            },
            actions_cbor,
        };
        if original
            .as_ref()
            .is_some_and(|original| original != &candidate)
        {
            return Err("active private original candidate did not reproduce".into());
        }
        let accessed = self.execution.retained_nodes.accessed_nodes()?;
        let nodes = if original.is_some() {
            // The original WAL also retains dependencies used to replay its old
            // segment to T. Starting from the verified T base reads a subset;
            // retries must preserve the complete original reference inventory.
            let package = self.private.export(&candidate.public_value)?;
            let retained = package
                .dependency_nodes
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>();
            if accessed
                .iter()
                .any(|(digest, bytes)| retained.get(digest) != Some(bytes))
            {
                return Err("active original candidate dependency bytes differ".into());
            }
            retained
        } else {
            accessed
        };
        live(deadline)?;
        self.private.retain(&candidate, &nodes, &mut self.ledger)?;
        self.verify_base(deadline)?;
        Ok(candidate)
    }
    pub(crate) fn append_certified(
        &mut self,
        certificate: FormalBftCertifiedHeaderV3,
        deadline: Instant,
    ) -> Result<String, String> {
        self.reset(deadline)?;
        let value = oasis7_consensus::tendermint_v3::BftHeaderValueV3 {
            header: certificate.header.clone(),
            execution_commitment: certificate.execution_commitment.clone(),
        };
        let candidate = self
            .private
            .candidate_for_value(&value)?
            .ok_or("active private original candidate unavailable")?;
        let input = PrivateHeaderEntryV3 {
            certificate,
            actions_cbor: candidate.actions_cbor,
        };
        self.execution = self.execution.fork_live_closed()?;
        let executed = self
            .execution
            .replay_next(&input.certificate, &input.actions_cbor)
            .map_err(|error| {
                if error == "injected original prefix publication interruption" {
                    error
                } else {
                    self.execution_error_context("append", error)
                }
            })?;
        let nodes = self.execution.retained_nodes.accessed_nodes()?;
        let previous = self
            .ledger
            .records()
            .last()
            .map(RecordContract::digest)
            .transpose()?
            .unwrap_or(self.ledger.origin_digest()?);
        let record = WindowRecord::from_actual(
            self.ledger.origin()?,
            &previous,
            &input,
            &executed,
            nodes.keys().cloned().collect(),
        )?;
        live(deadline)?;
        let digest = self.ledger.accept_prepared(
            record,
            &executed.snapshot_bytes,
            &executed.journal_bytes,
            &nodes,
            &self.store,
            true,
        )?;
        self.reset(deadline)?;
        if self
            .execution
            .tip
            .as_ref()
            .is_none_or(|tip| tip.certified_header() != &input.certificate)
        {
            return Err("active private publication disappeared after durable reread".into());
        }
        Ok(digest)
    }
}
fn private_directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        super::super::super::super::super::super::super::controlled_live::private_directory(path)?;
    }
    canonical_storage_path(path)?;
    crate::controlled_live_config::require_private_owned(path, true)
}
pub(super) fn base_identity(base: &VerifiedPrivateSuccessorBaseV1) -> Result<Vec<u8>, String> {
    let original = base
        .original
        .as_ref()
        .ok_or("private active original absent")?;
    encode(
        &(
            "oasis7.formal.bft.private-window.base.v1",
            base.origin.digest()?,
            &base.metadata.private_instance_id,
            &base.metadata.source,
            original
                .request
                .intent
                .body
                .checkpoint
                .digest()
                .map_err(err)?,
            &base.metadata.cut,
        ),
        MAX_METADATA,
    )
}

#[path = "driver_bft_v3_window_export.rs"]
mod export;

#[path = "driver_bft_v3_window_layout.rs"]
pub(super) mod layout;

#[path = "driver_bft_v3_window_install.rs"]
mod install;

#[path = "driver_bft_v3_window_node.rs"]
pub(super) mod node_adapter;
#[path = "driver_bft_v3_window_resume.rs"]
pub(super) mod resume;
