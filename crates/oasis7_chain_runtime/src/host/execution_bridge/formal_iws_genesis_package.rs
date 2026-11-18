//! Original signed GEN candidate DATA from the exact retained producer/header.
//! Endpoint stage, receipts, complete proof and Host admission remain separate.
use super::history::ProvisionalGenesisHeaderV1;
use super::*;
use ed25519_dalek::Signer;
use oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1;
use oasis7_formal_protocol::controlled_authority::{
    formal::{
        AUTHORITY_ID_DOMAIN_V1, CANDIDATE_SIGN_DOMAIN_V1, CANONICAL_RECORD_ID_DOMAIN_V1,
        CanonicalRecordV1, FormalCompletionBranchV1, FormalRecordKindV1, FormalWorldIdentityV1,
        GENESIS_HEAD_DOMAIN_V1, SignedCanonicalCandidateV1,
        bounded_context::WorkIdentityV1,
        context_wire,
        incremental::{IWS_RETENTION_RECORD_SCHEMA, authority_binding_bounded},
    },
    replicated_protocol::{ArtifactObject, ArtifactRole, ClosedRecord},
};
use std::collections::BTreeMap;

pub(crate) struct PaidGenesisCandidatePackageV1 {
    candidate: FormalCandidateMaterialV1,
    header: ProvisionalGenesisHeaderV1,
    native: super::super::StagedGenesisNativeV1,
    core: WorkIdentityV1,
    allocation: MemoryReservation,
}
impl PaidGenesisCandidatePackageV1 {
    pub(crate) fn candidate<'a>(
        &'a self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<&'a FormalCandidateMaterialV1> {
        self.header.validate_context(common)?;
        self.native
            .validate_original(self.header.inputs.native_original(), common)?;
        common.consume_steps(1).map_err(context_error)?;
        if self.core != common.work().identity() || !self.allocation.uses_budget(common.memory()) {
            return Err(PageStoreError::Corrupt("GEN candidate retained context").into());
        }
        Ok(&self.candidate)
    }

