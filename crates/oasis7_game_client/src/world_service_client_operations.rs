//! Formal admission is informational until a separately proved result is read.
use super::*;

impl<H: ClientWorldProfile> RemoteWorldServiceClient<H> {
    /// Recover the exact submitted envelope after a lost reply. Unlike a
    /// payload-only key lookup, this also compares the original deadline.
    pub fn lookup_formal_original(
        &self,
        original: SubmitIntentRequest<Payload<H>>,
    ) -> Result<IntentResponse<Value>, WorldServiceClientError> {
        original
            .validate()
            .map_err(|error| WorldServiceClientError::Assurance(error.to_string()))?;
        let request = LookupIntentRequest {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            key: original.correlation.key.clone(),
        };
        self.lookup_formal_with_original(
            request,
            original.signed_payload.clone(),
            Some(&original),
            None,
        )
    }

    /// Recover an original envelope at an explicitly selected qualified cut.
    /// The selector grants no trust; the normal signed lineage/result proof
    /// and exact original-envelope checks still apply.
    pub fn lookup_formal_original_at(
        &self,
        original: SubmitIntentRequest<Payload<H>>,
        fixed: CommitRef,
    ) -> Result<IntentResponse<Value>, WorldServiceClientError> {
        original
            .validate()
            .map_err(|error| WorldServiceClientError::Assurance(error.to_string()))?;
        fixed
            .validate()
            .map_err(|error| WorldServiceClientError::Assurance(error.to_string()))?;
        if fixed.world != self.reader.config.expected_world {
            return Err(WorldServiceClientError::Assurance(
                "formal lookup fixed cut belongs to another world".into(),
            ));
        }
        let request = LookupIntentRequest {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            key: original.correlation.key.clone(),
        };
        self.lookup_formal_with_original(
            request,
            original.signed_payload.clone(),
            Some(&original),
            Some(&fixed),
        )
    }

    pub(super) fn lookup_formal(
        &self,
        request: LookupIntentRequest,
        original: Payload<H>,
    ) -> Result<IntentResponse<Value>, WorldServiceClientError> {
        self.lookup_formal_with_original(request, original, None, None)
    }

