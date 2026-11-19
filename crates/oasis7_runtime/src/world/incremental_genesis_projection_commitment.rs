//! Complete original GEN projection commitment from the immutable paid capture.
//! GEN data does not impersonate a transaction or an independently accepted head.
use super::super::facts::read_source::{
    self, ProjectionProcessPolicyDataV1, ProjectionReadSourceV1,
};
use super::super::*;
use crate::world::incremental_authorization_root_current::{
    AuthorizationRootCacheLimitsV1, ProvisionalAuthorizationProjectionV1,
};
use crate::world::incremental_genesis_projection::RuntimeProjectedGenesisV1;
use crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapFinalityErrorV1;
use crate::world::incremental_normal_business_commitment::{
    OriginalProjectionReadMetadataV1,
    metadata_parent::{GenesisGovernanceProjectionDataV1, GenesisGovernanceSourceDataV1},
};
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_cbor_data::probe_cbor_data_shape_v1, context_wire,
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use std::mem::size_of;
#[path = "incremental_genesis_projection_serde_depth.rs"]
mod serde_depth;

/// Private, same-World original sources omitted from the observation columns.
pub(in crate::world) struct GenesisProjectionOriginalsV1 {
    authorization: ProvisionalAuthorizationProjectionV1,
    authorization_root: ReservedValue<String>,
    finality_epoch: Option<u64>,
    finality_invalid: bool,
    governance: GenesisGovernanceSourceDataV1,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
impl GenesisProjectionOriginalsV1 {
    pub(in crate::world) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(paid)?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || !self.authorization_root.uses_budget(common.memory())
        {
            return Err(invalid("GEN projection originals context"));
        }
        self.governance.validate_context(common)
    }
}

