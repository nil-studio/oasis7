//! Successor protected transactions. This stage intentionally retains the full
//! H prefix; no history eviction, rebasing, or height-65 permission is provided.
use super::*;
use crate::protected_bft_v3::successor_archive_store::BftSuccessorArchiveStoreV1;
use crate::protected_bft_v3::successor_public_archive::validate_actual_public_manifest;
use crate::protected_bft_v3::successor_state::*;
use crate::protected_bft_v3::successor_window::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Trusted deployment capability, not a deserializable caller assertion.
/// Implementations must independently reopen and verify the exact driver
/// pointer/WAL/private closure and readback state, under the same deadline.
pub trait BftSuccessorWindowDriverVerifierV1: Send + Sync {
    /// Independently validate the exact same-cut private closure before the
    /// protected minimum can advance. A caller DTO is not this capability.
    fn verify_prepared(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        public_state: &BftSignerStateV3,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3>;
    fn verify_adopted(
        &self,
        original: &BftSuccessorWindowStoredRequestV1,
        installed: &BftSignerStateV3,
        evidence: &BftSuccessorWindowAdoptedEvidenceV1,
        deadline: Instant,
    ) -> Result<(), BftSignerErrorV3>;
}
pub struct BftSuccessorWindowSupportV1 {
    pub(super) archive: Arc<dyn BftSuccessorArchiveStoreV1>,
    adoption: Option<Arc<dyn BftSuccessorWindowDriverVerifierV1>>,
    timeout: Duration,
    active_window_rebase: bool,
}
impl BftSuccessorWindowSupportV1 {
    pub fn new(
        archive: Arc<dyn BftSuccessorArchiveStoreV1>,
        timeout: Duration,
    ) -> Result<Self, BftSignerErrorV3> {
        if timeout.is_zero() || timeout > Duration::from_secs(120) {
            return Err(invalid("successor attempt timeout bounds"));
        }
        Ok(Self {
            archive,
            adoption: None,
            timeout,
            active_window_rebase: false,
        })
    }
    /// Explicit provider capability. All original archive objects remain retained;
    /// this only changes the protected hot replay representation after Adopt.
    pub fn with_active_window_rebase(mut self) -> Self {
        self.active_window_rebase = true;
        self
    }
    pub(super) fn validate_backend_capability(
        &self,
        snapshot: &BftSignerBackendSnapshotV3,
    ) -> Result<(), BftSignerErrorV3> {
        if self.active_window_rebase && snapshot.successor_archive_transfer.is_none() {
            return Err(invalid(
                "active window requires explicitly provisioned archive transfer custody",
            ));
        }
        if snapshot.state.active_window.is_some() && !self.active_window_rebase {
            return Err(invalid(
                "active window custody cannot downgrade provider capability",
            ));
        }
        Ok(())
    }
    pub(super) fn deadline(&self, started: Instant) -> Result<Instant, BftSignerErrorV3> {
        started
            .checked_add(self.timeout)
            .ok_or_else(|| invalid("successor attempt deadline overflow"))
    }
    pub fn with_driver_verifier(
        mut self,
        verifier: Arc<dyn BftSuccessorWindowDriverVerifierV1>,
    ) -> Self {
        self.adoption = Some(verifier);
        self
    }
    pub(super) fn verify_archives(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        provider: &str,
        deadline: Instant,
    ) -> Result<Option<VerifiedBftSuccessorWindowV1>, BftSignerErrorV3> {
        let Some(extension) = &state.successor_window else {
            return Ok(None);
        };
        extension.validate(binding, Some(provider))?;
        let mut chain =
            crate::protected_bft_v3::active_window::BftSuccessorArchiveChainVerifierV1::new();
        for record in &extension.requests {
            live(deadline)?;
            let original = self
                .archive
                .load_public_state(&record.original_public_state)?;
            chain.verify_next(binding, provider, record, &original)?;
        }
        if let Some(reservation) = &extension.reservation {
            let original = reservation.original_state_with_context(
                binding,
                state,
                provider,
                chain.context().cloned(),
            )?;
            validate_actual_public_manifest(&reservation.manifest, &original.state)?;
            reservation.body.checkpoint.bind_verified_state(
                binding,
                &original,
                chain.context().map(|c| c.checkpoint()),
            )?;
        }
        let predecessor = chain.context().map(|c| c.checkpoint().clone());
        live(deadline)?;
        Ok(predecessor)
    }
    pub(super) fn find_archived_result<T: PartialEq>(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        provider: &str,
        deadline: Instant,
        mut resolve: impl FnMut(&BftSignerStateV3) -> Option<T>,
    ) -> Result<Option<T>, BftSignerErrorV3> {
        let mut chain =
            crate::protected_bft_v3::active_window::BftSuccessorArchiveChainVerifierV1::new();
        let mut result = None;
        if let Some(extension) = &state.successor_window {
            for record in &extension.requests {
                live(deadline)?;
                let original = self
                    .archive
                    .load_public_state(&record.original_public_state)?;
                chain.verify_next(binding, provider, record, &original)?;
                if let Some(found) = resolve(&original) {
                    if result.as_ref().is_some_and(|previous| previous != &found) {
                        return Err(invalid("archived original result conflict"));
                    }
                    result = Some(found);
                }
            }
        }
        live(deadline)?;
        Ok(result)
    }
    pub(super) fn validate_public_state(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        provider: &str,
        deadline: Instant,
    ) -> Result<VerifiedBftSignerStateV3, BftSignerErrorV3> {
        if state.active_window.is_some() && !self.active_window_rebase {
            return Err(invalid(
                "active window custody requires explicit provider rebase capability",
            ));
        }
        let mut chain =
            crate::protected_bft_v3::active_window::BftSuccessorArchiveChainVerifierV1::new();
        if let Some(extension) = &state.successor_window {
            extension.validate(binding, Some(provider))?;
            for record in &extension.requests {
                live(deadline)?;
                let original = self
                    .archive
                    .load_public_state(&record.original_public_state)?;
                chain.verify_next(binding, provider, record, &original)?;
            }
        }
        // This observation authenticates both committed originals and any
        // in-progress reservation from the same immutable backend snapshot.
        // Callers must not traverse the complete chain a second time merely
        // to validate the reservation's reconstructed original State.
        if let Some(reservation) = state
            .successor_window
            .as_ref()
            .and_then(|window| window.reservation.as_ref())
        {
            let original = reservation.original_state_with_context(
                binding,
                state,
                provider,
                chain.context().cloned(),
            )?;
            validate_actual_public_manifest(&reservation.manifest, &original.state)?;
            reservation.body.checkpoint.bind_verified_state(
                binding,
                &original,
                chain.context().map(|context| context.checkpoint()),
            )?;
        }
        live(deadline)?;
        let verified = crate::protected_bft_v3::state::validate_state_with_active_context(
            binding,
            Some(provider),
            state,
            chain.context().cloned(),
        )?;
        live(deadline)?;
        Ok(verified)
    }
}
fn live(deadline: Instant) -> Result<(), BftSignerErrorV3> {
    if Instant::now() >= deadline {
        Err(invalid("successor attempt deadline expired"))
    } else {
        Ok(())
    }
}
impl BftSignerServiceV3 {
    /// Entered only after the enclosing SignedRequest and current lease have
    /// passed the existing service dispatch checks. No new requester identity.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn successor_command(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        credential: &BftSignerLeaseCredentialV3,
        token_digest: &str,
        mut snapshot: BftSignerBackendSnapshotV3,
        request: &SignedBftSignerRequestV3,
        request_digest: &str,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        let support = self
            .successor_support
            .as_ref()
            .ok_or_else(|| invalid("successor support is not configured"))?;
        let deadline = runtime
            .successor_deadline
            .ok_or_else(|| invalid("successor request deadline missing"))?;
        live(deadline)?;
        let provider = self.provider_key.public_key_hex();
        match &request.body.command {
            BftSignerCommandV3::PrepareSuccessorWindow { body, manifest } => {
                body.validate()?;
                manifest.validate_for_checkpoint(&body.checkpoint)?;
                if manifest.kind != BftSuccessorWindowArchiveKindV1::Public {
                    return Err(invalid("provider successor manifest must be public"));
                }
                body.checkpoint.source.validate_for_binding(&self.binding)?;
                if let Some(record) = snapshot
                    .state
                    .successor_window
                    .as_ref()
                    .and_then(|w| w.request(&body.checkpoint.request_id))
                    .cloned()
                {
                    if record.request.intent.body != **body || record.manifest != **manifest {
                        return Err(invalid("successor original prepare request reused"));
                    }
                    live(deadline)?;
                    return self.successor_reply(snapshot, request, request_digest, Some(record));
                }
                let mut extension = snapshot
                    .state
                    .successor_window
                    .clone()
                    .unwrap_or_else(BftSuccessorWindowStateV1::empty);
                if let Some(original) = &extension.reservation {
                    if original.body != **body || original.manifest != **manifest {
                        return Err(invalid(
                            "successor different original reservation is pending",
                        ));
                    }
                } else {
                    if extension.blocks_writes()
                        || extension.requests.len() >= 64
                        || body.expected_revision != snapshot.state.revision
                        || body.expected_state_digest != snapshot.state.state_digest
                        || body.expected_lease_generation != credential.generation
                    {
                        return Err(invalid(
                            "successor preparation expected state/lease or capacity",
                        ));
                    }
                    let predecessor = support.verify_archives(
                        &self.binding,
                        &snapshot.state,
                        &provider,
                        deadline,
                    )?;
                    body.checkpoint.bind_verified_state(
                        &self.binding,
                        runtime
                            .verified
                            .as_ref()
                            .ok_or_else(|| invalid("successor verified state missing"))?,
                        predecessor.as_ref(),
                    )?;
                    let bytes = encode_bounded(&snapshot.state, BFT_SIGNER_MAX_FRAME_BYTES_V3)?;
                    let reference = BftSuccessorWindowArchiveRefV1 {
                        digest: blake3::hash(&bytes).to_hex().to_string(),
                        bytes: bytes.len() as u64,
                    };
                    validate_actual_public_manifest(manifest, &snapshot.state)?;
                    body.checkpoint.usage.validate_after(&extension.usage)?;
                    if body
                        .checkpoint
                        .usage
                        .total_bytes
                        .checked_sub(extension.usage.total_bytes)
                        .is_none_or(|n| n < reference.bytes)
                        || body.checkpoint.usage.objects <= extension.usage.objects
                    {
                        return Err(invalid(
                            "successor reservation budget omits original material",
                        ));
                    }
                    let previous = snapshot
                        .state
                        .successor_window
                        .as_ref()
                        .map(BftSuccessorWindowStateV1::prior)
                        .transpose()?;
                    extension.usage = body.checkpoint.usage;
                    extension.active_request_id = Some(body.checkpoint.request_id.clone());
                    extension.reservation = Some(BftSuccessorWindowReservationV1 {
                        body: (**body).clone(),
                        manifest: (**manifest).clone(),
                        original_public_state: reference,
                        previous,
                    });
                    // Charge the failed/resumable reservation before any material write.
                    snapshot = self.commit_successor_state(
                        runtime,
                        credential,
                        token_digest,
                        &snapshot,
                        extension,
                        deadline,
                    )?;
                }
                snapshot = self.finish_successor_prepare(
                    runtime,
                    credential,
                    token_digest,
                    snapshot,
                    deadline,
                )?;
                let record = snapshot
                    .state
                    .successor_window
                    .as_ref()
                    .and_then(|w| w.request(&body.checkpoint.request_id))
                    .cloned()
                    .ok_or_else(|| invalid("successor prepared original missing"))?;
                self.successor_reply(snapshot, request, request_digest, Some(record))
            }
            BftSignerCommandV3::GetSuccessorWindow {
                request_id,
                checkpoint_digest,
            }
            | BftSignerCommandV3::CommitSuccessorWindow {
                request_id,
                checkpoint_digest,
            }
            | BftSignerCommandV3::ConfirmSuccessorWindowAdopted {
                request_id,
                checkpoint_digest,
                ..
            } => {
                if request_id.is_empty()
                    || request_id.len() > 256
                    || request_id.trim() != request_id
                {
                    return Err(invalid("successor request identity bounds"));
                }
                super::super::state::validate_digest(checkpoint_digest)?;
                if let Some(reservation) = snapshot
                    .state
                    .successor_window
                    .as_ref()
                    .and_then(|w| w.reservation.as_ref())
                    && reservation.body.checkpoint.request_id == *request_id
                {
                    if reservation.body.checkpoint.digest()? != *checkpoint_digest {
                        return Err(invalid("successor reserved body conflict"));
                    }
                    snapshot = self.finish_successor_prepare(
                        runtime,
                        credential,
                        token_digest,
                        snapshot,
                        deadline,
                    )?;
                }
                let mut extension = snapshot
                    .state
                    .successor_window
                    .clone()
                    .unwrap_or_else(BftSuccessorWindowStateV1::empty);
                let Some(index) = extension
                    .requests
                    .iter()
                    .position(|r| r.request.intent.body.checkpoint.request_id == *request_id)
                else {
                    if matches!(
                        &request.body.command,
                        BftSignerCommandV3::GetSuccessorWindow { .. }
                    ) {
                        live(deadline)?;
                        return self.successor_reply(snapshot, request, request_digest, None);
                    }
                    return Err(invalid("successor original request not found"));
                };
                let original = extension.requests[index].clone();
                if original.request.intent.body.checkpoint.digest()? != *checkpoint_digest {
                    return Err(invalid("successor original checkpoint digest conflict"));
                }
                match &request.body.command {
                    BftSignerCommandV3::GetSuccessorWindow { .. } => {}
                    BftSignerCommandV3::CommitSuccessorWindow { .. } => {
                        if original.request.status == BftSuccessorWindowStatusV1::Prepared {
                            if extension.active_request_id.as_deref() != Some(request_id.as_str())
                                || snapshot.state.revision != original.prepared_revision
                            {
                                return Err(invalid(
                                    "successor original prepared state was changed",
                                ));
                            }
                            support.verify_archives(
                                &self.binding,
                                &snapshot.state,
                                &provider,
                                deadline,
                            )?;
                            live(deadline)?;
                            let verifier = support.adoption.as_ref().ok_or_else(|| {
                                invalid("successor private closure verifier is not configured")
                            })?;
                            let public_state = support
                                .archive
                                .load_public_state(&original.original_public_state)?;
                            original
                                .original_public_state
                                .validate_bytes(&encode_bounded(
                                    &public_state,
                                    BFT_SIGNER_MAX_FRAME_BYTES_V3,
                                )?)?;
                            verifier.verify_prepared(&original, &public_state, deadline)?;
                            live(deadline)?;
                            let body = &original.request.intent.body.checkpoint;
                            let receipt = BftSuccessorWindowInstallReceiptV1::sign(
                                BftSuccessorWindowInstallReceiptBodyV1 {
                                    schema_version: 1,
                                    source: body.source.clone(),
                                    request_id: request_id.clone(),
                                    checkpoint_digest: checkpoint_digest.clone(),
                                    intent_digest: original.request.intent.digest()?,
                                    minimum: body.minimum()?,
                                    lease_generation: credential.generation,
                                    committed_revision: snapshot
                                        .state
                                        .revision
                                        .checked_add(1)
                                        .ok_or_else(|| {
                                            invalid("successor commit revision overflow")
                                        })?,
                                    driver_install_digest: body.cut.driver_install_digest.clone(),
                                },
                                self.provider_key.as_ref(),
                            )?;
                            receipt.verify_for(&original.request.intent, &provider)?;
                            extension.minimum = Some(body.minimum()?);
                            extension.requests[index].request.status =
                                BftSuccessorWindowStatusV1::InstalledPending;
                            extension.requests[index].request.receipt = Some(receipt);
                            snapshot = self.commit_successor_state(
                                runtime,
                                credential,
                                token_digest,
                                &snapshot,
                                extension,
                                deadline,
                            )?;
                        }
                    }
                    BftSignerCommandV3::ConfirmSuccessorWindowAdopted { evidence, .. } => {
                        if original.request.status == BftSuccessorWindowStatusV1::Adopted {
                            if original.request.adopted_evidence.as_ref() != Some(evidence.as_ref())
                            {
                                return Err(invalid(
                                    "successor adopted original evidence conflict",
                                ));
                            }
                        } else {
                            if original.request.status
                                != BftSuccessorWindowStatusV1::InstalledPending
                                || evidence.readback_state_digest != snapshot.state.state_digest
                            {
                                return Err(invalid(
                                    "successor adoption lacks exact installed state",
                                ));
                            }
                            evidence.validate_active_segment_origin()?;
                            if support.active_window_rebase
                                && evidence.active_segment_origin.is_none()
                            {
                                return Err(invalid(
                                    "active rebase requires verified WindowOrigin readback commitment",
                                ));
                            }
                            let verifier = support.adoption.as_ref().ok_or_else(|| {
                                invalid("successor trusted driver readback verifier unavailable")
                            })?;
                            support.verify_archives(
                                &self.binding,
                                &snapshot.state,
                                &provider,
                                deadline,
                            )?;
                            live(deadline)?;
                            verifier.verify_adopted(
                                &original,
                                &snapshot.state,
                                evidence,
                                deadline,
                            )?;
                            live(deadline)?;
                            extension.requests[index].request.status =
                                BftSuccessorWindowStatusV1::Adopted;
                            extension.requests[index].request.adopted_evidence =
                                Some((**evidence).clone());
                            snapshot = self.commit_successor_state(
                                runtime,
                                credential,
                                token_digest,
                                &snapshot,
                                extension,
                                deadline,
                            )?;
                        }
                    }
                    _ => unreachable!(),
                }
                live(deadline)?;
                let record = snapshot
                    .state
                    .successor_window
                    .as_ref()
                    .and_then(|w| w.request(request_id))
                    .cloned();
                self.successor_reply(snapshot, request, request_digest, record)
            }
            _ => Err(invalid("unsupported successor command")),
        }
    }
    fn finish_successor_prepare(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        credential: &BftSignerLeaseCredentialV3,
        token_digest: &str,
        snapshot: BftSignerBackendSnapshotV3,
        deadline: Instant,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let support = self
            .successor_support
            .as_ref()
            .ok_or_else(|| invalid("successor support unavailable"))?;
        let mut extension = snapshot
            .state
            .successor_window
            .clone()
            .ok_or_else(|| invalid("successor reservation missing"))?;
        let reserved = extension
            .reservation
            .as_ref()
            .ok_or_else(|| invalid("successor original reservation missing"))?
            .clone();
        if snapshot.state.revision
            != reserved
                .body
                .expected_revision
                .checked_add(1)
                .ok_or_else(|| invalid("successor reservation revision overflow"))?
            || credential.generation < reserved.body.expected_lease_generation
        {
            return Err(invalid("successor reservation state or lease changed"));
        }
        let original = reserved.original_state_with_context(
            &self.binding,
            &snapshot.state,
            &self.provider_key.public_key_hex(),
            runtime
                .verified
                .as_ref()
                .and_then(|v| v.active_context.clone()),
        )?;
        validate_actual_public_manifest(&reserved.manifest, &original.state)?;
        live(deadline)?;
        let reference = support.archive.persist_public_state(&original.state)?;
        live(deadline)?;
        if reference != reserved.original_public_state
            || support.archive.load_public_state(&reference)? != original.state
        {
            return Err(invalid(
                "successor exact original public material readback failed",
            ));
        }
        live(deadline)?;
        let intent =
            BftSuccessorWindowInstallIntentV1::sign(reserved.body, self.provider_key.as_ref())?;
        let record = BftSuccessorWindowStoredRequestV1 {
            original_public_state: reference,
            manifest: reserved.manifest,
            prepared_revision: snapshot
                .state
                .revision
                .checked_add(1)
                .ok_or_else(|| invalid("successor prepared revision overflow"))?,
            request: BftSuccessorWindowPersistedRequestV1 {
                intent,
                status: BftSuccessorWindowStatusV1::Prepared,
                receipt: None,
                adopted_evidence: None,
                superseded_reason: None,
            },
        };
        extension.reservation = None;
        extension.requests.push(record);
        self.commit_successor_state(
            runtime,
            credential,
            token_digest,
            &snapshot,
            extension,
            deadline,
        )
    }
    fn commit_successor_state(
        &self,
        runtime: &mut BftSignerServiceRuntimeV3,
        credential: &BftSignerLeaseCredentialV3,
        token_digest: &str,
        previous: &BftSignerBackendSnapshotV3,
        extension: BftSuccessorWindowStateV1,
        deadline: Instant,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        live(deadline)?;
        let verified = runtime
            .verified
            .as_ref()
            .ok_or_else(|| invalid("successor verified state missing"))?;
        if verified.state != previous.state {
            runtime.poisoned = true;
            return Err(invalid("successor CAS previous snapshot differs"));
        }
        let adopting = previous
            .state
            .successor_window
            .as_ref()
            .and_then(|w| w.active_request_id.as_ref().and_then(|id| w.request(id)))
            .zip(
                extension
                    .active_request_id
                    .as_ref()
                    .and_then(|id| extension.request(id)),
            )
            .is_some_and(|(a, b)| {
                a.request.status == BftSuccessorWindowStatusV1::InstalledPending
                    && b.request.status == BftSuccessorWindowStatusV1::Adopted
            });
        let adopting = adopting
            && self
                .successor_support
                .as_ref()
                .is_some_and(|s| s.active_window_rebase);
        if adopting && previous.successor_archive_transfer.is_none() {
            return Err(invalid(
                "active rebase requires explicitly provisioned archive transfer ledger",
            ));
        }
        let context = if adopting {
            let support = self
                .successor_support
                .as_ref()
                .ok_or_else(|| invalid("successor support unavailable"))?;
            let mut chain =
                crate::protected_bft_v3::active_window::BftSuccessorArchiveChainVerifierV1::new();
            for record in &extension.requests {
                live(deadline)?;
                let original = support
                    .archive
                    .load_public_state(&record.original_public_state)?;
                chain.verify_next(
                    &self.binding,
                    &self.provider_key.public_key_hex(),
                    record,
                    &original,
                )?;
            }
            chain.context().cloned()
        } else {
            verified.active_context.clone()
        };
        let next = previous.state.with_successor_window_context(
            &self.binding,
            extension,
            context.clone(),
            adopting,
        )?;
        let verified = validate_successor_only_transition_with_context(
            &self.binding,
            verified,
            &next,
            &self.provider_key.public_key_hex(),
            context,
        )?;
        self.sample_time(runtime, credential, token_digest)?;
        live(deadline)?;
        crate::protected_bft_v3::validate_successor_archive_budget(
            &next,
            runtime.observed_archive_transfer.as_ref(),
        )?;
        let result = runtime.backend.compare_and_swap(
            credential.generation,
            token_digest,
            previous.state.revision,
            &previous.state.state_digest,
            None,
            next.clone(),
        );
        let snapshot = match result {
            Ok(value) => value,
            Err(reason) => {
                runtime.poisoned = true;
                return Err(invalid(format!(
                    "successor CAS outcome unknown; provider poisoned: {reason}"
                )));
            }
        };
        if snapshot.successor_archive_transfer != runtime.observed_archive_transfer
            || snapshot.state != next
            || snapshot.lease_generation != credential.generation
            || snapshot.lease_token_digest != token_digest
            || snapshot.protected_time_floor_ms < runtime.observed_time_floor_ms
        {
            runtime.poisoned = true;
            return Err(invalid(
                "successor CAS result differs from exact original transition",
            ));
        }
        runtime.observe_archive_transfer(&snapshot)?;
        runtime.verified = Some(verified);
        runtime.observed_generation = snapshot.lease_generation;
        runtime.observed_time_floor_ms = snapshot.protected_time_floor_ms;
        if let Err(reason) = live(deadline) {
            runtime.poisoned = true;
            return Err(reason);
        }
        Ok(snapshot)
    }
    fn successor_reply(
        &self,
        snapshot: BftSignerBackendSnapshotV3,
        request: &SignedBftSignerRequestV3,
        digest: &str,
        record: Option<BftSuccessorWindowStoredRequestV1>,
    ) -> Result<ReplyDataV3, BftSignerErrorV3> {
        self.state_reply(
            snapshot,
            request,
            digest,
            BftSignerReplyKindV3::SuccessorWindow {
                record: record.map(Box::new),
            },
        )
    }
}
