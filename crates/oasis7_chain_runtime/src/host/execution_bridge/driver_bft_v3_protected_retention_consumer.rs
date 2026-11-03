//! Actual archived-input verification under an explicit exact-purpose source.
//! This file is staged separately until the registered metadata slice freezes.
use super::super::successor_archive::protected_retention::{
    ProtectedRetentionIndexV1, ValidatedProtectedRetentionArchiveV1,
};
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputReadV1 as InputRead,
    BftPrivateRetentionAdmissionInputRefV1 as InputRef,
    BftPrivateRetentionAdmissionInputV1 as Input, BftPrivateRetentionAttestorV1 as Role,
    BftPrivateRetentionInputObjectKindV1 as Kind, BftPrivateRetentionOperationProofV1 as Operation,
    BftPrivateRetentionSealVerificationContextV1 as OwnedContext, BftSignerBindingV3 as Binding,
};

pub(super) enum ProtectedInputAccessV1<'a, 'b> {
    External,
    Owned(&'a OwnedContext<'b>),
    HistoricalC0(&'a oasis7_distfs::protected_bft_v3::BftPrivateRetentionHistoricalOwnedC0V1<'b>),
}
pub(super) enum ScopedInputV1<'a> {
    External(InputRead),
    Owned(&'a Input),
}
impl ScopedInputV1<'_> {
    pub(super) fn input(&self) -> &Input {
        match self {
            Self::External(read) => read.input(),
            Self::Owned(input) => input,
        }
    }
}
impl ActualBftSuccessorWindowDriverVerifierV1 {
    pub(super) fn protected_source<'a>(
        &'a self,
        access: &'a ProtectedInputAccessV1<'_, '_>,
        state: &BftSignerStateV3,
    ) -> Result<(&'a Binding, &'a Role), String> {
        let (binding, role) = match access {
            ProtectedInputAccessV1::External => {
                let (role, _) = self.original_retention.as_ref().ok_or("covered archive requires independent OriginalAdmissionInput reader and fixed role")?;
                (
                    self.original_binding
                        .as_ref()
                        .ok_or("covered archive fixed original binding unavailable")?,
                    role,
                )
            }
            ProtectedInputAccessV1::HistoricalC0(context) => {
                if context.original_state() != state {
                    return Err(
                        "historical C0 context differs from exact authenticated original State"
                            .into(),
                    );
                }
                if self
                    .original_retention
                    .as_ref()
                    .is_some_and(|(fixed, _)| fixed != context.attestor())
                {
                    return Err(
                        "historical C0 context changed independently fixed original role".into(),
                    );
                }
                (context.binding(), context.attestor())
            }
            ProtectedInputAccessV1::Owned(context) => {
                if context.signer().state.private_retention != state.private_retention
                    || context.signer().state.binding_digest != state.binding_digest
                    || context.signer().state.revision < state.revision
                    || (context.signer().state.revision == state.revision
                        && context.signer().state.state_digest != state.state_digest)
                {
                    return Err(
                        "owned archive context differs from exact protected original State/head"
                            .into(),
                    );
                }
                if self
                    .original_retention
                    .as_ref()
                    .is_some_and(|(fixed, _)| fixed != context.attestor())
                {
                    return Err(
                        "owned archive context changed independently fixed original role".into(),
                    );
                }
                (context.binding(), context.attestor())
            }
        };
        let capability = role.digest().map_err(err)?;
        if role.source.binding_digest != binding.digest().map_err(err)?
            || state.binding_digest != role.source.binding_digest
            || state.private_retention.as_ref().is_none_or(|head| {
                head.source != role.source || head.capability_digest != capability
            })
        {
            return Err(
                "covered archive original fixed binding/provider/capability differs".into(),
            );
        }
        Ok((binding, role))
    }
    pub(super) fn load_protected_history(
        &self,
        metadata: &PrivateClosureV1,
        state: &BftSignerStateV3,
        access: &ProtectedInputAccessV1<'_, '_>,
        deadline: Instant,
    ) -> Result<Option<ValidatedProtectedRetentionArchiveV1>, String> {
        match (&metadata.protected_retention, &state.private_retention) {
            (None, None) => return Ok(None),
            (Some(_), Some(_))
                if metadata.publication_layout == Some(2) && metadata.active_base.is_none() => {}
            _ => {
                return Err(
                    "covered private archive lacks original protected history layout".into(),
                );
            }
        }
        let (binding, role) = self.protected_source(access, state)?;
        let reference = metadata
            .protected_retention
            .as_ref()
            .ok_or("covered history reference missing")?;
        let index: ProtectedRetentionIndexV1 =
            decode(&self.load(reference, MAX_METADATA, deadline)?, MAX_METADATA)?;
        let origin: storage::Origin = decode(
            &self.load(&metadata.origin, MAX_METADATA, deadline)?,
            MAX_METADATA,
        )?;
        let verified = ValidatedProtectedRetentionArchiveV1::verify(
            &self.archive,
            binding,
            role,
            state,
            &metadata.source,
            &metadata.private_instance_id,
            &origin.digest()?,
            index,
            deadline,
        )?;
        let expected_pending: BTreeSet<_> = verified
            .originals
            .keys()
            .filter(|key| key.height > metadata.cut.height)
            .map(|key| (key.height, key.header_id.clone()))
            .collect();
        let actual_pending: BTreeSet<_> = metadata
            .pending_candidates
            .iter()
            .map(|candidate| {
                if !candidate.roles.contains(&PendingRoleV1::ProtectedOriginal) {
                    return Err(
                        "covered pending candidate lacks protected original role".to_owned()
                    );
                }
                Ok((candidate.height, candidate.header_id.clone()))
            })
            .collect::<Result<_, _>>()?;
        if expected_pending != actual_pending
            || actual_pending.len() != metadata.pending_candidates.len()
        {
            return Err("covered archive omitted/added original T+1 alternative".into());
        }
        for candidate in &metadata.pending_candidates {
            let selected = verified
                .final_ledger
                .selection_index
                .as_ref()
                .is_some_and(|index| {
                    index.selections.iter().any(|selection| {
                        selection.key.height == candidate.height
                            && selection.key.header_id == candidate.header_id
                    })
                });
            if candidate.roles.contains(&PendingRoleV1::RetainedLocal) != selected {
                return Err(
                    "covered archive current selection role differs from protected original".into(),
                );
            }
        }
        let expected_publications: BTreeSet<_> = verified
            .originals
            .values()
            .map(|original| {
                let oasis7_distfs::protected_bft_v3::BftPrivateRetentionFactV1::Some {
                    original_data_ref,
                    metadata_ref,
                    ..
                } = &original.entry.fact
                else {
                    unreachable!()
                };
                (original_data_ref.clone(), metadata_ref.clone())
            })
            .collect();
        let actual_publications: BTreeSet<_> = metadata
            .publications
            .iter()
            .map(|publication| {
                if publication.original_data.is_some() {
                    return Err(
                        "source covered archive unexpectedly migrated original Data".to_owned()
                    );
                }
                Ok((
                    publication.data.digest.clone(),
                    publication.reservation.digest.clone(),
                ))
            })
            .collect::<Result<_, _>>()?;
        if expected_publications != actual_publications
            || actual_publications.len() != metadata.publications.len()
        {
            return Err("covered archive publications differ from all original Some facts".into());
        }
        Ok(Some(verified))
    }
    pub(super) fn scoped_original<'a>(
        &'a self,
        reference: &InputRef,
        access: &'a ProtectedInputAccessV1<'_, '_>,
        deadline: Instant,
    ) -> Result<ScopedInputV1<'a>, String> {
        live(deadline)?;
        match access {
            ProtectedInputAccessV1::HistoricalC0(context) => Ok(ScopedInputV1::Owned(
                context.owned_original(reference).map_err(err)?,
            )),
            ProtectedInputAccessV1::Owned(context) => Ok(ScopedInputV1::Owned(
                context.owned_original(reference).map_err(err)?,
            )),
            ProtectedInputAccessV1::External => {
                let (_, reader) = self
                    .original_retention
                    .as_ref()
                    .ok_or("independent original input reader unavailable")?;
                Ok(ScopedInputV1::External(
                    reader.read_original(reference, deadline).map_err(err)?,
                ))
            }
        }
    }
    /// Every original, including unsigned/unselected alternatives, is executed
    /// from its original authenticated parent. Only small verified pricing/ref
    /// sets survive each scoped read; no decoded raw-input inventory accumulates.
    pub(super) fn verify_protected_inputs(
        &self,
        metadata: &PrivateClosureV1,
        public_state: &BftSignerStateV3,
        protected: &ValidatedProtectedRetentionArchiveV1,
        segment_start: &FormalBftHeaderExecutionV3,
        h_base: &super::replay::ProtectedHReplayV1,
        access: &ProtectedInputAccessV1<'_, '_>,
        deadline: Instant,
    ) -> Result<Vec<node_execution::VerifiedPublicationOriginalV1>, String> {
        // restore_segment independently captured the real H and moved its
        // inventory into this strict shared base. Never capture all H through a
        // candidate overlay or require its accessed subset to include full H.
        let mut ordered = protected
            .originals
            .values()
            .map(|original| {
                let order = protected
                    .index
                    .operations
                    .iter()
                    .position(|reference| reference == &original.reservation)
                    .ok_or("protected original reservation is not in complete history")?;
                Ok((order, original))
            })
            .collect::<Result<Vec<_>, String>>()?;
        ordered.sort_by(|(a, x), (b, y)| a.cmp(b).then(x.entry.key.cmp(&y.entry.key)));
        let original_origin = self.load(
            &protected.index.private_origin,
            super::super::super::super::formal_ledger::MAX_RECORD,
            deadline,
        )?;
        let origin: storage::Origin = decode(
            &self.load(&metadata.origin, MAX_METADATA, deadline)?,
            MAX_METADATA,
        )?;
        if segment_start.schedule.height() != origin.first_height() || segment_start.tip.is_some() {
            return Err("covered original parent blueprint is not the verified initial H".into());
        }
        let mut retained = protected.activation.h_refs().clone();
        let mut batch = None;
        let mut batch_refs = BTreeSet::new();
        let mut prices = BTreeMap::new();
        let mut publications = Vec::new();
        for (order, original) in ordered {
            if batch.is_some_and(|old| old != order) {
                retained.append(&mut batch_refs);
            }
            batch = Some(order);
            let reservation = Operation::from_canonical_bytes(&self.load(
                &original.reservation,
                oasis7_distfs::protected_bft_v3::BFT_SIGNER_MAX_FRAME_BYTES_V3,
                deadline,
            )?)
            .map_err(|error| {
                format!(
                    "protected reservation codec {}: {error}",
                    original.reservation.digest
                )
            })?;
            let parent_history = reservation.state.history;
            if parent_history.len() > public_state.history.len()
                || parent_history != public_state.history[..parent_history.len()]
            {
                return Err(
                    "covered original parent is not exact authenticated canonical prefix".into(),
                );
            }
            // Do not nest the current 132MiB read with any earlier exact Input.
            // Each parent Record acquires and completes its own scoped read.
            let parent = self.replay_protected_parent_prefix(
                metadata,
                protected,
                &parent_history,
                h_base,
                access,
                &publications,
                deadline,
            )?;
            let (publication, price, references) = self.verify_scoped_original_execution(
                metadata,
                original,
                protected.selections.get(&original.entry.key),
                &original_origin,
                &origin,
                &parent_history,
                parent,
                h_base,
                access,
                &retained,
                deadline,
            )?;
            prices.insert(original.reference.admission_digest.clone(), price);
            publications.push(publication);
            batch_refs.extend(references);
            live(deadline)?;
            // The helper held the exact read through all original checks.
        }
        for accounting in &protected.accounting {
            accounting.verify_actual_prices(&prices).map_err(err)?;
        }
        Ok(publications)
    }
}

