//! Event-bound sparse projection for restricted starter-claim monetary events.

use serde::ser::SerializeStruct;
use std::collections::BTreeMap;

use super::super::{
    AgentCell, DomainEvent, MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
    MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL, MainTokenAccountBalance,
    MainTokenSupplyState, MaterialLedgerId, RestrictedStarterClaimGrantState,
    RestrictedStarterClaimGrantStatus, RestrictedStarterClaimLiveopsPoolTopUpRecord, WorldError,
    WorldState,
};
use super::economy_data_publication::SparseMapProjection;

#[derive(Debug)]
pub(crate) struct PreparedMainTokenRestrictedClaimEvent {
    event: DomainEvent,
    treasury: BTreeMap<String, u64>,
    balances: BTreeMap<String, MainTokenAccountBalance>,
    supply: Option<MainTokenSupplyState>,
    grants: BTreeMap<String, RestrictedStarterClaimGrantState>,
    topups: BTreeMap<String, RestrictedStarterClaimLiveopsPoolTopUpRecord>,
    world_materials: BTreeMap<String, i64>,
}

impl PreparedMainTokenRestrictedClaimEvent {
    pub(crate) fn prepare(state: &WorldState, event: &DomainEvent) -> Result<Self, WorldError> {
        let mut out = Self {
            event: event.clone(),
            treasury: BTreeMap::new(),
            balances: BTreeMap::new(),
            supply: None,
            grants: BTreeMap::new(),
            topups: BTreeMap::new(),
            world_materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|ledger| !ledger.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        };
        match event {
            DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp {
                controller_account_id,
                top_up_id,
                source_treasury_bucket_id,
                target_treasury_bucket_id,
                amount,
                topped_up_at_epoch,
            } => out.prepare_topup(
                state,
                controller_account_id,
                top_up_id,
                source_treasury_bucket_id,
                target_treasury_bucket_id,
                *amount,
                *topped_up_at_epoch,
            )?,
            DomainEvent::RestrictedStarterClaimGrantIssued {
                issuer_id,
                beneficiary_account_id,
                source_treasury_bucket_id,
                amount,
                issuance_reason,
                spend_scope,
                issued_at_epoch,
                expires_at_epoch,
            } => out.prepare_issue(
                state,
                issuer_id,
                beneficiary_account_id,
                source_treasury_bucket_id,
                *amount,
                issuance_reason,
                spend_scope,
                *issued_at_epoch,
                *expires_at_epoch,
            )?,
            DomainEvent::RestrictedStarterClaimGrantExpired {
                beneficiary_account_id,
                issuer_id,
                issuance_reason,
                spend_scope,
                source_treasury_bucket_id,
                issued_amount,
                expired_amount,
                issued_at_epoch,
                expired_at_epoch,
                configured_expires_at_epoch,
            } => out.prepare_terminal(
                state,
                beneficiary_account_id,
                issuer_id,
                issuance_reason,
                spend_scope,
                source_treasury_bucket_id,
                *issued_amount,
                *expired_amount,
                *issued_at_epoch,
                *expired_at_epoch,
                *configured_expires_at_epoch,
                None,
            )?,
            DomainEvent::RestrictedStarterClaimGrantRevoked {
                beneficiary_account_id,
                issuer_id,
                issuance_reason,
                spend_scope,
                source_treasury_bucket_id,
                issued_amount,
                revoked_amount,
                issued_at_epoch,
                revoked_at_epoch,
                configured_expires_at_epoch,
                revoke_reason,
            } => out.prepare_terminal(
                state,
                beneficiary_account_id,
                issuer_id,
                issuance_reason,
                spend_scope,
                source_treasury_bucket_id,
                *issued_amount,
                *revoked_amount,
                *issued_at_epoch,
                *revoked_at_epoch,
                *configured_expires_at_epoch,
                Some(revoke_reason),
            )?,
            _ => {
                return Err(invalid(
                    "restricted-claim preparation requires a supported event",
                ));
            }
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_topup(
        &mut self,
        state: &WorldState,
        controller: &str,
        top_up_id: &str,
        source: &str,
        target: &str,
        amount: u64,
        epoch: u64,
    ) -> Result<(), WorldError> {
        let controller = controller.trim();
        if controller.is_empty() {
            return Err(invalid(
                "restricted claim liveops pool top-up controller_account_id cannot be empty",
            ));
        }
        let top_up_id = top_up_id.trim();
        if top_up_id.is_empty() {
            return Err(invalid(
                "restricted claim liveops pool top_up_id cannot be empty",
            ));
        }
        if state
            .restricted_starter_claim_liveops_pool_top_up_records
            .contains_key(top_up_id)
        {
            return Err(invalid(format!(
                "restricted claim liveops pool top_up_id already exists: {top_up_id}"
            )));
        }
        let source = source.trim();
        if source != MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL {
            return Err(invalid(format!(
                "restricted claim liveops pool top-up source bucket must be ecosystem_pool: {source}"
            )));
        }
        let target = target.trim();
        if target != MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL {
            return Err(invalid(format!(
                "restricted claim liveops pool top-up target bucket must be restricted starter claim liveops pool: {target}"
            )));
        }
        if amount == 0 {
            return Err(invalid(
                "restricted claim liveops pool top-up amount must be > 0",
            ));
        }
        let mut treasury = state.main_token_treasury_balances.clone();
        helpers::debit(&mut treasury, source, amount)?;
        helpers::credit(&mut treasury, target, amount)?;
        self.treasury.insert(source.to_string(), treasury[source]);
        self.treasury.insert(target.to_string(), treasury[target]);
        self.topups.insert(
            top_up_id.to_string(),
            RestrictedStarterClaimLiveopsPoolTopUpRecord {
                controller_account_id: controller.to_string(),
                top_up_id: top_up_id.to_string(),
                source_treasury_bucket_id: source.to_string(),
                target_treasury_bucket_id: target.to_string(),
                amount,
                topped_up_at_epoch: epoch,
            },
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_issue(
        &mut self,
        state: &WorldState,
        issuer: &str,
        beneficiary: &str,
        source: &str,
        amount: u64,
        reason: &str,
        scope: &str,
        issued: u64,
        expires: u64,
    ) -> Result<(), WorldError> {
        let issuer = issuer.trim();
        if issuer.is_empty() {
            return Err(invalid("restricted grant issuer_id cannot be empty"));
        }
        let beneficiary = beneficiary.trim();
        if beneficiary.is_empty() {
            return Err(invalid(
                "restricted grant beneficiary_account_id cannot be empty",
            ));
        }
        let source = source.trim();
        if source.is_empty() {
            return Err(invalid(
                "restricted grant source_treasury_bucket_id cannot be empty",
            ));
        }
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(invalid("restricted grant issuance_reason cannot be empty"));
        }
        let scope = scope.trim();
        if scope.is_empty() {
            return Err(invalid("restricted grant spend_scope cannot be empty"));
        }
        if amount == 0 {
            return Err(invalid("restricted grant amount must be > 0"));
        }
        if expires <= issued {
            return Err(invalid(format!(
                "restricted grant expires_at_epoch must be > issued_at_epoch: expires={expires} issued={issued}"
            )));
        }
        if !helpers::can_insert(state, beneficiary) {
            return Err(invalid(format!(
                "restricted grant already active or pending settlement: beneficiary={beneficiary}"
            )));
        }
        if state
            .main_token_balances
            .get(beneficiary)
            .map(|v| v.restricted_starter_claim_balance)
            .unwrap_or(0)
            > 0
        {
            return Err(invalid(format!(
                "restricted grant beneficiary already has restricted balance: beneficiary={beneficiary}"
            )));
        }
        let mut treasury = state.main_token_treasury_balances.clone();
        helpers::debit(&mut treasury, source, amount)?;
        self.treasury.insert(source.to_string(), treasury[source]);
        let mut account = state
            .main_token_balances
            .get(beneficiary)
            .cloned()
            .unwrap_or_else(|| MainTokenAccountBalance {
                account_id: beneficiary.to_string(),
                ..Default::default()
            });
        account.restricted_starter_claim_balance = account.restricted_starter_claim_balance.checked_add(amount).ok_or_else(|| invalid(format!("restricted grant credit overflow: beneficiary={beneficiary} current={} amount={amount}", account.restricted_starter_claim_balance)))?;
        self.balances.insert(beneficiary.to_string(), account);
        let mut supply = state.main_token_supply.clone();
        supply.circulating_supply =
            supply
                .circulating_supply
                .checked_add(amount)
                .ok_or_else(|| {
                    invalid(format!(
                        "restricted grant circulating overflow: current={} amount={amount}",
                        supply.circulating_supply
                    ))
                })?;
        if supply.circulating_supply > supply.total_supply {
            return Err(invalid(format!(
                "restricted grant circulating exceeds total: circulating={} total={}",
                supply.circulating_supply, supply.total_supply
            )));
        }
        self.supply = Some(supply);
        self.grants.insert(
            beneficiary.to_string(),
            RestrictedStarterClaimGrantState {
                beneficiary_account_id: beneficiary.to_string(),
                issuer_id: issuer.to_string(),
                issuance_reason: reason.to_string(),
                spend_scope: scope.to_string(),
                source_treasury_bucket_id: source.to_string(),
                issued_amount: amount,
                issued_at_epoch: issued,
                expires_at_epoch: expires,
                status: RestrictedStarterClaimGrantStatus::Issued,
                status_updated_at_epoch: None,
                status_reason: None,
            },
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_terminal(
        &mut self,
        state: &WorldState,
        beneficiary: &str,
        issuer: &str,
        reason: &str,
        scope: &str,
        source: &str,
        issued_amount: u64,
        amount: u64,
        issued: u64,
        terminal_epoch: u64,
        configured_expiry: u64,
        revoke_reason: Option<&String>,
    ) -> Result<(), WorldError> {
        let mut grant = state
            .restricted_starter_claim_grants
            .get(beneficiary)
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "restricted grant not found for {}: beneficiary={beneficiary}",
                    if revoke_reason.is_some() {
                        "revoke"
                    } else {
                        "expiration"
                    }
                ))
            })?;
        let operation = if revoke_reason.is_some() {
            "revoke"
        } else {
            "expiration"
        };
        if grant.status != RestrictedStarterClaimGrantStatus::Issued {
            return Err(invalid(format!(
                "restricted grant already terminal before {operation}: beneficiary={beneficiary} status={:?}",
                grant.status
            )));
        }
        if grant.issuer_id != issuer
            || grant.issuance_reason != reason
            || grant.spend_scope != scope
            || grant.source_treasury_bucket_id != source
            || grant.issued_amount != issued_amount
            || grant.issued_at_epoch != issued
            || grant.expires_at_epoch != configured_expiry
        {
            return Err(invalid(format!(
                "restricted grant {operation} metadata mismatch: beneficiary={beneficiary}"
            )));
        }
        if let Some(reason) = revoke_reason {
            if reason.trim().is_empty() {
                return Err(invalid("restricted grant revoke_reason cannot be empty"));
            }
        } else if terminal_epoch < configured_expiry {
            return Err(invalid(format!(
                "restricted grant expired before configured epoch: beneficiary={beneficiary} configured={configured_expiry} actual={terminal_epoch}"
            )));
        }
        let mut balances = state.main_token_balances.clone();
        helpers::debit_restricted(&mut balances, beneficiary, amount)?;
        if let Some(account) = balances.get(beneficiary).cloned() {
            self.balances.insert(beneficiary.to_string(), account);
        }
        let mut treasury = state.main_token_treasury_balances.clone();
        helpers::credit(&mut treasury, source, amount)?;
        self.treasury.insert(source.to_string(), treasury[source]);
        if state.main_token_supply.circulating_supply < amount {
            return Err(invalid(format!(
                "restricted grant {operation} circulating insufficient: circulating={} amount={amount}",
                state.main_token_supply.circulating_supply
            )));
        }
        let mut supply = state.main_token_supply.clone();
        supply.circulating_supply -= amount;
        self.supply = Some(supply);
        grant.status = if revoke_reason.is_some() {
            RestrictedStarterClaimGrantStatus::Revoked
        } else {
            RestrictedStarterClaimGrantStatus::Expired
        };
        grant.status_updated_at_epoch = Some(terminal_epoch);
        grant.status_reason =
            Some(revoke_reason.map_or_else(|| "expired".to_string(), |v| v.clone()));
        self.grants.insert(beneficiary.to_string(), grant);
        Ok(())
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }
    fn routed_agents(&self, state: &WorldState) -> BTreeMap<String, AgentCell> {
        let id = match &self.event {
            DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp {
                controller_account_id,
                ..
            } => Some(controller_account_id),
            DomainEvent::RestrictedStarterClaimGrantIssued { issuer_id, .. }
            | DomainEvent::RestrictedStarterClaimGrantRevoked { issuer_id, .. } => Some(issuer_id),
            _ => None,
        };
        let mut out = BTreeMap::new();
        if let Some(id) = id
            && let Some(mut cell) = state.agents.get(id).cloned()
        {
            cell.mailbox.push_back(self.event.clone());
            out.insert(id.clone(), cell);
        }
        out
    }
    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state.main_token_treasury_balances.extend(self.treasury);
        state.main_token_balances.extend(self.balances);
        if let Some(supply) = self.supply {
            state.main_token_supply = supply;
        }
        state.restricted_starter_claim_grants.extend(self.grants);
        state
            .restricted_starter_claim_liveops_pool_top_up_records
            .extend(self.topups);
        state.materials = self.world_materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.world_materials);
    }
    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updates = self.routed_agents(state);
        out.serialize_field(
            "agents",
            &SparseMapProjection {
                base: &state.agents,
                updates: &updates,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_materials<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("materials", &self.world_materials)?;
        let updates = BTreeMap::from([(MaterialLedgerId::world(), self.world_materials.clone())]);
        out.serialize_field(
            "material_ledgers",
            &SparseMapProjection {
                base: &state.material_ledgers,
                updates: &updates,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_supply_balances<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "main_token_supply",
            self.supply.as_ref().unwrap_or(&state.main_token_supply),
        )?;
        out.serialize_field(
            "main_token_balances",
            &SparseMapProjection {
                base: &state.main_token_balances,
                updates: &self.balances,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_grants<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "restricted_starter_claim_grants",
            &SparseMapProjection {
                base: &state.restricted_starter_claim_grants,
                updates: &self.grants,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_treasury<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "main_token_treasury_balances",
            &SparseMapProjection {
                base: &state.main_token_treasury_balances,
                updates: &self.treasury,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_topups<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "restricted_starter_claim_liveops_pool_top_up_records",
            &SparseMapProjection {
                base: &state.restricted_starter_claim_liveops_pool_top_up_records,
                updates: &self.topups,
                deletion: None,
            },
        )
    }
}

mod helpers {
    use super::*;
    pub(super) fn debit(
        map: &mut BTreeMap<String, u64>,
        id: &str,
        amount: u64,
    ) -> Result<(), WorldError> {
        crate::state::apply_domain_event_main_token::helpers::debit_main_token_treasury_balance(map, id, amount)
    }
    pub(super) fn credit(
        map: &mut BTreeMap<String, u64>,
        id: &str,
        amount: u64,
    ) -> Result<(), WorldError> {
        crate::state::apply_domain_event_main_token::helpers::add_main_token_treasury_balance(map, id, amount)
    }
    pub(super) fn debit_restricted(
        map: &mut BTreeMap<String, MainTokenAccountBalance>,
        id: &str,
        amount: u64,
    ) -> Result<(), WorldError> {
        crate::state::apply_domain_event_main_token::helpers::debit_main_token_restricted_starter_claim_balance(map, id, amount)
    }
    pub(super) fn can_insert(state: &WorldState, id: &str) -> bool {
        crate::state::apply_domain_event_main_token::helpers::restricted_starter_claim_grant_can_be_inserted(state, id)
    }
}
fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