pub(in crate::world) fn capture_genesis_projection_originals_v1(
    world: &crate::World,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<GenesisProjectionOriginalsV1, BodyMutationError> {
    common
        .charge(1, size_of::<GenesisProjectionOriginalsV1>() as u64)
        .map_err(paid)?;
    let fee = common
        .memory()
        .reserve(
            size_of::<GenesisProjectionOriginalsV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    // Probe the exact original seven-field Serde source. These are shape caps,
    // not fresh work allowances: both encoders debit the existing Common child.
    // Each CBOR value can enlarge by at most eight header bytes when the original
    // canonical encoder chooses its canonical scalar width.
    let revocation =
        crate::world::incremental_authorization_root_projection::AuthorityRevocationViewV1(
            &world.capability_revocation_state,
        );
    let original =
        crate::world::incremental_authorization_root_projection::AuthorizationRootProjectionV1 {
            grants: &world.capability_grants_v2,
            revocation: &revocation,
            invocation_contexts: &world.capability_invocation_contexts,
            budget_accounts: &world.capability_budget_accounts,
            nonce_records: &world.capability_nonce_records,
            receipts: &world.capability_authorization_receipts,
            effect_receipt_links: &world.capability_effect_receipt_links,
        };
    let cap = usize::try_from(common.work().remaining_read_bytes())
        .map_err(|_| invalid("GEN authorization source wire cap"))?;
    let raw = context_wire::encode_paid(
        &original,
        cap,
        common,
        "GEN authorization source wire",
        "GEN authorization source",
    )
    .map_err(paid)?;
    let shape = probe_cbor_data_shape_v1(&raw, raw.len(), common).map_err(paid)?;
    let wire = usize::try_from(shape.values)
        .ok()
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_add(raw.len()))
        .ok_or_else(|| invalid("GEN authorization canonical shape"))?;
    let limits = AuthorizationRootCacheLimitsV1 {
        max_wire_bytes: wire,
        max_depth: serde_depth::measure(&original, common)?,
        max_map_keys: usize::try_from(shape.maps)
            .map_err(|_| invalid("GEN authorization map shape"))?
            .max(7),
        max_key_bytes: wire,
    };
    drop(raw);
    let authorization = world
        .prepare_iws_authorization_projection_data_v1(limits, common, MemoryClass::Ordinary)
        .map_err(paid)?;
    let n = size_of::<String>()
        .checked_add(world.capability_authorization_root.len())
        .ok_or_else(|| invalid("GEN authorization root layout"))?;
    common.charge(1, n as u64).map_err(paid)?;
    let root_fee = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let authorization_root = root_fee.retain(world.capability_authorization_root.clone());
    // Preserve the original last-Tick predicate, including the original SHA
    // preimage. An absent actual record remains absence, never a forged epoch.
    let (finality_epoch, finality_invalid) = match world.tick_consensus_records.last() {
        None => (None, false),
        Some(record) => {
            let digest = crate::world::incremental_normal_bootstrap_finality::legacy_tick_digest(
                &record.block,
                common,
            )?;
            let mut expected = [0u8; 64];
            hex::encode_to_slice(digest, &mut expected).map_err(paid)?;
            common
                .charge(1, record.certificate.block_hash.len() as u64 + 64)
                .map_err(paid)?;
            (
                record.block.header.chain_epoch,
                record.certificate.threshold == 0
                    || record.certificate.block_hash.as_bytes() != expected,
            )
        }
    };
    let governance = world.capture_genesis_governance_source_data_v1(common)?;
    common.consume_steps(0).map_err(paid)?;
    Ok(GenesisProjectionOriginalsV1 {
        authorization,
        authorization_root,
        finality_epoch,
        finality_invalid,
        governance,
        core: common.work().identity(),
        fee,
    })
}

/// Opaque computed DATA; no constructor accepts a digest or completion flag.
/// The later independently accepted bootstrap still recomputes the whole root.
pub struct PaidGenesisProjectionCommitmentV1 {
    root: ReservedValue<String>,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
impl PaidGenesisProjectionCommitmentV1 {
    pub fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(paid)?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || !self.root.uses_budget(common.memory())
        {
            return Err(invalid("GEN projection commitment context"));
        }
        Ok(())
    }
    pub fn root_bounded<'a>(
        &'a self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<&'a str, BodyMutationError> {
        self.validate_context(common)?;
        Ok(self.root.as_str())
    }
}

struct GenesisReadSource<'a> {
    projected: &'a RuntimeProjectedGenesisV1,
    governance: &'a GenesisGovernanceProjectionDataV1,
}
impl read_source::sealed::Source for GenesisReadSource<'_> {}
fn denied(
    reason: &str,
    common: &mut FormalBoundedContextV1<'_>,
) -> CurrentBootstrapFinalityErrorV1 {
    let n = size_of::<crate::WorldError>()
        + reason.len()
        + size_of::<CurrentBootstrapFinalityErrorV1>();
    if let Err(e) = common.charge(1, n as u64) {
        return CurrentBootstrapFinalityErrorV1::Source(paid(e));
    }
    match common.memory().reserve(n, MemoryClass::Ordinary) {
        Ok(fee) => CurrentBootstrapFinalityErrorV1::World(fee.retain(
            crate::WorldError::CapabilityAuthorizationDenied {
                reason: reason.to_owned(),
            },
        )),
        Err(e) => CurrentBootstrapFinalityErrorV1::Source(ValueTreeError::Memory(e).into()),
    }
}
impl ProjectionReadSourceV1 for GenesisReadSource<'_> {
    fn validate_metadata(
        &self,
        metadata: &OriginalProjectionReadMetadataV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        self.check(common)?;
        match metadata {
            OriginalProjectionReadMetadataV1::UncommittedGenesis { source, governance }
                if std::ptr::eq(*source, self.projected.business_metadata())
                    && std::ptr::eq(*governance, self.governance) =>
            {
                Ok(())
            }
            _ => Err(invalid("GEN projection exact immutable metadata source")),
        }
    }
    fn begin_facts(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        self.check(common)
    }
    fn check(&self, common: &mut FormalBoundedContextV1<'_>) -> Result<(), BodyMutationError> {
        self.projected.validate_context_bounded_v1(common)?;
        self.projected
            .projection_originals()
            .validate_context(common)?;
        self.governance.validate_context(common)
    }
    fn process_policy(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ProjectionProcessPolicyDataV1, BodyMutationError> {
        self.check(common)?;
        let source = self.projected.genesis().typed.process_source_v1();
        if !source.uses_budget(common.memory()) {
            return Err(invalid("GEN projection original process pool"));
        }
        common.charge(1, 2).map_err(paid)?;
        let (hardened, local_finality) = source.projection_policy_data_v1();
        Ok(ProjectionProcessPolicyDataV1 {
            hardened,
            local_finality,
        })
    }
    fn authorization(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        self.check(common)?;
        let originals = self.projected.projection_originals();
        let actual = originals.authorization.hash(common).map_err(paid)?;
        common
            .charge(
                1,
                (actual.len() + originals.authorization_root.len()) as u64,
            )
            .map_err(paid)?;
        if actual.as_str() != originals.authorization_root.as_str() {
            return Err(invalid(
                "capability authorization root does not match durable state",
            ));
        }
        Ok(())
    }
    fn finality_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        self.check(common)
    }
    fn finality(
        &self,
        epoch: u64,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), CurrentBootstrapFinalityErrorV1> {
        self.check(common)?;
        let original = self.projected.projection_originals();
        if original.finality_invalid {
            return Err(denied("capability live finality record is invalid", common));
        }
        if original.finality_epoch.is_some_and(|actual| actual < epoch) {
            return Err(denied(
                "capability finality epoch is ahead of live chain",
                common,
            ));
        }
        Ok(())
    }
}

