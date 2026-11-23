use super::*;
use crate::controlled_authority::LocalRequestIdentity;
use crate::controlled_authority::replicated_protocol::HeadAnchor;

impl FileFormalEndpoint {
    pub fn checkpoint(&mut self) -> Result<fencing::EndpointCheckpointBindingV1, FormalError> {
        let _guard = self.guard()?;
        Ok(self.store.checkpoint.clone())
    }
    /// Native seal and appends share one external guard. The exact tail is
    /// captured from the authenticated checkpoint, never operator-supplied JSON.
    pub fn seal_epoch(
        &mut self,
        operation: &str,
        next_epoch: u64,
        expected: &fencing::EndpointCheckpointBindingV1,
    ) -> Result<fencing::EpochSealReceiptV1, FormalError> {
        if self.authority.definition().profile_version == 2 {
            return Err(invalid(
                "native profile2 seal requires the independent writer epoch seal",
            ));
        }
        self.seal_epoch_inner(operation, next_epoch, expected, None)
    }

    pub fn seal_planned_epoch(
        &mut self,
        writer_seal: SignedWriterEpochSealV1,
        expected: &fencing::EndpointCheckpointBindingV1,
    ) -> Result<fencing::EpochSealReceiptV1, FormalError> {
        if self.authority.definition().profile_version != 2 {
            return Err(invalid("planned native recovery requires profile2"));
        }
        let operation = writer_seal.body.operation_id.clone();
        self.seal_epoch_inner(
            &operation,
            writer_seal.body.next_epoch,
            expected,
            Some(writer_seal),
        )
    }

    fn seal_epoch_inner(
        &mut self,
        operation: &str,
        next_epoch: u64,
        expected: &fencing::EndpointCheckpointBindingV1,
        writer_seal: Option<SignedWriterEpochSealV1>,
    ) -> Result<fencing::EpochSealReceiptV1, FormalError> {
        let mut guard = self.guard()?;
        let plan = self.store.body.recovery_plan.clone();
        if self.authority.definition().profile_version == 2 {
            let plan = plan.as_ref().ok_or_else(|| {
                invalid("native profile2 seal requires the original persisted recovery plan")
            })?;
            plan.verify(&self.authority)?;
            let ack = self.store.body.writer_plan_ack.as_ref().ok_or_else(|| {
                invalid("native recovery original writer acknowledgement missing")
            })?;
            let writer = writer_seal
                .as_ref()
                .ok_or_else(|| invalid("native recovery independent writer seal missing"))?;
            writer.verify_with_ack(plan, &self.authority, ack)?;
            let drained = self
                .store
                .body
                .recovery_drained
                .as_ref()
                .or_else(|| {
                    self.store
                        .body
                        .seal
                        .as_ref()
                        .and_then(|seal| seal.writer_seal.as_ref())
                        .map(|seal| &seal.drained)
                })
                .ok_or_else(|| {
                    invalid("native recovery cannot seal before actual old-epoch drain")
                })?;
            if plan.body.intent.operation_id != operation
                || plan.body.intent.successor.epoch != next_epoch
                || writer.drained != *drained
                || drained.verify(plan, &self.authority)? != self.store.body.head
                || self.store.body.pending.is_some()
                || self.store.body.recovery_progress.is_some()
            {
                return Err(FormalError::Conflict(
                    "seal differs from the original recovery plan/tail".into(),
                ));
            }
        }
        if let Some(body) = &self.store.body.seal {
            if body.recovery_operation != operation
                || body.next_epoch_floor != next_epoch
                || body.writer_seal != writer_seal
            {
                return Err(FormalError::Conflict(
                    "formal original epoch seal operation/floor".into(),
                ));
            }
            return self.signed_seal(body.clone(), guard.as_mut());
        }
        if expected != &self.store.checkpoint {
            return Err(FormalError::Conflict("formal stale seal checkpoint".into()));
        }
        let mut tail = fencing::SealedTailV1::empty();
        if let Some(id) = &self.store.body.pending {
            let entry: FormalRequestStateV1 = self.store.get(id)?;
            // This annotation retains the formal digest, not a fabricated user
            // identity. Reservation identity is the independently signed body.
            tail.request = Some(LocalRequestIdentity {
                verified_subject: "formal-request-digest".into(),
                operation_domain: "oasis7.formal.request.v1".into(),
                nonce_scope: self.authority.authority_id()?,
                request_id: entry.request_digest().into(),
            });
            tail.position = Some(entry.position());
            match entry {
                FormalRequestStateV1::Frozen {
                    input, certificate, ..
                } => {
                    tail.kind = fencing::SealedTailKindV1::Frozen;
                    tail.frozen_input_digest = Some(input.body.reservation_id()?);
                    tail.frozen_certificate_digest = certificate
                        .as_ref()
                        .map(|cert| hash("oasis7.formal.frozen_certificate.v1", cert))
                        .transpose()?;
                }
                FormalRequestStateV1::Prepared { candidate, .. }
                | FormalRequestStateV1::Decided { candidate, .. }
                | FormalRequestStateV1::Ready { candidate, .. } => {
                    tail.kind = if self.store.body.decision_head.is_some() {
                        fencing::SealedTailKindV1::Decision
                    } else {
                        fencing::SealedTailKindV1::Prepared
                    };
                    tail.proposal_digest = Some(candidate.candidate.record.record_id()?);
                    if let Some(cert) = candidate.frozen {
                        tail.frozen_input_digest = Some(cert.reservation.body.reservation_id()?);
                        tail.frozen_certificate_digest =
                            Some(hash("oasis7.formal.frozen_certificate.v1", &cert)?);
                    }
                }
                FormalRequestStateV1::Qualified { .. } => {
                    return Err(invalid("formal qualified entry remains pending"));
                }
            }
        }
        let body = fencing::EpochSealBodyV1 {
            schema_version: if plan.is_some() { 2 } else { 1 },
            recovery_plan: plan,
            writer_seal,
            instance: self.instance().clone(),
            recovery_operation: operation.into(),
            highest_complete: HeadAnchor {
                position: self.store.body.head.position,
                decision_hash: self.store.body.head.record_id.clone(),
                qualified: true,
            },
            checkpoint_before: self.store.checkpoint.clone(),
            tail,
            next_epoch_floor: next_epoch,
        };
        body.validate().map_err(fencing_error)?;
        let mut next = self.store.body.clone();
        next.seal = Some(body.clone());
        next.recovery_drained = None;
        if let Err(error) = self.publish_native(next, guard.as_mut()) {
            self.poisoned = true;
            return Err(error);
        }
        self.signed_seal(body, guard.as_mut())
    }
}
