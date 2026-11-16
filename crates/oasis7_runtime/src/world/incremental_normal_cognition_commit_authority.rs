//! Actual pre-allocation Runtime binding DATA. This carrier does not replace
//! task, capability, reorg, MVCC or full-step admission.
use super::facts::CurrentCognitionRetryProbeV1;
use super::*;
use crate::world::{
    incremental_authorization_root_current::{
        AuthorizationRootCacheErrorV1, AuthorizationRootSourceReadWorkV1, SourceError,
        with_authorization_source_read_work_v1,
    },
    incremental_module_invocation::*,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1;
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_world_state::{
    incremental_cognition_proof_ledger::{
        CognitionProofLedgerFieldV1, ProofLedgerReadWorkV1, ProofLedgerRecoveredV1,
    },
    incremental_field_read::WorldFieldReadContextV1,
};
pub(in crate::world) struct CurrentCognitionBaseSourceV1<'a, 'e, 'p, 'i> {
    probe: &'a CurrentCognitionRetryProbeV1<'a, 'e, 'p, 'i>,
    binding: ReservedValue<RuntimeCognitionBaseBindingV1>,
    _binding_source: ProofLedgerRecoveredV1<serde_json::Value>,
    _manifest_world: ReservedValue<String>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _fee: MemoryReservation,
}
impl CurrentCognitionBaseSourceV1<'_, '_, '_, '_> {
    pub(in crate::world) fn binding(&self) -> &RuntimeCognitionBaseBindingV1 {
        &self.binding
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        parent: &BoundNormalTickParentV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.probe.validate(parent, tx, common, live)?;
        tx.validate_exact_transaction_identity(&self.guard)?;
        tx.validate_read_source_guard(&self.guard, live)?;
        common.charge(3, 64).map_err(original)?;
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || !self.binding.uses_budget(common.memory())
            || !self._manifest_world.uses_budget(common.memory())
        {
            return Err(bad("Current Cognition base source context"));
        }
        Ok(())
    }
}
#[derive(Debug)]
struct SourceCause(SourceError);
impl std::fmt::Display for SourceCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Current Cognition source: {:?}", self.0)
    }
}
impl std::error::Error for SourceCause {}
fn source_error(e: SourceError) -> BodyMutationError {
    match e {
        SourceError::Execution(e) => e.into(),
        SourceError::Cache(AuthorizationRootCacheErrorV1::Value(e)) => e.into(),
        other => original(SourceCause(other)),
    }
}
#[derive(Debug)]
struct BindingCause {
    error: BodyMutationError,
    _fee: MemoryReservation,
}
impl std::fmt::Display for BindingCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Current Cognition binding: {:?}", self.error)
    }
}
impl std::error::Error for BindingCause {}
fn copy(value: &str, common: &mut FormalBoundedContextV1<'_>) -> Result<String, BodyMutationError> {
    let mut out = Vec::with_capacity(value.len());
    for chunk in value.as_bytes().chunks(8192) {
        common.charge(1, chunk.len() as u64).map_err(original)?;
        out.extend_from_slice(chunk);
    }
    String::from_utf8(out).map_err(original)
}
fn base(
    binding: &oasis7_agent_api::RuntimeBindingV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<RuntimeCognitionBaseBindingV1>, BodyMutationError> {
    let mut n = size_of::<RuntimeCognitionBaseBindingV1>();
    for value in [
        &binding.world_id,
        &binding.branch_id,
        &binding.finality_status,
        &binding.base_world_hash.0,
        &binding.runtime_manifest_hash.0,
    ]
    .into_iter()
    .map(Some)
    .chain([binding.finality_block_hash.as_ref().map(|v| &v.0)])
    {
        common.consume_steps(1).map_err(original)?;
        if let Some(value) = value {
            n = n
                .checked_add(value.len())
                .ok_or_else(|| bad("Cognition base binding copy layout"))?;
        }
    }
    common.charge(1, n as u64).map_err(original)?;
    let fee = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(RuntimeCognitionBaseBindingV1 {
        world_id: copy(&binding.world_id, common)?,
        branch_id: copy(&binding.branch_id, common)?,
        finality_epoch: binding.finality_epoch,
        finality_block_hash: binding
            .finality_block_hash
            .as_ref()
            .map(|v| copy(&v.0, common))
            .transpose()?,
        finality_status: copy(&binding.finality_status, common)?,
        base_tick: binding.base_tick,
        base_world_hash: copy(&binding.base_world_hash.0, common)?,
        reorg_epoch: binding.reorg_epoch,
        runtime_manifest_hash: copy(&binding.runtime_manifest_hash.0, common)?,
    }))
}
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn read_current_cognition_base_source_v1<'a, 'e, 'p, 'i, P, PE, PC>(
        &mut self,
        probe: &'a CurrentCognitionRetryProbeV1<'a, 'e, 'p, 'i>,
        parent: &BoundNormalTickParentV1<'_>,
        read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        io: &mut AuthorizationRootSourceReadWorkV1,
        ledger_work: &mut ProofLedgerReadWorkV1,
        ledger_scan: &mut StateScanBudget,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CurrentCognitionBaseSourceV1<'a, 'e, 'p, 'i>, CurrentCognitionCommitErrorV1>
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        probe.validate(parent, self.ctx.transaction, common, &live())?;
        let binding_source = probe.ledger_source_data_v1().read_field(
            self.ctx.transaction,
            CognitionProofLedgerFieldV1::RuntimeBinding,
            read,
            ledger_scan,
            ledger_work,
            common,
            &live(),
        )?;
        let Some(binding_source) = binding_source else {
            let shared = RefCell::new(common);
            let source = work::Source::new(&shared, canonical);
            return Err(source.refusal("runtime_binding_missing")?);
        };
        let tick = crate::world::incremental_normal_host_context::read_current_normal_clock_v1(
            self.families.roots,
            self.ctx,
            read,
            io,
            common,
        )?;
        let refs = parent.refs();
        let mut first = None;
        let world = with_authorization_source_read_work_v1(
            self.families.roots,
            self.ctx,
            read,
            io,
            common,
            |ctx, read, work| {
                let max = usize::try_from(work.max_text_bytes).map_err(|_| {
                    ValueTreeError::ResourceLimited("Cognition manifest world text bound")
                })?;
                match crate::world::incremental_normal_business_commitment::read_manifest_world(
                    ctx,
                    read,
                    refs.storage.current().runtime().resources(),
                    max,
                    &live(),
                ) {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        first = Some(error);
                        Err(SourceError::from(ValueTreeError::Invalid(
                            "Cognition actual resource world source",
                        )))
                    }
                }
            },
        );
        if let Some(first) = first {
            return Err(first.into());
        }
        let (world, world_fee) = world.map_err(source_error)?;
        let world = world_fee.retain(world);
        common.charge(1, 4096).map_err(original)?;
        let error_fee = common
            .memory()
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let actual =
            match crate::world::World::cognition_runtime_binding_from_current_fields_bounded_v1(
                binding_source.value(),
                &world,
                probe.input().before_manifest_hash(),
                probe.input().before_state_hash(),
                tick,
                common,
                canonical,
            ) {
                Ok(value) => value,
                Err(error) => {
                    return Err(original(BindingCause {
                        error,
                        _fee: error_fee,
                    })
                    .into());
                }
            };
        let binding = base(&actual, common)?;
        probe.validate(parent, self.ctx.transaction, common, &live())?;
        let n = size_of::<CurrentCognitionBaseSourceV1<'_, '_, '_, '_>>();
        common.charge(1, n as u64).map_err(original)?;
        let fee = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let output = CurrentCognitionBaseSourceV1 {
            probe,
            binding,
            _binding_source: binding_source,
            _manifest_world: world,
            guard: self.ctx.transaction.guard(),
            core: common.work().identity(),
            _fee: fee,
        };
        output.validate(parent, self.ctx.transaction, common, &live())?;
        Ok(output)
    }
}
