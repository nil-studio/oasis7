//! Exact approved C0 input and ordered same-producer GEN facts feed the one
//! original Header/History kernel. These persisted originals remain DATA.
use super::execution_inputs::ProvisionalGenesisExecutionInputsV1;
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{
            SignedInitialActivationV1, VerifiedAuthorityV1,
            bounded_context::WorkIdentityV1,
            context_wire,
            incremental::{
                IWS_ACTIVATION_SCHEMA, IWS_AUTHORITY_PROFILE_VERSION, authority_binding_bounded,
            },
        },
        replicated_protocol::ArtifactRole,
    },
    incremental_business_output::BusinessOutputLocatorV1,
    incremental_execution::{EXECUTION_MAX_FACTS, ExecutionFactKindV1, ExecutionFactRefV1},
    incremental_history::{CatalogDeltaV1, CatalogRootV1, HeaderPreparation, PreparedHistoryV1},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::MemoryBudget;

pub(crate) struct GenesisApprovalInputV1<'a> {
    pub authority: &'a VerifiedAuthorityV1,
    pub certificate: &'a SignedInitialActivationV1,
}

pub(super) struct Original {
    pub(super) reference: OriginalArtifactRefV1,
    pub(super) wire: ReservedValue<Vec<u8>>,
}

pub(crate) struct ProvisionalGenesisHeaderV1 {
    pub(super) inputs: ProvisionalGenesisExecutionInputsV1,
    pub(super) history: ReservedValue<PreparedHistoryV1>,
    pub(super) catalog: ReservedValue<CatalogDeltaV1>,
    pub(super) input: Original,
    pub(super) facts: Original,
    pub(super) segment: Original,
    pub(super) prefix: Original,
    pub(super) checkpoint: Original,
    pub(super) body: Original,
    pub(super) catalog_original: Original,
    pub(super) business_locator: Original,
    core: WorkIdentityV1,
    allocation: MemoryReservation,
}

impl ProvisionalGenesisHeaderV1 {
    pub(crate) fn validate_context(&self, common: &mut FormalBoundedContextV1<'_>) -> Result<()> {
        self.inputs.validate_context(common)?;
        common.charge(1, 128).map_err(context_error)?;
        if self.core != common.work().identity()
            || !self.allocation.uses_budget(common.memory())
            || !self.history.uses_budget(common.memory())
            || !self.catalog.uses_budget(common.memory())
            || self.history.body.roots != self.inputs.business().roots_original().0.id
            || self.history.body.facts != self.facts.reference.id
            || self.history.body.history != self.prefix.reference.id
            || self.history.body.checkpoint != Some(self.checkpoint.reference.id)
            || self.history.prefix.segment != self.segment.reference.id
            || self.catalog.entry.commit != self.body.reference.id
        {
            return Err(PageStoreError::Corrupt("GEN header context/source binding").into());
        }
        for original in [
            &self.input,
            &self.facts,
            &self.segment,
            &self.prefix,
            &self.checkpoint,
            &self.body,
            &self.catalog_original,
            &self.business_locator,
        ] {
            common.consume_steps(1).map_err(context_error)?;
            if !original.wire.uses_budget(common.memory())
                || original.reference.byte_len != original.wire.len() as u64
            {
                return Err(PageStoreError::Corrupt("GEN header retained wire owner").into());
            }
        }
        Ok(())
    }
}

#[path = "formal_iws_genesis_approval.rs"]
mod approval;
use approval::approved_input;

fn header(
    common: &mut FormalBoundedContextV1<'_>,
    encode: impl FnOnce(
        &mut dyn FnMut(u64, u64) -> std::result::Result<(), ExecutionError>,
    ) -> std::result::Result<Vec<u8>, ExecutionError>,
) -> Result<ReservedValue<Vec<u8>>> {
    const BOUND: usize =
        4 * oasis7_formal_protocol::incremental_history::HISTORY_MAX_HEADER_BYTES + 1024;
    common.charge(1, BOUND as u64).map_err(context_error)?;
    let fee = common
        .memory()
        .reserve(BOUND, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let raw = artifact_catalog::protocol(common, encode)?;
    if raw.capacity() > BOUND {
        return Err(PageStoreError::Corrupt("GEN header encoding capacity").into());
    }
    Ok(fee.retain(raw))
}

struct Empty<'a>(&'a MemoryBudget);
impl MerkleNodeReader for Empty<'_> {
    type Node = Vec<u8>;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.0)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, _: &str, _: usize) -> std::result::Result<Option<Vec<u8>>, String> {
        Err("fresh GEN history requested an old page".into())
    }
}

