//! Original candidate reservation retention and exact material claims.
use super::super::super::super::super::formal_ledger::PublicationPromiseInventoryV1;
use super::*;
pub(in super::super) struct ValidatedPrivatePublicationV1 {
    pub(in super::super) reference: String,
    pub(in super::super) claim:
        Box<dyn super::super::super::super::super::formal_ledger::RetainedPublicationClaimV1>,
}
impl NodePrivateStore {
    pub(in super::super) fn retain_planned(
        &mut self,
        candidate: &NodeBftPrivateCandidateV3,
        plan: &super::super::super::super::publication_plan::PublicationPlanV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        reserve: &mut impl FnMut(usize, PublicationPromiseInventoryV1) -> Result<(), String>,
    ) -> Result<String, String> {
        let origin = &self.origin.as_ref().ok_or("Node origin unavailable")?.data;
        let height = candidate.public_value.execution_commitment.height;
        self.check_counts(Some((height, candidate.public_value.block_id()?)))?;
        if height <= origin.height || height - origin.height > 64 {
            return Err("private Node height capacity".into());
        }
        let mut data = Data {
            schema_version: 3,
            domain: DOMAIN.into(),
            origin: false,
            ledger_origin_digest: origin.ledger_origin_digest.clone(),
            material_identity: origin.material_identity.clone(),
            instance_id: self.instance_id.clone(),
            height,
            value: Some(candidate.public_value.clone()),
            actions_cbor: candidate.actions_cbor.clone(),
            references: nodes.keys().cloned().collect(),
            publication_reservation_ref: None,
        };
        let publication = PublicationReservationV1::from_plan(&publication_identity(&data)?, plan)?;
        let reference = publication.digest()?;
        let base_usage = self.usage_with(Some(&data), nodes)?;
        let unbound_len = data.bytes()?.len();
        data.publication_reservation_ref = Some(reference.clone());
        publication.validate_identity(&publication_identity(&data)?)?;
        let final_bytes = data.bytes()?;
        let reserved_usage = base_usage
            .checked_add(publication.accounted_bytes()?)
            .and_then(|bytes| bytes.checked_add(2 * (final_bytes.len() - unbound_len)))
            .ok_or("publication fixture usage overflow")?;
        // The existing ledger callback must accept the whole promise before
        // even the metadata CAS can be created; no new side budget is used.
        let mut future = self.publication_promises()?;
        if let Some(previous) = self
            .candidates
            .get(&height)
            .and_then(|slot| slot.data.publication_reservation_ref.as_ref())
        {
            future.remove(previous);
        }
        future.insert(
            reference.clone(),
            publication.promise_for_identity(&publication_identity(&data)?)?,
        );
        reserve(reserved_usage, future)?;
        let path = self.candidate_path(height);
        if let Some(original) = self.candidates.get(&height) {
            if original.data != data {
                return Err("publication fixture changes original candidate".into());
            }
            verify_original_slot(&self.store, &path, original)?;
        } else {
            common_storage::put_cas_durable(
                &self.store,
                &publication.bytes()?,
                super::super::super::super::publication_reservation::MAX_PUBLICATION_METADATA,
            )?;
        }
        let complete = self
            .candidates
            .get(&height)
            .is_some_and(|slot| slot.complete);
        self.candidates.insert(
            height,
            Slot {
                data: data.clone(),
                complete: false,
            },
        );
        publish(&self.store, &path, &data, nodes, complete, false)?;
        let receipt = data.digest()?;
        self.candidates.insert(
            height,
            Slot {
                data,
                complete: true,
            },
        );
        Ok(receipt)
    }

    pub(in super::super) fn publication_reference(
        &self,
        value: &BftHeaderValueV3,
    ) -> Result<Option<String>, String> {
        let Some((slot, path)) = self.exact_slot(value)? else {
            return Ok(None);
        };
        verify_original_slot(&self.store, &path, slot)?;
        Ok(slot.data.publication_reservation_ref.clone())
    }
    pub(in super::super) fn publication_promises(
        &self,
    ) -> Result<PublicationPromiseInventoryV1, String> {
        let mut promises = PublicationPromiseInventoryV1::new();
        for slot in self.candidates.values().chain(self.values.values()) {
            if let Some(metadata) = read_publication(&self.store, &slot.data)? {
                let identity = publication_identity(&slot.data)?;
                let promise = metadata.promise_for_identity(&identity)?;
                promises.insert(metadata.digest()?, promise);
            }
        }
        Ok(promises)
    }
    pub(in super::super) fn publication_claim_for_actual(
        &self,
        candidate: &NodeBftPrivateCandidateV3,
        executed: &ExecutedFormalHeaderV3,
        accounting: &super::super::super::super::super::formal_ledger::LedgerPublicationAccountingV1,
    ) -> Result<Option<ValidatedPrivatePublicationV1>, String> {
        let Some((slot, path)) = self.exact_slot(&candidate.public_value)? else {
            return Err("publication original private candidate missing".into());
        };
        verify_original_slot(&self.store, &path, slot)?;
        if slot.data.actions_cbor != candidate.actions_cbor {
            return Err("publication original actions differ".into());
        }
        let Some(metadata) = read_publication(&self.store, &slot.data)? else {
            return Ok(None);
        };
        // A staged Data alone is not a durable promise. The exact original
        // pin transaction must have completed before consuming capacity.
        verify_material(&self.store, &slot.data)?;
        let identity = publication_identity(&slot.data)?;
        Ok(Some(ValidatedPrivatePublicationV1 {
            reference: metadata.digest()?,
            claim: metadata.claim_for_actual(
                &identity,
                &candidate.actions_cbor,
                executed,
                accounting,
            )?,
        }))
    }
}
