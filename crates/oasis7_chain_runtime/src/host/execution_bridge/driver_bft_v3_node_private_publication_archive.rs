#[path = "driver_bft_v3_carry_initialization.rs"]
pub(super) mod carry_initialization;
pub(in super::super::super) use carry_initialization::{
    CarryCopyDescriptorV1, carry_origin_usage, dependency_identity,
};
// Opaque provenance for one original pending reservation. No signing authority.
use super::*;
pub(in super::super::super) struct PublicationArchiveMaterialV1 {
    pub(in super::super::super) data: Vec<u8>,
    pub(in super::super::super) original_data: Option<Vec<u8>>,
    pub(in super::super::super) reservation: Vec<u8>,
}
#[derive(Clone)]
pub(in super::super::super) struct VerifiedPublicationOriginalV1 {
    data: Data,
    root: Data,
    metadata: PublicationReservationV1,
}
pub(super) fn read_original_data(store: &LocalCasStore, reference: &str) -> Result<Data, String> {
    let bytes = common_storage::read_cas_bounded(store, reference, MAX_RECORD)?;
    decode_data(&bytes)
}
fn decode_data(bytes: &[u8]) -> Result<Data, String> {
    if bytes.len() > MAX_RECORD {
        return Err("publication original Data capacity".into());
    }
    let data: Data =
        serde_cbor::from_slice(bytes).map_err(|_| "decode publication original Data")?;
    if data.bytes()? != bytes {
        return Err("publication original Data not canonical".into());
    }
    Ok(data)
}
pub(in super::super::super) struct PublicationArchiveOwnerV1<'a> {
    pub(in super::super::super) origin: &'a str,
    pub(in super::super::super) material: &'a str,
    pub(in super::super::super) instance: &'a str,
}
impl VerifiedPublicationOriginalV1 {
    pub(in super::super::super) fn verified_dependency_refs(&self) -> &[String] {
        &self.data.references
    }
    pub(in super::super::super) fn verify(
        raw: &PublicationArchiveMaterialV1,
        owner: PublicationArchiveOwnerV1<'_>,
        ancestors: &[Self],
    ) -> Result<Self, String> {
        let data = decode_data(&raw.data)?;
        if data.ledger_origin_digest != owner.origin
            || data.material_identity != owner.material
            || data.instance_id != owner.instance
            || data.origin
            || matches!(data.schema_version, 5 | 11 | 12 | 13)
        {
            return Err("archive publication Data owner differs".into());
        }
        let metadata = PublicationReservationV1::decode(&raw.reservation)?;
        let root = if let Some(reference) = &data.publication_original_data_ref {
            let bytes = raw
                .original_data
                .as_ref()
                .ok_or("archive publication ancestor missing")?;
            let root = decode_data(bytes)?;
            if root.digest()? != *reference
                || root.publication_original_data_ref.is_some()
                || !ancestors.iter().any(|prior| {
                    prior.root == root
                        && prior.metadata == metadata
                        && same_candidate(&prior.data, &data)
                })
            {
                return Err("archive publication ancestor lacks verified predecessor grant".into());
            }
            root
        } else {
            if raw.original_data.is_some() {
                return Err("unexpected publication ancestor".into());
            }
            data.clone()
        };
        if !same_candidate(&root, &data) {
            return Err("archive migrated original candidate differs".into());
        }
        metadata.validate_identity(&publication_identity(&root)?)?;
        Ok(Self {
            data,
            root,
            metadata,
        })
    }
    pub(in super::super::super) fn header_id(&self) -> Result<String, String> {
        self.data
            .value
            .as_ref()
            .ok_or("publication value absent")?
            .block_id()
    }
    pub(in super::super::super) fn reference(&self) -> Result<String, String> {
        self.metadata.digest()
    }
    pub(in super::super::super) fn validate_execution(
        &self,
        actions: &[u8],
        executed: &ExecutedFormalHeaderV3,
    ) -> Result<(), String> {
        if actions != self.data.actions_cbor {
            return Err("archive publication actions differ".into());
        }
        self.metadata.validate_execution(
            &self.root.ledger_origin_digest,
            self.data.height,
            actions,
            executed,
        )
    }
    pub(in super::super::super) fn validate_pending_dependencies(
        &self,
        refs: impl Iterator<Item = String>,
    ) -> Result<(), String> {
        if self.data.references.iter().cloned().ne(refs) {
            return Err("pending publication Data dependencies differ".into());
        }
        Ok(())
    }
    pub(in super::super::super) fn matches_candidate(
        &self,
        candidate: &NodeBftPrivateCandidateV3,
    ) -> bool {
        self.data.value.as_ref() == Some(&candidate.public_value)
            && self.data.actions_cbor == candidate.actions_cbor
    }
}
fn same_candidate(a: &Data, b: &Data) -> bool {
    a.material_identity == b.material_identity
        && a.instance_id == b.instance_id
        && a.height == b.height
        && a.value == b.value
        && a.actions_cbor == b.actions_cbor
        && a.references == b.references
        && a.publication_reservation_ref == b.publication_reservation_ref
}
#[cfg(test)]
impl NodePrivateStore {
    pub(in super::super) fn check_archive_publication_owner_for_test(&self) -> Result<(), String> {
        let slot = self
            .candidates
            .values()
            .find(|s| s.data.publication_reservation_ref.is_some())
            .ok_or("test original Some absent")?;
        let metadata = read_publication(&self.store, &slot.data)?.ok_or("test metadata absent")?;
        let raw = PublicationArchiveMaterialV1 {
            data: slot.data.bytes()?,
            original_data: None,
            reservation: metadata.bytes()?,
        };
        let owner = || PublicationArchiveOwnerV1 {
            origin: &slot.data.ledger_origin_digest,
            material: &slot.data.material_identity,
            instance: &slot.data.instance_id,
        };
        let original = VerifiedPublicationOriginalV1::verify(&raw, owner(), &[])?;
        let mut data = slot.data.clone();
        data.instance_id = "fe".repeat(32);
        let metadata = metadata.foreign_instance_for_test();
        data.publication_reservation_ref = Some(metadata.digest()?);
        let foreign = PublicationArchiveMaterialV1 {
            data: data.bytes()?,
            original_data: None,
            reservation: metadata.bytes()?,
        };
        if VerifiedPublicationOriginalV1::verify(&foreign, owner(), &[]).is_ok() {
            return Err("coherent foreign archived Data/metadata owner accepted".into());
        }
        let mut migrated = slot.data.clone();
        migrated.schema_version = 6;
        migrated.domain = MIGRATED_DOMAIN.into();
        migrated.protected_source_origin_ref = None;
        migrated.ledger_origin_digest = "fc".repeat(32);
        migrated.publication_original_data_ref = Some(slot.data.digest()?);
        let raw = PublicationArchiveMaterialV1 {
            data: migrated.bytes()?,
            original_data: Some(slot.data.bytes()?),
            reservation: raw.reservation,
        };
        let owner = || PublicationArchiveOwnerV1 {
            origin: &migrated.ledger_origin_digest,
            material: &migrated.material_identity,
            instance: &migrated.instance_id,
        };
        if VerifiedPublicationOriginalV1::verify(&raw, owner(), &[]).is_ok() {
            return Err("raw migrated Data minted predecessor grant".into());
        }
        VerifiedPublicationOriginalV1::verify(&raw, owner(), std::slice::from_ref(&original))?;
        if matches!(slot.data.schema_version, 9 | 10) {
            carry_initialization::check_source_alias_for_test(&original, &migrated, &raw)?;
        }
        Ok(())
    }
}
impl NodePrivateStore {
    pub(super) fn validate_publication_lineage(&self, data: &Data) -> Result<(), String> {
        if let Some(reference) = &data.publication_original_data_ref {
            let grant = self
                .publication_ancestors
                .get(reference)
                .ok_or("private publication migration grant absent")?;
            if !same_candidate(&grant.root, data) {
                return Err("private publication migration differs from verified original".into());
            }
            let root = read_original_data(&self.store, reference)?;
            if root != grant.root {
                return Err("private publication original Data replaced".into());
            }
        }
        Ok(())
    }
    pub(super) fn publication_identity_for_data(
        &self,
        data: &Data,
    ) -> Result<PublicationCandidateIdentityV1, String> {
        self.validate_publication_lineage(data)?;
        match &data.publication_original_data_ref {
            Some(reference) => publication_identity(&read_original_data(&self.store, reference)?),
            None => publication_identity(data),
        }
    }
    pub(in super::super) fn register_publication_ancestors(
        &mut self,
        grants: &[VerifiedPublicationOriginalV1],
    ) -> Result<(), String> {
        for grant in grants {
            self.publication_ancestors
                .insert(grant.root.digest()?, grant.clone());
        }
        Ok(())
    }
    pub(in super::super) fn publication_archive_materials(
        &self,
        headers: &BTreeSet<String>,
    ) -> Result<Vec<PublicationArchiveMaterialV1>, String> {
        let mut entries = BTreeMap::new();
        for (slot, local) in self
            .candidates
            .values()
            .map(|slot| (slot, true))
            .chain(self.values.values().map(|slot| (slot, false)))
        {
            if !headers.contains(
                &slot
                    .data
                    .value
                    .as_ref()
                    .ok_or("publication candidate absent")?
                    .block_id()?,
            ) {
                continue;
            }
            self.validate_publication_lineage(&slot.data)?;
            let path = if local {
                self.candidate_path(slot.data.height)
            } else {
                self.value_path(
                    slot.data.height,
                    &slot
                        .data
                        .value
                        .as_ref()
                        .ok_or("archive candidate missing")?
                        .block_id()?,
                )
            };
            verify_original_slot(&self.store, &path, slot)?;
            verify_material(&self.store, &slot.data)?;
            if let Some(metadata) = read_publication(&self.store, &slot.data)? {
                let original_data = slot
                    .data
                    .publication_original_data_ref
                    .as_ref()
                    .map(|r| read_original_data(&self.store, r).and_then(|d| d.bytes()))
                    .transpose()?;
                entries.insert(
                    slot.data.digest()?,
                    PublicationArchiveMaterialV1 {
                        data: slot.data.bytes()?,
                        original_data,
                        reservation: metadata.bytes()?,
                    },
                );
            }
        }
        Ok(entries.into_values().collect())
    }
    pub(in super::super) fn retain_migrated_publication(
        &mut self,
        grant: &VerifiedPublicationOriginalV1,
        local: bool,
        nodes: &BTreeMap<String, Vec<u8>>,
        reserve: &mut impl FnMut(
            usize,
            super::super::super::super::super::formal_ledger::PublicationPromiseInventoryV1,
        ) -> Result<(), String>,
    ) -> Result<(), String> {
        let origin = self
            .origin
            .as_ref()
            .ok_or("migration original origin absent")?;
        let mut data = grant.root.clone();
        data.schema_version = if local { 6 } else { 7 };
        data.domain = MIGRATED_DOMAIN.into();
        // Only the alias changes domain. The exact root retains Source9/10.
        data.protected_source_origin_ref = None;
        data.ledger_origin_digest = origin.data.ledger_origin_digest.clone();
        data.publication_original_data_ref = Some(grant.root.digest()?);
        self.validate_publication_lineage(&data)?;
        let retained = read_publication(&self.store, &grant.root)?
            .ok_or("original migrated publication metadata missing")?;
        if retained != grant.metadata {
            return Err("original migrated publication metadata changed".into());
        }
        let path = if local {
            self.candidate_path(data.height)
        } else {
            self.value_path(data.height, &grant.header_id()?)
        };
        let original = if local {
            self.candidates.get(&data.height)
        } else {
            self.values.get(&(data.height, grant.header_id()?))
        };
        if let Some(slot) = original {
            if slot.data != data {
                return Err("migration changes original private slot".into());
            }
            verify_original_slot(&self.store, &path, slot)?;
            return Ok(());
        }
        let mut promises = self.publication_promises()?;
        let mut promise = grant
            .metadata
            .promise_for_identity(&publication_identity(&grant.root)?)?;
        promise.origin = data.ledger_origin_digest.clone();
        promises.insert(grant.reference()?, promise);
        // Reserve the full prospective inventory before writing either old
        // canonical object into this new segment's CAS/pin transaction.
        let mut unbound = data.clone();
        unbound.schema_version = if local { 3 } else { 4 };
        unbound.domain = if local { DOMAIN } else { MULTI_DOMAIN }.into();
        unbound.publication_reservation_ref = None;
        unbound.publication_original_data_ref = None;
        let usage = self
            .usage_with(Some(&unbound), nodes)?
            .checked_add(carry_initialization::migrated_hold(&data, &grant.root)?)
            .and_then(|n| n.checked_add(grant.metadata.accounted_bytes().ok()?))
            .and_then(|n| n.checked_add(grant.root.bytes().ok()?.len()))
            .and_then(|n| {
                n.checked_add(2 * (data.bytes().ok()?.len() - unbound.bytes().ok()?.len()))
            })
            .ok_or("migration publication material capacity")?;
        reserve(usage, promises)?;
        common_storage::put_cas_durable(&self.store, &grant.root.bytes()?, MAX_RECORD)?;
        common_storage::put_cas_durable(
            &self.store,
            &grant.metadata.bytes()?,
            super::super::super::super::publication_reservation::MAX_PUBLICATION_METADATA,
        )?;
        publish(&self.store, &path, &data, nodes, false, false)?;
        let slot = Slot {
            data: data.clone(),
            complete: true,
        };
        if local {
            self.candidates.insert(data.height, slot);
        } else {
            self.values.insert((data.height, grant.header_id()?), slot);
        }
        Ok(())
    }
}
