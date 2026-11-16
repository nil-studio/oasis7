//! Complete original WorldState status over actual typed readonly rows. This
//! produces explanation DATA and cannot create any rule/execution authority.
use super::*;
use crate::{FactoryState, WorldState};
use oasis7_agent_api::world_action::*;
use serde::Serialize;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, mem::size_of, ops::ControlFlow};
type Error<S> = <S as IndustrialOwnerReadonlyFactsV1>::Error;
pub trait WorldRuleStatusReadonlyFactsV1:
    OrganizationActivityReadonlyFactsV1 + SubstitutionReadonlyFactsV1
{
    fn status_rules_present(&self) -> Result<bool, Error<Self>>;
    fn status_time(&self) -> Result<u64, Error<Self>>;
    fn status_order_count(&self) -> Result<usize, Error<Self>>;
    fn status_visit_orders(
        &self,
        visit: impl FnMut(&str, &AgentIndustrialOrderV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>>;
    fn status_visit_factories(
        &self,
        visit: impl FnMut(&str, &FactoryState) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>>;
    fn status_visit_roots(
        &self,
        visit: impl FnMut(&str, &RootOperationV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>>;
    fn status_visit_decisions(
        &self,
        visit: impl FnMut(&str, &SubstitutionDecisionV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>>;
    fn status_visit_settlements(
        &self,
        visit: impl FnMut(&str, &ConversionSettlementV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>>;
    fn status_visit_residuals(
        &self,
        visit: impl FnMut(&str, &SubstitutionResidualBalanceV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>>;
}
pub(super) fn before<S: WorldRuleStatusReadonlyFactsV1, T: Serialize + ?Sized>(
    s: &S,
    v: &T,
) -> Result<(), Error<S>> {
    if !s.substitution_metered() {
        return Ok(());
    }
    let n = match crate::util::measure_json_with_work_v1(v, &mut |steps, bytes| {
        s.owner_work(steps, bytes as usize)
    }) {
        Ok(n) => n,
        Err(crate::util::JsonHashWorkErrorV1::Work(e)) => return Err(e),
        Err(crate::util::JsonHashWorkErrorV1::Codec(_)) => {
            panic!("original typed status JSON serializable")
        }
    };
    let bytes = n
        .checked_mul(2048)
        .and_then(|n| n.checked_add(65536))
        .expect("status original graph layout");
    s.owner_work(1 + 32 * n as u64, n)?;
    s.owner_before_owned(bytes)
}
pub(super) fn json<S: WorldRuleStatusReadonlyFactsV1, T: Serialize + ?Sized>(
    s: &S,
    v: &T,
) -> Result<Value, Error<S>> {
    before(s, v)?;
    Ok(serde_json::to_value(v).expect("original status typed JSON"))
}
pub(super) fn field<S: WorldRuleStatusReadonlyFactsV1, T: Serialize + ?Sized>(
    s: &S,
    out: &mut Map<String, Value>,
    key: &str,
    v: &T,
) -> Result<(), Error<S>> {
    super::readonly_industrial_data::tree_slot::<_, Value>(s, key, out.len())?;
    let key = super::readonly_industrial_data::text(s, key)?;
    out.insert(key, json(s, v)?);
    Ok(())
}
pub(super) fn format<S: WorldRuleStatusReadonlyFactsV1>(
    s: &S,
    args: std::fmt::Arguments<'_>,
) -> Result<String, Error<S>> {
    super::format_readonly_projection_data_v1(args, &mut |steps, bytes, heap| {
        s.owner_work(steps, bytes)?;
        if heap > 0 {
            s.owner_before_owned(heap)?
        }
        Ok(())
    })
}
pub(super) fn map_insert<S: WorldRuleStatusReadonlyFactsV1, T>(
    s: &S,
    out: &mut BTreeMap<String, T>,
    key: &str,
    value: T,
) -> Result<(), Error<S>> {
    super::readonly_industrial_data::tree_slot::<_, T>(s, key, out.len())?;
    out.insert(super::readonly_industrial_data::text(s, key)?, value);
    Ok(())
}
pub fn world_rule_status_readonly_v1<S: WorldRuleStatusReadonlyFactsV1>(
    s: &S,
    reader: Option<&str>,
    binding: Option<&str>,
) -> Result<Option<Value>, Error<S>> {
    let now = s.status_time()?;
    let mut orders = BTreeMap::new();
    let mut count = 0usize;
    s.status_visit_orders(|id, order| {
        // Original take(128), before computing private ownership or blockers.
        if count == 128 {
            return Ok(ControlFlow::Break(()));
        }
        count += 1;
        s.owner_work(
            1,
            order.owner_id.len()
                + reader.map_or(0, str::len)
                + order.quote.world_binding.as_ref().map_or(0, String::len)
                + binding.map_or(0, str::len),
        )?;
        let owner = super::resolve_industrial_owner_readonly_v1(s, &order.owner_id)?;
        let readable = super::can_read_agent_industrial_order_from_owner_v1(
            order,
            reader,
            binding,
            owner.as_ref(),
        );
        let blocker = if readable {
            super::industrial_order_blocker_readonly_v1(s, order, now)?
        } else {
            None
        };
        let next = if order.refund_pending {
            if blocker.as_deref() == Some("refund_pending_authoritative_release") {
                "await_authoritative_refund_release"
            } else {
                "restore_original_owner_source_for_frozen_refund"
            }
        } else if order.is_terminal() {
            "query_original_receipt"
        } else if order.phase == AgentIndustrialPhaseV1::AwaitingDelivery {
            "deliver_or_resolve_blocker_before_deadline"
        } else {
            "await_legal_production_or_cancel_before_start"
        };
        let mut row = Map::new();
        field(s, &mut row, "phase", &order.phase)?;
        field(s, &mut row, "refund_pending", &order.refund_pending)?;
        field(s, &mut row, "refunded_at", &order.refunded_at)?;
        field(
            s,
            &mut row,
            "completed_work_ticks",
            &order.completed_work_ticks,
        )?;
        field(s, &mut row, "ready_at", &order.ready_at)?;
        field(
            s,
            &mut row,
            "production_deadline",
            &order.production_deadline(),
        )?;
        field(
            s,
            &mut row,
            "delivery_deadline",
            &order.produced_at.map(|at| at.saturating_add(24)),
        )?;
        field(s, &mut row, "blocker", &blocker)?;
        field(s, &mut row, "next_step", &next)?;
        map_insert(s, &mut orders, id, Value::Object(row))?;
        Ok(if count == 128 {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        })
    })?;
    let mut quotes = BTreeMap::new();
    if let Some(actor) = reader {
        let mut taken = 0;
        s.status_visit_factories(|id, factory| {
            s.owner_work(1, factory.builder_agent_id.len() + actor.len())?;
            if factory.builder_agent_id != actor {
                return Ok(ControlFlow::Continue(()));
            }
            let quote = super::agent_manufacturing_quote_readonly_v1(s, actor, id, now, binding)?;
            let mut row = Map::new();
            match quote {
                Ok(q) => field(s, &mut row, "quote", &q)?,
                Err(e) => {
                    let blocker = format(s, format_args!("{e:?}"))?;
                    field(s, &mut row, "blocker", &blocker)?;
                    field(s, &mut row, "next_step", &"restore_quote_prerequisites")?
                }
            }
            map_insert(s, &mut quotes, id, Value::Object(row))?;
            taken += 1;
            Ok(if taken == 128 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            })
        })?;
    }
    let candidates = super::organization_activity_candidates_readonly_v1(s, reader, binding)?;
    if !s.status_rules_present()? && quotes.is_empty() {
        return Ok(None);
    }
    let substitution =
        super::readonly_status_substitution::substitution_status_readonly_v1(s, reader, binding)?;
    let total = s.status_order_count()?;
    let mut out = Map::new();
    field(s, &mut out, "source_tick", &now)?;
    field(s, &mut out, "recipe_substitution", &substitution)?;
    field(s, &mut out, "agent_orders", &orders)?;
    field(s, &mut out, "manufacturing_quotes", &quotes)?;
    field(s, &mut out, "agent_order_count", &total)?;
    field(s, &mut out, "agent_orders_truncated", &(total > 128))?;
    field(
        s,
        &mut out,
        "organization_activity_contract_candidates",
        &candidates,
    )?;
    field(
        s,
        &mut out,
        "supply_next_step",
        &"inspect_verified_source_and_adapter_availability",
    )?;
    field(
        s,
        &mut out,
        "organization_next_step",
        &"inspect_explicit_continuity_registration_and_case",
    )?;
    Ok(Some(Value::Object(out)))
}
impl WorldRuleStatusReadonlyFactsV1 for WorldState {
    fn status_rules_present(&self) -> Result<bool, Error<Self>> {
        Ok(self.world_rules.is_some())
    }
    fn status_time(&self) -> Result<u64, Error<Self>> {
        Ok(self.time)
    }
    fn status_order_count(&self) -> Result<usize, Error<Self>> {
        Ok(self
            .world_rules
            .as_ref()
            .map_or(0, |v| v.agent_orders.len()))
    }
    fn status_visit_orders(
        &self,
        mut v: impl FnMut(&str, &AgentIndustrialOrderV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>> {
        if let Some(r) = &self.world_rules {
            for (k, row) in &r.agent_orders {
                if v(k, row)?.is_break() {
                    break;
                }
            }
        }
        Ok(())
    }
    fn status_visit_factories(
        &self,
        mut v: impl FnMut(&str, &FactoryState) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>> {
        for (k, row) in &self.factories {
            if v(k, row)?.is_break() {
                break;
            }
        }
        Ok(())
    }
    fn status_visit_roots(
        &self,
        mut v: impl FnMut(&str, &RootOperationV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>> {
        if let Some(r) = &self.world_rules {
            for (k, row) in &r.industrial_work.roots {
                if v(k, row)?.is_break() {
                    break;
                }
            }
        }
        Ok(())
    }
    fn status_visit_decisions(
        &self,
        mut v: impl FnMut(&str, &SubstitutionDecisionV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>> {
        if let Some(r) = &self.world_rules {
            for (k, row) in &r.substitution.decisions {
                if v(k, row)?.is_break() {
                    break;
                }
            }
        }
        Ok(())
    }
    fn status_visit_settlements(
        &self,
        mut v: impl FnMut(&str, &ConversionSettlementV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>> {
        if let Some(r) = &self.world_rules {
            for (k, row) in &r.substitution.settlements {
                if v(k, row)?.is_break() {
                    break;
                }
            }
        }
        Ok(())
    }
    fn status_visit_residuals(
        &self,
        mut v: impl FnMut(&str, &SubstitutionResidualBalanceV1) -> Result<ControlFlow<()>, Error<Self>>,
    ) -> Result<(), Error<Self>> {
        if let Some(r) = &self.world_rules {
            for (k, row) in &r.substitution.retained_residuals {
                if v(k, row)?.is_break() {
                    break;
                }
            }
        }
        Ok(())
    }
}