impl ActualBftSuccessorWindowDriverVerifierV1 {
    // Parent history replay must not retain this phase's World/plan temporaries
    // on its native stack. The exact Input scope stays inside this whole phase.
    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn verify_scoped_original_execution(
        &self,
        metadata: &PrivateClosureV1,
        original: &super::super::successor_archive::protected_retention::OriginalInputWitnessV1,
        selection: Option<&oasis7_distfs::protected_bft_v3::BftPrivateRetentionSelectionV1>,
        original_origin: &[u8],
        origin: &storage::Origin,
        parent_history: &[FormalBftCertifiedHeaderV3],
        parent: Box<FormalBftHeaderExecutionV3>,
        h_base: &super::replay::ProtectedHReplayV1,
        access: &ProtectedInputAccessV1<'_, '_>,
        retained: &BTreeSet<String>,
        deadline: Instant,
    ) -> Result<
        (
            node_execution::VerifiedPublicationOriginalV1,
            u64,
            BTreeSet<String>,
        ),
        String,
    > {
        let scoped = self.scoped_original(&original.reference, access, deadline)?;
        let input = scoped.input();
        if let Some(selected) = selection {
            selected.reference.validate_input(input).map_err(err)?;
            let identity = node_execution::source_selection_identity(input)?;
            if selected.source != input.source
                || selected.kind != identity.kind()
                || selected.local_identity_digest != identity.digest()
                || selected.local_identity_bytes != identity.bytes()
            {
                return Err(
                    "protected original selection identity differs from actual input".into(),
                );
            }
        }

        original.reference.validate_input(input).map_err(err)?;
        let publication = metadata
            .publications
            .iter()
            .find(|publication| {
                publication.data.digest
                    == match &original.entry.fact {
                        oasis7_distfs::protected_bft_v3::BftPrivateRetentionFactV1::Some {
                            original_data_ref,
                            ..
                        } => original_data_ref.as_str(),
                        _ => "",
                    }
            })
            .ok_or("covered original publication binding absent")?;
        for (kind, reference, max) in [
            (
                Kind::OriginalData,
                &publication.data,
                super::super::super::super::formal_ledger::MAX_RECORD,
            ),
            (
                Kind::PublicationMetadata,
                &publication.reservation,
                super::super::super::super::formal_ledger::MAX_PUBLICATION_METADATA_BYTES,
            ),
        ] {
            let object = input
                .objects
                .iter()
                .find(|object| object.kind == kind)
                .ok_or("covered original publication object absent")?;
            if reference != &object.reference
                || self.load(reference, max, deadline)? != object.bytes
            {
                return Err("covered archived original Data/metadata is missing or changed".into());
            }
        }
        // A grant/ref cannot substitute for complete archived bytes. The
        // encoded archive read is the second existing input buffer, dropped
        // before a detached execution seed is built.
        let archived = self.load(
            &original.reference.object,
            oasis7_distfs::protected_bft_v3::BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1,
            deadline,
        )?;
        original
            .reference
            .object
            .validate_bytes(&archived)
            .map_err(err)?;
        drop(archived);
        let candidate = node_execution::source_admission_candidate(input)?;
        let detached = self.fork_protected_current(&parent, h_base, input)?;
        drop(parent);
        if detached.schedule.height() != input.key.height {
            return Err("covered original does not execute at its authenticated parent".into());
        }
        let executed = detached.execute(&candidate.actions_cbor)?;
        if executed.candidate.header != candidate.public_value.header
            || executed.candidate.commitment != candidate.public_value.execution_commitment
        {
            return Err(
                "covered original candidate actions/value did not actually reproduce".into(),
            );
        }
        let price = detached.retained_nodes.with_accessed_nodes(|observed| {
            let supplied = input
                .objects
                .iter()
                .filter(|object| object.kind == Kind::Dependency);
            if supplied.clone().count() != observed.len()
                || supplied
                    .into_iter()
                    .any(|object| observed.get(&object.reference.digest) != Some(&object.bytes))
            {
                return Err("covered original exact closed dependency inventory differs".into());
            }
            let plan = super::super::super::publication_plan::PublicationPlanV1::from_actual(
                &detached.schedule,
                &executed,
                &candidate.actions_cbor,
                observed,
                &parent_history,
                storage::RECORD_DOMAIN,
            )?;
            node_execution::validate_source_admission_input(
                input,
                &original_origin,
                &candidate,
                &plan,
                observed,
            )?;
            node_execution::source_admission_archive_reservation_bytes(
                input, &plan, observed, &retained,
            )
        })?;
        if price != input.publication_reservation_bytes {
            return Err(
                "covered original publication price differs from actual original inventory".into(),
            );
        }
        let raw = node_execution::PublicationArchiveMaterialV1 {
            data: input
                .objects
                .iter()
                .find(|object| object.kind == Kind::OriginalData)
                .ok_or("covered original Data absent")?
                .bytes
                .clone(),
            original_data: None,
            reservation: input
                .objects
                .iter()
                .find(|object| object.kind == Kind::PublicationMetadata)
                .ok_or("covered original metadata absent")?
                .bytes
                .clone(),
        };
        let publication = node_execution::VerifiedPublicationOriginalV1::verify(
            &raw,
            node_execution::PublicationArchiveOwnerV1 {
                origin: &origin.digest()?,
                material: &metadata.source.material_identity,
                instance: &metadata.private_instance_id,
            },
            &[],
        )?;
        publication.validate_execution(&candidate.actions_cbor, &executed)?;
        let references = input
            .objects
            .iter()
            .filter(|object| object.kind == Kind::Dependency)
            .map(|object| object.reference.digest.clone())
            .collect();
        live(deadline)?;
        Ok((publication, price, references))
    }
}

