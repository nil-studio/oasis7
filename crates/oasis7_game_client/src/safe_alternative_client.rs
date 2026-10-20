//! Explicit owner commands. This adapter never borrows the executor signer.
use super::*;
use oasis7_client_api::safe_alternative::SafeAlternativeCommandV1;

impl WorldServiceTaskClient {
    /// Prepare an immutable original using an independently obtained owner proof.
    /// Persist the returned envelope before submitting it. Each operation, including
    /// confirmation, requires its own owner signature; proposals imply no consent.
    pub fn prepare_safe_alternative(
        &self,
        owner_proof: SignedReadRequest<SafeAlternativeCommandV1>,
    ) -> Result<SubmitIntentRequest<WorldServicePayloadV1>, Error> {
        owner_proof.request.validate().map_err(assurance)?;
        verify_read_request(owner_proof.request.signing_domain(), &owner_proof)
            .map_err(assurance)?;
        self.private_scope(
            &WorldIdentity {
                world_id: owner_proof.request.world_id.clone(),
                genesis_digest: owner_proof.request.genesis_digest.clone(),
            },
            &owner_proof.request.agent_id,
        )?;
        let payload = WorldServicePayloadV1::SafeAlternative(owner_proof);
        let correlation = derive_correlation(self.config().expected_world.clone(), &payload)
            .map_err(assurance)?;
        Ok(SubmitIntentRequest {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            correlation,
            deadline_unix_ms: None,
            signed_payload: payload,
        })
    }

    /// Submit only an existing persisted original. Unknown replies require recovery
    /// of this exact envelope, never a newly signed confirmation or execution.
    pub fn submit_safe_alternative(
        &self,
        original: SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<SubmitObservation<Value>, Error> {
        let WorldServicePayloadV1::SafeAlternative(proof) = &original.signed_payload else {
            return Err(assurance("safe alternative original required"));
        };
        if self.prepare_safe_alternative(proof.clone())? != original {
            return Err(assurance("safe alternative original envelope differs"));
        }
        self.submit(original)
    }

    pub fn recover_safe_alternative(
        &self,
        original: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<IntentResponse<Value>, Error> {
        let WorldServicePayloadV1::SafeAlternative(proof) = &original.signed_payload else {
            return Err(assurance("safe alternative original required"));
        };
        if self.prepare_safe_alternative(proof.clone())? != *original {
            return Err(assurance("safe alternative original envelope differs"));
        }
        self.lookup(
            LookupIntentRequest {
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_client_api::safe_alternative::*;
    #[test]
    fn owner_confirmation_proof_cannot_be_rewritten_or_signed_by_executor_adapter() {
        let client = WorldServiceTaskClient::new(WorldServiceClientConfig {
            endpoint: "http://127.0.0.1:9".into(),
            expected_world: WorldIdentity {
                world_id: "w".into(),
                genesis_digest: "g".into(),
            },
            trusted_service_public_key: "00".repeat(32),
            scope_id: "agent:a".into(),
            read_private_key_hex: "02".repeat(32),
            timeout: std::time::Duration::from_secs(1),
            max_response_bytes: 65536,
        })
        .unwrap();
        let command = SafeAlternativeCommandV1 {
            api_rev: 1,
            client_operation_id: "confirm-1".into(),
            world_id: "w".into(),
            genesis_digest: "g".into(),
            branch_id: "b".into(),
            reorg_epoch: 0,
            agent_id: "a".into(),
            owner_binding: "p".into(),
            owner_generation: 1,
            operation: SafeAlternativeOperationV1::Confirm {
                request_id: "r".into(),
                generation: 1,
                candidate_id: "c".into(),
                candidate_digest: "digest".into(),
                confirmation_id: "confirm".into(),
                valid_until_tick: 10,
            },
        };
        let proof = sign_read_request(command.signing_domain(), command, &"01".repeat(32)).unwrap();
        let original = client.prepare_safe_alternative(proof.clone()).unwrap();
        assert_eq!(
            original.signed_payload,
            WorldServicePayloadV1::SafeAlternative(proof.clone())
        );
        assert_ne!(
            proof.subject_public_key,
            sign_read_request("fixture", (), &"02".repeat(32))
                .unwrap()
                .subject_public_key
        );
        let mut tampered = proof;
        tampered.request.owner_generation += 1;
        assert!(client.prepare_safe_alternative(tampered).is_err());
        let mut envelope = original;
        envelope.deadline_unix_ms = Some(123);
        assert!(client.submit_safe_alternative(envelope).is_err());
    }
}
