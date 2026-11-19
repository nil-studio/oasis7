//! Original organization activity candidate query over readonly source rows.
use super::readonly_owner::*;
use super::{
    ORGANIZATION_CONTINUITY_POLICY_VERSION_V1, OrganizationActivityContractCandidateV1,
    OrganizationContinuityCaseV1, OrganizationContinuityPhaseV1, OrganizationContinuityStateV1,
    OrganizationControllerBindingV1, OrganizationRegistrationV1, controller_binding_from_owner_v1,
    controller_binding_matches_owner_v1, economic_contract_terms_hash_with_work_v1,
};
use crate::{
    AgentClaimState, AllianceState, EconomicContractFulfillmentKind, EconomicContractState,
    EconomicContractStatus, StarterOcClaimState, WorldState,
};
use std::{convert::Infallible, mem::size_of, ops::ControlFlow};
pub trait OrganizationActivityReadonlyFactsV1: IndustrialOwnerReadonlyFactsV1 {
    fn organization_time(&self) -> Result<u64, Self::Error>;
    fn organization_visit_registrations(
        &self,
        visit: impl FnMut(&OrganizationRegistrationV1) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error>;
    fn organization_visit_cases(
        &self,
        visit: impl FnMut(&OrganizationContinuityCaseV1) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error>;
    fn organization_with_alliance<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&AllianceState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
    fn organization_visit_contracts(
        &self,
        visit: impl FnMut(&EconomicContractState) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error>;
}
fn own<S: IndustrialOwnerReadonlyFactsV1>(s: &S, text: &str) -> Result<String, S::Error> {
    s.owner_work(1, text.len())?;
    s.owner_before_owned(text.len() + size_of::<String>())?;
    Ok(text.to_owned())
}
fn binding<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    agent: &str,
) -> Result<Result<OrganizationControllerBindingV1, crate::WorldError>, S::Error> {
    let owner = resolve_industrial_owner_readonly_v1(s, agent)?;
    s.owner_work(1, 0)?;
    Ok(controller_binding_from_owner_v1(owner))
}
fn push<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    rows: &mut Vec<OrganizationActivityContractCandidateV1>,
    row: OrganizationActivityContractCandidateV1,
) -> Result<(), S::Error> {
    if rows.len() == rows.capacity() {
        let capacity = rows.capacity().saturating_mul(2).max(4);
        s.owner_before_owned(
            capacity.saturating_mul(size_of::<OrganizationActivityContractCandidateV1>()),
        )?;
        s.owner_work(
            1,
            rows.len()
                .saturating_mul(size_of::<OrganizationActivityContractCandidateV1>()),
        )?;
        let mut next = Vec::with_capacity(capacity);
        assert_eq!(
            next.capacity(),
            capacity,
            "organization candidate exact capacity"
        );
        next.append(rows);
        *rows = next;
    }
    s.owner_work(1, size_of::<OrganizationActivityContractCandidateV1>())?;
    rows.push(row);
    Ok(())
}
fn sort<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    rows: &mut [OrganizationActivityContractCandidateV1],
) -> Result<(), S::Error> {
    let n = rows.len();
    if n < 2 {
        return Ok(());
    }
    s.owner_before_owned(n.saturating_mul(3 * size_of::<usize>()))?;
    let mut order = Vec::with_capacity(n);
    let mut scratch = Vec::with_capacity(n);
    let mut inverse = Vec::with_capacity(n);
    for i in 0..n {
        s.owner_work(1, 3 * size_of::<usize>())?;
        order.push(i);
        scratch.push(0);
        inverse.push(0);
    }
    let mut width = 1usize;
    while width < n {
        s.owner_work(1, 0)?;
        let mut base = 0usize;
        while base < n {
            s.owner_work(1, 0)?;
            let middle = base.saturating_add(width).min(n);
            let end = middle.saturating_add(width).min(n);
            let (mut left, mut right, mut out) = (base, middle, base);
            while left < middle && right < end {
                let a = &rows[order[left]];
                let b = &rows[order[right]];
                s.owner_work(
                    1,
                    a.organization_id.len()
                        + b.organization_id.len()
                        + a.contract_id.len()
                        + b.contract_id.len()
                        + size_of::<usize>(),
                )?;
                // Equal keys take the left row: same stable order as sort_by.
                if (&a.organization_id, &a.contract_id) <= (&b.organization_id, &b.contract_id) {
                    scratch[out] = order[left];
                    left += 1;
                } else {
                    scratch[out] = order[right];
                    right += 1;
                }
                out += 1;
            }
            while left < middle {
                s.owner_work(1, size_of::<usize>())?;
                scratch[out] = order[left];
                left += 1;
                out += 1;
            }
            while right < end {
                s.owner_work(1, size_of::<usize>())?;
                scratch[out] = order[right];
                right += 1;
                out += 1;
            }
            base = end;
        }
        std::mem::swap(&mut order, &mut scratch);
        width = width.saturating_mul(2);
    }
    for (position, old) in order.into_iter().enumerate() {
        s.owner_work(1, size_of::<usize>())?;
        inverse[old] = position;
    }
    for i in 0..n {
        while inverse[i] != i {
            s.owner_work(
                1,
                3 * size_of::<OrganizationActivityContractCandidateV1>() + 2 * size_of::<usize>(),
            )?;
            let j = inverse[i];
            rows.swap(i, j);
            inverse.swap(i, j);
        }
    }
    Ok(())
}
pub fn organization_activity_candidates_readonly_v1<S: OrganizationActivityReadonlyFactsV1>(
    s: &S,
    reader: Option<&str>,
    world_binding: Option<&str>,
) -> Result<Vec<OrganizationActivityContractCandidateV1>, S::Error> {
    let Some(reader) = reader else {
        return Ok(Vec::new());
    };
    let Some(world_binding) = world_binding else {
        return Ok(Vec::new());
    };
    let mut candidates = Vec::new();
    s.organization_visit_registrations(|registration| {
        s.owner_work(
            1,
            registration.founder_id.len()
                + reader.len()
                + registration.world_binding.len()
                + world_binding.len(),
        )?;
        if registration.founder_id != reader
            || registration.policy_version != ORGANIZATION_CONTINUITY_POLICY_VERSION_V1
            || registration.world_binding != world_binding
        {
            return Ok(ControlFlow::Continue(()));
        }
        let levels = (registration.controller_bindings.len() + 1).ilog2() as usize + 2;
        s.owner_work(1, (reader.len() + 1).saturating_mul(levels))?;
        let stored = registration.controller_bindings.get(reader);
        let owner = resolve_industrial_owner_readonly_v1(s, reader)?;
        if let (Some(stored), Some(owner)) = (stored, owner.as_ref()) {
            s.owner_work(
                1,
                stored.root_agent_id.len()
                    + owner.primary_agent_id.len()
                    + stored.player_id.len()
                    + owner.player_id.len()
                    + stored.source_id.len()
                    + owner.source_id.len(),
            )?;
        }
        if !controller_binding_matches_owner_v1(stored, owner.as_ref()) {
            return Ok(ControlFlow::Continue(()));
        }
        let mut open = false;
        s.organization_visit_cases(|case| {
            s.owner_work(
                1,
                case.organization_id.len() + registration.organization_id.len(),
            )?;
            if case.organization_id == registration.organization_id
                && !matches!(
                    case.phase,
                    OrganizationContinuityPhaseV1::Resolved
                        | OrganizationContinuityPhaseV1::Cancelled
                )
            {
                open = true;
                return Ok(ControlFlow::Break(()));
            }
            Ok(ControlFlow::Continue(()))
        })?;
        if open {
            return Ok(ControlFlow::Continue(()));
        }
        s.organization_with_alliance(&registration.organization_id, |alliance| {
            let Some(alliance) = alliance else {
                return Ok(());
            };
            let Some(founder) = registration.controller_bindings.get(reader) else {
                return Ok(());
            };
            s.owner_before_owned(
                alliance
                    .members
                    .len()
                    .saturating_mul(size_of::<OrganizationControllerBindingV1>()),
            )?;
            let mut members = Vec::with_capacity(alliance.members.len());
            assert_eq!(
                members.capacity(),
                alliance.members.len(),
                "organization members exact capacity"
            );
            for member in &alliance.members {
                s.owner_work(1, member.len())?;
                let Ok(value) = binding(s, member)? else {
                    return Ok(());
                };
                members.push(value);
            }
            s.organization_visit_contracts(|contract| {
                s.owner_work(
                    1,
                    contract.creator_agent_id.len()
                        + reader.len()
                        + contract.counterparty_agent_id.len(),
                )?;
                if contract.creator_agent_id != reader
                    || contract.counterparty_agent_id == reader
                    || contract.fulfillment_kind != EconomicContractFulfillmentKind::AtomicExchange
                    || contract.status != EconomicContractStatus::Accepted
                    || contract.expires_at < s.organization_time()?
                    || contract.settlement_amount <= 0
                    || contract.reputation_stake < 0
                {
                    return Ok(ControlFlow::Continue(()));
                }
                s.owner_work(
                    1,
                    (contract.contract_id.len() + 1).saturating_mul(
                        (registration.activity_sources.len() + 1).ilog2() as usize + 2,
                    ),
                )?;
                if registration
                    .activity_sources
                    .contains_key(&contract.contract_id)
                {
                    return Ok(ControlFlow::Continue(()));
                }
                let mut registered = false;
                s.organization_visit_registrations(|other| {
                    s.owner_work(
                        1,
                        other.organization_id.len()
                            + registration.organization_id.len()
                            + (contract.contract_id.len() + 1).saturating_mul(
                                (other.activity_sources.len() + 1).ilog2() as usize + 2,
                            ),
                    )?;
                    if other.organization_id != registration.organization_id
                        && other.activity_sources.contains_key(&contract.contract_id)
                    {
                        registered = true;
                        return Ok(ControlFlow::Break(()));
                    }
                    Ok(ControlFlow::Continue(()))
                })?;
                if registered {
                    return Ok(ControlFlow::Continue(()));
                }
                let Ok(counterparty) = binding(s, &contract.counterparty_agent_id)? else {
                    return Ok(ControlFlow::Continue(()));
                };
                let mut overlap = false;
                for member in &members {
                    s.owner_work(
                        1,
                        member.player_id.len()
                            + counterparty.player_id.len()
                            + member.root_agent_id.len()
                            + counterparty.root_agent_id.len(),
                    )?;
                    if member.player_id == counterparty.player_id
                        || member.root_agent_id == counterparty.root_agent_id
                    {
                        overlap = true;
                        break;
                    }
                }
                if !overlap {
                    s.owner_work(
                        1,
                        founder.player_id.len()
                            + counterparty.player_id.len()
                            + founder.root_agent_id.len()
                            + counterparty.root_agent_id.len(),
                    )?;
                    overlap = founder.player_id == counterparty.player_id
                        || founder.root_agent_id == counterparty.root_agent_id;
                }
                if overlap {
                    return Ok(ControlFlow::Continue(()));
                }
                s.owner_before_owned(64 + size_of::<String>())?;
                let expected_terms_hash = match economic_contract_terms_hash_with_work_v1(
                    contract,
                    &mut |steps, bytes| {
                        s.owner_work(steps, usize::try_from(bytes).unwrap_or(usize::MAX))
                    },
                ) {
                    Ok(value) => value,
                    Err(crate::util::JsonHashWorkErrorV1::Codec(_)) => {
                        return Ok(ControlFlow::Continue(()));
                    }
                    Err(crate::util::JsonHashWorkErrorV1::Work(error)) => return Err(error),
                };
                let row = OrganizationActivityContractCandidateV1 {
                    organization_id: own(s, &registration.organization_id)?,
                    contract_id: own(s, &contract.contract_id)?,
                    expected_terms_hash,
                    expires_at: contract.expires_at,
                };
                push(s, &mut candidates, row)?;
                Ok(ControlFlow::Continue(()))
            })
        })?;
        Ok(ControlFlow::Continue(()))
    })?;
    sort(s, &mut candidates)?;
    Ok(candidates)
}
struct Legacy<'a> {
    state: &'a WorldState,
    continuity: &'a OrganizationContinuityStateV1,
}
impl IndustrialOwnerReadonlyFactsV1 for Legacy<'_> {
    type Error = Infallible;
    fn owner_work(&self, _: u64, _: usize) -> Result<(), Infallible> {
        Ok(())
    }
    fn owner_before_owned(&self, _: usize) -> Result<(), Infallible> {
        Ok(())
    }
    fn owner_agent_present(&self, a: &str) -> Result<bool, Infallible> {
        self.state.owner_agent_present(a)
    }
    fn owner_visit_orders(
        &self,
        v: impl FnMut(&super::AgentIndustrialOrderV1) -> Result<ControlFlow<()>, Infallible>,
    ) -> Result<(), Infallible> {
        self.state.owner_visit_orders(v)
    }
    fn owner_with_claim<T>(
        &self,
        a: &str,
        v: impl FnOnce(Option<&AgentClaimState>) -> Result<T, Infallible>,
    ) -> Result<T, Infallible> {
        self.state.owner_with_claim(a, v)
    }
    fn owner_with_starter<T>(
        &self,
        a: &str,
        v: impl FnOnce(Option<&StarterOcClaimState>) -> Result<T, Infallible>,
    ) -> Result<T, Infallible> {
        self.state.owner_with_starter(a, v)
    }
    fn owner_acquisition_sequence(&self, a: &str) -> Result<Option<u64>, Infallible> {
        self.state.owner_acquisition_sequence(a)
    }
}
impl OrganizationActivityReadonlyFactsV1 for Legacy<'_> {
    fn organization_time(&self) -> Result<u64, Infallible> {
        Ok(self.state.time)
    }
    fn organization_visit_registrations(
        &self,
        mut v: impl FnMut(&OrganizationRegistrationV1) -> Result<ControlFlow<()>, Infallible>,
    ) -> Result<(), Infallible> {
        for value in self.continuity.registrations.values() {
            if v(value)?.is_break() {
                break;
            }
        }
        Ok(())
    }
    fn organization_visit_cases(
        &self,
        mut v: impl FnMut(&OrganizationContinuityCaseV1) -> Result<ControlFlow<()>, Infallible>,
    ) -> Result<(), Infallible> {
        for value in self.continuity.cases.values() {
            if v(value)?.is_break() {
                break;
            }
        }
        Ok(())
    }
    fn organization_with_alliance<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&AllianceState>) -> Result<T, Infallible>,
    ) -> Result<T, Infallible> {
        v(self.state.alliances.get(id))
    }
    fn organization_visit_contracts(
        &self,
        mut v: impl FnMut(&EconomicContractState) -> Result<ControlFlow<()>, Infallible>,
    ) -> Result<(), Infallible> {
        for value in self.state.economic_contracts.values() {
            if v(value)?.is_break() {
                break;
            }
        }
        Ok(())
    }
}
pub(super) fn legacy_candidates(
    state: &WorldState,
    continuity: &OrganizationContinuityStateV1,
    reader: Option<&str>,
    binding: Option<&str>,
) -> Vec<OrganizationActivityContractCandidateV1> {
    match organization_activity_candidates_readonly_v1(
        &Legacy { state, continuity },
        reader,
        binding,
    ) {
        Ok(value) => value,
        Err(never) => match never {},
    }
}
