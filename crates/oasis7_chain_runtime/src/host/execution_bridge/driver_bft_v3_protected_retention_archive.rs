//! Complete protected history is separate from bounded Current authentication.
use super::super::super::super::super::super::storage as common_storage;
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputReaderV1 as Reader,
    BftPrivateRetentionAdmissionInputRefV1 as InputRef, BftPrivateRetentionAttestorV1 as Role,
    BftPrivateRetentionEntryV1 as Entry, BftPrivateRetentionFactV1 as Fact,
    BftPrivateRetentionKeyV1 as Key, BftPrivateRetentionLedgerV1 as Ledger,
    BftPrivateRetentionLifecycleV1 as Life, BftPrivateRetentionOperationAccountingV1 as Accounting,
    BftPrivateRetentionOperationProofV1 as Operation, BftPrivateRetentionSealV1 as Seal,
    BftSignerBindingV3 as Binding, BftSignerLeaseV3,
};

pub(in super::super) const PROTECTED_DOMAIN: &str =
    "oasis7.formal.bft.successor-private-protected-retention.v1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct ProtectedRetentionIndexV1 {
    pub schema_version: u32,
    pub private_origin: BftSuccessorWindowArchiveRefV1,
    pub capsule: BftSuccessorWindowArchiveRefV1,
    #[serde(deserialize_with = "bounded_refs")]
    pub h_nodes: Vec<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub operations: Vec<BftSuccessorWindowArchiveRefV1>,
    #[serde(deserialize_with = "bounded_refs")]
    pub seals: Vec<BftSuccessorWindowArchiveRefV1>,
}
impl ProtectedRetentionIndexV1 {
    pub(in super::super) fn references(
        &self,
    ) -> impl Iterator<Item = &BftSuccessorWindowArchiveRefV1> {
        [&self.private_origin, &self.capsule]
            .into_iter()
            .chain(&self.h_nodes)
            .chain(&self.operations)
            .chain(&self.seals)
    }
}
pub(in super::super) struct OriginalInputWitnessV1 {
    pub entry: Entry,
    pub reference: InputRef,
    pub reservation: BftSuccessorWindowArchiveRefV1,
    pub predecessor: Option<BftSuccessorWindowArchiveRefV1>,
}
/// Constructed only after the externally fixed role, exact H capsule and every
/// contiguous original operation/seal have verified. It is not a ReadGrant.
pub(in super::super) struct ValidatedProtectedRetentionArchiveV1 {
    pub index: ProtectedRetentionIndexV1,
    pub final_ledger: Ledger,
    pub originals: BTreeMap<Key, OriginalInputWitnessV1>,
    pub selections: BTreeMap<Key, oasis7_distfs::protected_bft_v3::BftPrivateRetentionSelectionV1>,
    pub accounting: Vec<Accounting>,
    pub activation: node_execution::VerifiedSourceArchiveActivationV1,
    pub objects: BTreeMap<String, BftSuccessorWindowArchiveRefV1>,
}
impl ValidatedProtectedRetentionArchiveV1 {
    pub(in super::super) fn verify(
        archive: &LocalBftSuccessorArchiveStoreV1,
        binding: &Binding,
        role: &Role,
        state: &BftSignerStateV3,
        source: &BftSuccessorWindowSourceV1,
        instance: &str,
        origin: &str,
        index: ProtectedRetentionIndexV1,
        deadline: Instant,
    ) -> Result<Self, String> {
        if index.schema_version != 1
            || index.operations.is_empty()
            || index.seals.is_empty()
            || index
                .operations
                .len()
                .checked_add(index.seals.len())
                .is_none_or(|n| n > MAX_REFS)
        {
            return Err("protected archive complete history index bounds".into());
        }
        let load =
            |reference: &BftSuccessorWindowArchiveRefV1, max: usize| -> Result<Vec<u8>, String> {
                live(deadline)?;
                let bytes = archive.load_object(reference, max).map_err(err)?;
                reference.validate_bytes(&bytes).map_err(err)?;
                Ok(bytes)
            };
        let original = load(
            &index.private_origin,
            super::super::super::super::formal_ledger::MAX_RECORD,
        )?;
        let capsule = load(&index.capsule, 64 * 1024)?;
        let activation = node_execution::verify_source_archive_activation(
            &original,
            &capsule,
            binding,
            role,
            origin,
            &source.material_identity,
            instance,
        )?;
        let mut h_refs = BTreeSet::new();
        let mut h_bytes = 0u64;
        for reference in &index.h_nodes {
            reference.validate().map_err(err)?;
            if reference.bytes > formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES as u64
                || !h_refs.insert(reference.digest.clone())
            {
                return Err("protected archive complete H reference capacity/duplicate".into());
            }
            h_bytes = h_bytes
                .checked_add(reference.bytes)
                .filter(|bytes| {
                    *bytes <= formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES as u64
                })
                .ok_or("protected archive complete H byte capacity")?;
        }
        if &h_refs != activation.h_refs() {
            return Err("protected archive complete H refs differ from original activation".into());
        }
        let mut seals = BTreeMap::new();
        let mut previous_seal = None;
        for reference in &index.seals {
            let seal = Seal::from_canonical_bytes(&load(reference, MAX_METADATA + 32 * 1024)?)
                .map_err(|error| {
                    format!(
                        "protected original seal codec {}: {error}",
                        reference.digest
                    )
                })?;
            if seal.body.original.map.previous_seal_receipt_digest != previous_seal {
                return Err("protected archive seal predecessor gap".into());
            }
            seal.anchor().map_err(err)?.verify(role).map_err(err)?;
            let digest = seal.body.digest().map_err(err)?;
            if seals.insert(digest.clone(), reference.clone()).is_some() {
                return Err("protected archive duplicated original seal".into());
            }
            previous_seal = Some(digest);
        }
        let mut originals = BTreeMap::<Key, OriginalInputWitnessV1>::new();
        let mut selections = BTreeMap::new();
        let mut accounting = Vec::new();
        let mut previous: Option<Operation> = None;
        let mut previous_ref = None;
        let mut operation_ids = BTreeSet::new();
        let mut used_seals = BTreeSet::new();
        for reference in &index.operations {
            let proof = Operation::from_canonical_bytes(&load(
                reference,
                oasis7_distfs::protected_bft_v3::BFT_SIGNER_MAX_FRAME_BYTES_V3,
            )?)
            .map_err(|error| {
                format!(
                    "protected original operation codec {}: {error}",
                    reference.digest
                )
            })?;
            if !operation_ids.insert(proof.ledger.receipt.body.operation_digest.clone()) {
                return Err("protected archive duplicated original operation".into());
            }
            if previous.is_none() && proof.ledger != *activation.first() {
                return Err(
                    "protected archive first receipt differs from fixed original activation".into(),
                );
            }
            let seal_id = previous
                .as_ref()
                .filter(|old| {
                    old.ledger.map.coverage_start_height != proof.ledger.map.coverage_start_height
                })
                .and_then(|_| proof.ledger.map.previous_seal_receipt_digest.as_ref());
            let seal = seal_id
                .map(|id| {
                    let reference = seals
                        .get(id)
                        .ok_or("protected archive floor changed without original seal")?;
                    used_seals.insert(id.clone());
                    Seal::from_canonical_bytes(&load(reference, MAX_METADATA + 32 * 1024)?).map_err(
                        |error| format!("protected floor seal codec {}: {error}", reference.digest),
                    )
                })
                .transpose()?;
            accounting.push(
                proof
                    .verify_original_step(binding, role, previous.as_ref(), seal.as_ref())
                    .map_err(err)?,
            );
            if let Some(index) = &proof.ledger.selection_index {
                for selected in &index.selections {
                    if let Some(original) =
                        selections.insert(selected.key.clone(), selected.clone())
                    {
                        if original != *selected {
                            return Err("protected archive changed original selection".into());
                        }
                    }
                }
            }
            for entry in &proof.ledger.map.entries {
                match &entry.fact {
                    Fact::Some { .. } => {
                        let input =
                            InputRef::from_entry(&proof.ledger.map.source, entry).map_err(err)?;
                        match originals.get_mut(&entry.key) {
                            Some(original) => {
                                if original.reference != input
                                    || (original.entry.lifecycle == Life::Ready
                                        && entry.lifecycle != Life::Ready)
                                {
                                    return Err(
                                        "protected archive changed original key/ref/fact/lifecycle"
                                            .into(),
                                    );
                                }
                                original.entry = entry.clone();
                            }
                            None => {
                                if entry.lifecycle != Life::ReservedPending {
                                    return Err(
                                        "protected archive original lacks first Reserve".into()
                                    );
                                }
                                originals.insert(
                                    entry.key.clone(),
                                    OriginalInputWitnessV1 {
                                        entry: entry.clone(),
                                        reference: input,
                                        reservation: reference.clone(),
                                        predecessor: previous_ref.clone(),
                                    },
                                );
                            }
                        }
                    }
                    Fact::ExplicitLegacyNone => {
                        return Err("covered fresh source cannot infer original None".into());
                    }
                }
            }
            previous = Some(proof);
            previous_ref = Some(reference.clone());
        }
        let last = previous.ok_or("protected archive final original operation absent")?;
        if used_seals.len() != seals.len()
            || last.state != *state
            || state.private_retention.as_ref() != Some(&last.ledger.receipt.body.head)
            || last.ledger.map.previous_seal_receipt_digest != previous_seal
            || originals
                .values()
                .any(|original| original.entry.lifecycle != Life::Ready)
        {
            return Err("protected archive final post-seal State/history/Ready differs".into());
        }
        let mut objects = BTreeMap::new();
        for reference in index.references().chain(
            originals
                .values()
                .map(|original| &original.reference.object),
        ) {
            if objects
                .insert(reference.digest.clone(), reference.clone())
                .is_some_and(|old| old != *reference)
            {
                return Err("protected archive conflicting original object length".into());
            }
        }
        Ok(Self {
            index,
            final_ledger: last.ledger,
            originals,
            selections,
            accounting,
            activation,
            objects,
        })
    }
}