pub(in crate::world) fn prepare_genesis_projection_commitment_v1<
    R: crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
>(
    projected: &RuntimeProjectedGenesisV1,
    retained: &mut R,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut CanonicalCborWorkV1,
) -> Result<PaidGenesisProjectionCommitmentV1, BodyMutationError> {
    projected.validate_context_bounded_v1(common)?;
    if !retained.allocation_budget().same_pool(common.memory())
        || retained.allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid(
            "GEN projection retained original source pool/class",
        ));
    }
    common
        .charge(1, size_of::<PaidGenesisProjectionCommitmentV1>() as u64)
        .map_err(paid)?;
    let fee = common
        .memory()
        .reserve(
            size_of::<PaidGenesisProjectionCommitmentV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let seed = projected.columns();
    let state =
        super::super::state_columns::OriginalStateColumnsDataV1::from_original_seed_data_v1(
            seed, common,
        )?;
    let electricity = super::super::electricity::from_seed_v1(seed, common)?;
    let intents = super::super::intents::from_seed_v1(seed, common)?;
    let observation = seed.observation_columns_data_v1(common)?;
    let auxiliary = seed.auxiliary_columns_data_v1(common)?;
    let template = super::super::template::build_static_projection_schema_data_v1(common)?;
    let raw = super::super::raw_hash::PaidOriginalStateHashDataV1::from_genesis_metadata_v1(
        projected.business_metadata(),
        common,
    )?;
    let governance = projected
        .projection_originals()
        .governance
        .prepare_projection_data_v1(projected.business_metadata(), common, canonical)?;
    let metadata = OriginalProjectionReadMetadataV1::UncommittedGenesis {
        source: projected.business_metadata(),
        governance: &governance,
    };
    let binding =
        metadata.runtime_binding_for_state_hash_bounded_v1(raw.as_str(), common, canonical)?;
    let origin = GenesisReadSource {
        projected,
        governance: &governance,
    };
    let root = super::compute_projection_root_data_v1(
        &state,
        &electricity,
        &intents,
        &observation,
        &auxiliary,
        &template,
        &metadata,
        &raw,
        binding.as_ref(),
        &origin,
        retained,
        common,
        canonical,
    )?;
    origin.check(common)?;
    Ok(PaidGenesisProjectionCommitmentV1 {
        root,
        core: common.work().identity(),
        fee,
    })
}
