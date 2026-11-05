//! Independent Active11/12/13 typed identities. Source9/10/5 keep their codecs.
use super::*;

pub(super) fn active_selection_data(data: &Data) -> Result<Data, String> {
    data.validate()?;
    if data.schema_version != 12 || data.origin {
        return Err("Active selection requires original Received12".into());
    }
    let mut selection = data.clone();
    selection.schema_version = 13;
    selection.domain = ACTIVE_SELECTION_DOMAIN.into();
    selection.actions_cbor.clear();
    selection.references = vec![data.digest()?];
    selection.publication_reservation_ref = None;
    selection.protected_source_origin_ref = None;
    selection.validate()?;
    Ok(selection)
}

pub(super) fn active_selection_bound(data: &Data) -> Result<usize, String> {
    if data.schema_version != 12 {
        return Ok(0);
    }
    let selection = active_selection_data(data)?;
    selection
        .bytes()?
        .len()
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_add(private_pin_bound(&selection).ok()?))
        .ok_or("Active Received12 future selection overflow".into())
}

/// Created only after the actual C0/prefix owner and original before-inventory
/// reproduce this exact Active input. Metadata alone cannot construct it.
pub(in super::super::super::super) struct VerifiedActiveAdmissionInputV1 {
    reference: InputRef,
    reservation_bytes: u64,
    publication_metadata_ref: String,
    selection_identity:
        oasis7_distfs::protected_bft_v3::VerifiedBftPrivateRetentionSelectionIdentityV1,
}
impl VerifiedActiveAdmissionInputV1 {
    pub(in super::super::super::super) fn into_selection_identity(
        self,
    ) -> oasis7_distfs::protected_bft_v3::VerifiedBftPrivateRetentionSelectionIdentityV1 {
        self.selection_identity
    }

