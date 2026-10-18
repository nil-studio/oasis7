use super::super::state::ModuleReleaseRequestStatus;
use super::super::{
    Action, ActionEnvelope, ActionId, CausedBy, DomainEvent, GovernanceFinalityCertificate,
    ModuleActivation, ModuleChangeSet, ModuleProfileChanges, ModuleUpgrade, ProposalDecision,
    ProposalId, RejectReason, WorldError, WorldEventBody,
};
use super::World;
use super::event_processing::action_to_event_economy::ensure_profile_field_whitelist;
use super::governance::governance_finality_signed_stake_bps;
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_agent_api::world_action::ResourceKind;
use std::collections::BTreeSet;

mod artifact_actions;
mod release_actions;
mod release_normalization;
mod release_request_support;
mod release_support;
mod rollback_actions;

const MODULE_DEPLOY_FEE_BYTES_PER_ELECTRICITY: i64 = 2_048;
const MODULE_COMPILE_FEE_BYTES_PER_ELECTRICITY: i64 = 1_024;
const MODULE_LIST_FEE_AMOUNT: i64 = 1;
const MODULE_DELIST_FEE_AMOUNT: i64 = 1;
const MODULE_DESTROY_FEE_AMOUNT: i64 = 1;
const MODULE_RELEASE_DEFAULT_REQUIRED_ROLES: &[&str] = &["security", "economy", "runtime"];
const MODULE_RELEASE_PROFILE_CHANGE_LIMIT: usize = 50;
const MODULE_RELEASE_ATTESTATION_LIMIT: usize = 128;