/// Export metadata via the current authenticated owner. Raw inputs never use
/// signer RPC and are admitted only through the independently configured reader.
pub(in super::super) fn export_protected_retention(
    archive: &LocalBftSuccessorArchiveStoreV1,
    binding: &Binding,
    role: &Role,
    state: &BftSignerStateV3,
    source: &BftSuccessorWindowSourceV1,
    instance: &str,
    origin: &str,
    private_origin: Vec<u8>,
    capsule: Vec<u8>,
    lease: &mut dyn BftSignerLeaseV3,
    reader: &dyn Reader,
    source_store: &LocalCasStore,
    deadline: Instant,
) -> Result<
    (
        ValidatedProtectedRetentionArchiveV1,
        BftSuccessorWindowArchiveRefV1,
    ),
    String,
> {
    let role_digest = role.digest().map_err(err)?;
    let observation = lease.observe_private_retention(deadline).map_err(err)?;
    if observation.checkpoint().state != *state
        || observation.checkpoint().provider_public_key != role.provider_public_key
        || observation.ledger().map.source != role.source
        || state
            .private_retention
            .as_ref()
            .is_none_or(|head| head.capability_digest != role_digest)
    {
        return Err("protected export differs from fixed current original source".into());
    }
    let persist = |bytes: &[u8], max| archive.persist_object(bytes, max).map_err(err);
    let initial_activation = node_execution::verify_source_archive_activation(
        &private_origin,
        &capsule,
        binding,
        role,
        origin,
        &source.material_identity,
        instance,
    )?;
    let mut h_nodes = Vec::new();
    let mut h_bytes = 0usize;
    for digest in initial_activation.h_refs() {
        live(deadline)?;
        let bytes = common_storage::read_cas_bounded(
            source_store,
            digest,
            formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
        )?;
        h_bytes = h_bytes
            .checked_add(bytes.len())
            .filter(|bytes| *bytes <= formal_dependencies::MAX_RETAINED_INDEX_TOTAL_BYTES)
            .ok_or("protected export original H byte capacity")?;
        let reference = persist(&bytes, formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES)?;
        if reference.digest != *digest {
            return Err("protected export original H bytes changed".into());
        }
        h_nodes.push(reference);
    }

    let private_origin = persist(
        &private_origin,
        super::super::super::super::formal_ledger::MAX_RECORD,
    )?;
    let capsule = persist(&capsule, 64 * 1024)?;
    let mut id = Some(
        oasis7_distfs::protected_bft_v3::private_retention_receipt_digest(
            &observation.ledger().receipt,
        )
        .map_err(err)?,
    );
    let mut seen = BTreeSet::new();
    let mut operations = Vec::new();
    while let Some(digest) = id {
        live(deadline)?;
        if !seen.insert(digest.clone()) || operations.len() == MAX_REFS {
            return Err("protected export operation chain cycle/capacity".into());
        }
        let proof = lease
            .read_private_retention_operation_proof(digest.clone(), deadline)
            .map_err(err)?;
        if oasis7_distfs::protected_bft_v3::private_retention_receipt_digest(&proof.ledger.receipt)
            .map_err(err)?
            != digest
        {
            return Err("protected export original receipt identity changed".into());
        }
        id = proof
            .ledger
            .receipt
            .body
            .head
            .previous_receipt_digest
            .clone();
        let bytes = proof.canonical_bytes().map_err(err)?;
        drop(proof);
        operations.push(persist(
            &bytes,
            oasis7_distfs::protected_bft_v3::BFT_SIGNER_MAX_FRAME_BYTES_V3,
        )?);
    }
    operations.reverse();
    let mut id = observation
        .ledger()
        .map
        .previous_seal_receipt_digest
        .clone();
    let mut seals = Vec::new();
    let mut seen = BTreeSet::new();
    while let Some(digest) = id {
        live(deadline)?;
        if !seen.insert(digest.clone()) || seals.len() == MAX_REFS {
            return Err("protected export seal cycle/capacity".into());
        }
        let seal = lease
            .read_private_retention_seal_proof(digest.clone(), deadline)
            .map_err(err)?;
        if seal.body.digest().map_err(err)? != digest {
            return Err("protected export original seal identity changed".into());
        }
        id = seal.body.original.map.previous_seal_receipt_digest.clone();
        seals.push(persist(
            &seal.canonical_bytes().map_err(err)?,
            MAX_METADATA + 32 * 1024,
        )?);
    }
    seals.reverse();
    let index = ProtectedRetentionIndexV1 {
        schema_version: 1,
        private_origin,
        capsule,
        h_nodes,
        operations,
        seals,
    };
    let verified = ValidatedProtectedRetentionArchiveV1::verify(
        archive, binding, role, state, source, instance, origin, index, deadline,
    )?;
    let mut prices = BTreeMap::new();
    for original in verified.originals.values() {
        live(deadline)?;
        let read = reader
            .read_original(&original.reference, deadline)
            .map_err(err)?;
        original
            .reference
            .validate_input(read.input())
            .map_err(err)?;
        prices.insert(
            original.reference.admission_digest.clone(),
            read.input().publication_reservation_bytes,
        );
        let encoded = read
            .into_canonical_archive_read(&original.reference)
            .map_err(err)?;
        if persist(
            encoded.bytes(),
            oasis7_distfs::protected_bft_v3::BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1,
        )? != original.reference.object
        {
            return Err("protected export exact original Input object changed".into());
        }
    }
    for operation in &verified.accounting {
        operation.verify_actual_prices(&prices).map_err(err)?;
    }
    let after = lease.observe_private_retention(deadline).map_err(err)?;
    if after.checkpoint().state != *state
        || after.ledger() != observation.ledger()
        || after.checkpoint().lease_generation != observation.checkpoint().lease_generation
    {
        return Err("protected export current lease/head changed during original reads".into());
    }
    let reference = persist(&encode(&verified.index, MAX_METADATA)?, MAX_METADATA)?;
    Ok((verified, reference))
}
