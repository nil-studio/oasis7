//! Terminal assertions remain ordinary signed-service evidence, never formal proof.
use super::*;

pub(super) fn validate_prefix_result<H: ClientWorldProfile>(
    response: &IntentResponse<Value>,
    original: &Payload<H>,
) -> Result<(), WorldServiceClientError> {
    let Some(proof) = &response.terminal_recovery_authority else {
        return Ok(());
    };
    let fail = |message: &str| WorldServiceClientError::Assurance(message.into());
    if response.formal_profile.is_some() || response.execution_evidence_scope.is_some() {
        return Err(fail("terminal recovery requires ordinary service evidence"));
    }
    proof
        .current_runtime_binding
        .validate()
        .map_err(|e| fail(&format!("{e:?}")))?;
    proof
        .validate(&response.correlation)
        .map_err(|e| fail(&e.to_string()))?;
    let IntentOutcome::Committed { commit, receipt } = &response.outcome else {
        return Err(fail(
            "terminal recovery requires the committed original Prefix",
        ));
    };
    if commit.as_ref() != &proof.prefix_commit || commit.position != proof.prefix_committed_height {
        return Err(fail("terminal Prefix commit differs from full result"));
    }
    let legacy: oasis7_client_api::legacy_world_service::WorldServicePayloadV1 =
        serde_json::from_value(serde_json::to_value(original).map_err(|e| fail(&e.to_string()))?)
            .map_err(|_| fail("terminal recovery requires an original legacy ProviderPrefix"))?;
    let oasis7_client_api::legacy_world_service::WorldServicePayloadV1::Scheduler(signed) = &legacy
    else {
        return Err(fail("terminal recovery requires a signed Prefix"));
    };
    let oasis7_client_api::legacy_world_service::SchedulerOperationV1::ProviderPrefix {
        request,
        ..
    } = &signed.request.operation
    else {
        return Err(fail(
            "terminal recovery requires an original ProviderPrefix",
        ));
    };
    let invocation = request
        .base_decision_request
        .capability_invocation_context
        .as_ref()
        .ok_or_else(|| fail("terminal Prefix invocation missing"))?;
    if proof.agent_id != signed.request.agent_id
        || proof.agent_id != request.agent_subject
        || proof.agent_session_id != request.agent_session_id
        || proof.agent_turn_id != request.agent_turn_id
        || proof.decision_request_id != request.decision_request_id
        || proof.request_digest != request.request_digest.to_string()
        || proof.retry_seq != request.retry_seq
        || proof.transport_attempt != request.transport_attempt
        || proof.grant_id != invocation.grant_id
        || proof.writer_public_key != signed.subject_public_key
        || proof.delegation_generation != signed.request.delegation_generation
        || invocation.subject
            != (oasis7_wasm_abi::CapabilitySubject::Agent {
                agent_id: proof.agent_id.clone(),
                owner_binding: proof.owner_binding.clone(),
                generation: proof.identity_generation,
            })
    {
        return Err(fail(
            "terminal recovery differs from the exact original request authority",
        ));
    }
    validate_full_result::<H>(response, original, proof, receipt)
}

fn validate_full_result<H: ClientWorldProfile>(
    response: &IntentResponse<Value>,
    original: &Payload<H>,
    proof: &TerminalRecoveryAuthorityV1,
    receipt: &Value,
) -> Result<(), WorldServiceClientError> {
    let fail = |message: &str| WorldServiceClientError::Assurance(message.into());
    let result = CanonicalIntentResultV1::<H> {
        request: SubmitIntentRequest {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            correlation: response.correlation.clone(),
            deadline_unix_ms: proof.prefix_deadline_unix_ms,
            signed_payload: original.clone(),
        },
        action_id: proof.prefix_action_id,
        committed_height: proof.prefix_committed_height,
        receipt: receipt.clone(),
        rejected: None,
    };
    if request_digest("terminal-prefix-result.v1", &result).map_err(|e| fail(&e))?
        != proof.prefix_result_digest
    {
        return Err(fail("terminal Prefix full result digest mismatch"));
    }
    Ok(())
}

