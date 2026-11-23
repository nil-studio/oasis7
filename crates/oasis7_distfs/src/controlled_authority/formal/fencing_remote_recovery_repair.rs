//! The original remote provider authorizes one private replacement directory.
//! Ordinary acquire/registration never consumes this authorization.
use super::*;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(test)]
#[path = "fencing_remote_recovery_repair_tests.rs"]
mod tests;

pub struct RecoveryRepairEndpointProviderV1 {
    inner: Arc<dyn MonotonicEndpointProvider>,
    path: PathBuf,
    request: ProtectedRecoveryRepairRequestV1,
    original_grant: Mutex<Option<ProtectedRecoveryRepairGrantV1>>,
}

impl RecoveryRepairEndpointProviderV1 {
    pub fn new(
        inner: Arc<dyn MonotonicEndpointProvider>,
        path: PathBuf,
        authority: &crate::controlled_authority::formal::VerifiedAuthorityV1,
        request: ProtectedRecoveryRepairRequestV1,
    ) -> Result<Self, FencingError> {
        request.validate(authority)?;
        if directory_fingerprint(&path)? != request.directory_fingerprint {
            return Err(invalid(
                "repair target directory differs from original grant request",
            ));
        }
        Ok(Self {
            inner,
            path,
            request,
            original_grant: Mutex::new(None),
        })
    }

    fn acquire_original(
        &self,
        instance: &EndpointInstanceBindingV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        if directory_fingerprint(&self.path)? != self.request.directory_fingerprint {
            return Err(invalid("repair target directory disappeared or changed"));
        }
        let guard = self
            .inner
            .repair_protected(instance, self.request.clone())?;
        let grant = guard
            .recovery_repair_grant()
            .ok_or(FencingError::FencingUnproven)?;
        grant.verify_request(instance, &self.request)?;
        if guard.checkpoint().revision < grant.original_checkpoint.revision
            || (guard.checkpoint().revision == grant.original_checkpoint.revision
                && guard.checkpoint() != &grant.original_checkpoint)
        {
            return Err(FencingError::Rollback);
        }
        let mut held = self
            .original_grant
            .lock()
            .map_err(|_| invalid("repair grant poisoned"))?;
        if held.as_ref().is_some_and(|old| old != grant) {
            return Err(invalid(
                "provider replaced the original recovery directory grant",
            ));
        }
        *held = Some(grant.clone());
        Ok(guard)
    }
}

impl MonotonicEndpointProvider for RecoveryRepairEndpointProviderV1 {
    fn repair_protected(
        &self,
        instance: &EndpointInstanceBindingV1,
        request: ProtectedRecoveryRepairRequestV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        if request != self.request {
            return Err(invalid("repair changed its immutable original request"));
        }
        self.acquire_original(instance)
    }

    fn acquire(
        &self,
        instance: &EndpointInstanceBindingV1,
        initial: &EndpointCheckpointBindingV1,
    ) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
        let guard = self.acquire_original(instance)?;
        if guard.checkpoint() != initial
            && !guard
                .pending()
                .is_some_and(|pending| initial == &pending.previous || initial == &pending.next)
        {
            return Err(FencingError::Rollback);
        }
        Ok(guard)
    }
}

pub(super) fn open(
    provider: &RemoteMonotonicEndpointProvider,
    instance: &EndpointInstanceBindingV1,
    request: ProtectedRecoveryRepairRequestV1,
) -> Result<Box<dyn MonotonicEndpointGuard>, FencingError> {
    if hex::encode(provider.key.verifying_key().to_bytes())
        != request
            .plan
            .body
            .authorization
            .recovery_credential_public_key
    {
        return Err(invalid(
            "repair requires the plan's separate recovery credential",
        ));
    }
    let mut stream = connect_oracle(&provider.address, provider.timeout)?;
    let state = exchange(
        &mut stream,
        instance,
        &provider.key,
        &provider.policy,
        None,
        MonotonicOracleCommandV1::RepairProtected(Box::new(request.clone())),
    )?;
    let grant = state
        .recovery_repair
        .as_ref()
        .ok_or(FencingError::FencingUnproven)?;
    grant.verify_request(instance, &request)?;
    if state.protected_custody != Some(ProtectedSignerCustodyV1::NonexportableLeaseBoundV1)
        || state.durable_set.is_some()
        || state.native_checkpoint_bytes.as_ref().is_none_or(|bytes| {
            bytes.len() > 64 * 1024
                || blake3::hash(bytes).to_hex().as_str() != state.checkpoint.commitment
        })
        || state.checkpoint.revision < grant.original_checkpoint.revision
        || (state.checkpoint.revision == grant.original_checkpoint.revision
            && state.checkpoint != grant.original_checkpoint)
    {
        return Err(FencingError::FencingUnproven);
    }
    Ok(Box::new(RemoteGuard {
        failed: false,
        stream: Mutex::new(stream),
        instance: instance.clone(),
        key: provider.key.clone(),
        policy: provider.policy.clone(),
        state,
    }))
}

