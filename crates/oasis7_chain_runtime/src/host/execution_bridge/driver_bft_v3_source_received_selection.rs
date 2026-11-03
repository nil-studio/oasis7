//! Schema10 precharged immutable local selection over an original Ready input.
use super::*;
/// Exact future local-selection footprint reserved once by schema10. It is
/// immutable held material, transferred to the actual selection without refund.
pub(in super::super) fn source_selection_data(data: &Data) -> Result<Data, String> {
    if data.schema_version == 12 {
        return super::active_admission::active_selection_data(data);
    }
    if data.schema_version != 10 || data.origin {
        return Err("selection requires original protected received Data".into());
    }
    Ok(Data {
        schema_version: 5,
        domain: SELECTION_DOMAIN.into(),
        origin: false,
        ledger_origin_digest: data.ledger_origin_digest.clone(),
        material_identity: data.material_identity.clone(),
        instance_id: data.instance_id.clone(),
        height: data.height,
        value: data.value.clone(),
        actions_cbor: Vec::new(),
        references: vec![data.digest()?],
        publication_reservation_ref: None,
        publication_original_data_ref: None,
        source_activation: None,
        protected_source_origin_ref: None,
    })
}
pub(in super::super) fn source_selection_bound(data: &Data) -> Result<usize, String> {
    if data.schema_version == 12 {
        return super::active_admission::active_selection_bound(data);
    }
    if data.schema_version != 10 {
        return Ok(0);
    }
    let selection = source_selection_data(data)?;
    selection
        .bytes()?
        .len()
        .checked_mul(2)
        .and_then(|n| n.checked_add(private_pin_bound(&selection).ok()?))
        .ok_or("protected received selection material overflow".into())
}

impl NodePrivateStore {
    pub(in super::super::super) fn source_selection_keys(
        &self,
    ) -> Result<Vec<(u64, String)>, String> {
        self.selections
            .iter()
            .map(|(height, selection)| {
                let value = selection
                    .data
                    .value
                    .as_ref()
                    .ok_or("source selected value absent")?;
                let (slot, _) = self
                    .exact_slot(value)?
                    .ok_or("source selected original absent")?;
                if source_selection_data(&slot.data)? != selection.data {
                    return Err(
                        "source local selection differs from protected received original".into(),
                    );
                }
                Ok((*height, value.block_id()?))
            })
            .collect()
    }
    pub(in super::super::super) fn select_ready_source_candidate(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        proof: &ReservedSourceInputV1,
        deadline: Instant,
        reserve: &mut impl FnMut(
            usize,
            super::super::super::super::super::formal_ledger::PublicationPromiseInventoryV1,
        ) -> Result<(), String>,
        verify_owner: &mut impl FnMut() -> Result<(), String>,
    ) -> Result<String, String> {
        self.select_ready_protected_candidate(
            candidate,
            proof,
            deadline,
            reserve,
            verify_owner,
            false,
        )
    }
    pub(in super::super::super) fn select_ready_active_candidate(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        proof: &ReservedSourceInputV1,
        deadline: Instant,
        reserve: &mut impl FnMut(
            usize,
            super::super::super::super::super::formal_ledger::PublicationPromiseInventoryV1,
        ) -> Result<(), String>,
        verify_owner: &mut impl FnMut() -> Result<(), String>,
    ) -> Result<String, String> {
        self.select_ready_protected_candidate(
            candidate,
            proof,
            deadline,
            reserve,
            verify_owner,
            true,
        )
    }
    fn select_ready_protected_candidate(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        proof: &ReservedSourceInputV1,
        deadline: Instant,
        reserve: &mut impl FnMut(
            usize,
            super::super::super::super::super::formal_ledger::PublicationPromiseInventoryV1,
        ) -> Result<(), String>,
        verify_owner: &mut impl FnMut() -> Result<(), String>,
        active: bool,
    ) -> Result<String, String> {
        proof.require_ready()?;
        let selection_proof = proof
            .selection
            .as_ref()
            .ok_or("original protected selection absent")?;
        if !proof.has_original_selection() {
            return Err("original protected selection reference differs".into());
        }
        let digest = if active {
            self.verify_ready_active_candidate(candidate, &proof.reference, &proof.entry.fact)?
        } else {
            self.verify_ready_source_candidate(candidate, &proof.reference, &proof.entry.fact)?
        };
        let (slot, _) = self
            .source_slot_for_key(&proof.reference.key)?
            .ok_or("selected protected original absent")?;
        if SourceSlotKindV1::from_data(&slot.data)? == SourceSlotKindV1::Local {
            if selection_proof.local_identity_digest != slot.data.digest()?
                || selection_proof.local_identity_bytes != slot.data.bytes()?.len() as u64
            {
                return Err("original protected local identity differs".into());
            }
            return Ok(digest);
        }
        let selection = source_selection_data(&slot.data)?;
        if selection_proof.local_identity_digest != selection.digest()?
            || selection_proof.local_identity_bytes != selection.bytes()?.len() as u64
        {
            return Err("original protected selection5 identity differs".into());
        }
        if self.candidates.contains_key(&selection.height) {
            return Err("original local Node selection is already fixed".into());
        }
        if self
            .selections
            .get(&selection.height)
            .is_some_and(|old| old.data != selection)
        {
            return Err("original local Node selection differs".into());
        }
        let path = self
            .dir
            .join(format!("selection-{:020}.cbor", selection.height));
        let complete = self
            .selections
            .get(&selection.height)
            .is_some_and(|old| old.complete);
        reserve(
            self.usage_with(Some(&selection), &BTreeMap::new())?,
            self.publication_promises()?,
        )?;
        let mut check = || {
            if Instant::now() >= deadline {
                return Err("protected selection expired".into());
            }
            verify_owner()
        };
        check()?;
        self.selections.insert(
            selection.height,
            Slot {
                data: selection.clone(),
                complete: false,
            },
        );
        publish_checked(
            &self.store,
            &path,
            &selection,
            &BTreeMap::new(),
            complete,
            false,
            &mut check,
        )?;
        self.selections.insert(
            selection.height,
            Slot {
                data: selection,
                complete: true,
            },
        );
        check()?;
        Ok(digest)
    }
}

