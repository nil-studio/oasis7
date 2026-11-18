//! Fixed Bootstrap verification against the real Current governance and
//! authorization sources. This proves the indicated check only; publication
//! remains with the bootstrap transaction, never with a caller DTO.
use super::*;

/// The query is DATA. Neither variant is an authority-installation permit.
pub(in crate::world) enum BootstrapCapabilityCheckV1<'a> {
    AgentIdentity {
        agent_id: &'a str,
        identity: &'a crate::capability_authorization::CapabilityAgentIdentity,
    },
    AuthorityFinality {
        record: &'a CapabilityAuthorityRecord,
        proof: &'a CapabilityAuthorityFinalityProof,
    },
    IssuerRevocationAndParents(&'a oasis7_wasm_abi::CapabilityGrantV2),
    CatalogueIssuer(&'a oasis7_wasm_abi::CapabilityGrantV2),
    CatalogueRevocation(&'a oasis7_wasm_abi::CapabilityGrantV2),
    CatalogueParents(&'a oasis7_wasm_abi::CapabilityGrantV2),
    CatalogueParentsAt {
        grant: &'a oasis7_wasm_abi::CapabilityGrantV2,
        tick: u64,
    },
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn verify_current_bootstrap_capability_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    check: BootstrapCapabilityCheckV1<'_>,
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    live: &ExecutionFence,
) -> Result<PaidModuleCapabilityAdmissionV1, BodyMutationError> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(&tx.guard(), live)?;
        if !ctx.values.memory.same_pool(common.memory()) || !tx.uses_budget(common.memory()) {
            return Err(bad());
        }
        // Header/fee-slot storage precedes the first owned allocation. Record
        // and selected-source graphs are admitted by the original Current
        // Facts implementation at their actual use sites.
        let bytes = std::mem::size_of::<PaidModuleCapabilityAdmissionV1>()
            .checked_add(ModuleCapabilityAdmissionFeesV1::heap_bytes())
            .ok_or_else(bad)?
            .checked_add(16384)
            .ok_or_else(bad)?;
        common
            .charge(1, bytes as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let layout = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let slots = common
            .memory()
            .reserve(
                32 * std::mem::size_of::<MemoryReservation>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: tx,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        let mut current = Current {
            sources,
            ctx: &mut overlay,
            parent,
            factory,
            module,
            common,
            live,
            bootstrap_canonical:Some(canonical),
            held: ModuleCapabilityAdmissionFeesV1::from_admitted(layout, slots),
        };
        let result = (|| -> Result<(), Error> {
            // The original install and grant-registration APIs verify the
            // durable root before attempting any of the requested predicates.
            current.root()?;
            match check {
                BootstrapCapabilityCheckV1::AgentIdentity { agent_id,identity } => {
                    current.record(&(agent_id,identity))?;
                    crate::world::capability_authorization::validate_agent_identity(agent_id,identity)?;
                    let sources=current.sources;let live=current.live;
                    let present=with_module_lifecycle_read_io_v1(sources.families.roots,current.ctx,current.parent,current.factory,current.common,current.module,|ctx,parent,factory,_|{
                        let guard=ctx.transaction.guard();
                        oasis7_world_state::incremental_agent_subtree::read_current_agent_presence_v1(agent_id,sources.factory.lifecycle.agents,sources.families.roots,ctx,parent,&mut factory.lifecycle.pending_entries_remaining,&guard,live).map_err(super::overlay).map_err(Into::into)
                    }).map_err(read_error)?;
                    if present==oasis7_world_state::incremental_agent_subtree::CurrentAgentPresenceV1::Absent{
                        return Err(crate::world::capability_authorization::deny("capability agent identity requires a live agent").into())
                    }
                    let previous=with_module_lifecycle_read_io_v1(sources.families.roots,current.ctx,current.parent,current.factory,current.common,current.module,|ctx,parent,_,module|{
                        let guard=ctx.transaction.guard();
                        oasis7_world_state::incremental_runtime_revocation::read_current_revocation_agent_identity_v1(agent_id,sources.capability.typed.revocation(),ctx,parent,&mut module.capability_authorization.scan,&mut module.capability_authorization.typed,&guard,live).map_err(super::overlay).map_err(Into::into)
                    }).map_err(read_error)?;
                    if let Some(previous)=previous.identity(){
                        current.work(previous.owner_binding.len().checked_add(identity.owner_binding.len()).ok_or_else(bad)?)?;
                        if identity.generation<previous.generation{return Err(crate::world::capability_authorization::deny("capability agent identity generation regressed").into())}
                        if identity.generation==previous.generation&&identity!=previous{return Err(crate::world::capability_authorization::deny("capability agent identity changed without a new generation").into())}
                    }
                    Ok(())
                }
                BootstrapCapabilityCheckV1::AuthorityFinality { record, proof } => {
                    current.record(record)?;
                    validate_authority_record(record)?;
                    proof_check(&mut current, record, proof)
                }
                BootstrapCapabilityCheckV1::CatalogueIssuer(grant)=>issuer(&mut current,grant),
                BootstrapCapabilityCheckV1::CatalogueRevocation(grant)=>revocation(&mut current,grant),
                BootstrapCapabilityCheckV1::CatalogueParentsAt{grant,tick}=>parents(&mut current,grant,tick,&mut std::collections::BTreeSet::new()),
                BootstrapCapabilityCheckV1::CatalogueParents(grant)=>{let tick=current.tick()?;parents(&mut current,grant,tick,&mut std::collections::BTreeSet::new())},
                BootstrapCapabilityCheckV1::IssuerRevocationAndParents(grant) => {
                    issuer(&mut current, grant)?;
                    revocation(&mut current, grant)?;
                    let tick = current.tick()?;
                    parents(
                        &mut current,
                        grant,
                        tick,
                        &mut std::collections::BTreeSet::new(),
                    )
                }
            }
        })();
        let result = match result {
            Ok(()) => Ok(()),
            Err(Error::World(error)) => Err(error),
            Err(Error::Body(error)) => return Err(error),
        };
        current
            .common
            .consume_steps(0)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        current
            .ctx
            .transaction
            .validate_guard(&current.ctx.transaction.guard(), live)?;
        Ok(PaidModuleCapabilityAdmissionV1 {
            result,
            guard: current.ctx.transaction.guard(),
            core: current.common.work().identity(),
            memory: current.common.memory().clone(),
            fees: current.held,
        })
    })
}
