//! One readonly gameplay-capability predicate kernel. Sources provide original
//! typed rows in their original typed-key order; they cannot supply a verdict.
use super::*;
use std::{convert::Infallible, mem::size_of, ops::ControlFlow};
pub trait GameplayCapabilityReadonlyFactsV1 {
    type Error;
    fn work(&self, steps: u64, bytes: usize) -> Result<(), Self::Error>;
    /// Retain allocation admission until the produced DATA has been consumed.
    fn before_owned(&self, bytes: usize) -> Result<(), Self::Error>;
    fn time(&self) -> Result<u64, Self::Error>;
    fn visit_factories(
        &self,
        visit: impl FnMut(&str, &FactoryState) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
    fn visit_recipe_jobs(
        &self,
        visit: impl FnMut(u64, &RecipeJobState) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error>;
    fn with_factory<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&FactoryState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
    fn with_industry_progress<T>(
        &self,
        visit: impl FnOnce(&IndustryProgressState) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
    fn with_gameplay_policy<T>(
        &self,
        visit: impl FnOnce(&GameplayPolicyState) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
    fn with_site<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&FactorySiteAuthorityV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
    fn material_ledger_present(&self, id: &MaterialLedgerId) -> Result<bool, Self::Error>;
}
fn own<S: GameplayCapabilityReadonlyFactsV1>(s: &S, text: &str) -> Result<String, S::Error> {
    s.work(1, text.len())?;
    s.before_owned(text.len() + size_of::<String>())?;
    Ok(text.to_owned())
}
fn ledger_clone<S: GameplayCapabilityReadonlyFactsV1>(
    s: &S,
    id: &MaterialLedgerId,
) -> Result<MaterialLedgerId, S::Error> {
    Ok(match id {
        MaterialLedgerId::World => MaterialLedgerId::World,
        MaterialLedgerId::Agent(v) => MaterialLedgerId::Agent(own(s, v)?),
        MaterialLedgerId::Site(v) => MaterialLedgerId::Site(own(s, v)?),
        MaterialLedgerId::Factory(v) => MaterialLedgerId::Factory(own(s, v)?),
    })
}
pub fn current_candidate_matches_completed_readonly_v1<S: GameplayCapabilityReadonlyFactsV1>(
    s: &S,
    factory: &FactoryState,
) -> Result<bool, S::Error> {
    let Some(completed) = factory
        .production
        .last_completed_canonical_snapshot
        .as_ref()
    else {
        return Ok(false);
    };
    s.work(
        1,
        factory
            .production
            .current_recipe_id
            .as_ref()
            .map_or(0, String::len)
            + completed.recipe_id.len(),
    )?;
    if factory
        .production
        .current_recipe_id
        .as_ref()
        .is_some_and(|recipe| *recipe != completed.recipe_id)
    {
        return Ok(false);
    }
    let mut matches = true;
    s.visit_recipe_jobs(|_, job| {
        s.work(1, job.factory_id.len() + factory.factory_id.len())?;
        if job.factory_id == factory.factory_id {
            let snapshot =
                FactoryProductionSnapshot::from_recipe_job_with_work_v1(job, &mut |bytes| {
                    s.work(1, bytes)?;
                    s.before_owned(bytes)
                })?;
            // The original snapshot's full owned graph was admitted above.
            // Its structural equality visits at most that original graph.
            snapshot.visit_readonly_comparison_bytes_v1(&mut |bytes| s.work(1, bytes))?;
            if snapshot != *completed {
                matches = false;
                return Ok(ControlFlow::Break(()));
            }
        }
        Ok(ControlFlow::Continue(()))
    })?;
    Ok(matches)
}
pub fn has_current_stable_line_readonly_v1<S: GameplayCapabilityReadonlyFactsV1>(
    s: &S,
    factory: &FactoryState,
) -> Result<bool, S::Error> {
    s.work(1, 0)?;
    if matches!(
        factory.production.status,
        FactoryProductionStatus::Blocked | FactoryProductionStatus::Paused
    ) || !factory_has_canonical_stable_line(factory)
    {
        return Ok(false);
    }
    current_candidate_matches_completed_readonly_v1(s, factory)
}
pub(super) fn primary_outer_key<S: GameplayCapabilityReadonlyFactsV1>(
    s: &S,
    agent: &str,
) -> Result<Option<String>, S::Error> {
    let mut best = None;
    s.visit_factories(|outer, f| {
        s.work(1, f.builder_agent_id.len() + agent.len())?;
        if f.builder_agent_id != agent {
            return Ok(());
        }
        let p = &f.production;
        let rank = (
            has_current_stable_line_readonly_v1(s, f)?,
            p.completed_jobs > 0 || p.last_completed_at.is_some(),
            p.status != FactoryProductionStatus::Blocked,
            p.status == FactoryProductionStatus::Running,
            f.factory_id == "factory.smelter.mk1",
            p.completed_jobs,
            p.last_completed_at.unwrap_or(0),
        );
        s.work(1, 0)?;
        // Iterator::max_by_key deliberately selects the last equal key.
        if best.as_ref().is_none_or(|(_, old)| rank >= *old) {
            best = Some((own(s, outer)?, rank));
        }
        Ok(())
    })?;
    Ok(best.map(|(outer, _)| outer))
}
pub fn personal_stage_readonly_v1<S: GameplayCapabilityReadonlyFactsV1>(
    s: &S,
    factory: Option<&FactoryState>,
    agent: Option<&str>,
) -> Result<IndustryStage, S::Error> {
    let Some(agent) = agent else {
        return Ok(IndustryStage::Bootstrap);
    };
    let mut jobs = 0_u64;
    s.visit_factories(|_, f| {
        s.work(1, f.builder_agent_id.len() + agent.len())?;
        if f.builder_agent_id == agent {
            jobs = jobs.saturating_add(f.production.completed_jobs);
        }
        Ok(())
    })?;
    let stable = match factory {
        Some(f) => has_current_stable_line_readonly_v1(s, f)?,
        None => false,
    };
    s.with_gameplay_policy(|policy| {
        s.work(1, 0)?;
        Ok(industry_stage_for_progress(
            stable,
            jobs,
            0,
            policy.electricity_tax_bps > 0 || policy.data_tax_bps > 0,
        ))
    })
}
pub fn settled_outcome_readonly_v1<S: GameplayCapabilityReadonlyFactsV1>(
    s: &S,
    owner: Option<&str>,
) -> Result<Option<FirstSettlementEvidence>, S::Error> {
    s.with_industry_progress(|progress|{
        let Some(m)=progress.starter_industrial_milestone.as_ref() else{return Ok(None)};
        let Some(settlement)=m.settlement_summary.as_ref() else{return Ok(None)};
        let Some(owner)=owner else{return Ok(None)};
        let Some(origin)=m.committed_recipe_origin.as_ref() else{return Ok(None)};
        s.with_factory(&m.factory_id,|factory|{let Some(factory)=factory else{return Ok(None)};
            s.with_site(&factory.site_id,|site|{let Some(site)=site else{return Ok(None)};
                s.work(1,m.profile_id.len()+m.factory_id.len()+m.recipe_id.len()+settlement.requester_agent_id.len()+factory.builder_agent_id.len()+site.owner_agent_id.len()+owner.len()+factory.site_id.len())?;
                if m.profile_id!=STARTER_INDUSTRIAL_PROFILE_ID||m.profile_revision!=STARTER_INDUSTRIAL_PROFILE_REVISION||m.factory_id!=STARTER_SMELTER_FACTORY_ID||m.recipe_id!=STARTER_SMELTER_RECIPE_ID||!origin.submission.matches_recipe(owner,&m.factory_id,&m.recipe_id)||settlement.requester_agent_id!=owner||factory.builder_agent_id!=owner||site.owner_agent_id!=owner||!site.active||factory.output_ledger!=m.output_ledger||!matches!(&m.output_ledger,MaterialLedgerId::Site(id) if id==&factory.site_id)||!s.material_ledger_present(&m.output_ledger)?||settlement.accepted_batches==0||settlement.power_required<0{return Ok(None);}
                let mut found=false;for stack in &settlement.produce {s.work(1,stack.kind.len())?;if stack.kind=="iron_ingot"&&stack.amount>0{found=true;break;}}
                if !found{return Ok(None);}
                s.before_owned(size_of::<FirstSettlementEvidence>()+size_of::<StarterIndustrialSettlementSummaryV1>())?;
                let requester_agent_id=own(s,&settlement.requester_agent_id)?;
                let clone_stacks=|stacks:&Vec<oasis7_wasm_abi::MaterialStack>|->Result<Vec<oasis7_wasm_abi::MaterialStack>,S::Error>{
                    s.before_owned(stacks.len()*size_of::<oasis7_wasm_abi::MaterialStack>())?;
                    let mut result=Vec::with_capacity(stacks.len());for stack in stacks{s.work(1,stack.kind.len())?;result.push(oasis7_wasm_abi::MaterialStack{kind:own(s,&stack.kind)?,amount:stack.amount});}Ok(result)
                };
                let consume=clone_stacks(&settlement.consume)?;let produce=clone_stacks(&settlement.produce)?;
                let cloned=StarterIndustrialSettlementSummaryV1{requester_agent_id,accepted_batches:settlement.accepted_batches,consume,power_required:settlement.power_required,produce};
                Ok(Some(FirstSettlementEvidence{settlement_job_id:m.settlement_job_id,settled_at:m.settled_at,output_ledger:ledger_clone(s,&m.output_ledger)?,settlement:cloned}))
            })
        })
    })
}
pub fn gameplay_capability_query_readonly_v1<S: GameplayCapabilityReadonlyFactsV1>(
    s: &S,
    subject: GameplaySubject,
    goal: &str,
) -> Result<GameplayCapabilityQuery, S::Error> {
    let owner = subject.owner();
    let outer = match owner {
        Some(owner) => primary_outer_key(s, owner)?,
        None => None,
    };
    let reason = if goal != CURRENT_INDUSTRIAL_CAPABILITY_GOAL {
        "unsupported_recovery_goal"
    } else {
        match subject {
            GameplaySubject::Personal { .. } => "unsupported_recovery_action",
            GameplaySubject::Spectator => "spectator_has_no_personal_capability",
            GameplaySubject::Unavailable => "personal_owner_unavailable",
        }
    };
    // Keep source_tick/world stage reads before personal-stage evaluation, as
    // in the original struct expression.
    let source_tick = s.time()?;
    let goal_id = own(s, goal)?;
    let world_stage = s.with_industry_progress(|p| Ok(p.stage))?;
    let make = |factory: Option<&FactoryState>| -> Result<_, S::Error> {
        let personal_stage = match owner {
            Some(_) => Some(personal_stage_readonly_v1(s, factory, owner)?),
            None => None,
        };
        let primary_factory_id = factory.map(|f| own(s, &f.factory_id)).transpose()?;
        let current_recipe_id = factory
            .and_then(|f| {
                f.production
                    .current_recipe_id
                    .as_ref()
                    .or(f.production.last_completed_recipe_id.as_ref())
            })
            .map(|id| own(s, id))
            .transpose()?;
        let current_stable_line = match owner {
            Some(_) => Some(match factory {
                Some(f) => has_current_stable_line_readonly_v1(s, f)?,
                None => false,
            }),
            None => None,
        };
        let first_settlement = settled_outcome_readonly_v1(s, owner)?;
        s.before_owned(3 * size_of::<RecoveryAssessment>())?;
        let mut recovery = Vec::with_capacity(3);
        for kind in ["repair", "rebuild", "pivot"] {
            s.work(1, 0)?;
            recovery.push(RecoveryAssessment {
                kind: own(s, kind)?,
                status: own(s, "unknown")?,
                reason_code: own(s, reason)?,
                action: None,
                evidence: None,
                recheck: None,
            });
        }
        Ok((
            personal_stage,
            primary_factory_id,
            current_recipe_id,
            current_stable_line,
            first_settlement,
            recovery,
        ))
    };
    let (
        personal_stage,
        primary_factory_id,
        current_recipe_id,
        current_stable_line,
        first_settlement,
        recovery,
    ) = match outer {
        Some(outer) => s.with_factory(&outer, make)?,
        None => make(None)?,
    };
    Ok(GameplayCapabilityQuery {
        subject,
        source_tick,
        goal_id,
        world_stage,
        personal_stage,
        primary_factory_id,
        current_recipe_id,
        current_stable_line,
        first_settlement,
        recovery,
    })
}
impl GameplayCapabilityReadonlyFactsV1 for GameplayCapabilityFactsV1<'_> {
    type Error = Infallible;
    fn work(&self, _: u64, _: usize) -> Result<(), Infallible> {
        Ok(())
    }
    fn before_owned(&self, _: usize) -> Result<(), Infallible> {
        Ok(())
    }
    fn time(&self) -> Result<u64, Infallible> {
        Ok(self.time)
    }
    fn visit_factories(
        &self,
        mut visit: impl FnMut(&str, &FactoryState) -> Result<(), Infallible>,
    ) -> Result<(), Infallible> {
        for (k, v) in self.factories {
            visit(k, v)?;
        }
        Ok(())
    }
    fn visit_recipe_jobs(
        &self,
        mut visit: impl FnMut(u64, &RecipeJobState) -> Result<ControlFlow<()>, Infallible>,
    ) -> Result<(), Infallible> {
        for (k, v) in self.pending_recipe_jobs {
            if visit(*k, v)?.is_break() {
                break;
            }
        }
        Ok(())
    }
    fn with_factory<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&FactoryState>) -> Result<T, Infallible>,
    ) -> Result<T, Infallible> {
        visit(self.factories.get(id))
    }
    fn with_industry_progress<T>(
        &self,
        visit: impl FnOnce(&IndustryProgressState) -> Result<T, Infallible>,
    ) -> Result<T, Infallible> {
        visit(self.industry_progress)
    }
    fn with_gameplay_policy<T>(
        &self,
        visit: impl FnOnce(&GameplayPolicyState) -> Result<T, Infallible>,
    ) -> Result<T, Infallible> {
        visit(self.gameplay_policy)
    }
    fn with_site<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&FactorySiteAuthorityV1>) -> Result<T, Infallible>,
    ) -> Result<T, Infallible> {
        visit(self.factory_site_authorities.get(id))
    }
    fn material_ledger_present(&self, id: &MaterialLedgerId) -> Result<bool, Infallible> {
        Ok(self.material_ledgers.contains_key(id))
    }
}
