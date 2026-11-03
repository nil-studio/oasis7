//! Successor material export/install over the actual retained Node owner.
use super::*;
impl FormalBftHeaderNodeDriverV3 {
    pub(crate) fn verified_private_retention_cut(
        &mut self,
        source: &oasis7_distfs::protected_bft_v3::BftSuccessorWindowSourceV1,
        height: u64,
        header_id: &str,
        deadline: std::time::Instant,
    ) -> Result<oasis7_distfs::protected_bft_v3::BftSuccessorWindowCutV1, String> {
        self.require_configured()?;
        if source.material_identity != self.material_identity
            || self.configured_binding.as_ref() != Some(&source.binding_digest)
            || self.cursor_height != height
        {
            return Err("retention cut differs from installed original Node".into());
        }
        self.execution
            .verified_private_retention_cut(source, height, header_id, deadline)
    }
    /// Export only after actual durable H-to-cut replay; this does not adopt a
    /// successor minimum or permit a new signing window.
    pub(crate) fn export_successor_private_closure(
        &mut self,
        request: super::successor_archive::SuccessorPrivateExportRequestV1<'_>,
    ) -> Result<ExportedBftSuccessorPrivateClosureV1, String> {
        let super::successor_archive::SuccessorPrivateExportRequestV1 {
            archive,
            source,
            public_state,
            previous_usage,
            height,
            header_id,
            sequence,
            predecessor,
            deadline,
        } = request;
        self.require_configured()?;
        if self.covered_source.is_some() {
            return Err(
                "covered source archive requires original protected input seal chain".into(),
            );
        }
        let archive_root = canonical_storage_path(archive.root())?;
        crate::controlled_live_config::require_private_owned(&archive_root, true)?;
        for live in [
            &self.records_root,
            &canonical_storage_path(self.execution.store.root())?,
        ] {
            if archive_root.starts_with(live) || live.starts_with(&archive_root) {
                return Err("successor immutable archive overlaps live private storage".into());
            }
        }
        if source.material_identity != self.material_identity
            || source.binding_digest
                != *self
                    .configured_binding
                    .as_ref()
                    .ok_or("Node binding absent")?
            || self.cursor_height != height
        {
            return Err("successor export differs from installed original Node".into());
        }
        let mut pending_packages = Vec::new();
        let next_height = height
            .checked_add(1)
            .ok_or("successor pending height overflow")?;
        let mut required =
            super::successor_archive::required_pending(public_state.journal.as_ref())?;
        if let Some(local) = self.private.candidate(next_height)? {
            required
                .entry(local.public_value.block_id()?)
                .or_default()
                .insert(super::successor_archive::PendingRoleV1::RetainedLocal);
        }
        for (id, roles) in required {
            let value = self
                .private
                .public_value(next_height, &id)?
                .ok_or("original T+1 private candidate unavailable")?;
            pending_packages.push((roles, self.private.export(&value)?));
        }
        let mut publication_headers = self
            .execution
            .ledger
            .records()
            .map(|r| r.input().certificate.header.block_id())
            .collect::<Result<BTreeSet<_>, _>>()?;
        for (_, package) in &pending_packages {
            publication_headers.insert(package.candidate.public_value.block_id()?);
        }
        let publications = self
            .private
            .publication_archive_materials(&publication_headers)?;
        self.execution.export_successor_private_closure(
            super::successor_archive::SuccessorPrivateExportRequestV1 {
                archive,
                source,
                public_state,
                previous_usage,
                height,
                header_id,
                sequence,
                predecessor,
                deadline,
            },
            self.private.instance_id(),
            &pending_packages,
            &publications,
        )
    }