    pub(in super::super::super::super) fn selection_identity(
        &self,
    ) -> &oasis7_distfs::protected_bft_v3::VerifiedBftPrivateRetentionSelectionIdentityV1 {
        &self.selection_identity
    }
    pub(in super::super::super::super) fn reference(&self) -> &InputRef {
        &self.reference
    }
    pub(in super::super::super::super) fn publication_metadata_ref(&self) -> &str {
        &self.publication_metadata_ref
    }
    pub(in super::super::super::super) fn reservation_bytes(&self) -> u64 {
        self.reservation_bytes
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Independent opaque parent, actual execution and exact original material stay explicit."
)]
fn verify_active_admission_material(
    input: &Input,
    owner: &super::super::super::super::successor_verifier::VerifiedActiveAdmissionOwnerV1,
    parent: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionAuthenticatedAdmissionParentV1<'_>,
    candidate: &NodeBftPrivateCandidateV3,
    plan: &super::super::super::super::super::publication_plan::PublicationPlanV1,
    nodes: &BTreeMap<String, Vec<u8>>,
) -> Result<VerifiedActiveAdmissionInputV1, String> {
    input.validate().map_err(|e| e.to_string())?;
    if owner.parent_state_digest != parent.state().state_digest
        || &owner.execution.schedule != parent.schedule()
    {
        return Err("Active input opaque parent differs from actual replay".into());
    }
    let original = input
        .objects
        .iter()
        .find(|object| object.kind == Kind::OriginalData)
        .ok_or("Active input original Data absent")?;
    let data: Data =
        serde_cbor::from_slice(&original.bytes).map_err(|_| "Active Data decode rejected")?;
    let origin = Data {
        schema_version: 3,
        domain: DOMAIN.into(),
        origin: true,
        ledger_origin_digest: owner.origin.digest()?,
        material_identity: owner.base.metadata.source.material_identity.clone(),
        instance_id: owner.base.metadata.private_instance_id.clone(),
        height: owner.base.metadata.cut.height,
        value: None,
        actions_cbor: Vec::new(),
        references: Vec::new(),
        publication_reservation_ref: None,
        publication_original_data_ref: None,
        source_activation: None,
        protected_source_origin_ref: None,
    };
    if data.bytes()? != original.bytes
        || !matches!(data.schema_version, 11 | 12)
        || data.origin
        || data.protected_source_origin_ref.as_ref() != Some(&origin.digest()?)
        || data.ledger_origin_digest != origin.ledger_origin_digest
        || data.material_identity != origin.material_identity
        || data.instance_id != origin.instance_id
        || data.value.as_ref() != Some(&candidate.public_value)
        || data.actions_cbor != candidate.actions_cbor
        || data.references != nodes.keys().cloned().collect::<Vec<_>>()
        || input.key.ledger_origin_digest != data.ledger_origin_digest
        || input.key.height != data.height
        || input.key.header_id != candidate.public_value.block_id()?
    {
        // Bounded identity diagnostics expose only mismatch positions/counts,
        // never the original payload, namespace digests or private material.
        let mismatches = [
            data.bytes().as_deref() != Ok(original.bytes.as_slice()),
            !matches!(data.schema_version, 11 | 12),
            data.origin,
            data.protected_source_origin_ref.as_ref() != origin.digest().ok().as_ref(),
            data.ledger_origin_digest != origin.ledger_origin_digest,
            data.material_identity != origin.material_identity,
            data.instance_id != origin.instance_id,
            data.value.as_ref() != Some(&candidate.public_value),
            data.actions_cbor != candidate.actions_cbor,
            data.references != nodes.keys().cloned().collect::<Vec<_>>(),
            input.key.ledger_origin_digest != data.ledger_origin_digest,
            input.key.height != data.height,
            candidate.public_value.block_id().as_deref() != Ok(input.key.header_id.as_str()),
        ];
        let mask = mismatches
            .iter()
            .enumerate()
            .fold(0u16, |mask, (bit, differs)| {
                mask | (u16::from(*differs) << bit)
            });
        return Err(format!(
            "Active input differs from original namespace/parent/candidate/dependencies: mismatch_mask={mask} original_refs={} replay_refs={}",
            data.references.len(),
            nodes.len(),
        ));
    }
    let identity = publication_identity(&data)?;
    let publication = PublicationReservationV1::from_plan(&identity, plan)?;
    let mut retained = owner.retained_baseline_refs().clone();
    if let Some(before) = parent.before_inventory() {
        for original in before.values() {
            if original.source != input.source {
                return Err("Active before-inventory source differs".into());
            }
            for object in &original.objects {
                if object.kind == Kind::Dependency {
                    retained.insert(object.reference.digest.clone());
                }
            }
        }
    }
    let cost = active_reservation_cost(&data, &publication, nodes, &retained)?;
    if input.publication_reservation_bytes != cost {
        return Err("Active input reservation differs from original authenticated price".into());
    }
    let local = if data.schema_version == 12 {
        active_selection_data(&data)?
    } else {
        data.clone()
    };
    let local_bytes = local.bytes()?;
    let selection_identity = oasis7_distfs::protected_bft_v3::VerifiedBftPrivateRetentionSelectionIdentityV1::from_independent_verification(
        if data.schema_version == 12 { oasis7_distfs::protected_bft_v3::BftPrivateRetentionSelectionKindV1::ReceivedLocal }
        else { oasis7_distfs::protected_bft_v3::BftPrivateRetentionSelectionKindV1::OriginalLocal },
        blake3_hex(&local_bytes), local_bytes.len() as u64).map_err(|e| e.to_string())?;
    let expected = SourceAdmissionPlanV1 {
        data,
        publication,
        full_usage: 0,
        future: BTreeMap::new(),
        original_usage: 0,
        reservation_bytes: cost,
        resumed: false,
    };
    expected.matches_input(input, nodes)?;
    Ok(VerifiedActiveAdmissionInputV1 {
        reference: input.descriptor().map_err(|e| e.to_string())?,
        reservation_bytes: cost,
        publication_metadata_ref: expected.publication.digest()?,
        selection_identity,
    })
}