fn validate_watermark(
    proof: &TerminalRecoveryAuthorityV1,
    minimum: &CommitRef,
) -> Result<(), WorldServiceClientError> {
    if !proof
        .read_commit
        .satisfies_minimum(minimum)
        .map_err(|e| WorldServiceClientError::Assurance(e.to_string()))?
    {
        return Err(WorldServiceClientError::Assurance(
            "terminal recovery read precedes authenticated View".into(),
        ));
    }
    Ok(())
}

impl<H: ClientWorldProfile> RemoteWorldServiceClient<H> {
    /// Authenticate an exact original Lookup. The watermark is local verified
    /// read state, not an alteration of the immutable Lookup wire request.
    pub fn lookup_terminal_original(
        &self,
        request: LookupIntentRequest,
        original: Payload<H>,
        minimum: &CommitRef,
    ) -> Result<IntentResponse<Value>, WorldServiceClientError> {
        if self.reader.formal_read.is_some() {
            return Err(WorldServiceClientError::Assurance(
                "formal terminal recovery is unsupported".into(),
            ));
        }
        let response = self.lookup(request, original)?;
        if let Some(proof) = &response.terminal_recovery_authority {
            if minimum.world != self.reader.config.expected_world {
                return Err(WorldServiceClientError::Assurance(
                    "terminal recovery minimum belongs to another world".into(),
                ));
            }
            validate_watermark(proof, minimum)?;
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_service_test_profile::TestProfile;
    use serde_json::json;

    fn fixture() -> (IntentResponse<Value>, Payload<TestProfile>) {
        let world = WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        };
        let commit = CommitRef {
            world: world.clone(),
            binding: ExecutionBinding {
                provider_world_id: "provider".into(),
                branch_id: "main".into(),
                finality_ref: "finality".into(),
                reorg_generation: 0,
                governing_manifest_ref: "manifest".into(),
                authority_generation: 1,
                permission_generation: 1,
            },
            position: 7,
            execution_block_hash: "block".into(),
            state_root_ref: "root".into(),
        };
        let correlation = RequestCorrelation {
            key: RequestKey {
                world: world.clone(),
                verified_subject: "writer".into(),
                operation_domain: "scheduler".into(),
                nonce_scope: "writer".into(),
                request_id_or_nonce: "prefix".into(),
            },
            payload_digest: "payload".into(),
        };
        let original = Payload::<TestProfile>::GameplayJson(vec![1, 2, 3]);
        let receipt = json!({"dispatched":true,"request_digest":"request"});
        let mut proof: TerminalRecoveryAuthorityV1 = serde_json::from_value(json!({
            "schema_version":1,"role":"terminal_only","world":world,"agent_id":"agent","owner_binding":"owner","identity_generation":1,
            "agent_session_id":"session","agent_turn_id":"turn","decision_request_id":"decision","request_digest":"request","retry_seq":1,"transport_attempt":1,
            "grant_id":"grant","grant_issued_at_tick":1,"grant_expires_at_tick":3,"admitted_at_tick":2,
            "lease":{"lease_id":"lease","idempotency_key":"lease-key","account_id":"account","quote_digest":"quote","reserved_amount":1,"reserved_at_tick":1},
            "prefix_correlation":correlation,"prefix_result_digest":"pending","prefix_action_id":9,"prefix_deadline_unix_ms":null,"prefix_committed_height":7,"prefix_commit":commit,
            "writer_public_key":"writer","delegation_generation":1,
            "current_runtime_binding":{"world_id":"world","branch_id":"main","finality_epoch":1,"finality_status":"verified","base_tick":8,"base_world_hash":"blake3:0000000000000000000000000000000000000000000000000000000000000000","reorg_epoch":0,"runtime_manifest_hash":"blake3:0000000000000000000000000000000000000000000000000000000000000000"},
            "current_task_fence":null,"read_commit":commit
        })).unwrap();
        // Independent literal full-result encoding, rather than rebuilding the
        // implementation's typed result in the oracle.
        proof.prefix_result_digest = request_digest("terminal-prefix-result.v1", &json!({
            "request":{"contract_version":1,"correlation":correlation,"deadline_unix_ms":null,"signed_payload":original},
            "action_id":9,"committed_height":7,"receipt":receipt,"rejected":null,
        })).unwrap();
        (
            IntentResponse {
                contract_version: 1,
                formal_profile: None,
                execution_evidence_scope: None,
                terminal_recovery_authority: Some(Box::new(proof)),
                correlation,
                outcome: IntentOutcome::Committed {
                    commit: Box::new(commit),
                    receipt,
                },
            },
            original,
        )
    }

    #[test]
    fn terminal_full_result_binds_original_receipt_action_deadline_and_height() {
        let (response, original) = fixture();
        let proof = response.terminal_recovery_authority.as_ref().unwrap();
        let IntentOutcome::Committed { receipt, .. } = &response.outcome else {
            unreachable!()
        };
        assert!(validate_full_result::<TestProfile>(&response, &original, proof, receipt).is_ok());
        for field in ["action", "deadline", "height", "receipt", "original"] {
            let mut changed = proof.clone();
            let mut receipt = receipt.clone();
            let mut original = original.clone();
            match field {
                "action" => changed.prefix_action_id += 1,
                "deadline" => changed.prefix_deadline_unix_ms = Some(1),
                "height" => changed.prefix_committed_height += 1,
                "receipt" => receipt["dispatched"] = json!(false),
                "original" => original = Payload::<TestProfile>::GameplayJson(vec![4]),
                _ => unreachable!(),
            }
            assert!(
                validate_full_result::<TestProfile>(&response, &original, &changed, &receipt)
                    .is_err(),
                "accepted changed {field}"
            );
        }
    }

    #[test]
    fn terminal_read_watermark_rejects_old_conflicting_and_other_lineages() {
        let (response, _) = fixture();
        let proof = response.terminal_recovery_authority.as_ref().unwrap();
        assert!(validate_watermark(proof, &proof.read_commit).is_ok());
        for case in ["older", "conflict", "world", "generation"] {
            let mut minimum = proof.read_commit.clone();
            match case {
                "older" => minimum.position += 1,
                "conflict" => minimum.state_root_ref.push_str("other"),
                "world" => minimum.world.world_id.push_str("other"),
                "generation" => minimum.binding.authority_generation += 1,
                _ => unreachable!(),
            }
            assert!(
                validate_watermark(proof, &minimum).is_err(),
                "accepted {case}"
            );
        }
    }

    #[test]
    fn terminal_attachment_is_signed_with_exact_lookup_query_and_never_formal() {
        let (response, original) = fixture();
        let query = AuthenticatedLookup::<TestProfile> {
            request: LookupIntentRequest {
                contract_version: 1,
                key: response.correlation.key.clone(),
            },
            original: original.clone(),
        };
        let digest = request_digest(LOOKUP_PATH, &query).unwrap();
        let private = "11".repeat(32);
        let public = hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&[0x11; 32])
                .verifying_key()
                .to_bytes(),
        );
        let signed =
            sign_service_response(LOOKUP_PATH, digest.clone(), response.clone(), &private).unwrap();
        assert!(verify_service_response(LOOKUP_PATH, &digest, &signed, &public).is_ok());
        let mut changed = signed.clone();
        changed
            .payload
            .terminal_recovery_authority
            .as_mut()
            .unwrap()
            .identity_generation += 1;
        assert!(verify_service_response(LOOKUP_PATH, &digest, &changed, &public).is_err());
        assert!(verify_service_response(LOOKUP_PATH, "other-query", &signed, &public).is_err());
        let mut formal = response;
        formal.formal_profile = Some(FormalReadProfileV1::controlled());
        assert!(validate_prefix_result::<TestProfile>(&formal, &original).is_err());
    }
}
