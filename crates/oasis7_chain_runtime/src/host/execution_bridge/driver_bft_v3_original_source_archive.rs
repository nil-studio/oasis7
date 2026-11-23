//! Complete immutable first-H material for independently authorized Runtime replay.
//! The activation attestor is supplied by deployment, never by the archive.
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftAuthorizedOriginalReserveInputV1, BftFirstReserveSourceV1, BftPrivateRetentionAttestorV1,
    BftSignerBindingV3, BftSignerErrorV3, BftSignerStateV3,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

const DOMAIN: &str = "oasis7.formal.bft.original-source-archive.v1";
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_PRIVATE_ORIGIN_BYTES: usize = 4 * 1024 * 1024;
const MAX_ACTIVATION_BYTES: usize = 64 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OriginalSourceArchivePinV1 {
    pub(crate) manifest: BftSuccessorWindowArchiveRefV1,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    domain: String,
    source: BftSuccessorWindowSourceV1,
    marker: BftPrivateRetentionCreationMarkerV1,
    h_material: BftSuccessorWindowArchiveRefV1,
    ledger_origin: BftSuccessorWindowArchiveRefV1,
    private_origin: BftSuccessorWindowArchiveRefV1,
    activation_capsule: BftSuccessorWindowArchiveRefV1,
    #[serde(deserialize_with = "bounded_dependencies")]
    dependencies: Vec<BftSuccessorWindowArchiveRefV1>,
}

fn bounded_dependencies<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BftSuccessorWindowArchiveRefV1>, D::Error> {
    use serde::de::{Error, IgnoredAny, SeqAccess, Visitor};
    struct Dependencies;
    impl<'de> Visitor<'de> for Dependencies {
        type Value = Vec<BftSuccessorWindowArchiveRefV1>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 4096 retained H dependency references")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq
                .size_hint()
                .is_some_and(|n| n > MAX_RETAINED_INDEX_NODES)
            {
                return Err(A::Error::custom("original H dependency count capacity"));
            }
            let mut values = Vec::new();
            while values.len() < MAX_RETAINED_INDEX_NODES {
                let Some(value) = seq.next_element()? else {
                    return Ok(values);
                };
                values.push(value);
            }
            if seq.next_element::<IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("original H dependency count capacity"));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Dependencies)
}

struct OriginalParent {
    manifest: Manifest,
    execution: FormalBftHeaderExecutionV3,
    ledger_origin: String,
    private_origin: Vec<u8>,
    activation_capsule: Vec<u8>,
}

impl Manifest {
    fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.source.validate()?;
        if self.schema_version != 1
            || self.domain != DOMAIN
            || self.dependencies.len() > MAX_RETAINED_INDEX_NODES
            || self
                .dependencies
                .windows(2)
                .any(|pair| pair[0].digest >= pair[1].digest)
        {
            return Err("original source archive schema or dependency order differs".into());
        }
        let mut index_bytes = 0u64;
        for reference in &self.dependencies {
            reference.validate().map_err(error)?;
            if reference.bytes > BUSINESS_INDEX_MAX_NODE_BYTES as u64 {
                return Err("original H dependency object capacity".into());
            }
            index_bytes = index_bytes
                .checked_add(reference.bytes)
                .filter(|n| *n <= MAX_RETAINED_INDEX_TOTAL_BYTES as u64)
                .ok_or("original H dependency byte capacity")?;
        }
        for (reference, bound) in [
            (&self.h_material, MAX_H_BYTES),
            (&self.ledger_origin, MAX_ORIGIN_BYTES),
            (&self.private_origin, MAX_PRIVATE_ORIGIN_BYTES),
            (&self.activation_capsule, MAX_ACTIVATION_BYTES),
        ] {
            reference.validate().map_err(error)?;
            if reference.bytes > bound as u64 {
                return Err("original source object exceeds its role capacity".into());
            }
        }
        let bytes = serde_cbor::to_vec(self).map_err(error)?;
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err("original source manifest byte capacity".into());
        }
        Ok(bytes)
    }
}