/// Cost/identity reconstruction only. Caller must first independently verify
/// the exact original Ready input and current protected selection transition.
pub(in super::super::super::super) fn source_selection_identity(
    input: &Input,
) -> Result<oasis7_distfs::protected_bft_v3::VerifiedBftPrivateRetentionSelectionIdentityV1, String>
{
    use oasis7_distfs::protected_bft_v3::{
        BftPrivateRetentionSelectionKindV1 as SelectionKind,
        VerifiedBftPrivateRetentionSelectionIdentityV1 as Identity,
    };
    input.validate().map_err(|e| e.to_string())?;
    let data = input_data(input)?;
    let (kind, local) = match data.schema_version {
        9 => (SelectionKind::OriginalLocal, data),
        10 => (SelectionKind::ReceivedLocal, source_selection_data(&data)?),
        _ => return Err("selection identity requires original Source9/10".into()),
    };
    let bytes = local.bytes()?;
    Identity::from_independent_verification(kind, blake3_hex(&bytes), bytes.len() as u64)
        .map_err(|e| e.to_string())
}

pub(in super::super::super::super) fn verify_source_selection_current(
    records: &Path,
    store: &LocalCasStore,
    input: &Input,
) -> Result<(), String> {
    let data = input_data(input)?;
    let directory = records
        .join(storage::DIRECTORY)
        .join(storage::NODE_PRIVATE_DIRECTORY);
    let local = directory.join(format!("candidate-{:020}.cbor", data.height));
    if let Some(slot) = read_slot(&local)? {
        verify_original_slot(store, &local, &slot)?;
        if slot.data != data {
            return Err("selection conflicts with original fixed local candidate".into());
        }
    }
    let path = directory.join(format!("selection-{:020}.cbor", data.height));
    if let Some(slot) = read_slot(&path)? {
        verify_original_slot(store, &path, &slot)?;
        if data.schema_version != 10 || slot.data != source_selection_data(&data)? {
            return Err("selection conflicts with original immutable selection5".into());
        }
    }
    Ok(())
}

impl NodePrivateStore {
    pub(in super::super::super) fn original_source_slot_kind(
        &self,
        reference: &InputRef,
    ) -> Result<SourceSlotKindV1, String> {
        let (slot, _) = self
            .source_slot_for_key(&reference.key)?
            .ok_or("original protected source slot absent")?;
        SourceSlotKindV1::from_data(&slot.data)
    }
}
