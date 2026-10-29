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
    archive: Arc<dyn BftSuccessorArchiveStoreV1>,
    adoption: Option<Arc<dyn BftSuccessorWindowDriverVerifierV1>>,
    timeout: Duration,
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
        })
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
        let mut predecessor = None;
        for (index, record) in extension.requests.iter().enumerate() {
            live(deadline)?;
            let original = self
                .archive
                .load_public_state(&record.original_public_state)?;
            record
                .original_public_state
                .validate_bytes(&encode_bounded(&original, BFT_SIGNER_MAX_FRAME_BYTES_V3)?)?;
            let previous_minimum = predecessor
                .as_ref()
                .map(|token: &VerifiedBftSuccessorWindowV1| token.body().minimum())
                .transpose()?;
            let expected = &record.request.intent.body;
            if original.revision != expected.expected_revision
                || original.state_digest != expected.expected_state_digest
                || original.successor_window.as_ref().is_some_and(|previous| {
                    previous.reservation.is_some()
                        || previous.requests.as_slice() != &extension.requests[..index]
                        || previous.minimum != previous_minimum
                })
                || (index != 0 && original.successor_window.is_none())
            {
                return Err(invalid(
                    "successor archive differs from exact pre-prepare state",
                ));
            }
            let original = validate_state(binding, Some(provider), &original)?;
            validate_actual_public_manifest(&record.manifest, &original.state)?;
            let token = record.request.intent.body.checkpoint.bind_verified_state(
                binding,
                &original,
                predecessor.as_ref(),
            )?;
            if record.request.status == BftSuccessorWindowStatusV1::Adopted {
                predecessor = Some(token);
            }
        }
        if let Some(reservation) = &extension.reservation {
            let original = reservation.original_state(binding, state, provider)?;
            validate_actual_public_manifest(&reservation.manifest, &original.state)?;
            reservation.body.checkpoint.bind_verified_state(
                binding,
                &original,
                predecessor.as_ref(),
            )?;
        }
        live(deadline)?;
        Ok(predecessor)
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
    pub(super) fn verify_successor_archives(
        &self,
        state: &BftSignerStateV3,
        deadline: Option<Instant>,
    ) -> Result<(), BftSignerErrorV3> {
        if state.successor_window.is_some() {
            let support = self
                .successor_support
                .as_ref()
                .ok_or_else(|| invalid("successor support is not configured"))?;
            let deadline = match deadline {
                Some(value) => value,
                None => support.deadline(Instant::now())?,
            };
            support.verify_archives(
                &self.binding,
                state,
                &self.provider_key.public_key_hex(),
                deadline,
            )?;
        }
        Ok(())
    }
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
                {
                    if reservation.body.checkpoint.request_id == *request_id {
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
        let original = reserved.original_state(
            &self.binding,
            &snapshot.state,
            &self.provider_key.public_key_hex(),
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
        let next = previous
            .state
            .with_successor_window(&self.binding, extension)?;
        let verified = validate_successor_only_transition(
            &self.binding,
            verified,
            &next,
            &self.provider_key.public_key_hex(),
        )?;
        self.sample_time(runtime, credential, token_digest)?;
        live(deadline)?;
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
        if snapshot.state != next
            || snapshot.lease_generation != credential.generation
            || snapshot.lease_token_digest != token_digest
            || snapshot.protected_time_floor_ms < runtime.observed_time_floor_ms
        {
            runtime.poisoned = true;
            return Err(invalid(
                "successor CAS result differs from exact original transition",
            ));
        }
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
