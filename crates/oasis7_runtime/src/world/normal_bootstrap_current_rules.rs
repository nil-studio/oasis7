//! Original Bootstrap input predicates, shared by Legacy and actual Current.
//! These functions inspect DATA only. A successful result is not installation,
//! finality authority, a source qualification, or a committed transaction.
use super::CognitionProvisioningRequestV1;
use super::{ProviderBackedBootstrapAuthorityV1, bootstrap_denied};
use crate::WorldError;
use oasis7_agent_api::RuntimeBindingV1;
use oasis7_wasm_abi::CapabilitySubject;

pub(super) fn validate_binding_fields_v1(
    input: &ProviderBackedBootstrapAuthorityV1,
    binding: &RuntimeBindingV1,
) -> Result<(), WorldError> {
    validate_binding_scope_v1(
        input,
        &binding.world_id,
        &binding.branch_id,
        binding.reorg_epoch,
    )
}
pub(in crate::world) fn validate_binding_scope_v1(
    input: &ProviderBackedBootstrapAuthorityV1,
    world_id: &str,
    branch_id: &str,
    reorg_epoch: u64,
) -> Result<(), WorldError> {
    if input.agent_id.trim().is_empty()
        || input.owner_binding.trim().is_empty()
        || input.owner_generation == 0
        || input.identity.owner_binding != input.owner_binding
        || input.identity.generation != input.owner_generation
        || input.world_id != world_id
        || input.branch_id != branch_id
        || input.reorg_epoch != reorg_epoch
    {
        return Err(bootstrap_denied(
            "provider bootstrap identity does not match live Runtime binding",
        ));
    }
    Ok(())
}

pub(super) fn validate_authority_fields_v1(
    input: &ProviderBackedBootstrapAuthorityV1,
    binding: &RuntimeBindingV1,
) -> Result<(), WorldError> {
    validate_authority_scope_v1(input, binding.finality_epoch)?;
    let expected_request = CognitionProvisioningRequestV1::new(
        input.provision_id.clone(),
        input.owner_binding.clone(),
        input.owner_binding.clone(),
        input.owner_generation,
        input.world_id.clone(),
        input.branch_id.clone(),
        input.reorg_epoch,
        input.allowance,
        input.authority_context.clone(),
    );
    expected_request
        .validate()
        .map_err(|error| bootstrap_denied(format!("provider bootstrap request: {error}")))?;
    validate_request_digest_fields_v1(input, &expected_request)
}

pub(in crate::world) fn validate_authority_scope_v1(
    input: &ProviderBackedBootstrapAuthorityV1,
    finality_epoch: u64,
) -> Result<(), WorldError> {
    match &input.grant.subject {
        CapabilitySubject::Agent {
            agent_id,
            owner_binding,
            generation,
        } if agent_id == &input.agent_id
            && owner_binding == &input.owner_binding
            && *generation == input.owner_generation => {}
        _ => {
            return Err(bootstrap_denied(
                "provider bootstrap grant subject does not match identity",
            ));
        }
    }
    if input.grant.audience.world_id != input.world_id
        || input.grant.audience.branch_id != input.branch_id
        || input.grant.audience.finality_epoch != finality_epoch
    {
        return Err(bootstrap_denied(
            "provider bootstrap grant audience does not match Runtime binding",
        ));
    }
    if input.authority_record.world_id != input.world_id
        || input.authority_record.branch_id != input.branch_id
        || input.authority_record.finality_epoch != finality_epoch
        || input.authority_record.finality_status != "finalized"
        || input.authority_record.issuer_id != input.grant.issuer.issuer_id
    {
        return Err(bootstrap_denied(
            "provider bootstrap authority record does not match grant binding",
        ));
    }
    if input.invocation_context.grant_id != input.grant.grant_id
        || input.invocation_context.subject != input.grant.subject
        || input.invocation_context.audience != input.grant.audience
        || input.invocation_context.module_id != input.grant.scope.module_id
        || input.invocation_context.module_version != input.grant.scope.module_version
    {
        return Err(bootstrap_denied(
            "provider bootstrap invocation context identity mismatch",
        ));
    }

    Ok(())
}

pub(in crate::world) fn validate_request_digest_fields_v1(
    input: &ProviderBackedBootstrapAuthorityV1,
    expected_request: &CognitionProvisioningRequestV1,
) -> Result<(), WorldError> {
    if expected_request.authority_digest != input.authority_digest
        || expected_request.provisioning_digest != input.provisioning_digest
    {
        return Err(bootstrap_denied(
            "provider bootstrap provisioning digest does not match request",
        ));
    }
    Ok(())
}
