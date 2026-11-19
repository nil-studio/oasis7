//! Fixed IWS normal package DATA producer. Only an actual sealed Runtime and
//! the retained dual-Frozen/Node input can reach this constructor. Receipt and
//! accepted-source admission remain in the existing publication consumer.
use crate::iws_normal_input::PaidVerifiedNormalTickInputV1;
use ed25519_dalek::{Signer, SigningKey};
use oasis7::world_service::formal_result::{
    PaidFormalBusinessErrorV1, PaidFormalBusinessResultUpdateV1,
};
use oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1;
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{
            CANDIDATE_SIGN_DOMAIN_V1, CanonicalRecordV1, FormalCompletionBranchV1, FormalError,
            FormalFrozenBodyV1, FormalFrozenCertificateV1, FormalFrozenReceiptBodyV1,
            FormalFrozenReceiptV1, FormalHeadRefV1, FormalRecordKindV1, FormalWorldIdentityV1,
            ProfileHandoffV1, SignedCanonicalCandidateV1, SignedFormalFrozenV1,
            bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
            context_wire,
        },
        replicated_protocol::{
            ArtifactObject, ArtifactRole, ClosedRecord, package::BoundedIwsValidationErrorV1,
        },
    },
    incremental_execution::ExecutionError,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_runtime::{BodyMutationError, PreparedNormalHostRuntimeV1};
use std::{collections::BTreeMap, mem::size_of};

