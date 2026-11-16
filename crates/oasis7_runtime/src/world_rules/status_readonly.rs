//! Unique Runtime status additions over immutable raw source records. The
//! World and fixed Current adapter share every visibility/blocker predicate.
use super::action_results::readonly::{SourcesV1 as ResultsSources, recent_from_sources_v1};
use super::*;
use oasis7_world_state::world_rules::{
    IndustrialOwnerReadonlyFactsV1, ManufacturingQuoteReadonlyFactsV1,
    WorldRuleStatusReadonlyFactsV1,
};
use serde_json::{Map, Value};
use std::ops::ControlFlow;
type Error<S> = <S as ResultsSources>::Error;
pub(crate) trait SourcesV1: ResultsSources + Sized {
    type State: WorldRuleStatusReadonlyFactsV1 + IndustrialOwnerReadonlyFactsV1<Error = Error<Self>>;
    fn state_facts(&self) -> &Self::State;
    fn current_owner(&self, id: &str) -> Result<Option<crate::ResolvedAgentOwnerV1>, Error<Self>>;
    fn claim<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_world_state::AgentClaimState>) -> Result<T, Error<Self>>,
    ) -> Result<T, Error<Self>>;
}
fn field<S: SourcesV1, T: serde::Serialize + ?Sized>(
    s: &S,
    out: &mut Value,
    key: &str,
    value: &T,
) -> Result<(), Error<S>> {
    s.before_value(value)?;
    s.ownership(
        11 * (std::mem::size_of::<String>() + std::mem::size_of::<Value>()) + key.len() + 256,
    )?;
    out[key] = serde_json::to_value(value).expect("original Runtime status JSON");
    Ok(())
}
pub(crate) fn identity_conflicts_v1<S: ResultsSources>(
    s: &S,
    actor: &str,
    player: &str,
) -> Result<bool, Error<S>> {
    let identity = s.identity(actor)?;
    if let Some(identity) = identity {
        s.work(1, identity.owner_binding.len() + player.len())?;
        Ok(identity.owner_binding != player)
    } else {
        Ok(false)
    }
}
pub(crate) fn can_read_order_v1<S: SourcesV1>(
    s: &S,
    order: &AgentIndustrialOrderV1,
    reader: Option<&str>,
) -> Result<bool, Error<S>> {
    let binding = s.binding()?;
    if identity_conflicts_v1(s, &order.owner_id, &order.quote.owner_player_id)? {
        return Ok(false);
    }
    let owner = oasis7_world_state::world_rules::resolve_industrial_owner_readonly_v1(
        s.state_facts(),
        &order.owner_id,
    )?;
    s.before_value(&(
        order,
        reader,
        &binding,
        owner.as_ref().map(|v| {
            (
                &v.primary_agent_id,
                &v.player_id,
                &v.public_key,
                &v.source_id,
                v.generation,
            )
        }),
    ))?;
    Ok(
        oasis7_world_state::world_rules::can_read_agent_industrial_order_from_owner_v1(
            order,
            reader,
            binding.as_deref(),
            owner.as_ref(),
        ),
    )
}
pub(crate) fn blocker_v1<S: SourcesV1>(
    s: &S,
    order: &AgentIndustrialOrderV1,
    now: u64,
) -> Result<Option<String>, Error<S>> {
    let state = s.state_facts();
    let mut charge = |steps, bytes, heap| {
        state.owner_work(steps, bytes)?;
        if heap > 0 {
            state.owner_before_owned(heap)?
        }
        Ok(())
    };
    let validation = order.validate_with_work_v1(&mut charge)?;
    if let Err(error) = validation {
        return Ok(Some(
            oasis7_world_state::world_rules::format_readonly_projection_data_v1(
                format_args!("{error:?}"),
                &mut charge,
            )?,
        ));
    }
    let binding = s.binding()?;
    s.before_value(&(&order.quote.world_binding, &binding))?;
    if order.quote.world_binding != binding {
        return Ok(Some(copy(s, "manufacturing_world_binding_mismatch")?));
    }
    if identity_conflicts_v1(s, &order.owner_id, &order.quote.owner_player_id)? {
        return Ok(Some(copy(s, "manufacturing_owner_identity_changed")?));
    }
    if order.phase == AgentIndustrialPhaseV1::Producing {
        if order
            .production_deadline()
            .is_some_and(|deadline| now >= deadline)
        {
            return Ok(Some(copy(s, "manufacturing_production_expired")?));
        }
        return match oasis7_world_state::world_rules::order_production_eligibility_readonly_v1(
            state, order,
        )? {
            Ok(()) => Ok(None),
            Err(error) => Ok(Some(
                oasis7_world_state::world_rules::format_readonly_projection_data_v1(
                    format_args!("{error:?}"),
                    &mut charge,
                )?,
            )),
        };
    }
    oasis7_world_state::world_rules::industrial_order_blocker_readonly_v1(state, order, now)
}
fn copy<S: ResultsSources>(s: &S, text: &str) -> Result<String, Error<S>> {
    s.work(1, text.len())?;
    s.ownership(std::mem::size_of::<String>() + text.len())?;
    Ok(text.to_owned())
}
pub(crate) fn status_from_sources_v1<S: SourcesV1>(
    s: &S,
    reader: Option<&str>,
) -> Result<Option<Value>, Error<S>> {
    let binding = s.binding()?;
    let recent = recent_from_sources_v1(s, reader)?;
    let mut status = match oasis7_world_state::world_rules::world_rule_status_readonly_v1(
        s.state_facts(),
        reader,
        binding.as_deref(),
    )? {
        Some(value) => value,
        None => {
            let identity = match reader {
                Some(reader) => s.identity(reader)?,
                None => None,
            };
            if identity.is_none() {
                return Ok(None);
            }
            s.ownership(std::mem::size_of::<Value>())?;
            Value::Object(Map::new())
        }
    };
    if let Some(actor) = reader {
        if let Some(owner) = oasis7_world_state::world_rules::resolve_industrial_owner_readonly_v1(
            s.state_facts(),
            actor,
        )? {
            if identity_conflicts_v1(s, actor, &owner.player_id)? {
                field(
                    s,
                    &mut status,
                    "manufacturing_quotes",
                    &serde_json::json!({}),
                )?;
            }
        }
    }
    let mut visited = 0usize;
    s.state_facts().status_visit_orders(|id, order| {
        if visited == 128 {
            return Ok(ControlFlow::Break(()));
        }
        visited += 1;
        // Exactly the original get_mut skip: only the core's first 128 rows exist.
        let Some(value) = status["agent_orders"].get_mut(id) else {
            return Ok(ControlFlow::Continue(()));
        };
        let readable = can_read_order_v1(s, order, reader)?;
        field(s, value, "current_owner", &Value::Null)?;
        if readable {
            let mut control = Value::Object(Map::new());
            field(s, &mut control, "status", &"blocked")?;
            field(s, &mut control, "reason", &"current_authority_unavailable")?;
            field(s, value, "current_control", &control)?;
        } else {
            field(s, value, "current_control", &Value::Null)?;
        }
        if readable {
            // The original tuple eagerly reads all three actual owner/identity facts.
            let buyer = s.current_owner(&order.owner_id)?;
            let owner = s.current_owner(&order.target_agent_id)?;
            let identity = s.identity(&order.target_agent_id)?;
            if let (Some(buyer), Some(owner), Some(identity)) = (buyer, owner, identity) {
                s.before_value(&(
                    (
                        &buyer.primary_agent_id,
                        &buyer.player_id,
                        &buyer.public_key,
                        &buyer.source_id,
                        buyer.generation,
                    ),
                    (
                        &owner.primary_agent_id,
                        &owner.player_id,
                        &owner.public_key,
                        &owner.source_id,
                        owner.generation,
                    ),
                    &identity,
                ))?;
                if owner.primary_agent_id == buyer.primary_agent_id
                    && owner.player_id == buyer.player_id
                    && identity.owner_binding == owner.player_id
                {
                    let mut current = Value::Object(Map::new());
                    field(s, &mut current, "primary_agent_id", &owner.primary_agent_id)?;
                    field(s, &mut current, "player_id", &owner.player_id)?;
                    field(s, value, "current_owner", &current)?;
                    let mut control = Value::Object(Map::new());
                    field(s, &mut control, "status", &"active")?;
                    field(s, &mut control, "target_agent_id", &order.target_agent_id)?;
                    field(s, &mut control, "source_id", &owner.source_id)?;
                    s.claim(&order.target_agent_id, |claim| {
                        field(
                            s,
                            &mut control,
                            "source_generation",
                            &claim
                                .and_then(|claim| claim.control_source.as_ref())
                                .map(|source| source.acquisition_sequence),
                        )
                    })?;
                    s.claim(&order.target_agent_id, |claim| {
                        field(
                            s,
                            &mut control,
                            "claim_owner_id",
                            &claim.map(|claim| &claim.claim_owner_id),
                        )
                    })?;
                    field(s, &mut control, "generation", &owner.generation)?;
                    field(s, &mut control, "identity_generation", &identity.generation)?;
                    field(s, value, "current_control", &control)?;
                }
            }
        }
        let blocker = if readable {
            blocker_v1(s, order, s.state_facts().status_time()?)?
        } else {
            None
        };
        field(s, value, "blocker", &blocker)?;
        if order.refund_pending {
            field(
                s,
                value,
                "next_step",
                &if blocker.as_deref() == Some("refund_pending_authoritative_release") {
                    "await_authoritative_refund_release"
                } else {
                    "restore_original_owner_source_for_frozen_refund"
                },
            )?;
        }
        Ok(if visited == 128 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        })
    })?;
    field(s, &mut status, "recent_action_results", &recent)?;
    let manifest = s.manifest()?;
    s.work(1, 256)?;
    s.ownership(4096)?;
    let history = if manifest.genesis_ref.is_none() {
        serde_json::json!({"status":"unavailable","reason":"protocol_genesis_binding_unavailable","missing_result":"unknown"})
    } else {
        serde_json::json!({"status":"bounded","event_window":2048,"result_limit":16,"missing_result":"unknown"})
    };
    field(s, &mut status, "recent_action_results_history", &history)?;
    Ok(Some(status))
}

