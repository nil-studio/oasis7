//! Single terminal-provider rule kernel over complete ordered facts.
//! Crate-private source implementations are restricted to the legacy World
//! and the fixed Current adapter; these traits are not external admission APIs.
use super::provider_terminal_authority::ProviderCapabilityAvailability;
use super::{World, WorldError};
use crate::{CapabilityAgentIdentity, CapabilityInvocationContext, ResolvedAgentOwnerV1};
use oasis7_wasm_abi::CapabilityGrantV2;
use oasis7_wasm_abi::CapabilitySubject;
use serde_json::Value;
use std::collections::BTreeMap;
fn deny(reason: &str) -> WorldError {
    WorldError::CapabilityAuthorizationDenied {
        reason: reason.into(),
    }
}

pub(in crate::world) mod sealed {
    pub trait Sources {}
    pub trait Ops {}
}
/// The Current branch keeps the actual paid projection alive. It does not
/// invent unused legacy owner fields or treat an absent Current owner as a
/// canonical-owner mismatch.
pub(in crate::world) enum ProviderTerminalOwnerV1 {
    Legacy(ResolvedAgentOwnerV1),
 Current(oasis7_proto::memory_budget::ReservedValue<Option<super::incremental_industrial_eligibility::safe_alternative_owner::CurrentSafeAlternativeOwnerV1>>),
}
impl ProviderTerminalOwnerV1 {
    pub(in crate::world) fn player_and_key(&self) -> Option<(&str, Option<&str>)> {
        match self {
            Self::Legacy(owner) => Some((&owner.player_id, owner.public_key.as_deref())),
            Self::Current(owner) => (**owner)
                .as_ref()
                .map(|owner| (owner.player.as_str(), owner.public_key.as_deref())),
        }
    }
}

pub(in crate::world) trait ProviderTerminalSourcesV1: sealed::Sources {
    type Error: From<WorldError>;
    type LiveContextError;
    fn production_hardened(&self) -> bool;
    fn allow_local_finality_signing(&self) -> bool;
    fn agent_live(&self, agent: &str) -> Result<bool, Self::Error>;
    fn verify_authorization_root(&self) -> Result<(), Self::Error>;
    fn identity(&self, agent: &str) -> Result<Option<CapabilityAgentIdentity>, Self::Error>;
    fn owner(&self, agent: &str) -> Result<Option<ProviderTerminalOwnerV1>, Self::Error>;
    /// Outer error is infrastructure and must never trigger expired fallback.
    /// Only the original business context failure takes the historical path.
    fn live_context(
        &self,
        agent: &str,
    ) -> Result<Result<CapabilityInvocationContext, Self::LiveContextError>, Self::Error>;
    fn grants(&self) -> Result<&BTreeMap<String, Value>, Self::Error>;
    fn contexts(&self) -> Result<&BTreeMap<String, CapabilityInvocationContext>, Self::Error>;
    fn tick(&self) -> u64;
    fn registry(&self) -> Result<&crate::modules::ModuleRegistry, Self::Error>;
    fn verify_issuer(&self, g: &CapabilityGrantV2) -> Result<(), Self::Error>;
    fn verify_live_revocation(&self, g: &CapabilityGrantV2) -> Result<(), Self::Error>;
    fn verify_parent_chain_at_tick(
        &self,
        g: &CapabilityGrantV2,
        tick: u64,
    ) -> Result<(), Self::Error>;
    fn verify_audience(&self, g: &CapabilityGrantV2) -> Result<(), Self::Error>;
}
/// Fee hooks cannot return a predicate, decode, command list, or digest. A paid
/// adapter retains clone/codec/output and geometric Vec/BTree admission to Drop;
/// source methods independently cover their own reads and returned owned data.
pub(in crate::world) trait ProviderTerminalOpsV1<E>:
    sealed::Ops + oasis7_wasm_abi::CapabilityCanonicalWorkV1
{
    fn work(&mut self, visits: usize, bytes: usize) -> Result<(), E>;
    fn before_value<T: serde::Serialize>(&mut self, value: &T) -> Result<(), E>;
    fn before_slots<T>(&mut self, count: usize) -> Result<(), E>;
    fn before_map<K, V>(&mut self, count: usize) -> Result<(), E>;
    fn before_command_catalog(
        &mut self,
        registry: &crate::modules::ModuleRegistry,
    ) -> Result<(), E>;
}
fn contains_grant<E, O: ProviderTerminalOpsV1<E>>(
    ops: &mut O,
    ids: &[String],
    wanted: &str,
) -> Result<bool, E> {
    for id in ids {
        ops.work(1, id.len().saturating_add(wanted.len()))?;
        if id == wanted {
            return Ok(true);
        }
    }
    Ok(false)
}
fn map_work<E, O: ProviderTerminalOpsV1<E>>(
    ops: &mut O,
    rows: usize,
    key_bytes: usize,
) -> Result<(), E> {
    let comparisons = 11usize.saturating_mul(rows.saturating_add(1).ilog2() as usize + 1);
    ops.work(comparisons, key_bytes.saturating_mul(comparisons))
}
pub(in crate::world) fn local_provider_capability_availability_from_sources_v1<
    S: ProviderTerminalSourcesV1,
    O: ProviderTerminalOpsV1<S::Error>,
