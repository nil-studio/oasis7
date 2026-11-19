//! Exact C0 body and signed-input identity from the original verified authority.
use super::*;

fn same_hex(id: &[u8; 32], value: &str, common: &mut FormalBoundedContextV1<'_>) -> Result<bool> {
    common
        .charge(1, value.len() as u64 + 96)
        .map_err(context_error)?;
    let mut encoded = [0; 64];
    hex::encode_to_slice(id, &mut encoded)
        .map_err(|_| PageStoreError::Corrupt("GEN original digest encoding"))?;
    Ok(encoded.as_slice() == value.as_bytes())
}

pub(super) fn approved_input(
    inputs: &ProvisionalGenesisExecutionInputsV1,
    approval: &GenesisApprovalInputV1<'_>,
    max: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>> {
    let wire = signed_input(approval, max, common)?;
    let authority = approval.authority;
    let approved = authority
        .initial_body()
        .ok_or(PageStoreError::Corrupt("GEN initial authority missing"))?;
    validate_bindings(inputs, authority, approved, common)?;
    Ok(wire)
}

fn signed_input(
    approval: &GenesisApprovalInputV1<'_>,
    max: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>> {
    let authority = approval.authority;
    let approved = authority
        .initial_body()
        .ok_or(PageStoreError::Corrupt("GEN initial authority missing"))?;
    common.consume_steps(1).map_err(context_error)?;
    if authority.definition().profile_version != IWS_AUTHORITY_PROFILE_VERSION
        || approved.schema_version != IWS_ACTIVATION_SCHEMA
    {
        return Err(PageStoreError::Corrupt("GEN exact initial authority profile").into());
    }
    // Both encodings use the original serde grammar and the same finite Core.
    // Compare the retained verified body, then the *signed input* identity.
    let first = context_wire::encode_paid(
        approved,
        max,
        common,
        "GEN approval bytes",
        "GEN approved body",
    )
    .map_err(context_error)?;
    let second = context_wire::encode_paid(
        &approval.certificate.body,
        max,
        common,
        "GEN approval bytes",
        "GEN candidate body",
    )
    .map_err(context_error)?;
    common
        .charge(1, (first.len() + second.len()) as u64)
        .map_err(context_error)?;
    if first.as_slice() != second.as_slice() {
        return Err(
            PageStoreError::Corrupt("GEN signed approval differs from original authority").into(),
        );
    }
    drop(first);
    drop(second);
    let wire = context_wire::encode_paid(
        approval.certificate,
        max,
        common,
        "GEN input bytes",
        "GEN original signed input",
    )
    .map_err(context_error)?;
    let input = digest(&wire, common)?;
    if !same_hex(
        &input,
        authority
            .initial_input_digest()
            .ok_or(PageStoreError::Corrupt(
                "GEN approved input identity missing",
            ))?,
        common,
    )? {
        return Err(PageStoreError::Corrupt("GEN original signed input identity").into());
    }
    Ok(wire)
}

fn validate_bindings(
    inputs: &ProvisionalGenesisExecutionInputsV1,
    authority: &VerifiedAuthorityV1,
    approved: &oasis7_formal_protocol::controlled_authority::formal::InitialActivationBodyV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<()> {
    let business = inputs.business();
    let source = business
        .projected()
        .business_source_bounded_v1(common)
        .map_err(Error::Body)?;
    let identity = source.identity();
    let world = &authority.definition().world;
    common
        .charge(
            3,
            (identity.0.len()
                + identity.1.len()
                + identity.2.map_or(0, str::len)
                + world.world_id.len()
                + world.chain_id.len()
                + world.genesis_digest.len()) as u64,
        )
        .map_err(context_error)?;
    if identity.0 != world.world_id
        || identity.1 != world.chain_id
        || identity.2 != Some(world.genesis_digest.as_str())
    {
        return Err(PageStoreError::Corrupt("GEN actual World/approved identity").into());
    }
    for (id, value) in [
        (
            business.roots_original().0.id,
            approved.initial_state_root.as_str(),
        ),
        (
            inputs.manifest_original().0.id,
            approved.runtime_manifest_digest.as_str(),
        ),
        (
            inputs.governing_manifest(),
            approved.governing_manifest_digest.as_str(),
        ),
        (
            inputs.policy_original().0.id,
            approved.execution_policy_digest.as_str(),
        ),
    ] {
        if !same_hex(&id, value, common)? {
            return Err(PageStoreError::Corrupt("GEN approved actual original commitment").into());
        }
    }
    let roles = [
        (
            ArtifactRole::IncrementalGenesisConfig,
            business.config_original().0.id,
        ),
        (
            ArtifactRole::IncrementalRootSet,
            business.roots_original().0.id,
        ),
        (
            ArtifactRole::ExecutionManifest,
            inputs.manifest_original().0.id,
        ),
        (
            ArtifactRole::NativeExecutionEnvironment,
            inputs.native_original().0.id,
        ),
        (ArtifactRole::Wasm, business.wasm_catalog_original().0.id),
        (ArtifactRole::Rules, inputs.rules_catalog_original().0.id),
    ];
    if approved.artifact_roots.len() != roles.len() {
        return Err(PageStoreError::Corrupt("GEN approved role count").into());
    }
    for (role, id) in roles {
        common.consume_steps(1).map_err(context_error)?;
        if !same_hex(
            &id,
            approved
                .artifact_roots
                .get(&role)
                .ok_or(PageStoreError::Corrupt("GEN approved role missing"))?,
            common,
        )? {
            return Err(PageStoreError::Corrupt("GEN approved role original identity").into());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "formal_iws_genesis_approval_tests.rs"]
mod tests;
