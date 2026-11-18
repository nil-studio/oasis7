use super::super::{
    Action, ActionEnvelope, ActionId, CausedBy, DomainEvent, GovernanceFinalityCertificate,
    ModuleActivation, ModuleChangeSet, ModuleProfileChanges, ProposalDecision, ProposalId,
    RejectReason, WorldError, WorldEventBody,
};
use super::World;
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_agent_api::world_action::ResourceKind;

mod artifact_actions;
mod release_actions;
mod release_normalization;
mod release_request_support;
mod release_support;
mod rollback_actions;

const MODULE_DEPLOY_FEE_BYTES_PER_ELECTRICITY: i64 = 2_048;
const MODULE_COMPILE_FEE_BYTES_PER_ELECTRICITY: i64 = 1_024;
pub(in crate::world) const MODULE_LIST_FEE_AMOUNT: i64 = 1;
pub(in crate::world) const MODULE_DELIST_FEE_AMOUNT: i64 = 1;
pub(in crate::world) const MODULE_DESTROY_FEE_AMOUNT: i64 = 1;
const MODULE_RELEASE_DEFAULT_REQUIRED_ROLES: &[&str] = &["security", "economy", "runtime"];
pub(in crate::world) const MODULE_RELEASE_PROFILE_CHANGE_LIMIT: usize = 50;
const MODULE_RELEASE_ATTESTATION_LIMIT: usize = 128;
