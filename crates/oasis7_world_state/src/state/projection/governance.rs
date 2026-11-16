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
        ORIGINAL_GOVERNANCE_FIELD_SLOTS_V1[0].name,
        governance_registry_overlay.map_or(&state.node_identity_bindings, |o| &o.identity_bindings),
    )?;
    output.serialize_field(
        ORIGINAL_GOVERNANCE_FIELD_SLOTS_V1[1].name,
        governance_registry_overlay.map_or(&state.node_main_token_account_bindings, |o| {
            &o.account_bindings
        }),
    )?;
    output.serialize_field(
        ORIGINAL_GOVERNANCE_FIELD_SLOTS_V1[2].name,
        &state.governance_finality_signer_registry,
    )?;
    output.serialize_field(
        ORIGINAL_GOVERNANCE_FIELD_SLOTS_V1[3].name,
        governance_registry_overlay
            .map_or(&state.governance_validator_admissions, |o| &o.admissions),
    )?;
    let rotations = governance_registry_overlay
        .map_or(&state.governance_finality_key_rotations, |overlay| {
            &overlay.rotations
        });
    let bootstrap_rotations = governance_registry_overlay
        .map_or(&state.governance_bootstrap_finality_key_rotations, |o| {
            &o.bootstrap_rotations
        });
    crate::governance::validate_finality_key_rotation_history_capacity(
        rotations.len(),
        bootstrap_rotations.len(),
    )
    .map_err(serde::ser::Error::custom)?;
    output.serialize_field(
        ORIGINAL_GOVERNANCE_FIELD_SLOTS_V1[4].name,
        governance_registry_overlay.map_or(&state.governance_main_token_controller_registry, |o| {
            &o.controller_registry
        }),
    )?;
    let authorization_nonces = governance_registry_overlay
        .map_or(&state.governance_authorization_nonces, |overlay| {
            &overlay.authorization_nonces
        });
    if !authorization_nonces.is_empty() {
        output.serialize_field(ORIGINAL_GOVERNANCE_FIELD_SLOTS_V1[5].name, authorization_nonces)?;
    }
    output.serialize_field(
        ORIGINAL_GOVERNANCE_FIELD_SLOTS_V1[6].name,
        &state.reward_signature_governance_policy,
    )?;
    if !rotations.is_empty() {
        output.serialize_field(
            ORIGINAL_GOVERNANCE_FIELD_SLOTS_V1[7].name,
            &crate::governance::FinalityKeyRotationHistoryRef(rotations),
        )?;
    }
    if !bootstrap_rotations.is_empty() {
        output.serialize_field(
            ORIGINAL_GOVERNANCE_FIELD_SLOTS_V1[8].name,
            &crate::governance::BootstrapFinalityKeyRotationHistoryRef(bootstrap_rotations),
        )?;
    }
    Ok(())
}

pub(crate) const ORIGINAL_GOVERNANCE_FIELD_SLOTS_V1:[crate::state::projection::OriginalStateFieldSlotV1;9]=[
 crate::state::projection::OriginalStateFieldSlotV1{name:"node_identity_bindings",emission:crate::state::projection::OriginalStateFieldEmissionV1::Always},
 crate::state::projection::OriginalStateFieldSlotV1{name:"node_main_token_account_bindings",emission:crate::state::projection::OriginalStateFieldEmissionV1::Always},
 crate::state::projection::OriginalStateFieldSlotV1{name:"governance_finality_signer_registry",emission:crate::state::projection::OriginalStateFieldEmissionV1::Always},
 crate::state::projection::OriginalStateFieldSlotV1{name:"governance_validator_admissions",emission:crate::state::projection::OriginalStateFieldEmissionV1::Always},
 crate::state::projection::OriginalStateFieldSlotV1{name:"governance_main_token_controller_registry",emission:crate::state::projection::OriginalStateFieldEmissionV1::Always},
 crate::state::projection::OriginalStateFieldSlotV1{name:"governance_authorization_nonces",emission:crate::state::projection::OriginalStateFieldEmissionV1::NonEmpty},
 crate::state::projection::OriginalStateFieldSlotV1{name:"reward_signature_governance_policy",emission:crate::state::projection::OriginalStateFieldEmissionV1::Always},
 crate::state::projection::OriginalStateFieldSlotV1{name:"governance_finality_key_rotations",emission:crate::state::projection::OriginalStateFieldEmissionV1::NonEmpty},
 crate::state::projection::OriginalStateFieldSlotV1{name:"governance_bootstrap_finality_key_rotations",emission:crate::state::projection::OriginalStateFieldEmissionV1::NonEmpty},
];
