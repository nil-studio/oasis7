//! One catalog selection kernel. Inventory sources remain crate-private and
//! must supply the complete ordered live registry, never a selected input list.
use super::*;
use crate::capability_authorization::CapabilityAgentIdentity;
use crate::{modules::ModuleRegistry, policy::PolicySet};
use std::collections::BTreeMap;

pub(in crate::world) trait CapabilityCatalogSourcesV1 {
    type Error: From<WorldError>;
    fn agent_live(&self, id: &str) -> Result<bool, Self::Error>;
    fn identity(&self, id: &str) -> Result<Option<CapabilityAgentIdentity>, Self::Error>;
    fn grants(&self) -> Result<&BTreeMap<String, Value>, Self::Error>;
    fn registry(&self) -> Result<&ModuleRegistry, Self::Error>;
    fn policies(&self) -> Result<&PolicySet, Self::Error>;
    fn tick(&self) -> u64;
    fn world_journal_head(&self) -> Result<u64,Self::Error>;
    fn revocation_epoch(&self) -> Result<u64,Self::Error>;
    fn verify_issuer(&self, grant: &CapabilityGrantV2) -> Result<(), Self::Error>;
    fn verify_live_revocation(&self, grant: &CapabilityGrantV2) -> Result<(), Self::Error>;
    fn verify_parent_chain(&self, grant: &CapabilityGrantV2) -> Result<(), Self::Error>;
    fn verify_audience(&self, grant: &CapabilityGrantV2) -> Result<(), Self::Error>;
}
/// Hooks admit work and owned graphs before their original operations. They
/// cannot supply a match, digest, decoded grant, or change a business predicate.
/// Paid implementations retain every admitted temporary/output fee externally.
/// before_value covers clone/validate/codec scratch and string comparisons for
/// the following original operation; before_slots covers Vec geometric capacity,
/// including its minimum allocation, not only initialized elements. The source
/// methods themselves admit any identity clone or actual source/auth IO before
/// returning. before_command_catalog admits the entire original registry walk,
/// declaration validation, clone graph and sorting; it returns no declarations.
pub(in crate::world) trait CapabilityCatalogOpsV1<E>:
    oasis7_wasm_abi::CapabilityCanonicalWorkV1
{
    fn work(&mut self, visits: usize, bytes: usize) -> Result<(), E>;
    fn before_value<T: serde::Serialize>(&mut self, value: &T) -> Result<(), E>;
    fn before_slots<T>(&mut self, count: usize) -> Result<(), E>;
    fn before_command_catalog(&mut self, registry: &ModuleRegistry) -> Result<(), E>;
}
pub(in crate::world) fn capability_context_from_sources_v1<S, O>(
    source: &S,
    ops: &mut O,
    agent_id: &str,
    presenter: CapabilityPresenter,
    response_nonce: impl Into<String>,
) -> Result<(CapabilityCatalogSnapshot, CapabilityInvocationContext), S::Error>
where
    S: CapabilityCatalogSourcesV1,
    O: CapabilityCatalogOpsV1<S::Error>,
{
    ops.work(1, agent_id.len())?;
    if agent_id.trim().is_empty() || !source.agent_live(agent_id)? {
        return Err(deny("capability subject agent is not live").into());
    }
    ops.before_value(&presenter)?;
    presenter
        .validate_with_fallible_work_v1(&mut |s, b| ops.work(s as usize, b as usize))?
        .map_err(|error| deny(format!("capability presenter: {error}")))?;
    let response_nonce = response_nonce.into();
    ops.work(1, response_nonce.len())?;
    if response_nonce.trim().is_empty() {
        return Err(deny("capability response nonce is required").into());
    }
    let identity = source
        .identity(agent_id)?
        .ok_or_else(|| deny("capability subject identity is not installed"))?;

    ops.before_slots::<CapabilityGrantV2>(source.grants()?.len())?;
    let mut grants = Vec::new();
    for (grant_id, encoded) in source.grants()? {
        ops.work(1, grant_id.len())?;
        ops.before_value(encoded)?;
        let grant: CapabilityGrantV2 = serde_json::from_value(encoded.clone())
            .map_err(|_| deny("durable capability grant is malformed"))?;
        if grant.grant_id != *grant_id {
            return Err(deny("durable capability grant key does not match grant id").into());
        }
        let CapabilitySubject::Agent {
            agent_id: subject_id,
            owner_binding,
            generation,
        } = &grant.subject
        else {
            continue;
        };
        if subject_id != agent_id
            || owner_binding != &identity.owner_binding
            || *generation != identity.generation
        {
            continue;
        }
        ops.before_value(&grant)?;
        grant
            .validate_with_fallible_work_v1(&mut |s, b| ops.work(s as usize, b as usize))?
            .map_err(|error| deny(format!("capability grant: {error}")))?;
        ops.before_value(&grant)?;
        let body_hash =
            oasis7_wasm_abi::canonical_hash_with_work_v1(&grant.canonical_body_input_v1(), &*ops);
        ops.work(0, 0)?;
        let body_matches = body_hash
            .map_err(|error| deny(format!("capability grant body hash: {error}")))?
            .as_str()
            == grant.canonical_body_hash;
        if !body_matches {
            return Err(deny("capability grant canonical identity mismatch").into());
        }
        ops.before_value(&grant)?;
        let expected =
            oasis7_wasm_abi::canonical_hash_with_work_v1(&grant.canonical_body_input_v1(), &*ops);
        ops.work(0, 0)?;
        if expected
            .map_err(|error| deny(format!("capability grant id hash: {error}")))?
            .as_str()
            != grant.grant_id
        {
            return Err(deny("capability grant canonical identity mismatch").into());
        }
        if grant.status != "verified"
            || grant.issued_at_tick > source.tick()
            || grant
                .expires_at_tick
                .is_none_or(|expires_at| expires_at < source.tick())
        {
            continue;
        }
        source.verify_issuer(&grant)?;
        source.verify_live_revocation(&grant)?;
        source.verify_parent_chain(&grant)?;
        source.verify_audience(&grant)?;
        if grant.scope.object_kind != "command"
            || !matches!(grant.scope.operation.as_str(), "execute" | "invoke")
        {
            continue;
        }
        grants.push(grant);
    }
    if grants.is_empty() {
        return Err(deny("no live command capability grant for agent").into());
    }

    ops.before_value(&grants[0].audience)?;
    let first_audience = grants[0].audience.clone();
    let mut conflicts = false;
    for grant in &grants {
        ops.before_value(&grant.audience)?;
        if grant.audience != first_audience {
            conflicts = true;
            break;
        }
    }
    if conflicts {
        return Err(deny("agent capability grants have conflicting audiences").into());
    }
    let mut entries = Vec::new();
    ops.before_command_catalog(source.registry()?)?;
    for command in oasis7_world_state::modules::module_command_catalog(source.registry()?) {
        ops.before_value(&command)?;
        ops.before_slots::<String>(grants.len())?;
        let mut eligible_grant_ids = Vec::new();
        for grant in &grants {
            ops.before_value(&grant.scope)?;
            if grant.scope.module_id == command.module_id
                && grant.scope.module_version == command.module_version
                && grant.scope.namespace == command.namespace
                && grant.scope.object_name == command.name
            {
                ops.before_value(&grant.grant_id)?;
                eligible_grant_ids.push(grant.grant_id.clone());
            }
        }
        if eligible_grant_ids.is_empty() {
            continue;
        }
        let mut minimum = None;
        for grant in &grants {
            let mut eligible = false;
            for id in &eligible_grant_ids {
                ops.work(1, id.len().saturating_add(grant.grant_id.len()))?;
                if id == &grant.grant_id {
                    eligible = true;
                    break;
                }
            }
            if eligible
                && let Some(value) = grant.scope.max_payload_bytes
            {
                minimum = Some(minimum.map_or(value, |old: u64| old.min(value)));
            }
        }
        let max_payload_bytes = minimum
            .unwrap_or(command.max_payload_bytes)
            .min(command.max_payload_bytes);
        if max_payload_bytes == 0 {
            continue;
        }
        ops.before_slots::<CapabilityCatalogEntry>(entries.len().saturating_add(1))?;
        entries.push(CapabilityCatalogEntry {
            module_id: command.module_id,
            module_version: command.module_version,
            namespace: command.namespace,
            command: command.name,
            schema_version: command.schema_version,
            schema_hash: command.schema_hash,
            max_payload_bytes,
            eligible_grant_ids,
        });
    }
    let selected_entry = entries
        .first()
        .ok_or_else(|| deny("live command grants have no active declaration"))?;
    ops.before_value(selected_entry)?;
    let selected_grant_id = selected_entry
        .eligible_grant_ids
        .first()
        .cloned()
        .ok_or_else(|| deny("live command declaration has no eligible grant"))?;
    let selected_module_id = selected_entry.module_id.clone();
    let selected_module_version = selected_entry.module_version.clone();
    ops.before_value(source.registry()?)?;
    let module_hash = oasis7_wasm_abi::canonical_hash_with_work_v1(source.registry()?, &*ops);
    ops.work(0, 0)?;
    let module_hash =
        module_hash.map_err(|error| deny(format!("module registry hash: {error}")))?;
    ops.before_value(&module_hash.as_str())?;
    let module_registry_hash = module_hash.as_str().to_owned();
    ops.before_value(source.policies()?)?;
    let policy = oasis7_wasm_abi::canonical_hash_with_work_v1(source.policies()?, &*ops);
    ops.work(0, 0)?;
    let policy = policy.map_err(|error| deny(format!("policy hash: {error}")))?;
    ops.before_value(&policy.as_str())?;
    let policy_hash = policy.as_str().to_owned();
    ops.work(grants.len(), 0)?;
    let valid_until_tick = grants
        .iter()
        .filter_map(|grant| grant.expires_at_tick)
        .min()
        .ok_or_else(|| deny("live command grant expiry is missing"))?;
    ops.before_value(&first_audience)?;
    ops.before_value(&grants[0].subject)?;
    ops.before_value(&presenter)?;
    let catalog = CapabilityCatalogSnapshot {
        snapshot_id: String::new(),
        world_id: first_audience.world_id.clone(),
        world_head: source.world_journal_head()?,
        branch_id: first_audience.branch_id.clone(),
        finality_epoch: first_audience.finality_epoch,
        logical_tick: source.tick(),
        module_registry_hash,
        policy_hash,
        revocation_epoch: source.revocation_epoch()?,
        subject: grants[0].subject.clone(),
        presenter: presenter.clone(),
        audience: first_audience.clone(),
        entries,
        valid_until_tick,
    };
    ops.before_value(&catalog)?;
    let snapshot =
        oasis7_wasm_abi::canonical_hash_with_work_v1(&catalog.canonical_hash_input_v1(), &*ops);
    ops.work(0, 0)?;
    let snapshot = snapshot.map_err(|error| deny(format!("capability catalog hash: {error}")))?;
    ops.before_value(&snapshot.as_str())?;
    let snapshot_id = snapshot.as_str().to_owned();
    let mut catalog = catalog;
    ops.before_value(&snapshot_id)?;
    catalog.snapshot_id = snapshot_id.clone();
    ops.before_value(&catalog)?;
    catalog
        .validate_with_fallible_work_v1(&mut |s, b| ops.work(s as usize, b as usize))?
        .map_err(|error| deny(format!("capability catalog: {error}")))?;
    ops.before_value(&catalog.subject)?;
    ops.before_value(&catalog.audience)?;
    let invocation = CapabilityInvocationContext {
        grant_id: selected_grant_id,
        subject: catalog.subject.clone(),
        presenter,
        audience: catalog.audience.clone(),
        catalog_snapshot_id: snapshot_id,
        module_id: selected_module_id,
        module_version: selected_module_version,
        response_nonce,
    };
    Ok((catalog, invocation))
}