    /// Releases only the original producer DATA for the separately guarded
    /// accepted-GEN bootstrap. This method cannot issue an accepted source.
    pub(crate) fn into_projected_bounded_v1(
        self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<oasis7_runtime::incremental_genesis_projection::RuntimeProjectedGenesisV1> {
        self.candidate(common)?;
        self.header.inputs.into_projected_bounded_v1(common)
    }
}

struct Allocation {
    fee: MemoryReservation,
}
impl Allocation {
    fn new(common: &mut FormalBoundedContextV1<'_>) -> Result<Self> {
        let n = size_of::<PaidGenesisCandidatePackageV1>();
        common.charge(1, n as u64).map_err(context_error)?;
        Ok(Self {
            fee: common
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(PageStoreError::Memory)?,
        })
    }
    fn admit(&mut self, n: usize, common: &mut FormalBoundedContextV1<'_>) -> Result<()> {
        common.charge(1, n as u64).map_err(context_error)?;
        let mut fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(PageStoreError::Memory)?;
        self.fee
            .merge(&mut fee)
            .map_err(|_| PageStoreError::Corrupt("GEN package allocation pool"))?;
        Ok(())
    }
    fn vector<T>(&mut self, n: usize, common: &mut FormalBoundedContextV1<'_>) -> Result<Vec<T>> {
        let bytes = n
            .checked_mul(size_of::<T>())
            .and_then(|n| n.checked_add(size_of::<Vec<T>>()))
            .ok_or(PageStoreError::ResourceLimited("GEN package vector layout"))?;
        self.admit(bytes, common)?;
        let mut out = Vec::new();
        out.try_reserve_exact(n)
            .map_err(|_| PageStoreError::ResourceLimited("GEN package vector allocation"))?;
        if out.capacity() != n {
            return Err(PageStoreError::Corrupt("GEN package vector capacity").into());
        }
        Ok(out)
    }
    fn bytes(&mut self, raw: &[u8], common: &mut FormalBoundedContextV1<'_>) -> Result<Vec<u8>> {
        let mut out = self.vector(raw.len(), common)?;
        for chunk in raw.chunks(8192) {
            common
                .charge(1, chunk.len() as u64)
                .map_err(context_error)?;
            out.extend_from_slice(chunk);
        }
        Ok(out)
    }
    fn text(&mut self, raw: &str, common: &mut FormalBoundedContextV1<'_>) -> Result<String> {
        String::from_utf8(self.bytes(raw.as_bytes(), common)?)
            .map_err(|_| PageStoreError::Corrupt("GEN package text encoding").into())
    }
    fn hex(&mut self, id: &[u8; 32], common: &mut FormalBoundedContextV1<'_>) -> Result<String> {
        let mut raw = self.vector::<u8>(64, common)?;
        raw.resize(64, 0);
        common.charge(1, 96).map_err(context_error)?;
        hex::encode_to_slice(id, &mut raw)
            .map_err(|_| PageStoreError::Corrupt("GEN package digest encoding"))?;
        String::from_utf8(raw)
            .map_err(|_| PageStoreError::Corrupt("GEN package digest text").into())
    }
    fn roots(
        &mut self,
        roots: &BTreeMap<ArtifactRole, String>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<BTreeMap<ArtifactRole, String>> {
        self.admit(
            roots
                .len()
                .checked_mul(2048)
                .ok_or(PageStoreError::ResourceLimited("GEN root allocation"))?,
            common,
        )?;
        let mut out = BTreeMap::new();
        for (role, id) in roots {
            common.charge(1, 128).map_err(context_error)?;
            out.insert(*role, self.text(id, common)?);
        }
        Ok(out)
    }
    fn world(
        &mut self,
        world: &FormalWorldIdentityV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<FormalWorldIdentityV1> {
        Ok(FormalWorldIdentityV1 {
            world_id: self.text(&world.world_id, common)?,
            chain_id: self.text(&world.chain_id, common)?,
            genesis_digest: self.text(&world.genesis_digest, common)?,
        })
    }
}

fn framed_hash<T: serde::Serialize>(
    domain: &str,
    value: &T,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<[u8; 32]> {
    let raw = context_wire::encode_paid(
        value,
        64 << 20,
        common,
        "GEN formal encoding byte bound",
        "GEN formal original frame",
    )
    .map_err(context_error)?;
    common
        .charge(1, size_of::<blake3::Hasher>() as u64)
        .map_err(context_error)?;
    let _fee = common
        .memory()
        .reserve(size_of::<blake3::Hasher>(), MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let mut hash = blake3::Hasher::new();
    for raw in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain.as_bytes(),
        raw.as_slice(),
    ] {
        for chunk in raw.chunks(8192) {
            common
                .charge(1, chunk.len() as u64)
                .map_err(context_error)?;
            hash.update(chunk);
        }
    }
    common.charge(1, 32).map_err(context_error)?;
    Ok(*hash.finalize().as_bytes())
}

#[path = "formal_iws_genesis_package_material.rs"]
mod material;

pub(in super::super) fn prepare(
    header: ProvisionalGenesisHeaderV1,
    request: super::super::GenesisCandidateRequestV1<'_>,
    work: &mut IwsGenesisTransferWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    stop: &AtomicBool,
) -> Result<PaidGenesisCandidatePackageV1> {
    let super::super::GenesisCandidateRequestV1 {
        authority,
        writer,
        native,
        stores,
        ..
    } = request;
    header.validate_context(common)?;
    header.inputs.validate_stores(stores, common)?;
    native.validate_original(header.inputs.native_original(), common)?;
    let binding = authority_binding_bounded(authority, common).map_err(context_error)?;
    common.charge(1, 128).map_err(context_error)?;
    if binding != header.history.body.authority {
        return Err(PageStoreError::Corrupt("GEN original authority window changed").into());
    }
    let mut key = [0u8; 64];
    hex::encode_to_slice(writer.verifying_key().as_bytes(), &mut key)
        .map_err(|_| PageStoreError::Corrupt("GEN writer public key encoding"))?;
    if key.as_slice() != authority.definition().writer_public_key.as_bytes() {
        return Err(PageStoreError::Corrupt("GEN original writer key mismatch").into());
    }
    let mut allocation = Allocation::new(common)?;
    let mut material = material::build(&header, &mut allocation, common)?;
    let projected = header.inputs.business().projected().projected();
    let (events, _) = projected
        .formal_event_commitment_bounded_v1(common)
        .map_err(Error::Body)?;
    let business = header
        .inputs
        .business()
        .descriptor()
        .root_bounded(common)
        .map_err(business_error)?;
    let projection = header.inputs.projection_root_bounded(common)?;
    let genesis = framed_hash(
        GENESIS_HEAD_DOMAIN_V1,
        &authority.definition().world,
        common,
    )?;
    let authority_id = framed_hash(AUTHORITY_ID_DOMAIN_V1, authority.definition(), common)?;
    // No consensus action executes during GEN. This is the original empty
    // action-root kernel, with fixed complete admission before its allocation.
    allocation.admit(8192, common)?;
    common.charge(32, 8192).map_err(context_error)?;
    let actions = oasis7_node::compute_consensus_action_root(&[])
        .map_err(|error| Error::Body(BodyMutationError::Original(Box::new(error))))?;
    let record = CanonicalRecordV1 {
        schema_version: IWS_RETENTION_RECORD_SCHEMA,
        durable_set_transition_digest: None,
        kind: FormalRecordKindV1::InitialActivation,
        world: allocation.world(&authority.definition().world, common)?,
        position: 1,
        parent_record_id: allocation.hex(&genesis, common)?,
        original_authority_id: allocation.hex(&authority_id, common)?,
        original_epoch: authority.definition().epoch,
        activation_ref: allocation.text(authority.activation_ref(), common)?,
        request_digest: allocation.hex(&header.history.body.operation, common)?,
        payload_digest: allocation.hex(&header.input.reference.id, common)?,
        frozen_reservation_id: None,
        runtime_manifest_digest: allocation.hex(&header.inputs.manifest_original().0.id, common)?,
        governing_manifest_digest: allocation.hex(&header.inputs.governing_manifest(), common)?,
        execution_policy_digest: allocation.hex(&header.inputs.policy_original().0.id, common)?,
        before_state_root: allocation.text(&material.before_state_root, common)?,
        after_state_root: allocation.text(&material.after_state_root, common)?,
        action_root: actions,
        event_root: allocation.hex(&events, common)?,
        result_root: allocation.text(&business, common)?,
        projection_root: allocation.text(projection, common)?,
        artifact_roots: allocation.roots(&material.roots, common)?,
        handoff: None,
    };
    record
        .validate_with_work(&mut |n| common.charge(1, n as u64))
        .map_err(context_error)?;
    let record_id = framed_hash(CANONICAL_RECORD_ID_DOMAIN_V1, &record, common)?;
    let side = material::attach_projection_sidecar(
        &header,
        record_id,
        &mut material,
        &mut allocation,
        common,
    )?;
    let objects = [
        open_store(stores[0], 0, work, common, stop)?,
        open_store(stores[1], 1, work, common, stop)?,
        open_store(stores[2], 2, work, common, stop)?,
    ];
    // All other declared originals were already persisted by these same
    // producer/header owners. The record-bound sidecar is the sole new original.
    write_original(
        &objects,
        &side.wire,
        Some(side.reference.id),
        work,
        common,
        stop,
    )?;
    let payload = context_wire::encode_paid(
        &record,
        64 << 20,
        common,
        "GEN record byte bound",
        "GEN original candidate",
    )
    .map_err(context_error)?;
    let n = payload
        .len()
        .checked_add(CANDIDATE_SIGN_DOMAIN_V1.len())
        .and_then(|n| n.checked_add(8))
        .ok_or(PageStoreError::ResourceLimited("GEN signing frame layout"))?;
    let mut frame = allocation.vector::<u8>(n, common)?;
    for raw in [
        &(CANDIDATE_SIGN_DOMAIN_V1.len() as u64).to_be_bytes()[..],
        CANDIDATE_SIGN_DOMAIN_V1.as_bytes(),
        payload.as_slice(),
    ] {
        for chunk in raw.chunks(8192) {
            common
                .charge(1, chunk.len() as u64)
                .map_err(context_error)?;
            frame.extend_from_slice(chunk);
        }
    }
    common
        .charge(1, frame.len() as u64)
        .map_err(context_error)?;
    let signature = writer.sign(&frame).to_bytes();
    let mut signature_hex = allocation.vector::<u8>(128, common)?;
    signature_hex.resize(128, 0);
    common.charge(1, 192).map_err(context_error)?;
    hex::encode_to_slice(signature, &mut signature_hex)
        .map_err(|_| PageStoreError::Corrupt("GEN signature encoding"))?;
    let writer_signature_hex = String::from_utf8(signature_hex)
        .map_err(|_| PageStoreError::Corrupt("GEN signature text"))?;
    header.validate_context(common)?;
    cancelled(stop)?;
    Ok(PaidGenesisCandidatePackageV1 {
        candidate: FormalCandidateMaterialV1 {
            candidate: SignedCanonicalCandidateV1 {
                record,
                writer_signature_hex,
            },
            material,
            frozen: None,
            frozen_material: None,
            branch: FormalCompletionBranchV1::Normal,
        },
        header,
        native,
        core: common.work().identity(),
        allocation: allocation.fee,
    })
}
