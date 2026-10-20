//! World projection specialization of the independent client proof consumer.
pub use oasis7_game_client::formal_read::{
    FormalNormalDurableSetReadStepV1, FormalReadCheckpointV2, FormalReadFencingEvidenceV1,
    FormalReadHistoryPageV1, FormalReadHistoryV1, FormalReadOriginQualificationV2,
    FormalReadProjection, FormalReadReplayCursorV1, FormalReadStepV1, FormalRecoveryReadStepV1,
    MAX_FORMAL_HISTORY_STEPS,
};
pub type FormalReadVerifier =
    oasis7_game_client::formal_read::FormalReadVerifier<crate::WorldServiceProjection>;
pub type VerifiedFormalProjectionV1 =
    oasis7_game_client::formal_read::VerifiedFormalProjectionV1<crate::WorldServiceProjection>;

impl oasis7_game_client::formal_read::FormalReadProjection for crate::WorldServiceProjection {
    fn decode_signed_view_response(
        bytes: &[u8],
    ) -> Result<
        oasis7_client_api::world_service::SignedServiceResponse<crate::codec::WorldViewResponse>,
        String,
    > {
        serde_json::from_slice(bytes).map_err(|error| error.to_string())
    }
    fn encode_signed_view_response(
        response: &oasis7_client_api::world_service::SignedServiceResponse<
            crate::codec::WorldViewResponse,
        >,
    ) -> Result<Vec<u8>, String> {
        serde_json::to_vec(response).map_err(|error| error.to_string())
    }
    fn verify_signed_view_response(
        path: &str,
        request_digest: &str,
        response: &oasis7_client_api::world_service::SignedServiceResponse<
            crate::codec::WorldViewResponse,
        >,
        trusted_service_public_key: &str,
    ) -> Result<(), String> {
        oasis7_client_api::world_service::verify_service_response(
            path,
            request_digest,
            response,
            trusted_service_public_key,
        )
    }
    fn verify_response(
        verifier: &mut FormalReadVerifier,
        response: &crate::codec::WorldViewResponse,
        request: &oasis7_client_api::world_service::ReadWorldViewRequest,
        reader_public_key: &str,
    ) -> Result<VerifiedFormalProjectionV1, String> {
        verify_view(verifier, response, request, reader_public_key)
    }
    fn read_metadata(
        &self,
    ) -> Result<oasis7_client_api::world_service_read::WorldServiceProjectionMetadataV1, String>
    {
        Ok(
            oasis7_client_api::world_service_read::WorldServiceProjectionMetadataV1 {
                runtime_binding: self.runtime_binding.clone(),
                formal_commit_header: self.formal_commit_header.clone(),
                events: self
                    .events
                    .iter()
                    .map(serde_json::to_value)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?,
            },
        )
    }
    fn validate_contents(
        response: &oasis7_client_api::world_service::ReadWorldViewResponse<Self>,
        request: &oasis7_client_api::world_service::ReadWorldViewRequest,
    ) -> Result<(), String> {
        response.validate(request).map_err(|e| e.to_string())?;
        if response.view.state.time != response.logical_tick {
            return Err("projection tick differs from declared logical tick".into());
        }
        if let Some(binding) = &response.view.runtime_binding {
            binding.validate().map_err(|e| format!("{e:?}"))?;
            let expected = &response.version.commit.binding;
            if binding.world_id != expected.provider_world_id
                || binding.branch_id != expected.branch_id
                || binding.reorg_epoch != expected.reorg_generation
                || binding.base_tick != response.logical_tick
                || binding.runtime_manifest_hash.to_string() != expected.governing_manifest_ref
                || oasis7_client_api::world_service::request_digest(
                    "finality",
                    &(
                        binding.finality_epoch,
                        &binding.finality_status,
                        &binding.finality_block_hash,
                    ),
                )? != expected.finality_ref
            {
                return Err("projection runtime binding differs from committed view".into());
            }
        }
        let agent = request.scope_id.strip_prefix("agent:");
        let feedback_history_out_of_scope = match (agent, response.view.feedback_history.as_ref()) {
            (None, Some(_)) => true,
            (Some(agent), Some(history)) => {
                history.agent_id != agent
                    || history
                        .records
                        .iter()
                        .any(|record| record.feedback.agent_subject != agent)
            }
            _ => false,
        };
        if response
            .view
            .agent_context
            .as_ref()
            .is_some_and(|context| Some(context.agent_id.as_str()) != agent)
            || response
                .view
                .scheduler_wakes
                .iter()
                .any(|wake| Some(wake.agent_id.as_str()) != agent)
            || response
                .view
                .continuations
                .iter()
                .any(|entry| Some(entry.agent_id.as_str()) != agent)
            || response
                .view
                .cognition_leases
                .iter()
                .any(|lease| Some(lease.agent_id.as_str()) != agent)
            || feedback_history_out_of_scope
        {
            return Err("Agent observation differs from authorized visibility scope".into());
        }
        Ok(())
    }
}

/// Concrete specialization keeps proof-consumer code generation in this library.
pub fn verify_view(
    verifier: &mut FormalReadVerifier,
    response: &crate::codec::WorldViewResponse,
    request: &oasis7_client_api::world_service::ReadWorldViewRequest,
    reader_public_key: &str,
) -> Result<VerifiedFormalProjectionV1, String> {
    verifier.verify_view(response, request, reader_public_key)
}