    fn lookup_formal_with_original(
        &self,
        request: LookupIntentRequest,
        original: Payload<H>,
        expected_envelope: Option<&SubmitIntentRequest<Payload<H>>>,
        fixed: Option<&CommitRef>,
    ) -> Result<IntentResponse<Value>, WorldServiceClientError> {
        if self.reader.formal_read.is_none() {
            return Err(WorldServiceClientError::Configuration(
                "formal lookup requires configured external proof trust".into(),
            ));
        }
        request
            .validate()
            .map_err(|error| WorldServiceClientError::Assurance(error.to_string()))?;
        let correlation =
            H::derive_correlation(self.reader.config.expected_world.clone(), &original)
                .map_err(WorldServiceClientError::Assurance)?;
        if correlation.key != request.key {
            return Err(WorldServiceClientError::Assurance(
                "formal lookup key differs from original payload".into(),
            ));
        }
        // Each call examines at most eight bounded history pages. A subsequent
        // call continues from the exact verified cache, never a service pin.
        for _ in 0..8 {
            let view = self.reader.prepare_formal_request(ReadWorldViewRequest {
                observation_nonce: None,
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                world: self.reader.config.expected_world.clone(),
                scope_id: self.reader.config.scope_id.clone(),
                formal_anchor: None,
                min_commit: None,
                fixed_commit: fixed.cloned(),
                deadline_unix_ms: None,
            })?;
            let read = authority::sign_read_request(
                VIEW_PATH,
                view,
                &self.reader.config.read_private_key_hex,
            )
            .map_err(WorldServiceClientError::Assurance)?;
            let message = FormalAuthenticatedLookupV1 {
                schema_version: 1,
                lookup: AuthenticatedLookup::<H> {
                    request: request.clone(),
                    original: original.clone(),
                },
                read,
            };
            let page: FormalBusinessOutcomePageV1<H::Projection> =
                self.call(LOOKUP_PATH, &message)?;
            if page.target_request != message.read.request {
                return Err(WorldServiceClientError::Assurance(
                    "formal lookup response differs from the signed read request".into(),
                ));
            }
            let original_request = if let Some(proof) = &page.result {
                let stored =
                    crate::formal_business::decode_canonical_result_leaf::<H>(&proof.outcome)
                        .map_err(WorldServiceClientError::Assurance)?;
                if stored.request.signed_payload != original
                    || stored.request.correlation != correlation
                    || expected_envelope.is_some_and(|expected| &stored.request != expected)
                {
                    return Err(WorldServiceClientError::Assurance(
                        "formal lookup returned another original payload".into(),
                    ));
                }
                stored.request
            } else {
                SubmitIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    correlation: correlation.clone(),
                    deadline_unix_ms: None,
                    signed_payload: original.clone(),
                }
            };
            let cache =
                self.reader.formal_read.as_ref().ok_or_else(|| {
                    WorldServiceClientError::Assurance("formal cache missing".into())
                })?;
            let mut verifier = cache
                .lock()
                .map_err(|_| WorldServiceClientError::Assurance("formal cache poisoned".into()))?;
            let verified = crate::formal_business::verify_formal_business_outcome_page::<H>(
                &mut verifier,
                &page,
                &original_request,
                &message.read.subject_public_key,
            )
            .map_err(WorldServiceClientError::Assurance)?;
            if let crate::formal_business::FormalBusinessPageVerification::Complete(outcome) =
                verified
            {
                let outcome = match &outcome.result().outcome {
                    FormalBusinessOutcomeV1::Committed { .. } => outcome
                        .reply_committed()
                        .map_err(WorldServiceClientError::Assurance)?,
                    FormalBusinessOutcomeV1::Rejected { .. } => IntentOutcome::Rejected {
                        reason: WorldServiceErrorKind::Conflict,
                    },
                };
                return Ok(IntentResponse {
                    formal_profile: Some(FormalReadProfileV1::controlled()),
                    execution_evidence_scope: None,
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    correlation,
                    outcome,
                });
            }
        }
        // Unknown means eight valid, verified progress pages did not reach a
        // result. Transport, cooldown, and service rejections remain errors so
        // callers can distinguish an unavailable lookup from that bounded
        // history continuation case.
        Ok(IntentResponse {
            formal_profile: Some(FormalReadProfileV1::controlled()),
            execution_evidence_scope: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            correlation,
            outcome: IntentOutcome::Unknown,
        })
    }

    pub(super) fn submit_formal(
        &self,
        request: SubmitIntentRequest<Payload<H>>,
    ) -> Result<SubmitObservation<Value>, WorldServiceClientError> {
        if request.correlation.key.world != self.reader.config.expected_world {
            return Err(WorldServiceClientError::Assurance(
                "intent world differs from configuration".into(),
            ));
        }
        request
            .validate()
            .map_err(|error| WorldServiceClientError::Assurance(error.to_string()))?;
        let correlation = request.correlation.clone();
        match self.call::<_, IntentResponse<Value>>(SUBMIT_PATH, &request) {
            Ok(response) => {
                if response.formal_profile.as_ref() != Some(&FormalReadProfileV1::controlled())
                    || response.execution_evidence_scope.is_some()
                    || response.validate(&correlation).is_err()
                    || !matches!(
                        response.outcome,
                        IntentOutcome::Pending
                            | IntentOutcome::Received {
                                durability: AdmissionDurability::Volatile
                            }
                    )
                {
                    // A service signature cannot qualify a terminal result or
                    // prove the two durable Frozen acknowledgements.
                    return Ok(SubmitObservation::OutcomeUnknown(correlation));
                }
                Ok(SubmitObservation::Response(Box::new(response)))
            }
            Err(
                WorldServiceClientError::Transport(_)
                | WorldServiceClientError::Assurance(_)
                | WorldServiceClientError::Cooldown { .. },
            ) => Ok(SubmitObservation::OutcomeUnknown(correlation)),
            Err(error) => Err(error),
        }
    }
}