pub(super) fn active_reservation_cost(
    data: &Data,
    publication: &PublicationReservationV1,
    nodes: &BTreeMap<String, Vec<u8>>,
    retained: &BTreeSet<String>,
) -> Result<u64, String> {
    if !matches!(data.schema_version, 11 | 12) {
        return Err("Active price requires explicit domain11/12".into());
    }
    let dependencies = nodes
        .iter()
        .filter(|(digest, _)| !retained.contains(*digest))
        .try_fold(0usize, |sum, (_, bytes)| {
            sum.checked_add(bytes.len())
                .ok_or("Active dependency growth overflow")
        })?;
    let bytes = data
        .bytes()?
        .len()
        .checked_mul(2)
        .and_then(|n| n.checked_add(publication.accounted_bytes().ok()?))
        .and_then(|n| n.checked_add(private_pin_bound(data).ok()?))
        .and_then(|n| n.checked_add(active_selection_bound(data).ok()?))
        .and_then(|n| n.checked_add(dependencies))
        .filter(|n| *n <= 512 * 1024 * 1024)
        .ok_or("Active reservation growth capacity")?;
    u64::try_from(bytes).map_err(|_| "Active price conversion".into())
}

pub(in super::super::super::super) fn verify_active_admission_original(
    input: &Input,
    owner: &super::super::super::super::successor_verifier::VerifiedActiveAdmissionOwnerV1,
    parent: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionAuthenticatedAdmissionParentV1<'_>,
) -> Result<VerifiedActiveAdmissionInputV1, String> {
    input.validate().map_err(|e| e.to_string())?;
    let original = input
        .objects
        .iter()
        .find(|v| v.kind == Kind::OriginalData)
        .ok_or("Active original Data absent")?;
    let data: Data =
        serde_cbor::from_slice(&original.bytes).map_err(|_| "Active original Data decode")?;
    if !matches!(data.schema_version, 11 | 12) {
        return Err("Active original domain rejected".into());
    }
    let candidate = NodeBftPrivateCandidateV3 {
        public_value: data.value.ok_or("Active value absent")?,
        actions_cbor: data.actions_cbor,
    };
    let seed = input
        .objects
        .iter()
        .filter(|v| v.kind == Kind::Dependency)
        .map(|v| (v.reference.digest.clone(), v.bytes.clone()))
        .collect();
    let actual = owner.execution.fork_replay_current(seed)?;
    let executed = actual.execute(&candidate.actions_cbor)?;
    if executed.candidate.header != candidate.public_value.header
        || executed.candidate.commitment != candidate.public_value.execution_commitment
    {
        return Err("Active original execution mismatch".into());
    }
    actual.retained_nodes.with_accessed_nodes(|nodes| {
        let plan =
            super::super::super::super::super::publication_plan::PublicationPlanV1::from_actual(
                &owner.execution.schedule,
                &executed,
                &candidate.actions_cbor,
                nodes,
                parent.history(),
                super::super::super::super::window_storage::RECORD_DOMAIN,
            )?;
        verify_active_admission_material(input, owner, parent, &candidate, &plan, nodes)
    })
}

