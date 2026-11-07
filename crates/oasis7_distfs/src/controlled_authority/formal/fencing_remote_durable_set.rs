//! Healthy install/activate commands. Unlike InitializeProtected, these must
//! consume an old seal and retain the old-instance tombstone at the provider.
use super::*;
use crate::controlled_authority::formal::{
    VerifiedDurableSetActivationV1, VerifiedSealedDurableSetTransitionV1,
};
fn connect(provider: &RemoteMonotonicEndpointProvider) -> Result<TcpStream, FencingError> {
    connect_oracle(&provider.address, provider.timeout)
}
fn installed(
    provider: &RemoteMonotonicEndpointProvider,
    instance: &EndpointInstanceBindingV1,
    sealed: &VerifiedSealedDurableSetTransitionV1,
    stream: TcpStream,
    state: MonotonicOracleReplyBodyV1,
) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
    let installed = state
        .durable_set
        .as_ref()
        .ok_or(FencingError::FencingUnproven)?;
    if (state.pending.is_some() && installed.activation_digest.is_none())
        || state.seal.is_some()
        || state.protected_custody != Some(ProtectedSignerCustodyV1::NonexportableLeaseBoundV1)
        || installed.transition_digest != sealed.transition().digest()
        || installed.successor_authority_id
            != sealed
                .transition()
                .body()
                .successor
                .authority_id()
                .map_err(|e| invalid(e.to_string()))?
        || &installed.install_receipt.body.instance != instance
        || state.native_checkpoint_bytes.as_ref().is_none_or(|bytes| {
            blake3::hash(bytes).to_hex().as_str() != state.checkpoint.commitment
        })
    {
        return Err(FencingError::FencingUnproven);
    }
    crate::controlled_authority::formal::verify_one_durable_set_install(
        sealed,
        instance.endpoint_role,
        &installed.install_receipt,
    )
    .map_err(|e| invalid(e.to_string()))?;
    if state.checkpoint.revision < installed.install_receipt.body.checkpoint.revision
        || (state.checkpoint.revision == installed.install_receipt.body.checkpoint.revision
            && state.checkpoint != installed.install_receipt.body.checkpoint)
        || (installed.activation_digest.is_none()
            && state.checkpoint != installed.install_receipt.body.checkpoint)
    {
        return Err(FencingError::FencingUnproven);
    }
    Ok(Box::new(RemoteGuard {
        stream: Mutex::new(stream),
        instance: instance.clone(),
        key: provider.key.clone(),
        policy: provider.policy.clone(),
        state,
    }))
}
pub(super) fn install(
    provider: &RemoteMonotonicEndpointProvider,
    instance: &EndpointInstanceBindingV1,
    sealed: &VerifiedSealedDurableSetTransitionV1,
    request: DurableSetNativeInstallRequestV1,
) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
    request.validate_bounds()?;
    if request.authorization != *sealed.evidence()
        || request.registration.authority_id
            != sealed
                .transition()
                .body()
                .successor
                .authority_id()
                .map_err(|e| invalid(e.to_string()))?
        || request.registration.activation_ref
            != sealed.transition().predecessor_authority().activation_ref()
    {
        return Err(FencingError::FencingUnproven);
    }
    let expected = match instance.endpoint_role {
        EndpointRole::Primary => &sealed.transition().body().successor.primary.instance,
        EndpointRole::Replica => &sealed.transition().body().successor.replica.instance,
    };
    if instance != expected {
        return Err(FencingError::FencingUnproven);
    }
    let mut stream = connect(provider)?;
    let state = exchange(
        &mut stream,
        instance,
        &provider.key,
        &provider.policy,
        None,
        MonotonicOracleCommandV1::InstallDurableSet(Box::new(request)),
    )?;
    installed(provider, instance, sealed, stream, state)
}
pub(super) fn query(
    provider: &RemoteMonotonicEndpointProvider,
    instance: &EndpointInstanceBindingV1,
    sealed: &VerifiedSealedDurableSetTransitionV1,
) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
    let mut stream = connect(provider)?;
    let state = exchange(
        &mut stream,
        instance,
        &provider.key,
        &provider.policy,
        None,
        MonotonicOracleCommandV1::QueryDurableSetInstall(sealed.transition().digest().into()),
    )?;
    installed(provider, instance, sealed, stream, state)
}
pub(super) fn activate(
    provider: &RemoteMonotonicEndpointProvider,
    instance: &EndpointInstanceBindingV1,
    sealed: &VerifiedSealedDurableSetTransitionV1,
    activation: &VerifiedDurableSetActivationV1,
    request: DurableSetNativeActivationRequestV1,
) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
    request.install.validate_bounds()?;
    if request.schema_version != 1
        || request.install.authorization != *sealed.evidence()
        || activation.transition_digest() != sealed.transition().digest()
        || activation
            .predecessor()
            .context_digest()
            .map_err(|e| invalid(e.to_string()))?
            != sealed
                .transition()
                .predecessor_authority()
                .context_digest()
                .map_err(|e| invalid(e.to_string()))?
    {
        return Err(FencingError::FencingUnproven);
    }
    // Raw receipts cannot authorize this operation: independently re-verify both
    // and compare the complete stable activation identity with the opaque token.
    let checked = crate::controlled_authority::formal::verify_durable_set_activation(
        sealed.transition(),
        [
            &request.install.authorization.primary_seal,
            &request.install.authorization.replica_seal,
        ],
        [&request.primary_install, &request.replica_install],
    )
    .map_err(|e| invalid(e.to_string()))?;
    if checked.activation_digest() != activation.activation_digest() {
        return Err(FencingError::FencingUnproven);
    }
    let mut stream = connect(provider)?;
    let state = exchange(
        &mut stream,
        instance,
        &provider.key,
        &provider.policy,
        None,
        MonotonicOracleCommandV1::ActivateDurableSet(Box::new(request)),
    )?;
    if state
        .durable_set
        .as_ref()
        .and_then(|s| s.activation_digest.as_deref())
        != Some(activation.activation_digest())
    {
        return Err(FencingError::FencingUnproven);
    }
    installed(provider, instance, sealed, stream, state)
}
