//! Private software-backend witnesses for immutable inputs validated at actual
//! material publication. No DTO, read path or external backend can mint these.
use super::*;
use crate::protected_bft_v3::private_retention::{
    BftPrivateRetentionEntryV1, BftPrivateRetentionFactV1,
};
use crate::protected_bft_v3::private_retention_input::{
    BftPrivateRetentionAdmissionInputRefV1 as InputRef,
    BftPrivateRetentionAdmissionInputV1 as Input, BftPrivateRetentionInputObjectKindV1,
};
use std::collections::BTreeMap;

/// At most 56 inline bytes; the original immutable objects/payload are borrowed
/// through their existing Arc. Map key is a fixed 32-byte admission digest.
#[repr(C)]
#[derive(Clone)]
struct ValidatedMemoryInput {
    owner: Arc<Input>,
    object_digest: [u8; 32],
    object_bytes: u64,
    original_index: u16,
    metadata_index: u16,
}
#[derive(Clone)]
pub(super) struct ValidatedMemoryInputs {
    records: BTreeMap<[u8; 32], ValidatedMemoryInput>,
    role_digest: [u8; 32],
    epoch: u64,
    current_operation: Option<std::sync::Weak<BftPrivateRetentionSnapshotV1>>,
}
fn digest(value: &str) -> Result<[u8; 32], BftSignerErrorV3> {
    super::super::state::validate_digest(value)?;
    let mut result = [0; 32];
    hex::decode_to_slice(value, &mut result)
        .map_err(|_| invalid("memory immutable input digest"))?;
    Ok(result)
}
fn equals_digest(value: &str, expected: &[u8; 32]) -> bool {
    let mut encoded = [0; 64];
    hex::encode_to_slice(expected, &mut encoded).is_ok() && value.as_bytes() == encoded.as_slice()
}
impl ValidatedMemoryInput {
    fn from_validated_publication(
        input: &Arc<Input>,
        key: &[u8; 32],
    ) -> Result<Self, BftSignerErrorV3> {
        // Full validation remains at publication, not at a read-time cache miss.
        let reference = input.descriptor()?;
        if digest(&reference.admission_digest)? != *key {
            return Err(invalid("memory immutable input inventory key differs"));
        }
        let index = |kind| {
            input
                .objects
                .iter()
                .position(|object| object.kind == kind)
                .and_then(|index| u16::try_from(index).ok())
                .ok_or_else(|| invalid("memory immutable input role index"))
        };
        Ok(Self {
            owner: input.clone(),
            object_digest: digest(&reference.object.digest)?,
            object_bytes: reference.object.bytes,
            original_index: index(BftPrivateRetentionInputObjectKindV1::OriginalData)?,
            metadata_index: index(BftPrivateRetentionInputObjectKindV1::PublicationMetadata)?,
        })
    }
    fn matches_fact(&self, key: &[u8; 32], entry: &BftPrivateRetentionEntryV1) -> bool {
        let BftPrivateRetentionFactV1::Some {
            original_data_ref,
            metadata_ref,
            admission_digest,
            input_ref: Some(object),
        } = &entry.fact
        else {
            return false;
        };
        self.owner.key == entry.key
            && equals_digest(admission_digest, key)
            && object.bytes == self.object_bytes
            && equals_digest(&object.digest, &self.object_digest)
            && self
                .owner
                .objects
                .get(self.original_index as usize)
                .is_some_and(|original| original.reference.digest == *original_data_ref)
            && self
                .owner
                .objects
                .get(self.metadata_index as usize)
                .is_some_and(|metadata| metadata.reference.digest == *metadata_ref)
    }
}
impl ValidatedMemoryInputs {
    pub(super) fn empty(
        role: &BftPrivateRetentionAttestorV1,
        epoch: u64,
    ) -> Result<Self, BftSignerErrorV3> {
        Ok(Self {
            records: BTreeMap::new(),
            role_digest: digest(&role.digest()?)?,
            epoch,
            current_operation: None,
        })
    }
    fn identity(
        &self,
        role: &BftPrivateRetentionAttestorV1,
        epoch: u64,
    ) -> Result<(), BftSignerErrorV3> {
        if self.epoch != epoch || self.role_digest != digest(&role.digest()?)? {
            return Err(invalid("memory immutable input role/epoch differs"));
        }
        Ok(())
    }
    /// Prepared after the original complete CAS envelope validation. Only the
    /// caller's final mutex-protected publication installs this candidate.
    pub(super) fn prepare_publication(
        &self,
        role: &BftPrivateRetentionAttestorV1,
        current_epoch: u64,
        current: &Arc<BftPrivateRetentionSnapshotV1>,
        sealed: &BftPrivateRetentionInputInventoryV1,
    ) -> Result<(u64, Self), BftSignerErrorV3> {
        self.identity(role, current_epoch)?;
        let epoch = current_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("memory immutable input epoch overflow"))?;
        let mut next = Self::empty(role, epoch)?;
        next.current_operation = Some(Arc::downgrade(current));
        for (id, input) in current.inputs.iter().chain(sealed.iter()) {
            if input.source != role.source {
                return Err(invalid("memory immutable input publication source differs"));
            }
            let key = digest(id)?;
            if let Some(existing) = next.records.get(&key) {
                if !Arc::ptr_eq(&existing.owner, input) {
                    return Err(invalid("memory immutable input duplicate owner differs"));
                }
                continue;
            }
            let record = match self.records.get(&key) {
                Some(existing) if Arc::ptr_eq(&existing.owner, input) => existing.clone(),
                Some(_) => return Err(invalid("memory immutable published input owner replaced")),
                None => ValidatedMemoryInput::from_validated_publication(input, &key)?,
            };
            next.records.insert(key, record);
        }
        Ok((epoch, next))
    }
    fn owner(
        &self,
        id: &str,
        input: &Arc<Input>,
        role: &BftPrivateRetentionAttestorV1,
    ) -> Result<(&ValidatedMemoryInput, [u8; 32]), BftSignerErrorV3> {
        let key = digest(id)?;
        let record = self
            .records
            .get(&key)
            .ok_or_else(|| invalid("memory immutable input publication witness missing"))?;
        if !Arc::ptr_eq(&record.owner, input) || input.source != role.source {
            return Err(invalid(
                "memory immutable input actual owner/source differs",
            ));
        }
        Ok((record, key))
    }
    pub(super) fn verify_current(
        &self,
        memory: &Memory,
        role: &BftPrivateRetentionAttestorV1,
    ) -> Result<(), BftSignerErrorV3> {
        self.identity(role, memory.material_epoch)?;
        match (&memory.original, &self.current_operation) {
            (None, None) => {}
            (Some(actual), Some(original))
                if original
                    .upgrade()
                    .is_some_and(|published| Arc::ptr_eq(&published, actual)) => {}
            _ => {
                return Err(invalid(
                    "memory immutable published operation owner differs",
                ));
            }
        }
        let current = memory.original.as_ref().map(|original| &original.inputs);
        if let Some(original) = &memory.original {
            if original.retention.map.source != role.source {
                return Err(invalid("memory immutable current map source differs"));
            }
            let mut matched = 0usize;
            for entry in &original.retention.map.entries {
                if let BftPrivateRetentionFactV1::Some {
                    admission_digest, ..
                } = &entry.fact
                {
                    let input = original
                        .inputs
                        .get(admission_digest)
                        .ok_or_else(|| invalid("memory immutable current input missing"))?;
                    let (record, key) = self.owner(admission_digest, input, role)?;
                    if !record.matches_fact(&key, entry) {
                        return Err(invalid("memory immutable current key/fact differs"));
                    }
                    matched += 1;
                }
            }
            if matched != original.inputs.len() {
                return Err(invalid("memory immutable current input keys differ"));
            }
        } else if !memory.sealed_inputs.is_empty() || !memory.seals.is_empty() {
            return Err(invalid("memory immutable uncovered sealed inventory"));
        }
        let mut count = 0usize;
        if let Some(current) = current {
            for (id, input) in current {
                self.owner(id, input, role)?;
                count += 1;
            }
        }
        for (id, input) in &memory.sealed_inputs {
            self.owner(id, input, role)?;
            if let Some(active) = current.and_then(|current| current.get(id)) {
                if !Arc::ptr_eq(active, input) {
                    return Err(invalid("memory immutable active/sealed owner differs"));
                }
            } else {
                count += 1;
            }
        }
        if count != self.records.len() {
            return Err(invalid(
                "memory immutable publication inventory keys differ",
            ));
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn same_witnesses(&self, other: &Self) -> bool {
        self.epoch == other.epoch
            && self.role_digest == other.role_digest
            && match (&self.current_operation, &other.current_operation) {
                (None, None) => true,
                (Some(a), Some(b)) => std::sync::Weak::ptr_eq(a, b),
                _ => false,
            }
            && self.records.len() == other.records.len()
            && self.records.iter().all(|(id, a)| {
                other.records.get(id).is_some_and(|b| {
                    Arc::ptr_eq(&a.owner, &b.owner)
                        && a.object_digest == b.object_digest
                        && a.object_bytes == b.object_bytes
                        && a.original_index == b.original_index
                        && a.metadata_index == b.metadata_index
                })
            })
    }
    pub(super) fn verify_reference(
        &self,
        reference: &InputRef,
        input: &Arc<Input>,
        role: &BftPrivateRetentionAttestorV1,
        epoch: u64,
    ) -> Result<(), BftSignerErrorV3> {
        self.identity(role, epoch)?;
        reference.validate()?;
        let (record, _) = self.owner(&reference.admission_digest, input, role)?;
        if reference.source != input.source
            || reference.key != input.key
            || reference.object.bytes != record.object_bytes
            || !equals_digest(&reference.object.digest, &record.object_digest)
        {
            return Err(invalid("memory immutable exact input reference differs"));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memory_immutable_input_witness_has_fixed_metadata_bound() {
        assert!(std::mem::size_of::<ValidatedMemoryInput>() <= 56);
        assert!(std::mem::size_of::<([u8; 32], ValidatedMemoryInput)>() <= 88);
    }
}
