//! Exact protected source input construction/publication. A map/ref DTO alone
//! cannot construct ReservedSourceInputV1 or authorize private writes.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputRefV1 as InputRef,
    BftPrivateRetentionAdmissionInputV1 as Input, BftPrivateRetentionAttestorV1,
    BftPrivateRetentionEntryV1, BftPrivateRetentionFactV1,
    BftPrivateRetentionInputObjectKindV1 as Kind, BftPrivateRetentionInputObjectV1 as Object,
    BftPrivateRetentionLifecycleV1, BftPrivateRetentionSourceV1, BftSuccessorWindowArchiveRefV1,
    ValidatedBftPrivateRetentionObservationV1,
};
use std::time::Instant;

#[path = "driver_bft_v3_active_private_input.rs"]
mod active_admission;
#[path = "driver_bft_v3_protected_input_commit.rs"]
mod protected_commit;
#[path = "driver_bft_v3_protected_input_planner.rs"]
mod protected_planner;
#[path = "driver_bft_v3_source_received_selection.rs"]
mod received_selection;
pub(in super::super::super) use active_admission::{
    VerifiedActiveAdmissionInputV1, active_admission_candidate, active_admission_slot_kind,
    verify_active_admission_original, verify_active_admission_ready,
    verify_active_selection_current,
};
pub(super) use received_selection::{source_selection_bound, source_selection_data};
pub(in super::super::super) use received_selection::{
    source_selection_identity, verify_source_selection_current,
};