pub(in crate::world) struct LegacyCapabilityCatalogSourcesV1<'a>(pub &'a World);
impl CapabilityCatalogSourcesV1 for LegacyCapabilityCatalogSourcesV1<'_> {
    type Error = WorldError;
    fn agent_live(&self, id: &str) -> Result<bool, WorldError> {
        Ok(self.0.state.agents.contains_key(id))
    }
    fn identity(&self, id: &str) -> Result<Option<CapabilityAgentIdentity>, WorldError> {
        Ok(self.0.current_agent_identity(id))
    }
    fn grants(&self) -> Result<&BTreeMap<String, Value>, Self::Error> {
        Ok(&self.0.capability_grants_v2)
    }
    fn registry(&self) -> Result<&ModuleRegistry, Self::Error> {
        Ok(&self.0.module_registry)
    }
    fn policies(&self) -> Result<&PolicySet, Self::Error> {
        Ok(&self.0.policies)
    }
    fn tick(&self) -> u64 {
        self.0.state.time
    }
    fn world_journal_head(&self)->Result<u64,Self::Error>{Ok(self.0.journal.events.last().map(|event|event.id).unwrap_or(0))}
    fn revocation_epoch(&self)->Result<u64,Self::Error>{Ok(self.0.capability_revocation_state.epoch)}
    fn verify_issuer(&self, g: &CapabilityGrantV2) -> Result<(), WorldError> {
        self.0.verify_issuer(g)
    }
    fn verify_live_revocation(&self, g: &CapabilityGrantV2) -> Result<(), WorldError> {
        self.0.verify_live_revocation(g)
    }
    fn verify_parent_chain(&self, g: &CapabilityGrantV2) -> Result<(), WorldError> {
        self.0.verify_parent_chain(g)
    }
    fn verify_audience(&self, g: &CapabilityGrantV2) -> Result<(), WorldError> {
        self.0.verify_capability_catalog_audience(g)
    }
}
pub(in crate::world) struct LegacyCapabilityCatalogOpsV1;
impl CapabilityCatalogOpsV1<WorldError> for LegacyCapabilityCatalogOpsV1 {
    fn work(&mut self, _: usize, _: usize) -> Result<(), WorldError> {
        Ok(())
    }
    fn before_value<T: serde::Serialize>(&mut self, _: &T) -> Result<(), WorldError> {
        Ok(())
    }
    fn before_slots<T>(&mut self, _: usize) -> Result<(), WorldError> {
        Ok(())
    }
    fn before_command_catalog(&mut self, _: &ModuleRegistry) -> Result<(), WorldError> {
        Ok(())
    }
}

impl oasis7_wasm_abi::CanonicalCborAllocator for LegacyCapabilityCatalogOpsV1 {
    type Charge = ();
    fn reserve(&self, _: usize) -> Result<(), oasis7_wasm_abi::CanonicalCborStreamError> {
        Ok(())
    }
}
impl oasis7_wasm_abi::CapabilityCanonicalWorkV1 for LegacyCapabilityCatalogOpsV1 {
    type Loan<'a> = std::cell::RefMut<'a, oasis7_wasm_abi::CanonicalCborWorkV1>;
    fn is_metered(&self) -> bool {
        false
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        unreachable!("legacy has no bounded loan")
    }
}
