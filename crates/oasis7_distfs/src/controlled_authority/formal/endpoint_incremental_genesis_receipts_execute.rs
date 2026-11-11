use super::*;

impl FileFormalEndpoint {
    pub fn prepare_genesis_bounded(
        &mut self,
        candidate: &FormalCandidateMaterialV1,
        context: &VerifiedCandidateContextV1,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> ResultV<PaidGenesisPrepareReceiptV1> {
        if let Some(state) = retry_state(self, candidate, context, memory, work, stop)? {
            match state.state() {
                FormalRequestStateV1::Prepared {
                    receipt: Some(receipt),
                    ..
                }
                | FormalRequestStateV1::Decided {
                    prepare: receipt, ..
                }
                | FormalRequestStateV1::Ready {
                    prepare: receipt, ..
                } => {
                    verify_prepare_one(receipt, self.role, candidate, context, memory, work, stop)?;
                    return Ok(copy_prepare(
                        &receipt.body,
                        &receipt.signature_hex,
                        memory,
                        work,
                        stop,
                    )?);
                }
                FormalRequestStateV1::Qualified { .. } => {
                    return Err(FormalError::Conflict(
                        "GEN original request already Qualified".into(),
                    )
                    .into());
                }
                _ => {}
            }
        }
        admit(
            self, candidate, context, provider, policy, memory, work, stop,
        )?;
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("GEN page capability missing"))?;
        let _job = capability.jobs.genesis_job()?;
        if let Some(local) = self.store.read_request_bounded(
            &candidate.candidate.record.request_digest,
            memory,
            work,
            stop,
        )? {
            match local.state() {
                FormalRequestStateV1::Prepared {
                    candidate: old,
                    receipt: Some(receipt),
                } if same(old, candidate, memory, work, stop)? => {
                    verify_prepare_one(receipt, self.role, candidate, context, memory, work, stop)?;
                    return Ok(copy_prepare(
                        &receipt.body,
                        &receipt.signature_hex,
                        memory,
                        work,
                        stop,
                    )?);
                }
                FormalRequestStateV1::Prepared { candidate: old, .. }
                    if same(old, candidate, memory, work, stop)? => {}
                _ => {
                    return Err(FormalError::Conflict(
                        "GEN prepare retained original state".into(),
                    )
                    .into());
                }
            }
        }
        let instance = match self.role {
            EndpointRole::Primary => &self.authority.definition().primary.instance,
            EndpointRole::Replica => &self.authority.definition().replica.instance,
        };
        persist(
            &mut self.store,
            Next::Prepared {
                candidate,
                receipt: None,
            },
            provider,
            instance,
            policy,
            memory,
            work,
            stop,
        )?;
        let body = prepare_body(candidate, context, self.role, memory, work, stop)?;
        let signed = sign(
            provider,
            source(
                ReceiptBody::Prepare(body),
                instance,
                &self.store,
                memory,
                work,
                stop,
            )?,
            policy,
            memory,
            work,
            stop,
        )?;
        let ReceiptBody::Prepare(body) = &signed.source().body else {
            unreachable!("private GEN prepare source")
        };
        let receipt = copy_prepare(body, signed.endpoint_signature_hex(), memory, work, stop)?;
        drop(signed);
        persist(
            &mut self.store,
            Next::Prepared {
                candidate,
                receipt: Some(receipt.receipt()),
            },
            provider,
            instance,
            policy,
            memory,
            work,
            stop,
        )?;
        Ok(receipt)
    }
    pub fn finalize_genesis_bounded(
        &mut self,
        candidate: &FormalCandidateMaterialV1,
        context: &VerifiedCandidateContextV1,
        primary: &FormalPrepareReceiptV1,
        replica: &FormalPrepareReceiptV1,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> ResultV<PaidGenesisFinalReceiptV1> {
        if let Some(state) = retry_state(self, candidate, context, memory, work, stop)? {
            let receipt = match state.state() {
                FormalRequestStateV1::Decided { receipt, .. } => Some(receipt),
                FormalRequestStateV1::Ready { decision, .. } => Some(match self.role {
                    EndpointRole::Primary => &decision.primary,
                    EndpointRole::Replica => &decision.replica,
                }),
                FormalRequestStateV1::Qualified { proof, .. } => Some(match self.role {
                    EndpointRole::Primary => &proof.decision.primary,
                    EndpointRole::Replica => &proof.decision.replica,
                }),
                _ => None,
            };
            if let Some(receipt) = receipt {
                verify_final(receipt, candidate, context, self.role, memory, work, stop)?;
                return Ok(copy_final(
                    &receipt.body,
                    &receipt.signature_hex,
                    memory,
                    work,
                    stop,
                )?);
            }
        }
        admit(
            self, candidate, context, provider, policy, memory, work, stop,
        )?;
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("GEN page capability missing"))?;
        let _job = capability.jobs.genesis_job()?;
        let local = self
            .store
            .read_request_bounded(
                &candidate.candidate.record.request_digest,
                memory,
                work,
                stop,
            )?
            .ok_or_else(|| invalid("GEN durable prepare missing"))?;
        let (old, prepare, already) = match local.state() {
            FormalRequestStateV1::Prepared {
                candidate,
                receipt: Some(prepare),
            } => (candidate, prepare, None),
            FormalRequestStateV1::Decided {
                candidate,
                prepare,
                receipt,
            } => (candidate, prepare, Some(receipt)),
            FormalRequestStateV1::Ready {
                candidate,
                prepare,
                decision,
                ..
            } => (
                candidate,
                prepare,
                Some(match self.role {
                    EndpointRole::Primary => &decision.primary,
                    EndpointRole::Replica => &decision.replica,
                }),
            ),
            _ => return Err(invalid("GEN durable prepare receipt missing").into()),
        };
        if !same(old, candidate, memory, work, stop)? {
            return Err(invalid("GEN retained candidate mismatch").into());
        }
        verify_prepare_pair(primary, replica, candidate, context, memory, work, stop)?;
        if !same(
            prepare,
            match self.role {
                EndpointRole::Primary => primary,
                EndpointRole::Replica => replica,
            },
            memory,
            work,
            stop,
        )? {
            return Err(invalid("formal decision original local prepare").into());
        }
        let instance = match self.role {
            EndpointRole::Primary => &self.authority.definition().primary.instance,
            EndpointRole::Replica => &self.authority.definition().replica.instance,
        };
        if let Some(receipt) = already {
            verify_final(receipt, candidate, context, self.role, memory, work, stop)?;
            return Ok(copy_final(
                &receipt.body,
                &receipt.signature_hex,
                memory,
                work,
                stop,
            )?);
        }
        let mut ctx = work.bounded_context(memory, stop);
        let body=oasis7_formal_protocol::controlled_authority::formal::genesis_final_receipt_body_bounded(&candidate.candidate,context,self.role,&mut ctx)?;
        drop(ctx);
        let signed = sign(
            provider,
            source(
                ReceiptBody::Final(body),
                instance,
                &self.store,
                memory,
                work,
                stop,
            )?,
            policy,
            memory,
            work,
            stop,
        )?;
        let ReceiptBody::Final(body) = &signed.source().body else {
            unreachable!("private GEN final source")
        };
        let receipt = copy_final(body, signed.endpoint_signature_hex(), memory, work, stop)?;
        drop(signed);
        persist(
            &mut self.store,
            Next::Decided {
                candidate,
                prepare,
                receipt: receipt.receipt(),
            },
            provider,
            instance,
            policy,
            memory,
            work,
            stop,
        )?;
        Ok(receipt)
    }
    pub fn complete_genesis_bounded(
        &mut self,
        candidate: &FormalCandidateMaterialV1,
        context: &VerifiedCandidateContextV1,
        decision: &FormalDecisionProofV1,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> ResultV<PaidGenesisCompleteReceiptV1> {
        if let Some(state) = retry_state(self, candidate, context, memory, work, stop)? {
            let retained = match state.state() {
                FormalRequestStateV1::Ready {
                    decision: old,
                    complete: Some(receipt),
                    ..
                } => Some((old, receipt)),
                FormalRequestStateV1::Qualified { proof, .. } => Some((
                    &proof.decision,
                    match self.role {
                        EndpointRole::Primary => &proof.primary_complete,
                        EndpointRole::Replica => &proof.replica_complete,
                    },
                )),
                _ => None,
            };
            if let Some((old, receipt)) = retained {
                if !same(old, decision, memory, work, stop)? {
                    return Err(
                        FormalError::Conflict("GEN retained original decision".into()).into(),
                    );
                }
                let mut ctx = work.bounded_context(memory, stop);
                let verified=oasis7_formal_protocol::controlled_authority::formal::verify_genesis_decision_bounded(decision,&candidate.candidate,context,&mut ctx)?;
                let expected = verified.complete_receipt_body_bounded(self.role, &mut ctx)?;
                drop(ctx);
                if !same(&receipt.body, &*expected, memory, work, stop)? {
                    return Err(invalid("GEN retained complete context mismatch").into());
                }
                let key = match self.role {
                    EndpointRole::Primary => {
                        &self
                            .authority
                            .definition()
                            .primary
                            .instance
                            .endpoint_public_key
                    }
                    EndpointRole::Replica => {
                        &self
                            .authority
                            .definition()
                            .replica
                            .instance
                            .endpoint_public_key
                    }
                };
                verify_body_signature(
                    &receipt.body,
                    &receipt.signature_hex,
                    key,
                    ProtectedEndpointSignaturePurposeV1::ProofComplete,
                    memory,
                    work,
                    stop,
                )?;
                return Ok(copy_complete(
                    &receipt.body,
                    &receipt.signature_hex,
                    memory,
                    work,
                    stop,
                )?);
            }
        }
        admit(
            self, candidate, context, provider, policy, memory, work, stop,
        )?;
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("GEN page capability missing"))?;
        let _job = capability.jobs.genesis_job()?;
        let local = self
            .store
            .read_request_bounded(
                &candidate.candidate.record.request_digest,
                memory,
                work,
                stop,
            )?
            .ok_or_else(|| invalid("GEN durable decision missing"))?;
        let (old, prepare, receipt, already) = match local.state() {
            FormalRequestStateV1::Decided {
                candidate,
                prepare,
                receipt,
            } => (candidate, prepare, receipt, None),
            FormalRequestStateV1::Ready {
                candidate,
                prepare,
                decision: old,
                complete,
            } if same(old, decision, memory, work, stop)? => (
                candidate,
                prepare,
                match self.role {
                    EndpointRole::Primary => &old.primary,
                    EndpointRole::Replica => &old.replica,
                },
                complete.as_ref(),
            ),
            _ => return Err(invalid("GEN original durable decision required").into()),
        };
        if !same(old, candidate, memory, work, stop)?
            || !same(
                receipt,
                match self.role {
                    EndpointRole::Primary => &decision.primary,
                    EndpointRole::Replica => &decision.replica,
                },
                memory,
                work,
                stop,
            )?
        {
            return Err(
                invalid("formal proof installation exact retained original decision").into(),
            );
        }
        let mut ctx = work.bounded_context(memory, stop);
        let verified =
            oasis7_formal_protocol::controlled_authority::formal::verify_genesis_decision_bounded(
                decision,
                &candidate.candidate,
                context,
                &mut ctx,
            )?;
        drop(ctx);
        let instance = match self.role {
            EndpointRole::Primary => &self.authority.definition().primary.instance,
            EndpointRole::Replica => &self.authority.definition().replica.instance,
        };
        if let Some(receipt) = already {
            let mut ctx = work.bounded_context(memory, stop);
            let expected = verified.complete_receipt_body_bounded(self.role, &mut ctx)?;
            drop(ctx);
            if !same(&receipt.body, &*expected, memory, work, stop)? {
                return Err(invalid("GEN retained complete context mismatch").into());
            }
            verify_body_signature(
                &receipt.body,
                &receipt.signature_hex,
                &instance.endpoint_public_key,
                ProtectedEndpointSignaturePurposeV1::ProofComplete,
                memory,
                work,
                stop,
            )?;
            return Ok(copy_complete(
                &receipt.body,
                &receipt.signature_hex,
                memory,
                work,
                stop,
            )?);
        }
        persist(
            &mut self.store,
            Next::Ready {
                candidate,
                prepare,
                decision,
                complete: None,
            },
            provider,
            instance,
            policy,
            memory,
            work,
            stop,
        )?;
        let mut ctx = work.bounded_context(memory, stop);
        let body = verified.complete_receipt_body_bounded(self.role, &mut ctx)?;
        drop(ctx);
        let signed = sign(
            provider,
            source(
                ReceiptBody::Complete(body),
                instance,
                &self.store,
                memory,
                work,
                stop,
            )?,
            policy,
            memory,
            work,
            stop,
        )?;
        let ReceiptBody::Complete(body) = &signed.source().body else {
            unreachable!("private GEN complete source")
        };
        let receipt = copy_complete(body, signed.endpoint_signature_hex(), memory, work, stop)?;
        drop(signed);
        persist(
            &mut self.store,
            Next::Ready {
                candidate,
                prepare,
                decision,
                complete: Some(receipt.receipt()),
            },
            provider,
            instance,
            policy,
            memory,
            work,
            stop,
        )?;
        Ok(receipt)
    }
}
