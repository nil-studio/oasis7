use super::*;
use oasis7_agent_api::world_action::{OrganizationEstateRightV1, OrganizationEstateScopeV1};
use std::collections::BTreeSet;

mod organization_continuity;
#[cfg(test)]
pub(crate) use organization_continuity::economic_contract_terms_hash;
pub use organization_continuity::*;
pub(crate) use organization_continuity::{activity_catalog_root, activity_close_commitment};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationProtectionPhaseV1 {
    Active,
    Notice,
    Protected,
    Limited,
    Resolved,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationDispositionPhaseV1 {
    None,
    Frozen,
    Obligations,
    Business,
    Residual,
    Complete,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationEstateDepositV1 {
    pub id: String,
    pub organization_id: String,
    pub contributor_id: String,
    pub material: String,
    pub amount: i64,
    pub held_in_trust: bool,
    pub disposed: bool,
    pub effect_receipt: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationGrantV1 {
    pub id: String,
    pub delegate_id: String,
    pub scopes: BTreeSet<OrganizationEstateScopeV1>,
    pub revision: u64,
    pub expires_at: u64,
    pub source_proposal: String,
    pub replaces: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationRecoveryRequestV1 {
    pub requester_id: String,
    pub appeal: bool,
    pub approved: Option<bool>,
    pub resolved_by: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationCaseV1 {
    pub id: String,
    pub organization_id: String,
    pub founder_id: String,
    pub members: BTreeSet<String>,
    pub policy_version: u32,
    pub revision: u64,
    pub phase: OrganizationProtectionPhaseV1,
    pub disposition: OrganizationDispositionPhaseV1,
    pub notified_at: Option<u64>,
    pub protected_at: Option<u64>,
    pub grants: BTreeMap<String, OrganizationGrantV1>,
    pub requests: BTreeMap<String, OrganizationRecoveryRequestV1>,
    pub stage_receipts: Vec<String>,
}
fn current_case(rules: &WorldRulesStateV1, organization: &str) -> bool {
    rules.organization_cases.values().any(|c| {
        c.organization_id == organization
            && c.phase != OrganizationProtectionPhaseV1::Resolved
            && c.disposition != OrganizationDispositionPhaseV1::Complete
    })
}
fn last_active(state: &WorldState, members: &BTreeSet<String>) -> Result<u64, WorldError> {
    members
        .iter()
        .map(|id| {
            state
                .agents
                .get(id)
                .map(|a| a.last_active)
                .ok_or_else(|| invalid("member_activity_unknown"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|v| v.into_iter().max().unwrap_or(0))
}
fn checked_case(
    rules: &WorldRulesStateV1,
    id: &str,
    revision: u64,
) -> Result<OrganizationCaseV1, WorldError> {
    let case = rules
        .organization_cases
        .get(id)
        .ok_or_else(|| invalid("organization_case_missing"))?;
    if case.revision != revision
        || case.phase == OrganizationProtectionPhaseV1::Resolved
        || case.disposition == OrganizationDispositionPhaseV1::Complete
    {
        return Err(invalid("case_stale_or_terminal"));
    }
    Ok(case.clone())
}
impl PreparedWorldRule {
    pub(super) fn organization(
        &mut self,
        state: &WorldState,
        actor: &str,
        key: &str,
        command: &WorldRuleCommandV1,
        now: u64,
        world_binding: &str,
    ) -> Result<String, WorldError> {
        use OrganizationDispositionPhaseV1 as D;
        use OrganizationProtectionPhaseV1 as P;
        match command {
            WorldRuleCommandV1::DepositOrganizationEstate {
                organization_id,
                material,
                amount,
                held_in_trust,
            } => {
                let alliance = state
                    .alliances
                    .get(organization_id)
                    .ok_or_else(|| invalid("organization_missing"))?;
                if !self
                    .rules
                    .organization_continuity
                    .registrations
                    .contains_key(organization_id)
                {
                    return Err(invalid("organization_continuity_enable_required"));
                }
                if self.rules.organization_continuity.registrations[organization_id].world_binding
                    != world_binding
                {
                    return Err(invalid("organization_world_binding_or_policy_changed"));
                }
                if !alliance.members.iter().any(|id| id == actor)
                    || state.organization_risk_frozen(organization_id)
                    || self.rules.organization_cases.values().any(|c| {
                        c.organization_id == *organization_id
                            && (c.phase != P::Resolved || c.disposition == D::Complete)
                    })
                {
                    return Err(invalid("organization_frozen_or_not_member"));
                }
                if material.is_empty()
                    || material.len() > 128
                    || *amount <= 0
                    || *amount > 1_000_000_000
                {
                    return Err(invalid("estate_deposit_invalid"));
                }
                self.debit(
                    state,
                    &MaterialLedgerId::agent(actor),
                    &BTreeMap::from([(material.clone(), *amount)]),
                )?;
                self.rules.organization_estate.insert(
                    key.into(),
                    OrganizationEstateDepositV1 {
                        id: key.into(),
                        organization_id: organization_id.clone(),
                        contributor_id: actor.into(),
                        material: material.clone(),
                        amount: *amount,
                        held_in_trust: *held_in_trust,
                        disposed: false,
                        effect_receipt: None,
                    },
                );
                self.rules.organization_continuity.accept_material_deposit(
                    organization_id,
                    key,
                    key,
                    actor,
                    material,
                    *amount,
                    *held_in_trust,
                )?;
                Ok(key.into())
            }
            WorldRuleCommandV1::OpenOrganizationProtection { organization_id }
            | WorldRuleCommandV1::BeginOrganizationDissolution { organization_id } => {
                let alliance = state
                    .alliances
                    .get(organization_id)
                    .ok_or_else(|| invalid("organization_missing"))?;
                if current_case(&self.rules, organization_id) {
                    return Err(invalid("organization_case_already_open"));
                }
                let dissolution = matches!(
                    command,
                    WorldRuleCommandV1::BeginOrganizationDissolution { .. }
                );
                if dissolution
                    && (actor != alliance.formed_by_agent_id
                        || state.wars.values().any(|w| {
                            w.active
                                && (w.aggressor_alliance_id == *organization_id
                                    || w.defender_alliance_id == *organization_id)
                        }))
                {
                    return Err(invalid("founder_authority_or_war_blocked"));
                }
                let members = alliance.members.iter().cloned().collect::<BTreeSet<_>>();
                if members.is_empty()
                    || members.len() > 128
                    || members.iter().any(|id| !state.agents.contains_key(id))
                {
                    return Err(invalid("organization_members_unknown_or_over_limit"));
                }
                if !dissolution && now.saturating_sub(last_active(state, &members)?) < 600 {
                    return Err(invalid("organization_still_active"));
                }
                self.rules.organization_cases.insert(
                    key.into(),
                    OrganizationCaseV1 {
                        id: key.into(),
                        organization_id: organization_id.clone(),
                        founder_id: alliance.formed_by_agent_id.clone(),
                        members,
                        policy_version: 1,
                        revision: 1,
                        phase: if dissolution { P::Active } else { P::Notice },
                        disposition: if dissolution { D::Frozen } else { D::None },
                        notified_at: (!dissolution).then_some(now),
                        protected_at: None,
                        grants: BTreeMap::new(),
                        requests: BTreeMap::new(),
                        stage_receipts: vec![key.into()],
                    },
                );
                Ok(key.into())
            }
            WorldRuleCommandV1::AdvanceOrganization {
                case_id,
                expected_revision,
            } => {
                let mut case = checked_case(&self.rules, case_id, *expected_revision)?;
                if case.phase == P::Notice {
                    let activity = last_active(state, &case.members)?;
                    if activity > case.notified_at.unwrap_or(0) {
                        case.phase = P::Resolved;
                    } else if now >= case.notified_at.unwrap_or(now).saturating_add(120) {
                        case.phase = P::Protected;
                        case.protected_at = Some(now);
                    } else {
                        return Err(invalid("notification_period_pending"));
                    }
                } else if case.phase == P::Limited
                    && case
                        .grants
                        .values()
                        .all(|g| g.expires_at <= now || g.scopes.is_empty())
                {
                    case.phase = P::Protected;
                    case.protected_at = Some(now);
                } else {
                    if !matches!(case.phase, P::Active | P::Limited) {
                        return Err(invalid("organization_protection_blocks_disposition"));
                    }
                    let right = if case.disposition == D::Residual {
                        OrganizationEstateRightV1::FinalizeCase
                    } else {
                        OrganizationEstateRightV1::AdvanceCase
                    };
                    let scoped = OrganizationEstateScopeV1 {
                        object_id: case.id.clone(),
                        right,
                    };
                    let explicit = case
                        .grants
                        .values()
                        .find(|g| g.expires_at > now && g.scopes.contains(&scoped));
                    if explicit.map_or(actor != case.founder_id, |grant| grant.delegate_id != actor)
                        || case
                            .requests
                            .values()
                            .any(|r| r.appeal && r.approved.is_none())
                    {
                        return Err(invalid("case_authority_or_pending_appeal"));
                    }
                    case.disposition =
                        match case.disposition {
                            D::Frozen => D::Obligations,
                            D::Obligations => {
                                if self.rules.organization_estate.values().any(|d| {
                                    d.organization_id == case.organization_id
                                        && d.held_in_trust
                                        && !d.disposed
                                }) {
                                    return Err(invalid("trust_obligations_pending"));
                                }
                                D::Business
                            }
                            D::Business => D::Residual,
                            D::Residual => {
                                if state.wars.values().any(|w| {
                                    w.active
                                        && (w.aggressor_alliance_id == case.organization_id
                                            || w.defender_alliance_id == case.organization_id)
                                }) {
                                    return Err(invalid("active_war_blocks_finalization"));
                                }
                                if self.rules.organization_estate.values().any(|d| {
                                    d.organization_id == case.organization_id && !d.disposed
                                }) {
                                    return Err(invalid("residual_assets_pending"));
                                }
                                self.removed_alliance = Some(case.organization_id.clone());
                                case.phase = P::Resolved;
                                D::Complete
                            }
                            _ => return Err(invalid("case_has_no_advance")),
                        };
                }
                case.revision += 1;
                case.stage_receipts.push(key.into());
                self.rules.organization_cases.insert(case_id.clone(), case);
                Ok(case_id.clone())
            }
            WorldRuleCommandV1::GrantOrganizationScope {
                case_id,
                expected_revision,
                proposal_key,
                delegate_id,
                scopes,
                expires_at,
                replace_grant_id,
            } => {
                require_governance(state, actor, proposal_key, command)?;
                let mut case = checked_case(&self.rules, case_id, *expected_revision)?;
                if !state.agents.contains_key(delegate_id)
                    || state.gameplay_policy.blocked_agents.contains(delegate_id)
                    || scopes.is_empty()
                    || scopes.len() > 128
                    || *expires_at <= now
                    || *expires_at > now.saturating_add(600)
                    || case
                        .requests
                        .values()
                        .any(|r| r.appeal && r.approved.is_none())
                {
                    return Err(invalid("organization_grant_invalid"));
                }
                if !(matches!(case.phase, P::Active | P::Limited)
                    || case.phase == P::Protected
                        && now >= case.protected_at.unwrap_or(now).saturating_add(600))
                {
                    return Err(invalid("organization_protection_period_pending"));
                }
                for scope in scopes {
                    if matches!(
                        scope.right,
                        OrganizationEstateRightV1::FinalizeCase
                            | OrganizationEstateRightV1::AdvanceCase
                    ) {
                        if scope.object_id != case.id
                            || case.disposition == D::None
                            || scope.right == OrganizationEstateRightV1::FinalizeCase
                                && (case.disposition != D::Residual
                                    || self.rules.organization_estate.values().any(|d| {
                                        d.organization_id == case.organization_id && !d.disposed
                                    }))
                        {
                            return Err(invalid("finalization_scope_not_ready"));
                        }
                        continue;
                    }
                    let d = self
                        .rules
                        .organization_estate
                        .get(&scope.object_id)
                        .ok_or_else(|| invalid("estate_scope_unknown"))?;
                    if d.organization_id != case.organization_id || d.disposed {
                        return Err(invalid("estate_scope_inapplicable"));
                    }
                }
                if let Some(old_id) = replace_grant_id
                    && !case.grants.contains_key(old_id)
                {
                    return Err(invalid("old_grant_missing"));
                }
                for (id, grant) in &mut case.grants {
                    if grant.expires_at > now && !grant.scopes.is_disjoint(scopes) {
                        if replace_grant_id.as_ref() != Some(id) {
                            return Err(invalid("overlapping_authorization"));
                        }
                        grant.scopes = grant.scopes.difference(scopes).cloned().collect();
                        grant.revision += 1;
                    }
                }
                case.grants.insert(
                    key.into(),
                    OrganizationGrantV1 {
                        id: key.into(),
                        delegate_id: delegate_id.clone(),
                        scopes: scopes.clone(),
                        revision: 1,
                        expires_at: *expires_at,
                        source_proposal: proposal_key.clone(),
                        replaces: replace_grant_id.clone(),
                    },
                );
                case.phase = P::Limited;
                case.revision += 1;
                case.stage_receipts.push(key.into());
                self.rules.organization_cases.insert(case_id.clone(), case);
                Ok(key.into())
            }
            WorldRuleCommandV1::RevokeOrganizationScope {
                case_id,
                expected_revision,
                grant_id,
                proposal_key,
            } => {
                require_governance(state, actor, proposal_key, command)?;
                let mut case = checked_case(&self.rules, case_id, *expected_revision)?;
                let grant = case
                    .grants
                    .get_mut(grant_id)
                    .ok_or_else(|| invalid("grant_missing"))?;
                grant.scopes.clear();
                grant.revision += 1;
                if case
                    .grants
                    .values()
                    .all(|g| g.scopes.is_empty() || g.expires_at <= now)
                {
                    case.phase = P::Protected;
                    case.protected_at = Some(now);
                }
                case.revision += 1;
                case.stage_receipts.push(key.into());
                self.rules.organization_cases.insert(case_id.clone(), case);
                Ok(grant_id.clone())
            }
            WorldRuleCommandV1::RequestOrganizationRecovery {
                case_id,
                expected_revision,
                appeal,
            } => {
                let mut case = checked_case(&self.rules, case_id, *expected_revision)?;
                if !case.members.contains(actor)
                    || case.requests.values().any(|r| {
                        r.requester_id == actor && r.appeal == *appeal && r.approved.is_none()
                    })
                {
                    return Err(invalid("recovery_not_member_or_capacity"));
                }
                case.requests.insert(
                    key.into(),
                    OrganizationRecoveryRequestV1 {
                        requester_id: actor.into(),
                        appeal: *appeal,
                        approved: None,
                        resolved_by: None,
                    },
                );
                case.revision += 1;
                self.rules.organization_cases.insert(case_id.clone(), case);
                Ok(key.into())
            }
            WorldRuleCommandV1::ResolveOrganizationRecovery {
                case_id,
                expected_revision,
                recovery_request_id,
                approved,
                proposal_key,
            } => {
                require_governance(state, actor, proposal_key, command)?;
                let mut case = checked_case(&self.rules, case_id, *expected_revision)?;
                let request = case
                    .requests
                    .get_mut(recovery_request_id)
                    .ok_or_else(|| invalid("recovery_request_missing"))?;
                if request.approved.is_some() {
                    return Err(invalid("recovery_already_resolved"));
                }
                request.approved = Some(*approved);
                request.resolved_by = Some(key.into());
                if *approved {
                    case.phase = if request.appeal || case.disposition != D::None {
                        P::Protected
                    } else {
                        P::Resolved
                    };
                    case.protected_at = Some(now);
                    for grant in case.grants.values_mut() {
                        grant.scopes.clear();
                        grant.revision += 1;
                    }
                }
                case.revision += 1;
                case.stage_receipts.push(key.into());
                self.rules.organization_cases.insert(case_id.clone(), case);
                Ok(recovery_request_id.clone())
            }
            WorldRuleCommandV1::DisposeOrganizationEstate {
                case_id,
                expected_revision,
                grant_id,
                grant_revision,
                deposit_id,
            } => {
                let mut case = checked_case(&self.rules, case_id, *expected_revision)?;
                if case.phase != P::Limited
                    || case
                        .requests
                        .values()
                        .any(|r| r.appeal && r.approved.is_none())
                {
                    return Err(invalid("disposition_not_authorized_or_appeal_pending"));
                }
                let deposit = self
                    .rules
                    .organization_estate
                    .get(deposit_id)
                    .ok_or_else(|| invalid("estate_deposit_missing"))?
                    .clone();
                if deposit.organization_id != case.organization_id || deposit.disposed {
                    return Err(invalid("estate_already_disposed_or_scope_mismatch"));
                }
                let right = if deposit.held_in_trust
                    && matches!(case.disposition, D::Obligations | D::None)
                {
                    OrganizationEstateRightV1::ReturnTrust
                } else if !deposit.held_in_trust && case.disposition == D::Residual {
                    OrganizationEstateRightV1::DistributeResidual
                } else {
                    return Err(invalid("disposition_stage_mismatch"));
                };
                let grant = case
                    .grants
                    .get(grant_id)
                    .ok_or_else(|| invalid("grant_missing"))?;
                if grant.delegate_id != actor
                    || grant.revision != *grant_revision
                    || now >= grant.expires_at
                    || !grant.scopes.contains(&OrganizationEstateScopeV1 {
                        object_id: deposit_id.clone(),
                        right,
                    })
                {
                    return Err(invalid("grant_stale_or_scope_denied"));
                }
                if deposit.held_in_trust {
                    self.credit(
                        state,
                        &MaterialLedgerId::agent(&deposit.contributor_id),
                        &deposit.material,
                        deposit.amount,
                    )?;
                } else {
                    let count = case.members.len() as i64;
                    let each = deposit.amount / count;
                    let mut remainder = deposit.amount % count;
                    for member in &case.members {
                        let bonus = i64::from(remainder > 0);
                        remainder -= bonus;
                        self.credit(
                            state,
                            &MaterialLedgerId::agent(member),
                            &deposit.material,
                            each + bonus,
                        )?;
                    }
                }
                let d = self
                    .rules
                    .organization_estate
                    .get_mut(deposit_id)
                    .expect("validated");
                d.disposed = true;
                d.effect_receipt = Some(key.into());
                case.revision += 1;
                case.stage_receipts.push(key.into());
                self.rules.organization_cases.insert(case_id.clone(), case);
                Ok(deposit_id.clone())
            }
            _ => Err(invalid("unsupported_organization_command")),
        }
    }
}
