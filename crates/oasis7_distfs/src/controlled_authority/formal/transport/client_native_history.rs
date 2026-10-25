//! Historical private restoration from this client's freshly protected prefix.
//! The returned token is server-side evidence, not an ordinary reader trust pin.
use super::super::super::endpoint::VerifiedNativeHistoricalRestoreV1;
use super::*;
#[path = "native_history_content_cache.rs"]
mod content_cache;
impl RemoteFormalEndpoint {
    pub fn restore_native_at(
        &mut self,
        target: &FormalHeadRefV1,
        historical_authority: &VerifiedAuthorityV1,
        original: &VerifiedAuthorityV1,
        recovery: Option<&VerifiedRecovery>,
    ) -> Result<VerifiedNativeHistoricalRestoreV1, FormalError> {
        self.restore_native_position(
            target.position,
            Some(&target.record_id),
            historical_authority,
            original,
            recovery,
            None,
            None,
        )
    }
    pub fn locate_native_position(
        &mut self,
        position: u64,
        historical_authority: &VerifiedAuthorityV1,
        original: &VerifiedAuthorityV1,
        recovery: Option<&VerifiedRecovery>,
    ) -> Result<VerifiedNativeHistoricalRestoreV1, FormalError> {
        self.restore_native_position(
            position,
            None,
            historical_authority,
            original,
            recovery,
            None,
            None,
        )
    }
    pub fn restore_native_at_cached(
        &mut self,
        target: &FormalHeadRefV1,
        historical_authority: &VerifiedAuthorityV1,
        original: &VerifiedAuthorityV1,
        recovery: Option<&VerifiedRecovery>,
        cache: &dyn super::super::super::business_index::BusinessIndexNodeReader,
    ) -> Result<VerifiedNativeHistoricalRestoreV1, FormalError> {
        self.restore_native_position(
            target.position,
            Some(&target.record_id),
            historical_authority,
            original,
            recovery,
            None,
            Some(cache),
        )
    }
    pub fn locate_native_position_cached(
        &mut self,
        position: u64,
        historical_authority: &VerifiedAuthorityV1,
        original: &VerifiedAuthorityV1,
        recovery: Option<&VerifiedRecovery>,
        cache: &dyn super::super::super::business_index::BusinessIndexNodeReader,
    ) -> Result<VerifiedNativeHistoricalRestoreV1, FormalError> {
        self.restore_native_position(
            position,
            None,
            historical_authority,
            original,
            recovery,
            None,
            Some(cache),
        )
    }
    /// Returns immutable first-Normal historical evidence beneath a fresh
    /// protected successor prefix; never creates a reader's current trust pin.
    pub fn restore_first_normal_with_durable_set(
        &mut self,
        original: &VerifiedAuthorityV1,
        activation: &VerifiedDurableSetActivationV1,
        exact: Option<&FormalHeadRefV1>,
    ) -> Result<VerifiedNativeHistoricalRestoreV1, FormalError> {
        let position = activation
            .boundary()
            .head()
            .position
            .checked_add(1)
            .ok_or_else(|| invalid("healthy first Normal position overflow"))?;
        if exact.is_some_and(|head| head.position != position) {
            return Err(invalid("healthy first Normal exact selector position"));
        }
        self.restore_native_position(
            position,
            exact.map(|head| head.record_id.as_str()),
            activation.successor(),
            original,
            None,
            Some(activation),
            None,
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "historical restoration binds target and authority contexts, optional verified recovery or durable activation, and bounded content cache"
    )]
    fn restore_native_position(
        &mut self,
        position: u64,
        record_id: Option<&str>,
        historical_authority: &VerifiedAuthorityV1,
        original: &VerifiedAuthorityV1,
        recovery: Option<&VerifiedRecovery>,
        durable_set: Option<&VerifiedDurableSetActivationV1>,
        cache: Option<&dyn super::super::super::business_index::BusinessIndexNodeReader>,
    ) -> Result<VerifiedNativeHistoricalRestoreV1, FormalError> {
        let previous_deadline = self.operation_deadline;
        if previous_deadline.is_none() {
            self.set_operation_deadline(Instant::now() + self.timeout);
        }
        let mut local_cache = content_cache::RestoreContentCache::default();
        let result = (|| {
            self.complete(Command::Head)?;
            let context = self
                .last_native_context
                .clone()
                .ok_or_else(|| invalid("historical missing authenticated native context"))?;
            let start = context.clone();

            let restore_started = Instant::now();
            let restored = super::super::super::endpoint::restore_native_position(
                context,
                position,
                record_id,
                historical_authority,
                original,
                recovery,
                durable_set,
                &mut |id: &str| {
                    self.remaining_timeout(
                        self.operation_deadline
                            .ok_or_else(|| invalid("historical deadline absent"))?,
                    )?;
                    if let Some(bytes) = local_cache.get_checked(id, || {
                        self.remaining_timeout(
                            self.operation_deadline
                                .ok_or_else(|| invalid("historical deadline absent"))?,
                        )
                        .map(|_| ())
                    })? {
                        return Ok(bytes);
                    }
                    if let Some(cache) = cache
                        && let Some(bytes) = cache.read_cached_node_for_read(id)?
                    {
                        hash_hex(id)?;
                        if blake3::hash(&bytes).to_hex().as_str() != id {
                            return Err(invalid("historical cached CAS object hash mismatch"));
                        }
                        self.remaining_timeout(
                            self.operation_deadline
                                .ok_or_else(|| invalid("historical deadline absent"))?,
                        )?;
                        local_cache.record_external_cache_hit();
                        local_cache.insert(id, &bytes)?;
                        return Ok(bytes);
                    }
                    let fetch_started = Instant::now();
                    let fetched = self.fetch_hash_object(id);
                    let bytes = match fetched {
                        Ok(Some(bytes)) => {
                            local_cache.record_source_read(
                                fetch_started.elapsed(),
                                Some(&bytes),
                                false,
                            );
                            bytes
                        }
                        Ok(None) => {
                            local_cache.record_source_read(fetch_started.elapsed(), None, true);
                            return Err(FormalError::HistoryUnavailable(
                                "HistoryUnavailable: historical referenced object missing".into(),
                            ));
                        }
                        Err(error) => {
                            local_cache.record_source_read(fetch_started.elapsed(), None, true);
                            return Err(error);
                        }
                    };
                    if let Some(cache) = cache {
                        cache.cache_content_node_for_read(id, &bytes)?;
                    }
                    local_cache.insert(id, &bytes)?;
                    self.remaining_timeout(
                        self.operation_deadline
                            .ok_or_else(|| invalid("historical deadline absent"))?,
                    )?;
                    Ok(bytes)
                },
            );
            local_cache.record_restore_elapsed(restore_started.elapsed());
            let restored = restored?;
            self.complete(Command::Head)?;
            let finish = self
                .last_native_context
                .as_ref()
                .ok_or_else(|| invalid("historical final protected context missing"))?;
            verify_historical_forward(&start, finish)?;
            // The immutable cut retains its START prefix witness. Later responses
            // already verified ancestry from our monotonically advanced minimum.
            self.remaining_timeout(
                self.operation_deadline
                    .ok_or_else(|| invalid("restore deadline absent"))?,
            )?;
            Ok(restored)
        })();
        local_cache.record_outcome(result.is_ok());
        #[cfg(test)]
        {
            self.last_native_restore_source_reads = local_cache.source_reads();
        }
        drop(local_cache);
        self.operation_deadline = previous_deadline;
        result
    }