fn active_data(input: &Input) -> Result<Data, String> {
    input.validate().map_err(|e| e.to_string())?;
    let bytes = &input
        .objects
        .iter()
        .find(|v| v.kind == Kind::OriginalData)
        .ok_or("Active Data absent")?
        .bytes;
    let data: Data = serde_cbor::from_slice(bytes).map_err(|_| "Active Data decode")?;
    if !matches!(data.schema_version, 11 | 12) || data.origin || data.bytes()? != *bytes {
        return Err("Active original domain/bytes rejected".into());
    }
    Ok(data)
}
impl NodePrivateStore {
    pub(in super::super::super) fn resume_active_admission_plan(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        actual: &super::super::super::super::super::publication_plan::PublicationPlanV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        input: &Input,
        proof: &ReservedSourceInputV1,
        retained: &BTreeSet<String>,
    ) -> Result<SourceAdmissionPlanV1, String> {
        proof
            .reference
            .validate_input(input)
            .map_err(|e| e.to_string())?;
        if input.fact().map_err(|e| e.to_string())? != proof.entry.fact {
            return Err("Active exact retry original fact changed".into());
        }
        let data = active_data(input)?;
        let kind = SourceSlotKindV1::from_protected_data(&data)?;
        let old = match kind {
            SourceSlotKindV1::Local => self.candidates.remove(&input.key.height),
            SourceSlotKindV1::Received => self
                .values
                .remove(&(input.key.height, input.key.header_id.clone())),
        };
        let planned = self.plan_active_admission(candidate, actual, nodes, kind, retained);
        if let Some(slot) = old {
            self.insert_source_slot(slot.data, slot.complete)?;
        }
        let mut planned = planned?;
        planned.matches_input(input, nodes)?;
        // This is the already authenticated original Reserve, not a new quote.
        // Current inventory growth cannot refund or reprice it; the service
        // independently verifies its historical first-Reserve predecessor.
        planned.reservation_bytes = input.publication_reservation_bytes;
        if self
            .source_slot(&planned.data)?
            .is_some_and(|slot| slot.data != planned.data)
        {
            return Err("Active exact retry WAL changed".into());
        }
        planned.resumed = true;
        Ok(planned)
    }
    pub(in super::super::super) fn verify_ready_active_candidate(
        &self,
        candidate: &NodeBftPrivateCandidateV3,
        reference: &InputRef,
        fact: &BftPrivateRetentionFactV1,
    ) -> Result<String, String> {
        let (slot, path) = self
            .source_slot_for_key(&reference.key)?
            .ok_or("Active original WAL missing")?;
        verify_original_slot(&self.store, &path, slot)?;
        verify_material(&self.store, &slot.data)?;
        if !slot.complete
            || !matches!(slot.data.schema_version, 11 | 12)
            || slot.data.value.as_ref() != Some(&candidate.public_value)
            || slot.data.actions_cbor != candidate.actions_cbor
        {
            return Err("Active Ready candidate changed".into());
        }
        let BftPrivateRetentionFactV1::Some {
            original_data_ref,
            metadata_ref,
            ..
        } = fact
        else {
            return Err("Active Ready is not Some".into());
        };
        if slot.data.digest()? != *original_data_ref
            || slot.data.publication_reservation_ref.as_ref() != Some(metadata_ref)
        {
            return Err("Active Ready Data/metadata differs from protected fact".into());
        }
        Ok(slot.data.digest()?)
    }
}

pub(in super::super::super::super) fn verify_active_admission_ready(
    directory: &Path,
    store: &LocalCasStore,
    input: &Input,
    grant: &VerifiedActiveAdmissionInputV1,
) -> Result<(), String> {
    grant
        .reference
        .validate_input(input)
        .map_err(|e| e.to_string())?;
    let data = active_data(input)?;
    let origin_path = directory.join("origin.cbor");
    let origin = read_slot(&origin_path)?.ok_or("Active original Origin3 missing")?;
    verify_original_slot(store, &origin_path, &origin)?;
    verify_material(store, &origin.data)?;
    if !origin.complete
        || origin.data.schema_version != 3
        || !origin.data.origin
        || data.protected_source_origin_ref.as_ref() != Some(&origin.data.digest()?)
        || data.ledger_origin_digest != origin.data.ledger_origin_digest
        || data.material_identity != origin.data.material_identity
        || data.instance_id != origin.data.instance_id
    {
        return Err("Active Ready original Origin3 changed".into());
    }
    let path = if data.schema_version == 11 {
        directory.join(format!("candidate-{:020}.cbor", data.height))
    } else {
        directory.join(format!(
            "candidate-{:020}-{}.cbor",
            data.height, input.key.header_id
        ))
    };
    let slot = read_slot(&path)?.ok_or("Active Ready original WAL missing")?;
    if !slot.complete
        || slot.data != data
        || data.publication_reservation_ref.as_deref() != Some(grant.publication_metadata_ref())
    {
        return Err("Active Ready original WAL/metadata changed".into());
    }
    verify_original_slot(store, &path, &slot)?;
    verify_material(store, &slot.data)
}