impl ActualBftSuccessorWindowDriverVerifierV1 {
    pub(super) fn fork_protected_current(
        &self,
        current: &FormalBftHeaderExecutionV3,
        base: &super::replay::ProtectedHReplayV1,
        input: &Input,
    ) -> Result<FormalBftHeaderExecutionV3, String> {
        let baseline = current
            .retained_nodes
            .with_accessed_nodes(|nodes| Ok(nodes.keys().cloned().collect::<Vec<_>>()))?;
        let seed = input
            .objects
            .iter()
            .filter(|object| object.kind == Kind::Dependency)
            .map(|object| (object.reference.digest.clone(), object.bytes.clone()))
            .collect();
        let retained_nodes =
            formal_dependencies::RecordingBusinessIndexReaderV1::new_live_closed_layer(
                base.nodes.clone(),
                seed,
            )?;
        let next = FormalBftHeaderExecutionV3 {
            handoff: current.handoff.clone(),
            schedule: current.schedule.clone(),
            parent_record: current.parent_record.clone(),
            parent_world: current
                .parent_world
                .clone_with_independent_formal_history_reader(retained_nodes.clone())?,
            parent_result_tree: current.parent_result_tree.clone(),
            business_reader: formal_business_package::SharedBusinessReader(retained_nodes.clone()),
            retained_nodes,
            tip: current.tip.clone(),
        };
        for digest in baseline {
            if next.retained_nodes.read_node(&digest)?.is_none() {
                return Err("protected current overlay lost actual parent baseline".into());
            }
        }
        Ok(next)
    }
}