>(
    source: &S,
    ops: &mut O,
    agent: &str,
) -> Result<ProviderCapabilityAvailability, S::Error> {
    ops.work(1, agent.len())?;
    if source.production_hardened()
        || !source.allow_local_finality_signing()
        || !source.agent_live(agent)?
    {
        return Err(
            deny("terminal recovery is unavailable outside live DevLocal authority").into(),
        );
    }
    source.verify_authorization_root()?;
    let identity = source
        .identity(agent)?
        .ok_or_else(|| deny("terminal Agent identity missing"))?;
    let owner = source.owner(agent)?;
    let (player, public_key) = owner
        .as_ref()
        .and_then(ProviderTerminalOwnerV1::player_and_key)
        .ok_or_else(|| deny("terminal current owner unavailable"))?;
    ops.before_value(&player)?;
    ops.before_value(&public_key)?;
    ops.before_value(&identity)?;
    if player != identity.owner_binding || public_key.is_none() {
        return Err(deny("terminal current canonical owner changed").into());
    }
    if let Ok(context) = source.live_context(agent)? {
        ops.before_slots::<String>(source.grants()?.len())?;
        let mut live = Vec::new();
        for encoded in source.grants()?.values() {
            ops.before_value(encoded)?;
            let grant: CapabilityGrantV2 = serde_json::from_value(encoded.clone())
                .map_err(|_| deny("live local grant malformed"))?;
            if grant.issuer.issuer_id
                == super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_ISSUER_ID
                && matches!(&grant.subject, CapabilitySubject::Agent { agent_id, owner_binding, generation }
                        if agent_id == agent && owner_binding == &identity.owner_binding && *generation == identity.generation)
                && grant.issued_at_tick <= source.tick()
                && grant
                    .expires_at_tick
                    .is_some_and(|expiry| source.tick() <= expiry)
            {
                validate_local_terminal_grant_from_sources_v1(source, ops, &grant, source.tick())?;
                live.push(grant.grant_id);
            }
        }
        ops.before_value(&context)?;
        ops.before_value(&live)?;
        if live.len() != 1 || live[0] != context.grant_id {
            return Err(deny("live local provider grant is missing or ambiguous").into());
        }
        return Ok(ProviderCapabilityAvailability::Live);
    }
    ops.before_map::<String, CapabilityGrantV2>(source.grants()?.len())?;
    let mut candidates = std::collections::BTreeMap::new();
    for (key, value) in source.grants()? {
        ops.work(1, key.len())?;
        ops.before_value(value)?;
        let grant: CapabilityGrantV2 = serde_json::from_value(value.clone())
            .map_err(|_| deny("terminal grant is malformed"))?;
        let CapabilitySubject::Agent {
            agent_id,
            owner_binding,
            generation,
        } = &grant.subject
        else {
            continue;
        };
        if agent_id != agent {
            continue;
        }
        if owner_binding != &identity.owner_binding || *generation != identity.generation {
            return Err(deny("terminal grant owner or generation changed").into());
        }
        if grant.scope.object_kind != "command"
            || !matches!(grant.scope.operation.as_str(), "execute" | "invoke")
        {
            continue;
        }
        if key != &grant.grant_id {
            return Err(deny("terminal grant key mismatch").into());
        }
        validate_local_terminal_grant_from_sources_v1(source, ops, &grant, grant.issued_at_tick)?;
        ops.before_value(&grant.grant_id)?;
        map_work(ops, candidates.len(), grant.grant_id.len())?;
        candidates.insert(grant.grant_id.clone(), grant);
    }
    if candidates.is_empty() {
        return Err(deny("terminal expired grant set missing").into());
    }
    let mut not_expired = false;
    for grant in candidates.values() {
        ops.work(1, 0)?;
        if grant.issued_at_tick > source.tick()
            || grant
                .expires_at_tick
                .is_none_or(|expiry| source.tick() <= expiry)
        {
            not_expired = true;
            break;
        }
    }
    if not_expired {
        return Err(deny("terminal candidates are not exclusively expired").into());
    }
    ops.before_map::<String, ()>(source.contexts()?.len())?;
    let mut contexts = std::collections::BTreeSet::new();
    for (key, context) in source.contexts()? {
        ops.work(1, key.len())?;
        ops.before_value(context)?;
        if matches!(&context.subject, CapabilitySubject::Agent { agent_id, .. } if agent_id == agent)
        {
            super::capability_authorization::validate_invocation_context_with_work_v1(
                context,
                &mut |s, b| ops.work(s as usize, b as usize),
            )??;
            map_work(ops, candidates.len(), context.grant_id.len())?;
            let grant = candidates
                .get(&context.grant_id)
                .ok_or_else(|| deny("terminal invocation has no matching historical grant"))?;
            if context.subject != grant.subject
                || context.audience != grant.audience
                || context.module_id != grant.scope.module_id
                || context.module_version != grant.scope.module_version
                || context.presenter.presenter_kind != "provider"
                || context_key_with_ops_v1(ops, context)?.as_str() != key
            {
                return Err(deny("terminal persisted invocation identity is invalid").into());
            }
            ops.before_value(&context.grant_id)?;
            map_work(ops, contexts.len(), context.grant_id.len())?;
            contexts.insert(context.grant_id.clone());
        }
    }
    if contexts.len() != candidates.len() {
        return Err(deny("terminal historical provider invocation missing").into());
    }
    ops.before_slots::<String>(candidates.len())?;
    Ok(ProviderCapabilityAvailability::ExpiredOnly {
        grant_ids: candidates.into_keys().collect(),
    })
}
pub(in crate::world) fn validate_provider_grant_for_terminal_from_sources_v1<
    S: ProviderTerminalSourcesV1,
    O: ProviderTerminalOpsV1<S::Error>,