pub(crate) struct Legacy<'a>(pub(crate) &'a crate::World);
impl super::action_results::readonly::sealed::Sources for Legacy<'_> {}
impl super::action_results::readonly::SourcesV1 for Legacy<'_> {
    type Error = std::convert::Infallible;
    type Event<'a>
        = &'a crate::WorldEvent
    where
        Self: 'a;
    fn identity(&self, id: &str) -> Result<Option<crate::CapabilityAgentIdentity>, Self::Error> {
        Ok(self.0.current_agent_identity(id))
    }
    fn public_key(&self, id: &str) -> Result<Option<String>, Self::Error> {
        Ok(crate::service_admission::agent_authority::owner_public_key(self.0, id).ok())
    }
    fn manifest(&self) -> Result<super::action_results::readonly::ManifestDataV1, Self::Error> {
        let manifest = self.0.chain_resource_manifest();
        Ok(super::action_results::readonly::ManifestDataV1 {
            world_id: manifest.world_id.clone(),
            genesis_ref: manifest.genesis_ref.clone(),
        })
    }
    fn binding(&self) -> Result<Option<String>, Self::Error> {
        Ok(self.0.world_rule_binding())
    }
    fn events(&self) -> Result<Vec<Self::Event<'_>>, Self::Error> {
        Ok(self.0.journal().events.iter().rev().take(2048).collect())
    }
    fn visit_commits(
        &self,
        mut visit: impl FnMut(&Value) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        if let Some(rows) = self.0.cognition()["commit_records"].as_array() {
            for row in rows.iter().rev().take(2048) {
                if visit(row)?.is_break() {
                    break;
                }
            }
        }
        Ok(())
    }
    fn visit_receipts(
        &self,
        mut visit: impl FnMut(&Value) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        if let Some(rows) = self.0.cognition()["receipt_lineage_registry"].as_array() {
            for row in rows.iter().rev().take(2048) {
                if visit(row)?.is_break() {
                    break;
                }
            }
        }
        Ok(())
    }
    fn with_action_digest<T>(
        &self,
        id: &str,
        visit: impl FnOnce(&Value) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        visit(&self.0.cognition()["action_digests"][id])
    }
    fn before_value<T: serde::Serialize + ?Sized>(&self, _: &T) -> Result<(), Self::Error> {
        Ok(())
    }
    fn work(&self, _: u64, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn ownership(&self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn check(&self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn stream(&self, _: oasis7_wasm_abi::CanonicalCborStreamError) -> Self::Error {
        unreachable!("Legacy observation has no bounded resource source")
    }
}
impl SourcesV1 for Legacy<'_> {
    type State = oasis7_world_state::WorldState;
    fn state_facts(&self) -> &Self::State {
        self.0.state()
    }
    fn current_owner(
        &self,
        id: &str,
    ) -> Result<Option<crate::ResolvedAgentOwnerV1>, std::convert::Infallible> {
        Ok(self.0.current_agent_owner(id))
    }
    fn claim<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_world_state::AgentClaimState>,
        ) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        visit(self.0.state().agent_claims.get(id))
    }
}
impl oasis7_wasm_abi::CanonicalCborAllocator for Legacy<'_> {
    type Charge = ();
    fn reserve(&self, _: usize) -> Result<(), oasis7_wasm_abi::CanonicalCborStreamError> {
        Ok(())
    }
}
impl oasis7_wasm_abi::CapabilityCanonicalWorkV1 for Legacy<'_> {
    type Loan<'a>
        = std::cell::RefMut<'a, oasis7_wasm_abi::CanonicalCborWorkV1>
    where
        Self: 'a;
    fn is_metered(&self) -> bool {
        false
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        unreachable!("Legacy observation has no canonical loan")
    }
}
