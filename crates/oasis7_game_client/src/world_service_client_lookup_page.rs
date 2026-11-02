//! One physical authenticated lookup page, retaining opaque proof strength.
use super::*;
use crate::formal_business::FormalBusinessPageVerification;

impl<H: ClientWorldProfile> RemoteWorldServiceClient<H> {
    /// At most one HTTP lookup. Progress is verified lineage, never a canonical
    /// business fact. Complete retains the exact-original opaque result proof,
    /// including the distinction between committed and business-rejected.
    /// Caller must prepay every invocation; this method never retries or loops.
    pub fn lookup_formal_original_page(
        &self,
        original: SubmitIntentRequest<Payload<H>>,
    ) -> Result<FormalBusinessPageVerification<H>, WorldServiceClientError> {
        self.lookup_formal_original_page_inner(original, None)
    }

    /// Same one-page contract at a qualified fixed cut. The cut is a selector,
    /// never trust. It cannot weaken original-envelope or proof verification.
    pub fn lookup_formal_original_page_at(
        &self,
        original: SubmitIntentRequest<Payload<H>>,
        fixed: CommitRef,
    ) -> Result<FormalBusinessPageVerification<H>, WorldServiceClientError> {
        fixed
            .validate()
            .map_err(|e| WorldServiceClientError::Assurance(e.to_string()))?;
        if fixed.world != self.reader.config.expected_world {
            return Err(WorldServiceClientError::Assurance(
                "formal lookup fixed cut belongs to another world".into(),
            ));
        }
        self.lookup_formal_original_page_inner(original, Some(fixed))
    }

    fn lookup_formal_original_page_inner(
        &self,
        envelope: SubmitIntentRequest<Payload<H>>,
        fixed: Option<CommitRef>,
    ) -> Result<FormalBusinessPageVerification<H>, WorldServiceClientError> {
        envelope
            .validate()
            .map_err(|e| WorldServiceClientError::Assurance(e.to_string()))?;
        self.reader.transport.require_one_http()?;
        let request = LookupIntentRequest {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            key: envelope.correlation.key.clone(),
        };
        let original = envelope.signed_payload.clone();
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
        if correlation != envelope.correlation {
            return Err(WorldServiceClientError::Assurance(
                "formal lookup key differs from original payload".into(),
            ));
        }
        let view = self.reader.prepare_formal_request(ReadWorldViewRequest {
            observation_nonce: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: self.reader.config.expected_world.clone(),
            scope_id: self.reader.config.scope_id.clone(),
            formal_anchor: None,
            min_commit: None,
            fixed_commit: fixed.clone(),
            deadline_unix_ms: None,
        })?;
        let read =
            authority::sign_read_request(VIEW_PATH, view, &self.reader.config.read_private_key_hex)
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
            self.reader.transport.call_once(LOOKUP_PATH, &message)?;
        if page.target_request != message.read.request {
            return Err(WorldServiceClientError::Assurance(
                "formal lookup response differs from the signed read request".into(),
            ));
        }
        let original_request = if let Some(proof) = &page.result {
            let stored = crate::formal_business::decode_canonical_result_leaf::<H>(&proof.outcome)
                .map_err(WorldServiceClientError::Assurance)?;
            if stored.request.signed_payload != original
                || stored.request.correlation != correlation
                || stored.request != envelope
            {
                return Err(WorldServiceClientError::Assurance(
                    "formal lookup returned another original payload".into(),
                ));
            }
            stored.request
        } else {
            envelope
        };
        let cache = self
            .reader
            .formal_read
            .as_ref()
            .ok_or_else(|| WorldServiceClientError::Assurance("formal cache missing".into()))?;
        let mut verifier = cache
            .lock()
            .map_err(|_| WorldServiceClientError::Assurance("formal cache poisoned".into()))?;
        crate::formal_business::verify_formal_business_outcome_page::<H>(
            &mut verifier,
            &page,
            &original_request,
            &message.read.subject_public_key,
        )
        .map_err(WorldServiceClientError::Assurance)
    }
}