#[derive(Debug)]
pub(crate) enum NormalPackageErrorV1 {
    Boundary(super::IwsHostRuntimeErrorV1),
    Invalid(&'static str),
    Context(FormalError),
    Body(BodyMutationError),
    Execution(ExecutionError),
    Value(ValueTreeError),
    Material(BoundedIwsValidationErrorV1),
    Business(PaidFormalBusinessErrorV1),
}
impl From<FormalError> for NormalPackageErrorV1 {
    fn from(e: FormalError) -> Self {
        Self::Context(e)
    }
}
impl From<BodyMutationError> for NormalPackageErrorV1 {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl From<ExecutionError> for NormalPackageErrorV1 {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
fn invalid() -> NormalPackageErrorV1 {
    NormalPackageErrorV1::Invalid("fixed normal package source/bounds")
}

/// Immutable DATA and all of its actual owned allocations. Neither a candidate
/// supplied by a caller nor a bare Result/root can construct this owner.
pub(crate) struct PaidNormalCandidatePackageV1 {
    candidate: FormalCandidateMaterialV1,
    commit: [u8; 32],
    material: [u8; 32],
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
impl PaidNormalCandidatePackageV1 {
    pub(crate) fn candidate<'a>(
        &'a self,
        prepared: &PreparedNormalHostRuntimeV1,
        input: &PaidVerifiedNormalTickInputV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<&'a FormalCandidateMaterialV1, NormalPackageErrorV1> {
        input.validate_context(common)?;
        prepared.validate_input_source_bounded_v1(input.data(), common)?;
        common.charge(1, 64)?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || self.commit != prepared.candidate().commit_id()
            || self.material != input.data().checked().material_id()
        {
            return Err(invalid());
        }
        Ok(&self.candidate)
    }
}

/// One fee owns the complete graph. Reservations merge before allocation, so
/// temporary copies never borrow capacity released by another source owner.
struct Allocation {
    fee: MemoryReservation,
}
impl Allocation {
    fn new(common: &mut FormalBoundedContextV1<'_>) -> Result<Self, NormalPackageErrorV1> {
        Ok(Self {
            fee: common
                .memory()
                .reserve(
                    size_of::<PaidNormalCandidatePackageV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(|e| NormalPackageErrorV1::Value(ValueTreeError::Memory(e)))?,
        })
    }
    fn admit(
        &mut self,
        n: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), NormalPackageErrorV1> {
        common.charge(1, u64::try_from(n).map_err(|_| invalid())?)?;
        let mut next = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(|e| NormalPackageErrorV1::Value(ValueTreeError::Memory(e)))?;
        self.fee.merge(&mut next).map_err(|_| invalid())?;
        Ok(())
    }
    fn vector<T>(
        &mut self,
        n: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Vec<T>, NormalPackageErrorV1> {
        self.admit(
            n.checked_mul(size_of::<T>())
                .and_then(|n| n.checked_add(size_of::<Vec<T>>()))
                .ok_or_else(invalid)?,
            common,
        )?;
        let mut out = Vec::new();
        out.try_reserve_exact(n).map_err(|_| invalid())?;
        if out.capacity() > n {
            return Err(invalid());
        }
        Ok(out)
    }
    fn bytes(
        &mut self,
        raw: &[u8],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Vec<u8>, NormalPackageErrorV1> {
        let mut out = self.vector(raw.len(), common)?;
        for chunk in raw.chunks(8192) {
            common.charge(1, chunk.len() as u64)?;
            out.extend_from_slice(chunk)
        }
        Ok(out)
    }
    fn text(
        &mut self,
        text: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<String, NormalPackageErrorV1> {
        let raw = self.bytes(text.as_bytes(), common)?;
        String::from_utf8(raw).map_err(|_| invalid())
    }
    fn hex(
        &mut self,
        id: &[u8; 32],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<String, NormalPackageErrorV1> {
        let mut raw = self.vector::<u8>(64, common)?;
        raw.resize(64, 0);
        common.charge(1, 96)?;
        hex::encode_to_slice(id, &mut raw).map_err(|_| invalid())?;
        String::from_utf8(raw).map_err(|_| invalid())
    }
    fn hash(
        &mut self,
        raw: &[u8],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<String, NormalPackageErrorV1> {
        let mut hash = blake3::Hasher::new();
        for chunk in raw.chunks(8192) {
            common.charge(1, chunk.len() as u64)?;
            hash.update(chunk);
        }
        self.hex(hash.finalize().as_bytes(), common)
    }
    fn world(
        &mut self,
        w: &FormalWorldIdentityV1,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<FormalWorldIdentityV1, NormalPackageErrorV1> {
        Ok(FormalWorldIdentityV1 {
            world_id: self.text(&w.world_id, c)?,
            chain_id: self.text(&w.chain_id, c)?,
            genesis_digest: self.text(&w.genesis_digest, c)?,
        })
    }
    fn handoff(
        &mut self,
        h: &Option<ProfileHandoffV1>,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<ProfileHandoffV1>, NormalPackageErrorV1> {
        match h {
            None => Ok(None),
            Some(h) => Ok(Some(ProfileHandoffV1 {
                next_profile: self.text(&h.next_profile, c)?,
                next_profile_version: h.next_profile_version,
                next_authority_digest: self.text(&h.next_authority_digest, c)?,
                valid_from_position: h.valid_from_position,
            })),
        }
    }
    fn frozen_body(
        &mut self,
        b: &FormalFrozenBodyV1,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<FormalFrozenBodyV1, NormalPackageErrorV1> {
        Ok(FormalFrozenBodyV1 {
            schema_version: b.schema_version,
            world: self.world(&b.world, c)?,
            authority_id: self.text(&b.authority_id, c)?,
            epoch: b.epoch,
            activation_ref: self.text(&b.activation_ref, c)?,
            parent: FormalHeadRefV1 {
                position: b.parent.position,
                record_id: self.text(&b.parent.record_id, c)?,
            },
            next_position: b.next_position,
            request_digest: self.text(&b.request_digest, c)?,
            payload_digest: self.text(&b.payload_digest, c)?,
            runtime_manifest_digest: self.text(&b.runtime_manifest_digest, c)?,
            governing_manifest_digest: self.text(&b.governing_manifest_digest, c)?,
            execution_policy_digest: self.text(&b.execution_policy_digest, c)?,
            pre_state_root: self.text(&b.pre_state_root, c)?,
            frozen_material_digest: self.text(&b.frozen_material_digest, c)?,
            handoff: self.handoff(&b.handoff, c)?,
        })
    }
    fn receipt(
        &mut self,
        r: &FormalFrozenReceiptV1,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<FormalFrozenReceiptV1, NormalPackageErrorV1> {
        Ok(FormalFrozenReceiptV1 {
            body: FormalFrozenReceiptBodyV1 {
                authority_id: self.text(&r.body.authority_id, c)?,
                role: r.body.role,
                instance_digest: self.text(&r.body.instance_digest, c)?,
                reservation_id: self.text(&r.body.reservation_id, c)?,
            },
            signature_hex: self.text(&r.signature_hex, c)?,
        })
    }
    fn certificate(
        &mut self,
        f: &FormalFrozenCertificateV1,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<FormalFrozenCertificateV1, NormalPackageErrorV1> {
        Ok(FormalFrozenCertificateV1 {
            reservation: SignedFormalFrozenV1 {
                body: self.frozen_body(&f.reservation.body, c)?,
                signature_hex: self.text(&f.reservation.signature_hex, c)?,
            },
            primary: self.receipt(&f.primary, c)?,
            replica: self.receipt(&f.replica, c)?,
        })
    }
    fn roots(
        &mut self,
        roots: &BTreeMap<ArtifactRole, String>,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<BTreeMap<ArtifactRole, String>, NormalPackageErrorV1> {
        // libstd BTree node has <=11 entries and <=12 edges. Paying a whole
        // maximal node per inserted row bounds even the smallest split nodes.
        self.admit(
            roots
                .len()
                .checked_mul(11 * (size_of::<(ArtifactRole, String)>() + size_of::<usize>()) + 128)
                .ok_or_else(invalid)?,
            c,
        )?;
        let mut out = BTreeMap::new();
        for (role, id) in roots {
            c.charge(1, 64)?;
            out.insert(*role, self.text(id, c)?);
        }
        Ok(out)
    }
    fn object(
        &mut self,
        o: &ArtifactObject,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ArtifactObject, NormalPackageErrorV1> {
        let mut references = self.vector(o.references.len(), c)?;
        for r in &o.references {
            references.push(self.text(r, c)?);
        }
        Ok(ArtifactObject {
            content_hash: self.text(&o.content_hash, c)?,
            bytes: self.bytes(&o.bytes, c)?,
            references,
        })
    }
    fn closed(
        &mut self,
        m: &ClosedRecord,
        c: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ClosedRecord, NormalPackageErrorV1> {
        let mut objects = self.vector(m.objects.len(), c)?;
        for o in &m.objects {
            c.consume_steps(1)?;
            objects.push(self.object(o, c)?);
        }
        Ok(ClosedRecord {
            payload_digest: self.text(&m.payload_digest, c)?,
            before_state_root: self.text(&m.before_state_root, c)?,
            after_state_root: self.text(&m.after_state_root, c)?,
            roots: self.roots(&m.roots, c)?,
            objects,
        })
    }
}

#[path = "iws_host_normal_package_material.rs"]
mod material;

#[path = "iws_host_normal_input_package.rs"]
mod input;
pub(in crate::iws_host_runtime) use input::PaidPreparedNodeNormalInputV1;

/// Fixed ordinary retention-profile producer. Durable-set production requires
/// its genuine arm owner; no parent digest can stand in for that authorization.
/// `parent` is borrowed from this input's real Frozen primary source scope.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_normal_candidate_package_bounded_v1(
    prepared: &PreparedNormalHostRuntimeV1,
    input: &PaidVerifiedNormalTickInputV1<'_>,
    parent: &oasis7_distfs::controlled_authority::formal::endpoint::HostAcceptedParentReadViewV1<
        '_,
    >,
    writer: &SigningKey,
    business_update: &PaidFormalBusinessResultUpdateV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidNormalCandidatePackageV1, NormalPackageErrorV1> {
    input.validate_context(common)?;
    prepared.validate_input_source_bounded_v1(input.data(), common)?;
    common.charge(1, 256)?;
    let checked = input.data().checked();
    if !std::ptr::eq(checked.parent(),parent.head())
        || prepared.candidate().expected_fence()!=parent.fence()
        || !parent.core().uses_budget(common.memory())
        || parent.authority().definition().profile_version
            !=oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_AUTHORITY_PROFILE_VERSION {
        return Err(invalid())
    }
    if !oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_initial_record_schema(checked.parent().record().schema_version)
        || checked
            .parent()
            .record()
            .durable_set_transition_digest
            .is_some()
    {
        return Err(NormalPackageErrorV1::Invalid(
            "normal durable retention profile requires actual durable-set arm source",
        ));
    }
    let mut key = [0u8; 64];
    hex::encode_to_slice(writer.verifying_key().as_bytes(), &mut key).map_err(|_| invalid())?;
    common.charge(1, 128)?;
    if key.as_slice() != parent.authority().definition().writer_public_key.as_bytes() {
        return Err(NormalPackageErrorV1::Invalid(
            "normal candidate original writer key mismatch",
        ));
    }
    let mut allocation = Allocation::new(common)?;
    business_update.validate_context(common)?;
    prepared.validate_business_source_bounded_v1(business_update.actual_source(), common)?;
    let business = business_update
        .root_bounded(common)
        .map_err(NormalPackageErrorV1::Business)?;
    let descriptor = business_update
        .descriptor_cbor_bounded(common)
        .map_err(NormalPackageErrorV1::Business)?;
    let delta = business_update
        .delta_cbor_bounded(common)
        .map_err(NormalPackageErrorV1::Business)?;
    let mut material = material::build(
        prepared,
        input,
        &descriptor,
        &delta,
        &business_update.delta().node_writes,
        &mut allocation,
        common,
    )?;
    let governing = prepared.formal_governing_manifest_digest_bounded_v1(common, canonical)?;
    let (events, _count) = prepared.formal_event_commitment_bounded_v1(common)?;
    // This fixed facade getter validates its genuine captured source against
    // this exact terminal seal. The business owner was checked above likewise.
    let projection = prepared.formal_projection_root_bounded_v1(common)?;
    if !governing.uses_budget(common.memory()) || !business.uses_budget(common.memory()) {
        return Err(invalid());
    }
    let body = checked.reservation().body();
    let record = CanonicalRecordV1 {
        schema_version: oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_RETENTION_RECORD_SCHEMA,
        durable_set_transition_digest: None,
        kind: FormalRecordKindV1::Execution,
        world: allocation.world(&body.world, common)?,
        position: body.next_position,
        parent_record_id: allocation.text(&body.parent.record_id, common)?,
        original_authority_id: allocation.text(&body.authority_id, common)?,
        original_epoch: body.epoch,
        activation_ref: allocation.text(&body.activation_ref, common)?,
        request_digest: allocation.text(&body.request_digest, common)?,
        payload_digest: allocation.text(&body.payload_digest, common)?,
        frozen_reservation_id: Some(allocation.hex(&checked.reservation_id(), common)?),
        runtime_manifest_digest: allocation.text(&body.runtime_manifest_digest, common)?,
        governing_manifest_digest: allocation.text(&governing, common)?,
        execution_policy_digest: allocation.text(&body.execution_policy_digest, common)?,
        before_state_root: allocation.text(&body.pre_state_root, common)?,
        after_state_root: allocation.text(&material.after_state_root, common)?,
        action_root: allocation.text(&input.data().context().action_root, common)?,
        event_root: allocation.hex(&events, common)?,
        result_root: allocation.text(&business, common)?,
        projection_root: allocation.text(&projection, common)?,
        artifact_roots: allocation.roots(&material.roots, common)?,
        handoff: allocation.handoff(&body.handoff, common)?,
    };
    business_update
        .validate_record_bounded_v1(&record, common)
        .map_err(NormalPackageErrorV1::Business)?;
    // Exact shared original record grammar and first refusal. The wire remains
    // original serde_cbor, then original u64 domain-length/domain framing.
    record.validate_with_work(&mut |n| common.charge(1, n as u64))?;
    let payload = context_wire::encode_paid(
        &record,
        64 << 20,
        common,
        "formal encoding exceeds byte bound",
        "normal canonical candidate",
    )?;
    // The record identity hashes this same paid original CBOR. Declared graph
    // edges do not change the body bytes/ID and are not a semantic signature;
    // bootstrap independently recomputes the signed projection commitment.
    let domain =
        oasis7_formal_protocol::controlled_authority::formal::CANONICAL_RECORD_ID_DOMAIN_V1
            .as_bytes();
    allocation.admit(
        size_of::<blake3::Hasher>() + size_of::<blake3::Hash>() + 96,
        common,
    )?;
    let mut record_hash = blake3::Hasher::new();
    for raw in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain,
        payload.as_slice(),
    ] {
        for chunk in raw.chunks(8192) {
            common.charge(1, chunk.len() as u64)?;
            record_hash.update(chunk);
        }
    }
    material::attach_projection_sidecar(
        prepared,
        *record_hash.finalize().as_bytes(),
        &mut material,
        &mut allocation,
        common,
    )?;
    let n = payload
        .len()
        .checked_add(CANDIDATE_SIGN_DOMAIN_V1.len())
        .and_then(|n| n.checked_add(8))
        .ok_or_else(invalid)?;
    let mut frame = allocation.vector::<u8>(n, common)?;
    for raw in [
        &(CANDIDATE_SIGN_DOMAIN_V1.len() as u64).to_be_bytes()[..],
        CANDIDATE_SIGN_DOMAIN_V1.as_bytes(),
        payload.as_slice(),
    ] {
        for chunk in raw.chunks(8192) {
            common.charge(1, chunk.len() as u64)?;
            frame.extend_from_slice(chunk)
        }
    }
    common.charge(1, frame.len() as u64)?;
    let signature = writer.sign(&frame).to_bytes();
    let mut signature_hex = allocation.vector::<u8>(128, common)?;
    signature_hex.resize(128, 0);
    common.charge(1, 192)?;
    hex::encode_to_slice(signature, &mut signature_hex).map_err(|_| invalid())?;
    let signature_hex = String::from_utf8(signature_hex).map_err(|_| invalid())?;
    let frozen = allocation.certificate(input.frozen_sources().certificate(), common)?;
    let frozen_material = allocation.closed(checked.material(), common)?;
    // Recheck after the complete encoding/signing phase; cancellation and a
    // foreign Core cannot turn a partially built graph into an output owner.
    prepared.validate_input_source_bounded_v1(input.data(), common)?;
    input.validate_context(common)?;
    Ok(PaidNormalCandidatePackageV1 {
        candidate: FormalCandidateMaterialV1 {
            candidate: SignedCanonicalCandidateV1 {
                record,
                writer_signature_hex: signature_hex,
            },
            material,
            frozen: Some(frozen),
            frozen_material: Some(frozen_material),
            branch: FormalCompletionBranchV1::Normal,
        },
        commit: prepared.candidate().commit_id(),
        material: checked.material_id(),
        core: common.work().identity(),
        fee: allocation.fee,
    })
}

#[path = "iws_host_normal_package_persist.rs"]
mod persist;
pub(crate) use persist::PersistedNormalCandidatePackageV1;

#[path = "iws_host_normal_package_business_reader.rs"]
mod business_reader;
pub(in crate::iws_host_runtime) use business_reader::{
    HostBusinessOriginalReadErrorV1, HostNormalBusinessOriginalReaderV1,
    PaidHostBusinessOriginalFailureV1,
};