impl OriginalSourceArchivePinV1 {
    fn open(
        &self,
        archive: &LocalBftSuccessorArchiveStoreV1,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
    ) -> Result<OriginalParent, String> {
        let bytes = archive
            .load_object(&self.manifest, MAX_MANIFEST_BYTES)
            .map_err(error)?;
        let manifest: Manifest = serde_cbor::from_slice(&bytes).map_err(error)?;
        if manifest.canonical_bytes()? != bytes {
            return Err("original source manifest is not canonical".into());
        }
        let handoff = verify_formal_bft_handoff_v2(&anchor, target.clone())?;
        source_at_h(&manifest.source, &handoff)?;
        let material_bytes = archive
            .load_object(&manifest.h_material, MAX_H_BYTES)
            .map_err(error)?;
        let material: ClosedRecord = serde_cbor::from_slice(&material_bytes).map_err(error)?;
        if encoded_h_material(&material)? != material_bytes {
            return Err("original source H material is not canonical".into());
        }
        let origin_bytes = archive
            .load_object(&manifest.ledger_origin, MAX_ORIGIN_BYTES)
            .map_err(error)?;
        let origin: storage::Origin = serde_cbor::from_slice(&origin_bytes).map_err(error)?;
        origin.validate()?;
        if encode_origin(&origin)? != origin_bytes
            || origin.first_height() != handoff.activation_height()?
        {
            return Err("original source ledger origin differs from qualified H".into());
        }
        let seeds = manifest
            .dependencies
            .iter()
            .map(|reference| {
                archive
                    .load_object(reference, BUSINESS_INDEX_MAX_NODE_BYTES)
                    .map(|bytes| (reference.digest.clone(), bytes))
                    .map_err(error)
            })
            .collect::<Result<_, _>>()?;
        let execution = FormalBftHeaderExecutionV3::from_handoff(
            anchor,
            target,
            &material,
            Arc::new(ArchiveOnlyReader),
            seeds,
            true,
        )?;
        capture_h_index(&execution)?;
        if material_pin_digest(&execution, &material)? != origin.material_pin_digest()
            || execution
                .retained_nodes
                .accessed_nodes()?
                .keys()
                .ne(manifest
                    .dependencies
                    .iter()
                    .map(|reference| &reference.digest))
        {
            return Err("original source has missing, extra or changed H dependencies".into());
        }
        let ledger_origin = origin.digest()?;
        let marker = &manifest.marker;
        if marker.ledger_origin_digest != ledger_origin
            || marker.activation_digest != handoff.activation_digest()
            || marker.initial_height != execution.schedule.height()
            || marker.source.binding_digest != manifest.source.binding_digest
            || marker.source.handoff_digest != manifest.source.handoff_record_id
            || marker.source.material_source_digest != manifest.source.material_identity
            || node_material_identity(
                &ledger_origin,
                &marker.creation_identity,
                origin.material_pin_digest(),
            )? != manifest.source.material_identity
        {
            return Err("original source marker differs from independently qualified H".into());
        }
        let private_origin = archive
            .load_object(&manifest.private_origin, MAX_PRIVATE_ORIGIN_BYTES)
            .map_err(error)?;
        let activation_capsule = archive
            .load_object(&manifest.activation_capsule, MAX_ACTIVATION_BYTES)
            .map_err(error)?;
        Ok(OriginalParent {
            manifest,
            execution,
            ledger_origin,
            private_origin,
            activation_capsule,
        })
    }

    pub(crate) fn binding_parts(
        &self,
        archive: &LocalBftSuccessorArchiveStoreV1,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        creation_identity: &str,
    ) -> Result<
        (
            VerifiedBftHandoffTargetV2,
            BftHeaderHeightScheduleV3,
            String,
        ),
        String,
    > {
        let parent = self.open(archive, anchor, target)?;
        if parent.manifest.marker.creation_identity != creation_identity {
            return Err("original source creation identity differs from deployment pin".into());
        }
        Ok((
            parent.execution.handoff,
            parent.execution.schedule,
            parent.manifest.source.material_identity,
        ))
    }
}