fn facts(
    inputs: &ProvisionalGenesisExecutionInputsV1,
    input: [u8; 32],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<ExecutionFactRefV1>>> {
    let projected = inputs.business().projected();
    let journal = projected
        .journal_originals_bounded_v1(common)
        .map_err(Error::Body)?;
    let snapshots = projected
        .snapshot_originals_bounded_v1(common)
        .map_err(Error::Body)?;
    let (ticks, archived) = projected
        .hot_tick_originals_bounded_v1(common)
        .map_err(Error::Body)?;
    let audits = projected
        .retained_audit_originals_bounded_v1(common)
        .map_err(Error::Body)?;
    if archived != 0 {
        return Err(PageStoreError::Corrupt(
            "GEN archived Tick originals require their actual history source",
        )
        .into());
    }
    let count = [journal.len(), snapshots.len(), ticks.len(), audits.len()]
        .into_iter()
        .try_fold(1usize + inputs.source_history().result_count(), |n, k| {
            n.checked_add(k)
        })
        .filter(|n| *n <= EXECUTION_MAX_FACTS)
        .ok_or(PageStoreError::ResourceLimited("GEN ordered fact count"))?;
    let bytes = count
        .checked_mul(size_of::<ExecutionFactRefV1>())
        .and_then(|n| n.checked_add(size_of::<Vec<ExecutionFactRefV1>>()))
        .ok_or(PageStoreError::ResourceLimited("GEN fact allocation"))?;
    common.charge(1, bytes as u64).map_err(context_error)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let mut facts = Vec::new();
    facts
        .try_reserve_exact(count)
        .map_err(|_| PageStoreError::ResourceLimited("GEN fact allocation"))?;
    if facts.capacity() != count {
        return Err(PageStoreError::Corrupt("GEN fact capacity").into());
    }
    facts.push(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: input,
    });
    // These independently checked source manifests occupy the same initial
    // Result ordinals used by their Runtime occurrence indexes.
    for original in inputs.source_history().originals() {
        common.charge(1, 40).map_err(context_error)?;
        facts.push(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Result,
            id: original.reference.id,
        });
    }
    // Preserve every original occurrence and each producer's order. Results
    // retain their original envelopes; Event order is the complete Journal.
    for (kind, originals) in [
        (ExecutionFactKindV1::Result, snapshots.as_slice()),
        (ExecutionFactKindV1::Result, ticks.as_slice()),
        (ExecutionFactKindV1::Result, audits.as_slice()),
        (ExecutionFactKindV1::Event, journal.as_slice()),
    ] {
        for original in originals {
            common.charge(1, 40).map_err(context_error)?;
            facts.push(ExecutionFactRefV1 {
                kind,
                id: original.id,
            });
        }
    }
    Ok(fee.retain(facts))
}