/// Physical Local choice only; caller must separately verify the current Head3
/// exact index/typed selection operation and original RetainedLocal role.
pub(in super::super::super::super) fn verify_active_selection_current(
    directory: &Path,
    store: &LocalCasStore,
    input: &Input,
    grant: &VerifiedActiveAdmissionInputV1,
) -> Result<(), String> {
    verify_active_admission_ready(directory, store, input, grant)?;
    let data = active_data(input)?;
    if data.schema_version == 11 {
        return Ok(());
    }
    let expected = active_selection_data(&data)?;
    let path = directory.join(format!("selection-{:020}.cbor", data.height));
    let slot = read_slot(&path)?.ok_or("Active original selection13 missing")?;
    if !slot.complete || slot.data != expected {
        return Err("Active original selection13 changed".into());
    }
    verify_original_slot(store, &path, &slot)?;
    verify_material(store, &slot.data)
}

impl NodePrivateStore {
    /// New Reserve pricing reads exact originals from one signed inventory.
    /// Pending material is already charged and cannot be excluded for lacking a WAL.
    pub(in super::super::super) fn active_retained_refs(
        &self,
        baseline: &BTreeSet<String>,
        binding: &BftSignerBindingV3,
        role: &BftPrivateRetentionAttestorV1,
        observation: &ValidatedBftPrivateRetentionObservationV1,
        io: &dyn oasis7_node::NodeBftAdmissionInputPublicationIoV1,
        deadline: Instant,
    ) -> Result<BTreeSet<String>, String> {
        let origin = &self
            .origin
            .as_ref()
            .ok_or("Active original Origin3 absent")?
            .data;
        if origin.schema_version != 3 {
            return Err("Active price owner is not Origin3".into());
        }
        let mut retained = baseline.clone();
        for entry in &observation.ledger().map.entries {
            if entry.key.ledger_origin_digest != origin.ledger_origin_digest {
                continue;
            }
            if Instant::now() >= deadline {
                return Err("Active original inventory deadline exhausted".into());
            }
            let reference = InputRef::from_entry(&observation.ledger().map.source, entry)
                .map_err(|e| e.to_string())?;
            let _proof = ReservedSourceInputV1::from_observation(
                binding,
                role,
                &reference,
                &entry.fact,
                observation,
            )?;
            // One original read permit at a time. Never collect a raw input union.
            let read = io.read_original(&reference, deadline)?;
            reference
                .validate_input(read.input())
                .map_err(|e| e.to_string())?;
            if read.input().fact().map_err(|e| e.to_string())? != entry.fact {
                return Err("Active price exact original fact changed".into());
            }
            let data = active_data(read.input())?;
            if data.ledger_origin_digest != origin.ledger_origin_digest
                || data.material_identity != origin.material_identity
                || data.instance_id != origin.instance_id
                || data.protected_source_origin_ref.as_ref() != Some(&origin.digest()?)
            {
                return Err("Active price original owner changed".into());
            }
            retained.extend(data.references);
            drop(read);
        }
        Ok(retained)
    }
}

pub(in super::super::super::super) fn active_admission_slot_kind(
    input: &Input,
) -> Result<SourceSlotKindV1, String> {
    SourceSlotKindV1::from_protected_data(&active_data(input)?)
}

pub(in super::super::super::super) fn active_admission_candidate(
    input: &Input,
) -> Result<NodeBftPrivateCandidateV3, String> {
    let data = active_data(input)?;
    Ok(NodeBftPrivateCandidateV3 {
        public_value: data.value.ok_or("Active original public value absent")?,
        actions_cbor: data.actions_cbor,
    })
}