    pub(crate) fn export_successor_private_closure_with_retention(
        &mut self,
        request: super::successor_archive::SuccessorPrivateExportRequestV1<'_>,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        original_reader: &dyn oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReaderV1,
    ) -> Result<ExportedBftSuccessorPrivateClosureV1, String> {
        self.require_configured()?;
        let covered = self
            .covered_source
            .as_ref()
            .ok_or("protected export requires original covered source")?;
        covered.verify()?;
        let binding = covered.archive_binding().clone();
        let role = covered.archive_attestor().clone();
        let archive_root = canonical_storage_path(request.archive.root())?;
        crate::controlled_live_config::require_private_owned(&archive_root, true)?;
        for live in [
            &self.records_root,
            &canonical_storage_path(self.execution.store.root())?,
        ] {
            if archive_root.starts_with(live) || live.starts_with(&archive_root) {
                return Err("protected immutable archive overlaps live source storage".into());
            }
        }
        if request.source.material_identity != self.material_identity
            || self.configured_binding.as_ref() != Some(&request.source.binding_digest)
            || self.cursor_height != request.height
        {
            return Err("protected export differs from installed original Node".into());
        }
        let (private_origin, capsule) = self
            .private
            .source_archive_activation_materials(&self.records_root)?;
        let origin = self.execution.ledger.origin()?.digest()?;
        let (protected, protected_ref) =
            super::successor_archive::protected_retention::export_protected_retention(
                request.archive,
                &binding,
                &role,
                request.public_state,
                request.source,
                self.private.instance_id(),
                &origin,
                private_origin,
                capsule,
                lease,
                original_reader,
                &self.execution.store,
                request.deadline,
            )?;
        let next_height = request
            .height
            .checked_add(1)
            .ok_or("protected pending height overflow")?;
        let mut required =
            super::successor_archive::required_pending(request.public_state.journal.as_ref())?;
        for original in protected
            .originals
            .values()
            .filter(|original| original.entry.key.height > request.height)
        {
            if original.entry.key.height != next_height {
                return Err("protected original is beyond exact T+1".into());
            }
            required
                .entry(original.entry.key.header_id.clone())
                .or_default()
                .insert(super::successor_archive::PendingRoleV1::ProtectedOriginal);
        }
        let protected_choice = protected
            .final_ledger
            .selection_index
            .as_ref()
            .and_then(|index| {
                index
                    .selections
                    .iter()
                    .find(|selection| selection.key.height == next_height)
            });
        let actual_choice = self.private.candidate(next_height)?;
        match (actual_choice.as_ref(), protected_choice) {
            (Some(candidate), Some(selected))
                if candidate.public_value.block_id()? == selected.key.header_id => {}
            (None, None) => {}
            _ => return Err("protected current selection differs from original local WAL".into()),
        }
        if let Some(index) = &protected.final_ledger.selection_index {
            for selection in &index.selections {
                if selection.key.height == next_height {
                    required
                        .entry(selection.key.header_id.clone())
                        .or_default()
                        .insert(super::successor_archive::PendingRoleV1::RetainedLocal);
                }
            }
        }
        let mut pending_packages = Vec::new();
        for (id, roles) in required {
            if !protected
                .originals
                .keys()
                .any(|key| key.height == next_height && key.header_id == id)
            {
                return Err("protected public pending lacks exact original Some".into());
            }
            let value = self
                .private
                .public_value(next_height, &id)?
                .ok_or("protected original T+1 candidate unavailable")?;
            pending_packages.push((roles, self.private.export(&value)?));
        }
        let headers = protected
            .originals
            .keys()
            .map(|key| key.header_id.clone())
            .collect();
        let publications = self.private.publication_archive_materials(&headers)?;
        let expected_state = request.public_state.clone();
        let deadline = request.deadline;
        let exported = self
            .execution
            .export_successor_private_closure_with_protected(
                request,
                self.private.instance_id(),
                &pending_packages,
                &publications,
                Some((&protected, &protected_ref)),
            )?;
        let current = lease
            .observe_private_retention(deadline)
            .map_err(|error| error.to_string())?;
        if current.checkpoint().state != expected_state
            || current.ledger() != &protected.final_ledger
            || current.checkpoint().provider_public_key != role.provider_public_key
        {
            return Err("protected export lease/head changed before publication".into());
        }
        Ok(exported)
    }

    pub(crate) fn install_successor_window(
        &mut self,
        request: super::successor_install::SuccessorPrivateInstallRequestV1<'_>,
    ) -> Result<oasis7_distfs::protected_bft_v3::BftSuccessorWindowAdoptedEvidenceV1, String> {
        self.install_successor_window_inner(request, None)
    }
    pub(crate) fn install_successor_window_with_retention(
        &mut self,
        request: super::successor_install::SuccessorPrivateInstallRequestV1<'_>,
        retention: &super::successor_install::ConfiguredRetentionInstallV1,
    ) -> Result<oasis7_distfs::protected_bft_v3::BftSuccessorWindowAdoptedEvidenceV1, String> {
        self.install_successor_window_inner(request, Some(retention))
    }
    fn install_successor_window_inner(
        &mut self,
        request: super::successor_install::SuccessorPrivateInstallRequestV1<'_>,
        retention: Option<&super::successor_install::ConfiguredRetentionInstallV1>,
    ) -> Result<oasis7_distfs::protected_bft_v3::BftSuccessorWindowAdoptedEvidenceV1, String> {
        if let Some(retention) = retention {
            retention.validate_for(&request)?;
        }
        let binding = request.binding;
        let archive = &request.archive;
        self.require_configured()?;
        if self.check_binding(binding)?
            != *self
                .configured_binding
                .as_ref()
                .ok_or("Node binding absent")?
        {
            return Err("successor install binding changed".into());
        }
        let archive_root = canonical_storage_path(archive.root())?;
        for live in [
            &self.records_root,
            &canonical_storage_path(self.execution.store.root())?,
        ] {
            if archive_root.starts_with(live) || live.starts_with(&archive_root) {
                return Err("successor installation archive overlaps live storage".into());
            }
        }
        self.execution.install_successor_window(
            &self.records_root,
            &self.material_identity,
            self.cursor_height,
            request,
            retention,
        )
    }
}
