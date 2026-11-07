use super::*;
pub(super) fn initialize(
    provider: &RemoteMonotonicEndpointProvider,
    instance: &EndpointInstanceBindingV1,
    registration: ProtectedEndpointRegistrationV1,
) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
    if instance.endpoint_public_key == hex::encode(provider.key.verifying_key().to_bytes()) {
        return Err(FencingError::FencingUnproven);
    }
    canonical_hash(&registration.authority_id)?;
    canonical_hash(&registration.activation_ref)?;
    if registration.unsigned_checkpoint_preimage.len() > 64 * 1024
        || registration
            .existing_checkpoint
            .as_ref()
            .is_some_and(|bytes| bytes.len() > 64 * 1024)
    {
        return Err(FencingError::FencingUnproven);
    }
    let mut stream = connect_oracle(&provider.address, provider.timeout)?;
    let state = exchange(
        &mut stream,
        instance,
        &provider.key,
        &provider.policy,
        None,
        MonotonicOracleCommandV1::InitializeProtected(Box::new(registration)),
    )?;
    if state.protected_custody != Some(ProtectedSignerCustodyV1::NonexportableLeaseBoundV1)
        || state.native_checkpoint_bytes.is_none()
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
pub(super) fn protected_sign(
    guard: &mut RemoteGuard,
    request: ProtectedEndpointSigningRequestV1,
) -> Result<ProtectedEndpointSignatureV1, FencingError> {
    request.validate()?;
    if guard
        .state
        .durable_set
        .as_ref()
        .is_some_and(|s| s.activation_digest.is_none())
    {
        return Err(FencingError::FencingUnproven);
    }
    if guard.state.protected_custody != Some(ProtectedSignerCustodyV1::NonexportableLeaseBoundV1)
        || request.instance_digest != guard.instance.digest()?
        || request.checkpoint != guard.state.checkpoint
        || guard.state.pending.is_some()
        || (guard.state.seal.is_some() && request.purpose.mutation())
    {
        return Err(FencingError::FencingUnproven);
    }
    let state = exchange(
        guard
            .stream
            .get_mut()
            .map_err(|_| invalid("protected oracle stream poisoned"))?,
        &guard.instance,
        &guard.key,
        &guard.policy,
        Some(&guard.state.lease),
        MonotonicOracleCommandV1::ProtectedSign(Box::new(request.clone())),
    )?;
    if state.durable_set != guard.state.durable_set
        || state.resource_binding != guard.state.resource_binding
        || state.protected_custody != guard.state.protected_custody
        || state.native_checkpoint_bytes != guard.state.native_checkpoint_bytes
        || state.checkpoint != guard.state.checkpoint
        || state.pending.is_some()
        || state.seal != guard.state.seal
    {
        return Err(FencingError::FencingUnproven);
    }
    let signature = state
        .protected_signature
        .clone()
        .ok_or(FencingError::FencingUnproven)?;
    if signature.body.resource_binding != state.resource_binding {
        return Err(FencingError::FencingUnproven);
    }
    verify_protected_endpoint_signature(&signature, &request, &guard.instance, &guard.policy)?;
    guard.state = state;
    Ok(signature)
}
