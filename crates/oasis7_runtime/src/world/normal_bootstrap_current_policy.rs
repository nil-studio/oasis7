//! Ordered complete Current PolicySet, using the original five-column codec.
//! This is source DATA for the original catalogue hash, not policy permission.
use super::*;
use oasis7_world_state::effect::OriginKind;
use oasis7_world_state::incremental_runtime_module_authorization::*;
use oasis7_world_state::policy::{PolicyDecision, PolicyRule, PolicySet, PolicyWhen};

pub(in crate::world) struct PaidCurrentBootstrapPolicyV1 {
    policy: PolicySet,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    fees: ModuleCapabilityAdmissionFeesV1,
}
impl PaidCurrentBootstrapPolicyV1 {
    pub(in crate::world) fn value(&self) -> &PolicySet {
        &self.policy
    }
    pub(in crate::world) fn validate_current<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_read_source_guard(&self.guard, live)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.fees._layout.uses_budget(common.memory())
        {
            return Err(bad());
        }
        Ok(())
    }
}
pub(in crate::world) enum CurrentBootstrapPolicyErrorV1 {
    Body(BodyMutationError),
    World {
        error: WorldError,
        fees: ModuleCapabilityAdmissionFeesV1,
    },
}
impl std::fmt::Debug for CurrentBootstrapPolicyErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Body(e) => f.debug_tuple("Body").field(e).finish(),
            Self::World { error, .. } => f.debug_tuple("World").field(error).finish(),
        }
    }
}
impl std::fmt::Display for CurrentBootstrapPolicyErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CurrentBootstrapPolicyErrorV1 {}
impl<M, P, R, E, C, PE, PC> Current<'_, '_, '_, '_, '_, '_, '_, '_, M, P, R, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    fn policy_cell(
        &mut self,
        index: u64,
        column: &'static str,
        output: &mut impl std::io::Write,
    ) -> Result<ReservedValue<ModuleAuthorizationValueV1>, Error> {
        let capability = self.sources.capability;
        let live = self.live;
        crate::world::incremental_authorization_root_current::with_authorization_source_read_work_v1(
            capability.roots,self.ctx,self.parent,self.module.capability_authorization,self.common,
            |ctx,_,work|{
                let body=open_module_authorization_body_overlay_v1(ModuleAuthorizationRecordKeyV1::Policy(index),capability.legacy,ctx)
                    .map_err(SourceError::Overlay)?.ok_or_else(||SourceError::from(ValueTreeError::Invalid("bootstrap Current policy row missing")))?;
                let guard=ctx.transaction.guard();
                read_module_authorization_overlay_v1(ModuleAuthorizationCellRequestV1{
                    key:ModuleAuthorizationRecordKeyV1::Policy(index),address:ModuleAuthorizationAddressV1{column,index:None},
                    family:capability.legacy,body:&body,max_payload_bytes:work.module_authorization.max_payload_bytes,
                },ctx,&mut work.module_authorization,&guard,live,output).map_err(SourceError::Overlay)
            }).map_err(|error|Error::Body(source(error)))
    }
    fn policy_text(&mut self, index: u64, column: &'static str) -> Result<Option<String>, Error> {
        let size = match &*self.policy_cell(index, column, &mut std::io::sink())? {
            ModuleAuthorizationValueV1::Unit => return Ok(None),
            ModuleAuthorizationValueV1::Text { utf8_bytes } => {
                usize::try_from(*utf8_bytes).map_err(|_| bad())?
            }
            _ => return Err(bad().into()),
        };
        self.common
            .charge(1, size as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let bytes = size
            .checked_add(std::mem::size_of::<String>())
            .ok_or_else(bad)?;
        let fee = self
            .common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut raw = Vec::new();
        raw.try_reserve_exact(size).map_err(|_| bad())?;
        if raw.capacity() > size {
            return Err(bad().into());
        }
        let read = self.policy_cell(index, column, &mut raw)?;
        if !matches!(&*read,ModuleAuthorizationValueV1::Text{utf8_bytes} if *utf8_bytes==size as u64)
            || raw.len() != size
            || raw.capacity() > size
        {
            return Err(bad().into());
        }
        let text = String::from_utf8(raw).map_err(|_| bad())?;
        self.hold(fee)?;
        Ok(Some(text))
    }
    fn complete_policy_set(&mut self) -> Result<PolicySet, Error> {
        let capability = self.sources.capability;
        let count=crate::world::incremental_authorization_root_current::with_authorization_source_read_work_v1(
            capability.roots,self.ctx,self.parent,self.module.capability_authorization,self.common,
            |ctx,_,_|read_module_policy_length_overlay_v1(capability.legacy,ctx).map_err(SourceError::Overlay)
        ).map_err(|error|Error::Body(source(error)))?;
        let n = usize::try_from(count).map_err(|_| bad())?;
        let size = n
            .checked_mul(std::mem::size_of::<PolicyRule>())
            .and_then(|n| n.checked_add(std::mem::size_of::<PaidCurrentBootstrapPolicyV1>()))
            .ok_or_else(bad)?;
        self.common
            .charge(count, size as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let fee = self
            .common
            .memory()
            .reserve(size, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut rules = Vec::new();
        rules.try_reserve_exact(n).map_err(|_| bad())?;
        if rules.capacity() > n {
            return Err(bad().into());
        }
        self.hold(fee)?;
        for index in 0..count {
            let effect_kind = self.policy_text(index, "when.effect_kind")?;
            let origin_kind =
                match &*self.policy_cell(index, "when.origin_kind", &mut std::io::sink())? {
                    ModuleAuthorizationValueV1::Unit => None,
                    ModuleAuthorizationValueV1::Label("reducer") => Some(OriginKind::Reducer),
                    ModuleAuthorizationValueV1::Label("plan") => Some(OriginKind::Plan),
                    ModuleAuthorizationValueV1::Label("module") => Some(OriginKind::Module),
                    ModuleAuthorizationValueV1::Label("system") => Some(OriginKind::System),
                    _ => return Err(bad().into()),
                };
            let cap_name = self.policy_text(index, "when.cap_name")?;
            let allow = match &*self.policy_cell(index, "decision.kind", &mut std::io::sink())? {
                ModuleAuthorizationValueV1::Label("allow") => true,
                ModuleAuthorizationValueV1::Label("deny") => false,
                _ => return Err(bad().into()),
            };
            let reason = self.policy_text(index, "decision.reason")?;
            let decision = match (allow, reason) {
                (true, None) => PolicyDecision::Allow,
                (false, Some(reason)) => PolicyDecision::Deny { reason },
                _ => return Err(bad().into()),
            };
            rules.push(PolicyRule {
                when: PolicyWhen {
                    effect_kind,
                    origin_kind,
                    cap_name,
                },
                decision,
            });
        }
        Ok(PolicySet { rules })
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn read_current_bootstrap_policy_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
>(
    sources: &ModuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    factory: &mut FactoryScheduleWorkV1,
    module: &mut ModuleLifecycleWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidCurrentBootstrapPolicyV1, CurrentBootstrapPolicyErrorV1> {
    let memory = common.memory().clone();
    let layout = memory
        .reserve(
            std::mem::size_of::<PaidCurrentBootstrapPolicyV1>()
                + ModuleCapabilityAdmissionFeesV1::heap_bytes(),
            MemoryClass::Ordinary,
        )
        .map_err(|e| CurrentBootstrapPolicyErrorV1::Body(ValueTreeError::Memory(e).into()))?;
    let slots = memory
        .reserve(
            32 * std::mem::size_of::<MemoryReservation>(),
            MemoryClass::Ordinary,
        )
        .map_err(|e| CurrentBootstrapPolicyErrorV1::Body(ValueTreeError::Memory(e).into()))?;
    let mut current = Current {
        sources,
        ctx,
        parent,
        factory,
        module,
        common,
        live,
        // This fixed observer only reads the ordered five policy columns;
        // it never enters grant validation/parents or hashes a policy. The
        // catalogue hashes the returned paid PolicySet with its mandatory
        // actual Cognition canonical-work loan after this observer returns.
        bootstrap_canonical: None,
        held: ModuleCapabilityAdmissionFeesV1::from_admitted(layout, slots),
    };
    let policy = match current.complete_policy_set() {
        Ok(value) => value,
        Err(Error::Body(e)) => return Err(CurrentBootstrapPolicyErrorV1::Body(e)),
        Err(Error::World(error)) => {
            return Err(CurrentBootstrapPolicyErrorV1::World {
                error,
                fees: current.held,
            });
        }
    };
    let guard = current.ctx.transaction.guard();
    current
        .ctx
        .transaction
        .validate_guard(&guard, live)
        .map_err(|e| CurrentBootstrapPolicyErrorV1::Body(e.into()))?;
    Ok(PaidCurrentBootstrapPolicyV1 {
        policy,
        guard,
        core: current.common.work().identity(),
        memory,
        fees: current.held,
    })
}