pub(in super::super) fn prepare(
    inputs: ProvisionalGenesisExecutionInputsV1,
    approval: GenesisApprovalInputV1<'_>,
    stores: [&IncrementalPageStore; 3],
    work: &mut IwsGenesisTransferWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    stop: &AtomicBool,
) -> Result<ProvisionalGenesisHeaderV1> {
    inputs.validate_stores(stores, common)?;
    let n = size_of::<ProvisionalGenesisHeaderV1>();
    common.charge(1, n as u64).map_err(context_error)?;
    let allocation = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let objects = [
        open_store(stores[0], 0, work, common, stop)?,
        open_store(stores[1], 1, work, common, stop)?,
        open_store(stores[2], 2, work, common, stop)?,
    ];
    let input_wire = approved_input(&inputs, &approval, work.max_original_bytes, common)?;
    let input = Original {
        reference: write_original(&objects, &input_wire, None, work, common, stop)?,
        wire: input_wire,
    };
    let facts = facts(&inputs, input.reference.id, common)?;
    let operation_wire = context_wire::encode_paid(
        &(
            "oasis7.formal.initial_request.v1",
            &approval.authority.definition().world,
            approval.authority.activation_ref(),
        ),
        work.max_original_bytes,
        common,
        "GEN operation bytes",
        "GEN original request",
    )
    .map_err(context_error)?;
    let operation = digest(&operation_wire, common)?;
    drop(operation_wire);
    common.charge(1, 64 << 10).map_err(context_error)?;
    let _fixed = common
        .memory()
        .reserve(64 << 10, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let config_wire = inputs.business().config_original().1;
    common
        .charge(32, config_wire.len() as u64 * 4)
        .map_err(context_error)?;
    let config = GenesisConfigV1::from_canonical_bytes(config_wire)
        .map_err(artifact_catalog::execution_error)?;
    let memory = common.memory().clone();
    let scope = artifact_catalog::protocol(common, |pay| config.scope_with_work(pay))?;
    let empty =
        artifact_catalog::protocol(common, |pay| CatalogRootV1::empty_with_work(scope, pay))?;
    let authority = authority_binding_bounded(approval.authority, common).map_err(context_error)?;
    let history = artifact_catalog::protocol(common, |pay| {
        HeaderPreparation {
            config: &config,
            catalog: &empty,
            roots: inputs.business().projected().roots(),
            parent: None,
            parent_history: None,
            transition: None,
            genesis_facts: &facts,
            operation,
            authority,
            execution_environment: inputs.native_original().0.id,
            checkpoint: true,
        }
        .prepare_with_work(&memory, pay)
    })?;
    drop(facts);
    let catalog = artifact_catalog::protocol(common, |pay| {
        empty.extend_with_work(
            &history.body,
            &Empty(&memory),
            &memory,
            2 * (256 + 1) * oasis7_formal_protocol::incremental_history::CATALOG_MAX_NODE_BYTES,
            pay,
        )
    })?;
    // Every actual target owns its own page copy. The same original physical
    // lanes pay for these new History pages; no new account/Core is introduced.
    for node in &catalog.new_nodes {
        common
            .charge(1, node.digest.len() as u64)
            .map_err(context_error)?;
        let mut id = [0; 32];
        hex::decode_to_slice(&node.digest, &mut id)
            .map_err(|_| PageStoreError::Corrupt("GEN history page digest"))?;
        for (i, store) in stores.iter().enumerate() {
            cancelled(stop)?;
            common
                .charge(1, (node.bytes.len() + size_of::<Vec<u8>>()) as u64)
                .map_err(context_error)?;
            let fee = store
                .memory_budget()
                .reserve(
                    node.bytes.len() + size_of::<Vec<u8>>(),
                    MemoryClass::Ordinary,
                )
                .map_err(PageStoreError::Memory)?;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(node.bytes.len())
                .map_err(|_| PageStoreError::ResourceLimited("GEN history page copy"))?;
            if bytes.capacity() != node.bytes.len() {
                return Err(PageStoreError::Corrupt("GEN history page capacity").into());
            }
            for chunk in node.bytes.chunks(8192) {
                common
                    .charge(1, chunk.len() as u64)
                    .map_err(context_error)?;
                bytes.extend_from_slice(chunk);
            }
            let bytes = fee.retain(bytes);
            let mut io = ObjectIo {
                physical: &mut work.objects_physical[i],
                logical: &mut work.objects_logical[i],
                class: MemoryClass::Ordinary,
                cancelled: stop,
            };
            metered(common, &mut io, |io| {
                store.put_page(id, &bytes, io.physical, stop)
            })?;
        }
    }
    let mut persist = |wire: ReservedValue<Vec<u8>>,
                       common: &mut FormalBoundedContextV1<'_>|
     -> Result<Original> {
        Ok(Original {
            reference: write_original(&objects, &wire, None, work, common, stop)?,
            wire,
        })
    };
    let facts = artifact_catalog::protocol(common, |pay| {
        history.facts.canonical_bytes_with_work(&memory, pay)
    })?;
    let facts = persist(facts, common)?;
    let segment = header(common, |pay| history.segment.canonical_bytes_with_work(pay))?;
    let segment = persist(segment, common)?;
    let prefix = header(common, |pay| history.prefix.canonical_bytes_with_work(pay))?;
    let prefix = persist(prefix, common)?;
    let checkpoint = history
        .checkpoint
        .as_ref()
        .ok_or(PageStoreError::Corrupt("GEN actual checkpoint missing"))?;
    let checkpoint = header(common, |pay| checkpoint.canonical_bytes_with_work(pay))?;
    let checkpoint = persist(checkpoint, common)?;
    let body = header(common, |pay| history.body.canonical_bytes_with_work(pay))?;
    let body = persist(body, common)?;
    let catalog_wire = header(common, |pay| catalog.after.canonical_bytes_with_work(pay))?;
    let catalog_original = persist(catalog_wire, common)?;
    let locator = BusinessOutputLocatorV1 {
        commit_body: body.reference.id,
        descriptor: inputs.business().descriptor_original().0,
        delta: inputs.business().delta_original().0,
    };
    let locator = artifact_catalog::protocol(common, |pay| {
        locator.canonical_bytes_with_work(&memory, pay)
    })?;
    let business_locator = persist(locator, common)?;
    let output = ProvisionalGenesisHeaderV1 {
        inputs,
        history,
        catalog,
        input,
        facts,
        segment,
        prefix,
        checkpoint,
        body,
        catalog_original,
        business_locator,
        core: common.work().identity(),
        allocation,
    };
    output.validate_context(common)?;
    Ok(output)
}