pub(super) fn replay(
    guard: &mut RemoteGuard,
    request: ProtectedRecoveryReceiptReplayV1,
) -> Result<ProtectedEndpointSignatureV1, FencingError> {
    // All chunks share this original operation's lease, wall-clock budget and
    // signed state. No transport retry or new acquire occurs inside the replay.
    let (read_timeout, write_timeout) = {
        let stream = guard
            .stream
            .get_mut()
            .map_err(|_| invalid("repair stream poisoned"))?;
        (
            stream
                .read_timeout()
                .map_err(|e| FencingError::Io(e.to_string()))?,
            stream
                .write_timeout()
                .map_err(|e| FencingError::Io(e.to_string()))?,
        )
    };
    let timeout = read_timeout
        .zip(write_timeout)
        .map(|(read, write)| read.min(write))
        .filter(|timeout| !timeout.is_zero())
        .ok_or_else(|| invalid("repair requires a finite original transport deadline"))?;
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| invalid("repair original deadline overflow"))?;
    request.signing.validate()?;
    let total = request
        .entry_prefix
        .len()
        .checked_add(request.entry_suffix.len())
        .and_then(|bytes| bytes.checked_add(128))
        .ok_or_else(|| invalid("repair original receipt size overflow"))?;
    if total > 64 * 1024 * 1024
        || guard.state.recovery_repair.is_none()
        || guard.state.pending.is_some()
        || guard.state.seal.is_some()
        || request.signing.checkpoint != guard.state.checkpoint
        || request.signing.instance_digest != guard.instance.digest()?
    {
        return Err(FencingError::FencingUnproven);
    }
    let header = ProtectedRecoveryReceiptReplayHeaderV1 {
        signing: request.signing.clone(),
        prefix_bytes: u64::try_from(request.entry_prefix.len())
            .map_err(|_| invalid("repair original prefix size"))?,
        suffix_bytes: u64::try_from(request.entry_suffix.len())
            .map_err(|_| invalid("repair original suffix size"))?,
    };
    let state = replay_frame(
        guard,
        MonotonicOracleCommandV1::BeginReplay(Box::new(header)),
        deadline,
    )?;
    if state.protected_signature.is_some() {
        return Err(FencingError::FencingUnproven);
    }
    let mut offset = 0u64;
    for bytes in [&request.entry_prefix, &request.entry_suffix] {
        for chunk in bytes.chunks(64 * 1024) {
            let state = replay_frame(
                guard,
                MonotonicOracleCommandV1::ReplayChunk {
                    offset,
                    bytes: chunk.to_vec(),
                },
                deadline,
            )?;
            if state.protected_signature.is_some() {
                return Err(FencingError::FencingUnproven);
            }
            offset = offset
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| invalid("repair original stream offset overflow"))?;
        }
    }
    let state = replay_frame(guard, MonotonicOracleCommandV1::FinishReplay, deadline)?;
    let signature = state
        .protected_signature
        .clone()
        .ok_or(FencingError::FencingUnproven)?;
    if signature.body.resource_binding != state.resource_binding {
        return Err(FencingError::FencingUnproven);
    }
    verify_protected_endpoint_signature(
        &signature,
        &request.signing,
        &guard.instance,
        &guard.policy,
    )?;
    remaining(deadline)?;
    // The successful replay must not shorten the independent next operation.
    // Any failure retires this guard through with_live_lease instead.
    let stream = guard
        .stream
        .get_mut()
        .map_err(|_| invalid("repair stream poisoned"))?;
    stream
        .set_read_timeout(read_timeout)
        .map_err(|e| FencingError::Io(e.to_string()))?;
    stream
        .set_write_timeout(write_timeout)
        .map_err(|e| FencingError::Io(e.to_string()))?;
    guard.state = state;
    Ok(signature)
}

fn replay_frame(
    guard: &mut RemoteGuard,
    command: MonotonicOracleCommandV1,
    deadline: Instant,
) -> Result<MonotonicOracleReplyBodyV1, FencingError> {
    let state = exchange_before(
        guard
            .stream
            .get_mut()
            .map_err(|_| invalid("repair stream poisoned"))?,
        &guard.instance,
        &guard.key,
        &guard.policy,
        Some(&guard.state.lease),
        command,
        Some(deadline),
    )?;
    if state.recovery_repair != guard.state.recovery_repair
        || state.durable_set != guard.state.durable_set
        || state.resource_binding != guard.state.resource_binding
        || state.protected_custody != guard.state.protected_custody
        || state.native_checkpoint_bytes != guard.state.native_checkpoint_bytes
        || state.checkpoint != guard.state.checkpoint
        || state.pending.is_some()
        || state.seal != guard.state.seal
    {
        return Err(FencingError::FencingUnproven);
    }
    Ok(state)
}