    #[cfg(test)]
    pub(crate) fn last_native_restore_source_reads(&self) -> usize {
        self.last_native_restore_source_reads
    }
}

fn verify_historical_forward(
    start: &VerifiedNativeRpcContextV1,
    finish: &VerifiedNativeRpcContextV1,
) -> Result<(), FormalError> {
    if start.authority().definition() != finish.authority().definition()
        || start.authority().policy() != finish.authority().policy()
        || start.authority().activation_ref() != finish.authority().activation_ref()
        || start.endpoint_public_key() != finish.endpoint_public_key()
        || start.instance_digest() != finish.instance_digest()
        || start.resource_binding() != finish.resource_binding()
    {
        return Err(FormalError::Rollback(
            "historical protected identity/authority/activation/resource changed".into(),
        ));
    }
    verify_revision_advance(start.checkpoint(), finish.checkpoint())
}
fn verify_revision_advance(
    start: &fencing::EndpointCheckpointBindingV1,
    finish: &fencing::EndpointCheckpointBindingV1,
) -> Result<(), FormalError> {
    if finish.revision < start.revision || (finish.revision == start.revision && finish != start) {
        return Err(FormalError::Rollback(
            "historical protected checkpoint revision/commitment regression".into(),
        ));
    }
    // Higher head fork/epoch/resource evidence cannot enter these arguments:
    // every call independently verifies fixed authority/instance/custody and
    // the authenticated ancestry bridge from the prior response minimum.
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_historical_checkpoint_allows_forward_revision_but_not_rollback_or_same_revision_fork()
     {
        let start = fencing::EndpointCheckpointBindingV1 {
            revision: 7,
            commitment: "ab".repeat(32),
            head: crate::controlled_authority::replicated_protocol::HeadAnchor {
                position: 2,
                decision_hash: "cd".repeat(32),
                qualified: true,
            },
        };
        assert!(verify_revision_advance(&start, &start).is_ok());
        let mut next = start.clone();
        next.revision = 8;
        next.commitment = "ef".repeat(32);
        next.head.position = 3;
        assert!(verify_revision_advance(&start, &next).is_ok());
        next.revision = 6;
        assert!(verify_revision_advance(&start, &next).is_err());
        next.revision = 7;
        assert!(verify_revision_advance(&start, &next).is_err());
    }
}