impl FormalBftHeaderLedgerExecutionV3 {
    pub(crate) fn archive_first_original_source(
        &self,
        archive: &LocalBftSuccessorArchiveStoreV1,
        binding: &BftSignerBindingV3,
        attestor: &BftPrivateRetentionAttestorV1,
        marker: &BftPrivateRetentionCreationMarkerV1,
        source: &BftSuccessorWindowSourceV1,
    ) -> Result<OriginalSourceArchivePinV1, String> {
        source_at_h(source, &self.origin.handoff)?;
        let (private_origin, capsule) =
            self.original_source_archive_material(binding, attestor, marker)?;
        let execution = FormalBftHeaderExecutionV3::from_handoff(
            self.inputs.anchor.clone(),
            self.inputs.target.clone(),
            &self.inputs.material,
            self.inputs.source.clone(),
            Vec::new(),
            false,
        )?;
        capture_h_index(&execution)?;
        let dependencies = execution.retained_nodes.accessed_nodes()?;
        let (retained, _) = super::super::super::node_execution::verify_archived_original_source(
            &private_origin,
            &capsule,
            binding,
            attestor,
            marker,
        )?;
        if dependencies.keys().ne(retained.iter()) {
            return Err(
                "original H archive inventory differs from authenticated private Origin".into(),
            );
        }
        let mut manifest = Manifest {
            schema_version: 1,
            domain: DOMAIN.into(),
            source: source.clone(),
            marker: marker.clone(),
            h_material: object_ref(&encoded_h_material(&self.inputs.material)?),
            ledger_origin: object_ref(&encode_origin(self.ledger.origin()?)?),
            private_origin: object_ref(&private_origin),
            activation_capsule: object_ref(&capsule),
            dependencies: dependencies
                .values()
                .map(|bytes| object_ref(bytes))
                .collect(),
        };
        manifest
            .dependencies
            .sort_by(|a, b| a.digest.cmp(&b.digest));
        let encoded = manifest.canonical_bytes()?;
        // Bounds and full original inventory are checked before writing any object.
        for (bytes, maximum) in [
            (encoded_h_material(&self.inputs.material)?, MAX_H_BYTES),
            (encode_origin(self.ledger.origin()?)?, MAX_ORIGIN_BYTES),
            (private_origin, MAX_PRIVATE_ORIGIN_BYTES),
            (capsule, MAX_ACTIVATION_BYTES),
        ] {
            archive.persist_object(&bytes, maximum).map_err(error)?;
        }
        for bytes in dependencies.values() {
            archive
                .persist_object(bytes, BUSINESS_INDEX_MAX_NODE_BYTES)
                .map_err(error)?;
        }
        let pin = OriginalSourceArchivePinV1 {
            manifest: archive
                .persist_object(&encoded, MAX_MANIFEST_BYTES)
                .map_err(error)?,
        };
        pin.open(
            archive,
            self.inputs.anchor.clone(),
            self.inputs.target.clone(),
        )?;
        self.ledger.verify_carriers()?;
        Ok(pin)
    }
}

fn object_ref(bytes: &[u8]) -> BftSuccessorWindowArchiveRefV1 {
    BftSuccessorWindowArchiveRefV1 {
        digest: blake3_hex(bytes),
        bytes: bytes.len() as u64,
    }
}

/// A provider's explicit permission for one exact original. Archive presence is
/// insufficient: the reference, private-origin pin and attestor are independently
/// configured here. One encoded/decoded input hold remains live through readback.
pub(crate) struct ArchivedOriginalSourceReaderV1 {
    archive: Arc<LocalBftSuccessorArchiveStoreV1>,
    source: OriginalSourceArchivePinV1,
    anchor: VerifiedFormalReadAnchorV1,
    target: BftProfileTargetV2,
    binding: BftSignerBindingV3,
    attestor: BftPrivateRetentionAttestorV1,
    candidate: OriginalCandidateArchivePinV1,
    occupied: Arc<AtomicBool>,
}