/// Caller intent applies only to a new exact key. An existing protected key
/// always resumes its original schema and slot, regardless of the caller route.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum SourceSlotKindV1 {
    Local,
    Received,
}
impl SourceSlotKindV1 {
    fn from_data(data: &Data) -> Result<Self, String> {
        match data.schema_version {
            9 | 11 => Ok(Self::Local),
            10 | 12 => Ok(Self::Received),
            _ => Err("protected source original slot kind absent".into()),
        }
    }
}
impl NodePrivateStore {
    pub(super) fn source_slot_path(&self, data: &Data) -> Result<PathBuf, String> {
        Ok(if matches!(data.schema_version, 4 | 7 | 10 | 12) {
            self.value_path(
                data.height,
                &data
                    .value
                    .as_ref()
                    .ok_or("source value absent")?
                    .block_id()?,
            )
        } else {
            self.candidate_path(data.height)
        })
    }
    fn source_slot_for_key(
        &self,
        key: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionKeyV1,
    ) -> Result<Option<(&Slot, PathBuf)>, String> {
        let local = self.candidates.get(&key.height).filter(|slot| {
            slot.data
                .value
                .as_ref()
                .is_some_and(|value| value.block_id().is_ok_and(|id| id == key.header_id))
        });
        let slot = local.or_else(|| self.values.get(&(key.height, key.header_id.clone())));
        slot.map(|slot| self.source_slot_path(&slot.data).map(|path| (slot, path)))
            .transpose()
    }
    fn source_slot(&self, data: &Data) -> Result<Option<&Slot>, String> {
        Ok(match SourceSlotKindV1::from_data(data)? {
            SourceSlotKindV1::Local => self.candidates.get(&data.height),
            SourceSlotKindV1::Received => self.values.get(&(
                data.height,
                data.value
                    .as_ref()
                    .ok_or("source value absent")?
                    .block_id()?,
            )),
        })
    }
    fn insert_source_slot(&mut self, data: Data, complete: bool) -> Result<(), String> {
        match SourceSlotKindV1::from_data(&data)? {
            SourceSlotKindV1::Local => {
                self.candidates.insert(data.height, Slot { data, complete });
            }
            SourceSlotKindV1::Received => {
                self.values.insert(
                    (
                        data.height,
                        data.value
                            .as_ref()
                            .ok_or("source value absent")?
                            .block_id()?,
                    ),
                    Slot { data, complete },
                );
            }
        }
        Ok(())
    }
}
pub(in super::super) struct SourceAdmissionPlanV1 {
    data: Data,
    publication: PublicationReservationV1,
    full_usage: usize,
    future: super::super::super::super::super::formal_ledger::PublicationPromiseInventoryV1,
    original_usage: usize,
    reservation_bytes: u64,
    resumed: bool,
}
impl SourceAdmissionPlanV1 {
    pub(in super::super) fn input(
        &self,
        source: &BftPrivateRetentionSourceV1,
        nodes: BTreeMap<String, Vec<u8>>,
    ) -> Result<Input, String> {
        let object = |kind, bytes: Vec<u8>| Object {
            kind,
            reference: BftSuccessorWindowArchiveRefV1 {
                digest: blake3_hex(&bytes),
                bytes: bytes.len() as u64,
            },
            bytes,
        };
        let mut objects = vec![
            object(Kind::OriginalData, self.data.bytes()?),
            object(Kind::PublicationMetadata, self.publication.bytes()?),
        ];
        for (digest, bytes) in nodes {
            if blake3_hex(&bytes) != digest {
                return Err("source input dependency hash mismatch".into());
            }
            objects.push(object(Kind::Dependency, bytes));
        }
        objects.sort_by(|a, b| a.reference.digest.cmp(&b.reference.digest));
        let input = Input {
            schema_version: 1,
            source: source.clone(),
            key: oasis7_distfs::protected_bft_v3::BftPrivateRetentionKeyV1 {
                ledger_origin_digest: self.data.ledger_origin_digest.clone(),
                height: self.data.height,
                header_id: self
                    .data
                    .value
                    .as_ref()
                    .ok_or("source input public value missing")?
                    .block_id()?,
            },
            publication_reservation_bytes: self.reservation_bytes,
            objects,
        };
        input
            .validate()
            .map_err(|_| "source original input capacity/binding rejected")?;
        Ok(input)
    }
    pub(in super::super) fn matches_input(
        &self,
        input: &Input,
        nodes: &BTreeMap<String, Vec<u8>>,
    ) -> Result<(), String> {
        input.validate().map_err(|e| e.to_string())?;
        let data = self.data.bytes()?;
        let metadata = self.publication.bytes()?;
        if input.objects.len() != nodes.len() + 2 {
            return Err("original source input object count differs".into());
        }
        for object in &input.objects {
            let expected: &[u8] = match object.kind {
                Kind::OriginalData => &data,
                Kind::PublicationMetadata => &metadata,
                Kind::Dependency => nodes
                    .get(&object.reference.digest)
                    .ok_or("original source input dependency absent")?,
            };
            if expected != object.bytes.as_slice() {
                return Err(
                    "original source input differs from actual Data/metadata/closed dependency set"
                        .into(),
                );
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
pub(in super::super) struct ReservedSourceInputV1 {
    pub(in super::super) entry: BftPrivateRetentionEntryV1,
    pub(in super::super) reference: InputRef,
    selection: Option<oasis7_distfs::protected_bft_v3::BftPrivateRetentionSelectionV1>,
    selection_verified: bool,
}
impl ReservedSourceInputV1 {
    pub(in super::super) fn require_ready(&self) -> Result<(), String> {
        if self.entry.lifecycle != BftPrivateRetentionLifecycleV1::Ready {
            return Err("original protected input not Ready".into());
        }
        Ok(())
    }
    pub(in super::super) fn has_original_selection(&self) -> bool {
        self.selection_verified
            && self.selection.as_ref().is_some_and(|selection| {
                selection.key == self.reference.key && selection.reference == self.reference
            })
    }
    pub(in super::super) fn current_selection_operation(&self) -> Option<String> {
        self.selection
            .as_ref()
            .map(|selection| selection.operation_digest.clone())
    }
    pub(in super::super) fn verify_original_selection(
        &mut self,
        original: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionOperationProofV1,
    ) -> Result<(), String> {
        let selected = self
            .selection
            .as_ref()
            .ok_or("protected current choice absent")?;
        let entry = original
            .ledger
            .selection_index
            .as_ref()
            .and_then(|index| {
                index
                    .selections
                    .iter()
                    .find(|entry| entry.key == self.reference.key)
            })
            .ok_or("original selection operation lacks exact choice")?;
        if entry.operation.digest().map_err(|e| e.to_string())? != entry.operation_digest {
            return Err("original selection typed operation domain/body differs".into());
        }
        if entry != selected
            || entry.reference != self.reference
            || original.ledger.receipt.body.operation_digest != entry.operation_digest
            || original.state.revision != original.ledger.receipt.body.after_revision
            || original.state.state_digest != original.ledger.receipt.body.after_state_digest
            || original.state.private_retention.as_ref() != Some(&original.ledger.receipt.body.head)
            || original
                .ledger
                .map
                .entries
                .iter()
                .find(|entry| entry.key == self.entry.key)
                != Some(&self.entry)
        {
            return Err("original selection receipt/post-State/current choice differs".into());
        }
        self.selection_verified = true;
        Ok(())
    }
    pub(in super::super) fn from_observation(
        binding: &BftSignerBindingV3,
        role: &BftPrivateRetentionAttestorV1,
        reference: &InputRef,
        fact: &BftPrivateRetentionFactV1,
        observation: &ValidatedBftPrivateRetentionObservationV1,
    ) -> Result<Self, String> {
        let checkpoint = observation.checkpoint();
        let head = checkpoint
            .state
            .private_retention
            .as_ref()
            .ok_or("source protected input head missing")?;
        if checkpoint.state.binding_digest != binding.digest().map_err(|e| e.to_string())?
            || checkpoint.provider_public_key != role.provider_public_key
            || head.capability_digest != role.digest().map_err(|e| e.to_string())?
            || !matches!(head.schema_version, 2 | 3)
            || head.source != role.source
            || reference.source != role.source
        {
            return Err(
                "source protected reservation differs from fixed original role/binding".into(),
            );
        }
        let entry = observation
            .ledger()
            .map
            .entries
            .iter()
            .find(|entry| entry.key == reference.key)
            .ok_or("source protected original input entry missing")?;
        if &entry.fact != fact
            || InputRef::from_entry(&observation.ledger().map.source, entry)
                .map_err(|e| e.to_string())?
                != *reference
        {
            return Err("source original reservation fact/ref changed".into());
        }
        Ok(Self {
            entry: entry.clone(),
            reference: reference.clone(),
            selection_verified: false,
            selection: observation
                .ledger()
                .selection_index
                .as_ref()
                .and_then(|index| {
                    index
                        .selections
                        .iter()
                        .find(|selected| selected.key == reference.key)
                })
                .cloned(),
        })
    }
}

impl NodePrivateStore {
    pub(in super::super) fn publish_source_admission(
        &mut self,
        plan: SourceAdmissionPlanV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        proof: &ReservedSourceInputV1,
        deadline: Instant,
        reserve: &mut impl FnMut(
            usize,
            super::super::super::super::super::formal_ledger::PublicationPromiseInventoryV1,
        ) -> Result<(), String>,
        verify_owner: &mut impl FnMut() -> Result<(), String>,
    ) -> Result<String, String> {
        let live = || -> Result<(), String> {
            if Instant::now() >= deadline {
                Err("source original publication expired".into())
            } else {
                Ok(())
            }
        };
        live()?;
        verify_owner()?;
        let BftPrivateRetentionFactV1::Some {
            original_data_ref,
            metadata_ref,
            ..
        } = &proof.entry.fact
        else {
            return Err("source original publication lacks Some proof".into());
        };
        if &plan.data.digest()? != original_data_ref
            || &plan.publication.digest()? != metadata_ref
            || proof.reference.key.height != plan.data.height
            || proof.reference.key.header_id
                != plan
                    .data
                    .value
                    .as_ref()
                    .ok_or("source value absent")?
                    .block_id()?
        {
            return Err("source publication differs from original protected Data/metadata".into());
        }
        if !plan.resumed && self.usage()? != plan.original_usage {
            return Err("source material changed after original reservation plan".into());
        }
        reserve(plan.full_usage, plan.future)?;
        live()?;
        verify_owner()?;
        let path = self.source_slot_path(&plan.data)?;
        let complete = self
            .source_slot(&plan.data)?
            .is_some_and(|old| old.complete);
        if self
            .source_slot(&plan.data)?
            .is_some_and(|old| old.data != plan.data)
        {
            return Err("source original Data identity changed".into());
        }
        let pins = self
            .store
            .list_pin_scope_shards(SCOPE)
            .map_err(|_| "inspect source original pins")?
            .contains(&plan.data.shard());
        if retention_required(&path)? || pins {
            verify_material(&self.store, &plan.data)?;
        }
        self.insert_source_slot(plan.data.clone(), false)?;
        live()?;
        verify_owner()?;
        files::publish(
            &path.with_extension("reservation.cbor"),
            &plan.data.bytes()?,
        )?;
        live()?;
        verify_owner()?;
        common_storage::put_cas_durable(
            &self.store,
            &plan.publication.bytes()?,
            super::super::super::super::publication_reservation::MAX_PUBLICATION_METADATA,
        )?;
        publish_checked(
            &self.store,
            &path,
            &plan.data,
            nodes,
            complete,
            false,
            &mut || {
                live()?;
                verify_owner()
            },
        )?;
        let digest = plan.data.digest()?;
        self.insert_source_slot(plan.data, true)?;
        live()?;
        verify_owner()?;
        Ok(digest)
    }
}

pub(super) fn private_pin_bound(data: &Data) -> Result<usize, String> {
    #[derive(serde::Serialize)]
    struct Receipt {
        pins: BTreeSet<String>,
    }
    serde_json::to_vec(&Receipt {
        pins: data.retained()?,
    })
    .map_err(|_| "encode source private pin bound")?
    .len()
    .checked_mul(2)
    .ok_or("source private pin bound overflow".into())
}

/// Structural extraction only. Actual execution and fixed source remain the
/// independent verifier's responsibility.
pub(in super::super) fn source_admission_slot_kind(
    input: &Input,
) -> Result<SourceSlotKindV1, String> {
    SourceSlotKindV1::from_data(&input_data(input)?)
}
pub(in super::super::super) fn validate_source_admission_slot_kind(
    input: &Input,
) -> Result<(), String> {
    source_admission_slot_kind(input).map(|_| ())
}
pub(in super::super::super) fn source_admission_candidate(
    input: &Input,
) -> Result<NodeBftPrivateCandidateV3, String> {
    let data = input_data(input)?;
    Ok(NodeBftPrivateCandidateV3 {
        public_value: data.value.ok_or("input value absent")?,
        actions_cbor: data.actions_cbor,
    })
}
fn input_data(input: &Input) -> Result<Data, String> {
    input.validate().map_err(|e| e.to_string())?;
    let bytes = &input
        .objects
        .iter()
        .find(|o| o.kind == Kind::OriginalData)
        .ok_or("input Data absent")?
        .bytes;
    let data: Data = serde_cbor::from_slice(bytes).map_err(|_| "decode input Data")?;
    if data.bytes()? != *bytes || !matches!(data.schema_version, 9 | 10) || data.origin {
        return Err("input original Data schema/canonical bytes differ".into());
    }
    Ok(data)
}
pub(in super::super::super) fn read_source_origin(
    records: &Path,
    store: &LocalCasStore,
) -> Result<Vec<u8>, String> {
    let dir = records
        .join(storage::DIRECTORY)
        .join(storage::NODE_PRIVATE_DIRECTORY);
    crate::controlled_live_config::require_private_owned(&dir, true)?;
    let path = dir.join("origin.cbor");
    let slot = read_slot(&path)?.ok_or("original covered source Origin missing")?;
    if !slot.complete || slot.data.schema_version != 8 || !slot.data.origin {
        return Err("original covered source Origin incomplete".into());
    }
    slot.data
        .source_activation
        .as_ref()
        .ok_or("source activation missing")?
        .verify_directory(&dir)?;
    verify_original_slot(store, &path, &slot)?;
    verify_material(store, &slot.data)?;
    slot.data.bytes()
}
pub(in super::super::super) fn validate_source_admission_input(
    input: &Input,
    origin_data_bytes: &[u8],
    candidate: &NodeBftPrivateCandidateV3,
    plan: &super::super::super::super::publication_plan::PublicationPlanV1,
    nodes: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let origin: Data =
        serde_cbor::from_slice(origin_data_bytes).map_err(|_| "decode original source Origin")?;
    let data = input_data(input)?;
    if origin.bytes()? != origin_data_bytes
        || origin.schema_version != 8
        || !origin.origin
        || data.protected_source_origin_ref.as_ref() != Some(&origin.digest()?)
        || data.ledger_origin_digest != origin.ledger_origin_digest
        || data.material_identity != origin.material_identity
        || data.instance_id != origin.instance_id
        || data.height <= origin.height
        || data.height - origin.height > 64
        || data.value.as_ref() != Some(&candidate.public_value)
        || data.actions_cbor != candidate.actions_cbor
        || data.references != nodes.keys().cloned().collect::<Vec<_>>()
        || input.key.ledger_origin_digest != data.ledger_origin_digest
        || input.key.height != data.height
        || input.key.header_id != candidate.public_value.block_id()?
    {
        return Err("input differs from original source/candidate/closed dependencies".into());
    }
    let publication = PublicationReservationV1::from_plan(&publication_identity(&data)?, plan)?;
    publication.validate_identity(&publication_identity(&data)?)?;
    let expected = SourceAdmissionPlanV1 {
        data,
        publication,
        full_usage: 0,
        future: BTreeMap::new(),
        original_usage: 0,
        reservation_bytes: input.publication_reservation_bytes,
        resumed: false,
    };
    expected.matches_input(input, nodes)
}
pub(in super::super::super) fn verify_source_admission_ready(
    records: &Path,
    store: &LocalCasStore,
    input: &Input,
) -> Result<(), String> {
    let original = read_source_origin(records, store)?;
    let data = input_data(input)?;
    if data.protected_source_origin_ref.as_ref() != Some(&blake3_hex(&original)) {
        return Err("ready source Origin differs".into());
    }
    let path = records
        .join(storage::DIRECTORY)
        .join(storage::NODE_PRIVATE_DIRECTORY)
        .join(
            if SourceSlotKindV1::from_data(&data)? == SourceSlotKindV1::Local {
                format!("candidate-{:020}.cbor", data.height)
            } else {
                format!("candidate-{:020}-{}.cbor", data.height, input.key.header_id)
            },
        );
    let slot = read_slot(&path)?.ok_or("ready original candidate absent")?;
    if !slot.complete || slot.data != data {
        return Err("ready original candidate incomplete or changed".into());
    }
    verify_original_slot(store, &path, &slot)?;
    verify_material(store, &data)?;
    for object in &input.objects {
        if common_storage::read_cas_bounded(store, &object.reference.digest, object.bytes.len())?
            != object.bytes
        {
            return Err("ready exact input object missing or changed".into());
        }
    }
    Ok(())
}

impl NodePrivateStore {
    /// Current protected Ready is a readback obligation, never permission to
    /// reconstruct a lost original from backend input or a new local WAL.
    pub(in super::super) fn verify_ready_source_entry(
        &self,
        reference: &InputRef,
        fact: &BftPrivateRetentionFactV1,
    ) -> Result<(), String> {
        let origin = &self
            .origin
            .as_ref()
            .ok_or("Ready source original Origin missing")?
            .data;
        let (slot, path) = self
            .source_slot_for_key(&reference.key)?
            .ok_or("Ready protected source original WAL missing")?;
        let BftPrivateRetentionFactV1::Some {
            original_data_ref,
            metadata_ref,
            admission_digest,
            input_ref: Some(input_ref),
        } = fact
        else {
            return Err("covered source Ready entry lacks original Some input".into());
        };
        if !slot.complete
            || !matches!(slot.data.schema_version, 9 | 10)
            || reference.key.ledger_origin_digest != origin.ledger_origin_digest
            || slot.data.ledger_origin_digest != reference.key.ledger_origin_digest
            || slot.data.height != reference.key.height
            || slot
                .data
                .value
                .as_ref()
                .ok_or("Ready source original value absent")?
                .block_id()?
                != reference.key.header_id
            || slot.data.protected_source_origin_ref.as_ref() != Some(&origin.digest()?)
            || slot.data.material_identity != origin.material_identity
            || slot.data.instance_id != origin.instance_id
            || slot.data.digest()? != *original_data_ref
            || slot.data.publication_reservation_ref.as_ref() != Some(metadata_ref)
            || reference.admission_digest != *admission_digest
            || reference.object != *input_ref
        {
            return Err("Ready protected source original identity/fact differs".into());
        }
        verify_original_slot(&self.store, &path, slot)?;
        verify_material(&self.store, &slot.data)
    }
    pub(in super::super) fn verify_ready_source_candidate(
        &self,
        candidate: &NodeBftPrivateCandidateV3,
        reference: &InputRef,
        fact: &BftPrivateRetentionFactV1,
    ) -> Result<String, String> {
        let (slot, path) = self
            .source_slot_for_key(&reference.key)?
            .ok_or("Ready source original candidate missing")?;
        verify_original_slot(&self.store, &path, slot)?;
        verify_material(&self.store, &slot.data)?;
        if !slot.complete
            || !matches!(slot.data.schema_version, 9 | 10)
            || slot.data.value.as_ref() != Some(&candidate.public_value)
            || slot.data.actions_cbor != candidate.actions_cbor
        {
            return Err("Ready source original candidate changed".into());
        }
        let BftPrivateRetentionFactV1::Some {
            original_data_ref,
            metadata_ref,
            ..
        } = fact
        else {
            return Err("Ready source input is not Some".into());
        };
        if slot.data.digest()? != *original_data_ref
            || slot.data.publication_reservation_ref.as_ref() != Some(metadata_ref)
        {
            return Err("Ready source protected Data/metadata changed".into());
        }
        slot.data.digest()
    }
}

pub(in super::super::super) fn verify_source_input_context_role(
    records: &Path,
    store: &LocalCasStore,
    binding: &BftSignerBindingV3,
    context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
) -> Result<(), String> {
    let origin = read_source_origin(records, store)?;
    let data: Data =
        serde_cbor::from_slice(&origin).map_err(|_| "decode source original role owner")?;
    let activation = data
        .source_activation
        .as_ref()
        .ok_or("source original activation missing")?;
    let bytes = common_storage::read_cas_bounded(
        store,
        &activation.reference,
        usize::try_from(activation.encoded_bytes)
            .map_err(|_| "source original role capsule size")?,
    )?;
    super::super::super::source_creation::activation::validate_input_context_role(
        &bytes, binding, context,
    )
}

pub(in super::super::super) fn verify_source_seal_context_role(
    records: &Path,
    store: &LocalCasStore,
    binding: &BftSignerBindingV3,
    context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionSealVerificationContextV1<'_>,
) -> Result<(), String> {
    let origin = read_source_origin(records, store)?;
    let data: Data =
        serde_cbor::from_slice(&origin).map_err(|_| "decode source seal original role")?;
    let activation = data
        .source_activation
        .as_ref()
        .ok_or("source seal original activation missing")?;
    let bytes = common_storage::read_cas_bounded(
        store,
        &activation.reference,
        usize::try_from(activation.encoded_bytes)
            .map_err(|_| "source seal original role capsule size")?,
    )?;
    super::super::super::source_creation::activation::validate_input_role(
        &bytes,
        binding,
        context.attestor(),
        &context.signer().state,
    )
}

pub(in super::super::super) fn verify_source_input_historical_role(
    records: &Path,
    store: &LocalCasStore,
    binding: &oasis7_distfs::protected_bft_v3::BftSignerBindingV3,
    context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionHistoricalInputContextV1<'_>,
) -> Result<(), String> {
    let original = read_source_origin(records, store)?;
    let data: Data =
        serde_cbor::from_slice(&original).map_err(|_| "decode original source Origin")?;
    let activation = data
        .source_activation
        .as_ref()
        .ok_or("source original capsule missing")?;
    let bytes = common_storage::read_cas_bounded(
        store,
        &activation.reference,
        usize::try_from(activation.encoded_bytes)
            .map_err(|_| "source original role capsule size")?,
    )?;
    super::super::super::source_creation::activation::validate_input_role(
        &bytes,
        binding,
        context.attestor(),
        &context.reservation().signer.state,
    )
}

/// Reconstruct the pre-publication private inventory without opening any ledger
/// writer. Removing this exact candidate in memory models its original growth;
/// no disk object, promise or protected entry is removed.
pub(in super::super::super) fn source_admission_reservation_bytes(
    records: &Path,
    store: &LocalCasStore,
    input: &Input,
    plan: &super::super::super::super::publication_plan::PublicationPlanV1,
    nodes: &BTreeMap<String, Vec<u8>>,
    context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
    h_refs: &BTreeSet<String>,
) -> Result<u64, String> {
    source_admission_reservation_bytes_inner(records, store, input, plan, nodes, context, h_refs)
}

pub(in super::super::super) fn source_admission_historical_reservation_bytes(
    records: &Path,
    store: &LocalCasStore,
    input: &Input,
    plan: &super::super::super::super::publication_plan::PublicationPlanV1,
    nodes: &BTreeMap<String, Vec<u8>>,
    context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionHistoricalInputContextV1<'_>,
    h_refs: &BTreeSet<String>,
) -> Result<u64, String> {
    source_admission_reservation_bytes_inner(records, store, input, plan, nodes, context, h_refs)
}

fn source_admission_reservation_bytes_inner(
    records: &Path,
    store: &LocalCasStore,
    input: &Input,
    plan: &super::super::super::super::publication_plan::PublicationPlanV1,
    nodes: &BTreeMap<String, Vec<u8>>,
    context: &dyn SourceBudgetAuthority,
    h_refs: &BTreeSet<String>,
) -> Result<u64, String> {
    let original = read_source_origin(records, store)?;
    let origin: Data =
        serde_cbor::from_slice(&original).map_err(|_| "decode original source Origin")?;
    let data = input_data(input)?;
    let publication = PublicationReservationV1::from_plan(&publication_identity(&data)?, plan)?;
    if origin.references.iter().ne(h_refs.iter()) {
        return Err("source original H inventory differs from independent H closure".into());
    }
    let ledger = context
        .ledger()
        .ok_or("source budget lacks original covered map")?;
    if ledger.map.source != input.source || context.binding_digest() != input.source.binding_digest
    {
        return Err("source budget original protected context differs".into());
    }
    // Only independently reproduced H and same-backend immutable original
    // inputs authorize deduplication. Local slots are checked below but never
    // supply authority to reduce the new publication reservation.
    let mut retained = h_refs.clone();
    context.add_deduction_refs(input, &mut retained)?;
    let dir = records
        .join(storage::DIRECTORY)
        .join(storage::NODE_PRIVATE_DIRECTORY);
    let mut targets = BTreeSet::new();
    for (count, entry) in fs::read_dir(&dir)
        .map_err(|_| "read original source budget inventory")?
        .enumerate()
    {
        if count > 8 * (MAX_CANDIDATES + 64) + 8 {
            return Err("source budget inventory capacity".into());
        }
        let entry = entry.map_err(|_| "read source budget inventory entry")?;
        if !entry
            .file_type()
            .map_err(|_| "read source budget inventory type")?
            .is_file()
        {
            return Err("source budget inventory nonregular entry".into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "source budget inventory filename")?;
        let target = files::stage_target(&name).unwrap_or(&name);
        let base = target
            .strip_suffix(".reservation.cbor")
            .or_else(|| target.strip_suffix(".intent.cbor"))
            .map(|s| format!("{s}.cbor"))
            .unwrap_or_else(|| target.to_owned());
        if ["origin.cbor", "ready"].contains(&base.as_str()) {
            continue;
        }
        if base.starts_with("candidate-") || multivalue::parse_target(&base)?.is_some() {
            targets.insert(base);
        } else {
            return Err("source budget unknown original private inventory".into());
        }
    }
    if targets.len() > MAX_CANDIDATES + 64 {
        return Err("source budget inventory count capacity".into());
    }
    for target in targets {
        let slot = read_slot(&dir.join(target))?.ok_or("source budget original slot missing")?;
        if slot.data.ledger_origin_digest != origin.ledger_origin_digest
            || slot.data.material_identity != origin.material_identity
            || slot.data.instance_id != origin.instance_id
        {
            return Err("source budget original owner differs".into());
        }
        if slot.data.schema_version == 5 {
            let value = slot
                .data
                .value
                .as_ref()
                .ok_or("source selection value absent")?;
            let key = oasis7_distfs::protected_bft_v3::BftPrivateRetentionKeyV1 {
                ledger_origin_digest: slot.data.ledger_origin_digest.clone(),
                height: slot.data.height,
                header_id: value.block_id()?,
            };
            let entry = context
                .original_entry(&key)
                .map_err(|e| e.to_string())?
                .ok_or("source selection lacks protected original fact")?;
            let BftPrivateRetentionFactV1::Some {
                admission_digest, ..
            } = &entry.fact
            else {
                return Err("source selection original is not Some".into());
            };
            let original_input = context
                .original_input(admission_digest)
                .ok_or("source selection original input absent")?;
            InputRef::from_entry(&ledger.map.source, entry)
                .map_err(|e| e.to_string())?
                .validate_input(original_input)
                .map_err(|e| e.to_string())?;
            let original = input_data(original_input)?;
            if source_selection_data(&original)? != slot.data {
                return Err(
                    "source selection differs from original protected received Data".into(),
                );
            }
            // Selection refs never authorize business-index deduplication.
            continue;
        }
        let key = oasis7_distfs::protected_bft_v3::BftPrivateRetentionKeyV1 {
            ledger_origin_digest: slot.data.ledger_origin_digest.clone(),
            height: slot.data.height,
            header_id: slot
                .data
                .value
                .as_ref()
                .ok_or("source budget original value missing")?
                .block_id()?,
        };
        let entry = context
            .original_entry(&key)
            .map_err(|error| error.to_string())?
            .ok_or("local source slot has no protected original fact")?;
        let BftPrivateRetentionFactV1::Some {
            admission_digest,
            original_data_ref,
            metadata_ref,
            ..
        } = &entry.fact
        else {
            return Err("source budget local slot is not protected Some".into());
        };
        let original_input = context
            .original_input(admission_digest)
            .ok_or("source budget protected original input missing")?;
        let original_data = original_input
            .objects
            .iter()
            .find(|object| object.kind == Kind::OriginalData)
            .ok_or("source budget protected original Data missing")?;
        if slot.data.bytes()? != original_data.bytes
            || slot.data.digest()? != *original_data_ref
            || slot.data.publication_reservation_ref.as_ref() != Some(metadata_ref)
        {
            return Err(
                "local source budget slot differs from protected original Data/metadata".into(),
            );
        }
        if key == input.key
            && original_input.descriptor().map_err(|e| e.to_string())?
                != input.descriptor().map_err(|e| e.to_string())?
        {
            return Err("source budget excluded a different original target".into());
        }
    }

    let dependencies = nodes
        .iter()
        .filter(|(digest, _)| !retained.contains(*digest))
        .try_fold(0usize, |total, (_, bytes)| {
            total
                .checked_add(bytes.len())
                .ok_or("source dependency growth overflow")
        })?;
    // The producer's 2*unbound + 2*(bound-unbound) is exactly 2*bound.
    // Measure the original Data directly, without cloning its action payload.
    let bytes = data
        .bytes()?
        .len()
        .checked_mul(2)
        .and_then(|n| n.checked_add(publication.accounted_bytes().ok()?))
        .and_then(|n| n.checked_add(private_pin_bound(&data).ok()?))
        .and_then(|n| n.checked_add(source_selection_bound(&data).ok()?))
        .and_then(|n| n.checked_add(dependencies))
        .filter(|n| *n <= 512 * 1024 * 1024)
        .ok_or("source publication reservation growth capacity")?;
    u64::try_from(bytes).map_err(|_| "source reservation growth conversion".into())
}

impl NodePrivateStore {
    pub(in super::super) fn resume_source_admission_plan(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        actual: &super::super::super::super::publication_plan::PublicationPlanV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        input: &Input,
        proof: &ReservedSourceInputV1,
    ) -> Result<SourceAdmissionPlanV1, String> {
        proof
            .reference
            .validate_input(input)
            .map_err(|e| e.to_string())?;
        if input.fact().map_err(|e| e.to_string())? != proof.entry.fact {
            return Err("recovered protected input fact changed".into());
        }
        let data = input_data(input)?;
        let kind = SourceSlotKindV1::from_data(&data)?;
        let old = match kind {
            SourceSlotKindV1::Local => self.candidates.remove(&input.key.height),
            SourceSlotKindV1::Received => self
                .values
                .remove(&(input.key.height, input.key.header_id.clone())),
        };
        let result = self.plan_source_admission(candidate, actual, nodes, kind);
        if let Some(slot) = old {
            self.insert_source_slot(slot.data, slot.complete)?;
        }
        let mut plan = result?;
        plan.matches_input(input, nodes)?;
        if plan.reservation_bytes != input.publication_reservation_bytes {
            return Err("recovered original reservation budget changed".into());
        }
        if self
            .source_slot(&plan.data)?
            .is_some_and(|s| s.data != plan.data)
        {
            return Err("recovered original candidate identity changed".into());
        }
        plan.resumed = true;
        Ok(plan)
    }
}

// Private implementation interface: callers can enter only with one of the
// Distfs service-created opaque current/historical contexts above.
trait SourceBudgetAuthority {
    fn ledger(&self) -> Option<&oasis7_distfs::protected_bft_v3::BftPrivateRetentionLedgerV1>;
    fn binding_digest(&self) -> &str;
    fn add_deduction_refs(
        &self,
        input: &Input,
        retained: &mut BTreeSet<String>,
    ) -> Result<(), String>;
    fn original_entry(
        &self,
        key: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionKeyV1,
    ) -> Result<
        Option<&oasis7_distfs::protected_bft_v3::BftPrivateRetentionEntryV1>,
        oasis7_distfs::protected_bft_v3::BftSignerErrorV3,
    >;
    fn original_input(&self, digest: &str) -> Option<&std::sync::Arc<Input>>;
}
fn add_original_deduction_refs<'a>(
    originals: impl Iterator<Item = &'a std::sync::Arc<Input>>,
    input: &Input,
    retained: &mut BTreeSet<String>,
) -> Result<(), String> {
    for original in originals {
        if original.key == input.key {
            if original.descriptor().map_err(|e| e.to_string())?
                != input.descriptor().map_err(|e| e.to_string())?
                || original.fact().map_err(|e| e.to_string())?
                    != input.fact().map_err(|e| e.to_string())?
            {
                return Err(
                    "source budget target differs from protected original key/ref/fact".into(),
                );
            }
            continue;
        }
        retained.extend(
            original
                .objects
                .iter()
                .filter(|object| object.kind == Kind::Dependency)
                .map(|object| object.reference.digest.clone()),
        );
    }
    Ok(())
}
impl SourceBudgetAuthority
    for oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>
{
    fn ledger(&self) -> Option<&oasis7_distfs::protected_bft_v3::BftPrivateRetentionLedgerV1> {
        self.ledger()
    }
    fn binding_digest(&self) -> &str {
        &self.signer().state.binding_digest
    }
    fn add_deduction_refs(
        &self,
        input: &Input,
        retained: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        add_original_deduction_refs(self.original_inputs(), input, retained)
    }
    fn original_entry(
        &self,
        key: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionKeyV1,
    ) -> Result<
        Option<&oasis7_distfs::protected_bft_v3::BftPrivateRetentionEntryV1>,
        oasis7_distfs::protected_bft_v3::BftSignerErrorV3,
    > {
        self.original_entry(key)
    }
    fn original_input(&self, digest: &str) -> Option<&std::sync::Arc<Input>> {
        self.original_input(digest)
    }
}
impl SourceBudgetAuthority
    for oasis7_distfs::protected_bft_v3::BftPrivateRetentionHistoricalInputContextV1<'_>
{
    fn ledger(&self) -> Option<&oasis7_distfs::protected_bft_v3::BftPrivateRetentionLedgerV1> {
        Some(&self.reservation().retention)
    }
    fn binding_digest(&self) -> &str {
        &self.reservation().signer.state.binding_digest
    }
    fn add_deduction_refs(
        &self,
        input: &Input,
        retained: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        add_original_deduction_refs(self.deduction_inputs(), input, retained)
    }
    fn original_entry(
        &self,
        key: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionKeyV1,
    ) -> Result<
        Option<&oasis7_distfs::protected_bft_v3::BftPrivateRetentionEntryV1>,
        oasis7_distfs::protected_bft_v3::BftSignerErrorV3,
    > {
        self.original_entry(key)
    }
    fn original_input(&self, digest: &str) -> Option<&std::sync::Arc<Input>> {
        self.original_input(digest)
    }
}

#[path = "driver_bft_v3_source_archive_budget.rs"]
mod archive_budget;
pub(in super::super::super) use archive_budget::source_admission_archive_reservation_bytes;