>(
    source: &S,
    ops: &mut O,
    agent: &str,
    grant_id: &str,
    admitted_at_tick: u64,
) -> Result<CapabilityGrantV2, S::Error> {
    match local_provider_capability_availability_from_sources_v1(source, ops, agent)? {
        ProviderCapabilityAvailability::ExpiredOnly { grant_ids }
            if contains_grant(ops, &grant_ids, grant_id)? => {}
        _ => {
            return Err(deny("terminal recovery requires the exact expired original grant").into());
        }
    }
    map_work(ops, source.grants()?.len(), grant_id.len())?;
    let encoded = &source.grants()?[grant_id];
    ops.before_value(encoded)?;
    let grant: CapabilityGrantV2 = serde_json::from_value(encoded.clone())
        .map_err(|_| deny("terminal original grant malformed"))?;
    validate_local_terminal_grant_from_sources_v1(source, ops, &grant, admitted_at_tick)?;
    Ok(grant)
}
pub(in crate::world) fn validate_local_terminal_grant_from_sources_v1<
    S: ProviderTerminalSourcesV1,
    O: ProviderTerminalOpsV1<S::Error>,
>(
    source: &S,
    ops: &mut O,
    grant: &CapabilityGrantV2,
    tick: u64,
) -> Result<(), S::Error> {
    ops.before_value(grant)?;
    grant
        .validate_with_fallible_work_v1(&mut |s, b| ops.work(s as usize, b as usize))?
        .map_err(|_| deny("terminal grant structure invalid"))?;
    ops.before_value(grant)?;
    if grant.issuer.issuer_id != super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_ISSUER_ID
        || grant.scope.module_id
            != super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_MODULE_ID
        || grant.scope.module_version
            != super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_MODULE_VERSION
        || grant.scope.namespace
            != super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_NAMESPACE
        || grant.scope.object_name
            != super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_COMMAND
        || grant.status != "verified"
        || grant.issued_at_tick > tick
        || grant.expires_at_tick.is_none_or(|expiry| tick > expiry)
    {
        return Err(deny("terminal original grant lifetime or identity invalid").into());
    }
    let body =
        oasis7_wasm_abi::canonical_hash_with_work_v1(&grant.canonical_body_input_v1(), &*ops);
    ops.work(0, 0)?;
    if body
        .map_err(|_| deny("terminal grant hash invalid"))?
        .as_str()
        != grant.canonical_body_hash
    {
        return Err(deny("terminal original grant lifetime or identity invalid").into());
    }
    let expected =
        oasis7_wasm_abi::canonical_hash_with_work_v1(&grant.canonical_body_input_v1(), &*ops);
    ops.work(0, 0)?;
    if expected
        .map_err(|_| deny("terminal grant identity invalid"))?
        .as_str()
        != grant.grant_id
    {
        return Err(deny("terminal original grant lifetime or identity invalid").into());
    }
    source.verify_issuer(grant)?;
    source.verify_live_revocation(grant)?;
    source.verify_parent_chain_at_tick(grant, tick)?;
    source.verify_audience(grant)?;
    ops.before_command_catalog(source.registry()?)?;
    let mut declared = false;
    for command in oasis7_world_state::modules::module_command_catalog(source.registry()?) {
        ops.before_value(&command)?;
        ops.before_value(&grant.scope)?;
        if grant.scope.module_id == command.module_id
            && grant.scope.module_version == command.module_version
            && grant.scope.namespace == command.namespace
            && grant.scope.object_name == command.name
        {
            declared = true;
            break;
        }
    }
    if !declared {
        return Err(deny("terminal grant has no unchanged active declaration").into());
    }
    Ok(())
}