struct OriginalTransportHold(Arc<AtomicBool>);
impl Drop for OriginalTransportHold {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl ArchivedOriginalSourceReaderV1 {
    pub(crate) fn new(
        archive: Arc<LocalBftSuccessorArchiveStoreV1>,
        source: OriginalSourceArchivePinV1,
        anchor: VerifiedFormalReadAnchorV1,
        target: BftProfileTargetV2,
        binding: BftSignerBindingV3,
        attestor: BftPrivateRetentionAttestorV1,
        candidate: OriginalCandidateArchivePinV1,
    ) -> Self {
        Self {
            archive,
            source,
            anchor,
            target,
            binding,
            attestor,
            candidate,
            occupied: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn verify(
        &self,
        input: &BftPrivateRetentionAdmissionInputV1,
    ) -> Result<(ReplayedOriginalCandidateV1, BftFirstReservationPriceV1), String> {
        self.verify_parent(input, None)
    }

    fn verify_parent(
        &self,
        input: &BftPrivateRetentionAdmissionInputV1,
        covered: Option<&BftSignerStateV3>,
    ) -> Result<(ReplayedOriginalCandidateV1, BftFirstReservationPriceV1), String> {
        if !self.occupied.load(Ordering::Acquire) {
            return Err("original archive transport hold was released before verification".into());
        }
        self.candidate
            .reference
            .validate_input(input)
            .map_err(error)?;
        let parent = self
            .source
            .open(&self.archive, self.anchor.clone(), self.target.clone())?;
        let manifest = &parent.manifest;
        let marker = &manifest.marker;
        oasis7_distfs::protected_bft_v3::validate_successor_source_for_binding_v3(
            &manifest.source,
            &self.binding,
        )
        .map_err(error)?;
        if self.binding.digest().map_err(error)? != manifest.source.binding_digest
            || self.candidate.original_private_origin_digest != manifest.private_origin.digest
            || self.candidate.original_instance_id != marker.creation_identity
        {
            return Err("original reader authority differs from archived source".into());
        }
        let (retained, activation) =
            super::super::super::node_execution::verify_archived_original_source(
                &parent.private_origin,
                &parent.activation_capsule,
                &self.binding,
                &self.attestor,
                marker,
            )?;
        if retained.iter().ne(manifest
            .dependencies
            .iter()
            .map(|reference| &reference.digest))
        {
            return Err(
                "original reader H inventory differs from authenticated private Origin".into(),
            );
        }
        if let Some(covered) = covered {
            activation
                .verify(
                    &self.binding,
                    &self.attestor,
                    marker,
                    covered,
                    activation.receipt.body.original_lease_generation,
                )
                .map_err(error)?;
        }
        let replayed = self.candidate.replay_candidate(
            input,
            &manifest.source,
            &parent.execution,
            &parent.ledger_origin,
            &[],
        )?;
        if !matches!(replayed.data_schema, 9 | 10) {
            return Err("original first reader requires Local or Received source Data".into());
        }
        let price = first_price_bounds(input, replayed.publication_bytes, retained)?;
        price.verify_input(input).map_err(error)?;
        Ok((replayed, price))
    }
}

fn check_deadline(deadline: Instant) -> Result<(), BftSignerErrorV3> {
    if Instant::now() >= deadline {
        return Err(BftSignerErrorV3(
            "original archive reader deadline elapsed".into(),
        ));
    }
    Ok(())
}

impl BftFirstReserveSourceV1 for ArchivedOriginalSourceReaderV1 {
    fn read_authorized_original(
        &self,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
        deadline: Instant,
    ) -> Result<BftAuthorizedOriginalReserveInputV1, BftSignerErrorV3> {
        check_deadline(deadline)?;
        if reference != &self.candidate.reference {
            return Err(BftSignerErrorV3(
                "original archive read is not authorized for this reference".into(),
            ));
        }
        if self
            .occupied
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(BftSignerErrorV3(
                "original archive transport hold is occupied".into(),
            ));
        }
        let hold = OriginalTransportHold(self.occupied.clone());
        let input = Arc::new(self.archive.load_original_admission_input(reference)?);
        check_deadline(deadline)?;
        Ok(BftAuthorizedOriginalReserveInputV1 {
            input,
            transport_hold: Box::new(hold),
        })
    }

    fn verify_actual_first_price(
        &self,
        binding: &BftSignerBindingV3,
        marker: &BftPrivateRetentionCreationMarkerV1,
        covered: &BftSignerStateV3,
        input: &BftPrivateRetentionAdmissionInputV1,
        deadline: Instant,
    ) -> Result<BftFirstReservationPriceV1, BftSignerErrorV3> {
        check_deadline(deadline)?;
        if binding.digest()? != self.binding.digest()?
            || marker.digest()? != self.attestor.creation_marker_digest
        {
            return Err(BftSignerErrorV3(
                "original archive verifier authority differs".into(),
            ));
        }
        let (_, price) = self
            .verify_parent(input, Some(covered))
            .map_err(BftSignerErrorV3)?;
        check_deadline(deadline)?;
        Ok(price)
    }
}
