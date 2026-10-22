//! Governance projection shares the same overlay as atomic event installation.
use super::*;

pub(super) fn serialize_fields<S: serde::ser::SerializeStruct>(
    output: &mut S,
    state: &WorldState,
    governance_registry_overlay: Option<
        &crate::world::governance_registry_publication::PreparedGovernanceRegistryEvent,
    >,
) -> Result<(), S::Error> {
    output.serialize_field(
        "node_identity_bindings",
        governance_registry_overlay.map_or(&state.node_identity_bindings, |o| &o.identity_bindings),
    )?;
    output.serialize_field(
        "node_main_token_account_bindings",
        governance_registry_overlay.map_or(&state.node_main_token_account_bindings, |o| {
            &o.account_bindings
        }),
    )?;
    output.serialize_field(
        "governance_finality_signer_registry",
        &state.governance_finality_signer_registry,
    )?;
    output.serialize_field(
        "governance_validator_admissions",
        governance_registry_overlay
            .map_or(&state.governance_validator_admissions, |o| &o.admissions),
    )?;
    output.serialize_field(
        "governance_main_token_controller_registry",
        governance_registry_overlay.map_or(&state.governance_main_token_controller_registry, |o| {
            &o.controller_registry
        }),
    )?;
    let authorization_nonces = governance_registry_overlay
        .map_or(&state.governance_authorization_nonces, |overlay| {
            &overlay.authorization_nonces
        });
    if !authorization_nonces.is_empty() {
        output.serialize_field("governance_authorization_nonces", authorization_nonces)?;
    }
    output.serialize_field(
        "reward_signature_governance_policy",
        &state.reward_signature_governance_policy,
    )?;
    Ok(())
}