pub(in crate::world) struct LegacyTerminalSourcesV1<'a>(pub &'a World);
impl sealed::Sources for LegacyTerminalSourcesV1<'_> {}
impl ProviderTerminalSourcesV1 for LegacyTerminalSourcesV1<'_> {
    type Error = WorldError;
    type LiveContextError = String;
    fn production_hardened(&self) -> bool {
        self.0.release_security_policy().is_production_hardened()
    }
    fn allow_local_finality_signing(&self) -> bool {
        self.0
            .release_security_policy()
            .allow_local_finality_signing
    }
    fn agent_live(&self, agent: &str) -> Result<bool, WorldError> {
        Ok(self.0.state.agents.contains_key(agent))
    }
    fn verify_authorization_root(&self) -> Result<(), WorldError> {
        self.0.verify_capability_authorization_root()
    }
    fn identity(&self, agent: &str) -> Result<Option<CapabilityAgentIdentity>, WorldError> {
        Ok(self.0.current_agent_identity(agent))
    }
    fn owner(&self, agent: &str) -> Result<Option<ProviderTerminalOwnerV1>, WorldError> {
        Ok(self
            .0
            .current_agent_owner(agent)
            .map(ProviderTerminalOwnerV1::Legacy))
    }
    fn live_context(
        &self,
        agent: &str,
    ) -> Result<Result<CapabilityInvocationContext, String>, WorldError> {
        Ok(self
            .0
            .canonical_agent_service_context(agent)
            .map(|context| context.capability_invocation_context))
    }
    fn grants(&self) -> Result<&BTreeMap<String, Value>, Self::Error> {
        Ok(&self.0.capability_grants_v2)
    }
    fn contexts(&self) -> Result<&BTreeMap<String, CapabilityInvocationContext>, Self::Error> {
        Ok(&self.0.capability_invocation_contexts)
    }
    fn tick(&self) -> u64 {
        self.0.state.time
    }
    fn registry(&self) -> Result<&crate::modules::ModuleRegistry, Self::Error> {
        Ok(&self.0.module_registry)
    }
    fn verify_issuer(&self, g: &CapabilityGrantV2) -> Result<(), WorldError> {
        self.0.verify_issuer(g)
    }
    fn verify_live_revocation(&self, g: &CapabilityGrantV2) -> Result<(), WorldError> {
        self.0.verify_live_revocation(g)
    }
    fn verify_parent_chain_at_tick(
        &self,
        g: &CapabilityGrantV2,
        tick: u64,
    ) -> Result<(), WorldError> {
        self.0.verify_parent_chain_at_tick(g, tick)
    }
    fn verify_audience(&self, g: &CapabilityGrantV2) -> Result<(), WorldError> {
        self.0.verify_capability_catalog_audience(g)
    }
}
pub(in crate::world) struct LegacyTerminalOpsV1;
impl sealed::Ops for LegacyTerminalOpsV1 {}
impl ProviderTerminalOpsV1<WorldError> for LegacyTerminalOpsV1 {
    fn work(&mut self, _: usize, _: usize) -> Result<(), WorldError> {
        Ok(())
    }
    fn before_value<T: serde::Serialize>(&mut self, _: &T) -> Result<(), WorldError> {
        Ok(())
    }
    fn before_slots<T>(&mut self, _: usize) -> Result<(), WorldError> {
        Ok(())
    }
    fn before_map<K, V>(&mut self, _: usize) -> Result<(), WorldError> {
        Ok(())
    }
    fn before_command_catalog(
        &mut self,
        _: &crate::modules::ModuleRegistry,
    ) -> Result<(), WorldError> {
        Ok(())
    }
}

