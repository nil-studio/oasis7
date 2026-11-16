//! Genuine immutable GEN metadata and final index-finish roots. Provisional
//! output only; activation, signed candidate and publication are separate gates.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::business_index::PaidBusinessIndexDeltaV1;
use oasis7_runtime::PaidProjectedGenesisBusinessSourceV1;
use std::cell::RefCell;
pub struct PaidGenesisFormalBusinessResultUpdateV1<'a> {
    source: PaidProjectedGenesisBusinessSourceV1<'a>,
    tree: FormalBusinessResultTreeV1,
    delta: PaidBusinessIndexDeltaV1,
    core: WorkIdentityV1,
    phase: MemoryReservation,
}
impl PaidGenesisFormalBusinessResultUpdateV1<'_> {
    pub fn tree(&self) -> &FormalBusinessResultTreeV1 {
        &self.tree
    }
    pub fn delta(&self) -> &BusinessIndexDeltaV1 {
        self.delta.delta()
    }
    pub fn actual_source(&self) -> &PaidProjectedGenesisBusinessSourceV1<'_> {
        &self.source
    }
    pub fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), PaidFormalBusinessErrorV1> {
        self.source.validate_context(common).map_err(body)?;
        self.delta
            .validate_context(common)
            .map_err(|e| fail(e, None))?;
        if self.core != common.work().identity() || !self.phase.uses_budget(common.memory()) {
            return Err(fail(invalid("GEN business output context"), None));
        }
        Ok(())
    }
    pub fn root_bounded(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<String>, PaidFormalBusinessErrorV1> {
        self.validate_context(common)?;
        paid_root(&self.tree, common)
    }
    pub fn descriptor_cbor_bounded(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, PaidFormalBusinessErrorV1> {
        self.validate_context(common)?;
        encode(&self.tree, common)
    }
    pub fn delta_cbor_bounded(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, PaidFormalBusinessErrorV1> {
        self.validate_context(common)?;
        encode(self.delta.delta(), common)
    }
    pub fn validate_record_bounded_v1(
        &self,
        record: &CanonicalRecordV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), PaidFormalBusinessErrorV1> {
        self.validate_context(common)?;
        record_matches(&self.tree, record, common)
    }
}
struct GenesisFacts<'a, 'b, 'c, 'd> {
    state: &'a RefCell<BuildState<'b, 'c>>,
    canonical: &'a RefCell<&'d mut oasis7_wasm_abi::CanonicalCborWorkV1>,
}
impl kernel::CanonicalOps for GenesisFacts<'_, '_, '_, '_> {
    fn canonical<T: Serialize>(&mut self, value: &T) -> Result<Vec<u8>, String> {
        canonical::encode(value, self.state, self.canonical)
    }
}
impl kernel::Facts for GenesisFacts<'_, '_, '_, '_> {
    fn has_world_results(&self) -> bool {
        false
    }
    fn check_result(&mut self, _: &str, _: &CanonicalIntentResultV1) -> Result<(), String> {
        Err("formal initial business index requires explicit legacy result migration".into())
    }
    fn scope(&mut self, _: &SubmitIntentRequest<WorldServicePayloadV1>) -> Result<String, String> {
        Err("formal initial business index has no outcome scope".into())
    }
}
/// The only initialization branch accepted by the original business kernel:
/// a real GEN World with no unindexed legacy outcomes. Nonempty ledgers retain
/// the original migration refusal, never drop outcomes or reset firstposition.
pub fn build_genesis_formal_business_result_bounded_v1<'a>(
    source: PaidProjectedGenesisBusinessSourceV1<'a>,
    reader: &impl PaidBusinessIndexNodeReaderV1,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
) -> Result<PaidGenesisFormalBusinessResultUpdateV1<'a>, PaidFormalBusinessErrorV1> {
    source.validate_context(common).map_err(body)?;
    if source.world_result_count() != 0 {
        return Err(fail(
            invalid("formal initial business index requires explicit legacy result migration"),
            None,
        ));
    }
    let (world, chain, genesis) = source.identity();
    let genesis = genesis.ok_or_else(|| {
        fail(
            invalid("formal result World differs from qualified canonical record"),
            None,
        )
    })?;
    let n = world
        .len()
        .checked_add(chain.len())
        .and_then(|n| n.checked_add(genesis.len()))
        .and_then(|n| n.checked_mul(16))
        .and_then(|n| n.checked_add(128 * 1024))
        .ok_or_else(|| fail(overflow(), None))?;
    common.charge(1, n as u64).map_err(|e| fail(e, None))?;
    let mut phase = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| fail(invalid("GEN business metadata phase"), None))?;
    let binding = source
        .runtime_binding_bounded_v1(common, canonical)
        .map_err(body)?;
    let governing = binding.runtime_manifest_hash.to_string();
    let governing = governing
        .strip_prefix("blake3:")
        .ok_or_else(|| fail(invalid("formal initial governing digest"), None))?;
    let copies = binding.world_id.len()
        + binding.branch_id.len()
        + binding.finality_status.len()
        + binding
            .finality_block_hash
            .as_ref()
            .map_or(0, |v| v.as_str().len())
        + governing.len()
        + source.after_state_root().len();
    common.charge(1, copies as u64).map_err(|e| fail(e, None))?;
    // Runtime's paid binding retains its exact raw aliases and original digest
    // preimages. Admit the actual copied aliases before constructing metadata.
    let mut aliases_fee = common
        .memory()
        .reserve(
            copies
                .checked_mul(4)
                .and_then(|n| n.checked_add(size_of::<FormalBusinessCommitMetadataV1>()))
                .ok_or_else(|| fail(overflow(), None))?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| fail(invalid("GEN business aliases phase"), None))?;
    phase
        .merge(&mut aliases_fee)
        .map_err(|e| fail(invalid(e), None))?;
    let (floor, high, era) = source.journal_bounds();
    let metadata = FormalBusinessCommitMetadataV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        formal_position: 1,
        world_id: world.to_owned(),
        chain_id: chain.to_owned(),
        genesis_digest: genesis.to_owned(),
        after_state_root: source.after_state_root().to_owned(),
        provider_world_id: binding.world_id.clone(),
        branch_id: binding.branch_id.clone(),
        finality_epoch: binding.finality_epoch,
        finality_status: binding.finality_status.clone(),
        finality_block_hash: binding
            .finality_block_hash
            .as_ref()
            .map(ToString::to_string)
            .map(|s| s.strip_prefix("blake3:").unwrap_or(&s).to_owned()),
        reorg_generation: binding.reorg_epoch,
        governing_manifest_digest: governing.to_owned(),
        authority_generation: source.authority_generation(),
        permission_generation: source.permission_generation(),
        event_id_era: era,
        journal_retention_floor: floor,
        journal_high_watermark: high,
    };
    metadata
        .validate()
        .map_err(|e| fail(invalid(e.to_string()), None))?;
    let core = common.work().identity();
    let state = RefCell::new(BuildState {
        common,
        first: None,
        leaf_phase: None,
    });
    let canonical = RefCell::new(canonical);
    let result = {
        let mut index = PaidIndex {
            state: &state,
            reader,
        };
        let mut facts = GenesisFacts {
            state: &state,
            canonical: &canonical,
        };
        kernel::extend_with_index(
            &mut facts,
            metadata,
            None,
            std::iter::empty::<&CanonicalIntentResultV1>(),
            &mut index,
            Vec::new(),
        )
    };
    let mut state = state.into_inner();
    if let Some(mut fee) = state.leaf_phase.take() {
        phase.merge(&mut fee).map_err(|e| fail(invalid(e), None))?
    }
    if let Some(mut error) = state.first {
        error._phase = Some(phase);
        return Err(error);
    }
    match result {
        Ok((tree, delta)) => Ok(PaidGenesisFormalBusinessResultUpdateV1 {
            source,
            tree,
            delta,
            core,
            phase,
        }),
        Err(note) => Err(fail(invalid(note), Some(phase))),
    }
}