impl oasis7_wasm_abi::CanonicalCborAllocator for LegacyTerminalOpsV1 {
    type Charge = ();
    fn reserve(&self, _: usize) -> Result<(), oasis7_wasm_abi::CanonicalCborStreamError> {
        Ok(())
    }
}
impl oasis7_wasm_abi::CapabilityCanonicalWorkV1 for LegacyTerminalOpsV1 {
    type Loan<'a> = std::cell::RefMut<'a, oasis7_wasm_abi::CanonicalCborWorkV1>;
    fn is_metered(&self) -> bool {
        false
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        unreachable!("legacy has no bounded loan")
    }
}

fn context_key_with_ops_v1<E: From<WorldError>, O: ProviderTerminalOpsV1<E>>(
    ops: &mut O,
    context: &CapabilityInvocationContext,
) -> Result<oasis7_wasm_abi::CanonicalSha256DataV1<O::Charge>, E> {
    ops.work(1, context.grant_id.len() + context.response_nonce.len())?;
    let input = super::capability_authorization_events::capability_invocation_context_key_input_v1(
        &context.grant_id,
        &context.response_nonce,
    )?;
    let key = oasis7_wasm_abi::canonical_hash_with_work_v1(&input, &*ops);
    ops.work(0, 0)?;
    key.map_err(|e| {
        super::capability_authorization::deny(format!("invocation context key: {e}")).into()
    })
}
