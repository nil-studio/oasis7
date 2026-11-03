//! Required-tier replay probes using the production reconciliation builders.
use super::*;

#[cfg(any(test, feature = "test_tier_required"))]
impl crate::viewer::ViewerRuntimeLiveServer {
    /// Required-tier drive of the production Act reconciliation decision and
    /// original operation builder with genuine signed service responses.
    pub fn test_replay_original_hosted_act(
        client: crate::world_service::client::RemoteWorldServiceClient,
        original: SubmitIntentRequest<crate::world_service::WorldServicePayloadV1>,
    ) -> Result<SubmitObservation<serde_json::Value>, String> {
        use crate::world_service::client::WorldServicePort;
        original.validate().map_err(|error| error.to_string())?;
        let response = client
            .lookup(
                LookupIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    key: original.correlation.key.clone(),
                },
                original.signed_payload.clone(),
            )
            .map_err(|error| error.to_string())?;
        if !original_act_requires_replay(&response, &original.correlation)? {
            return Err("test drive requires genuine unpublished original outcome".into());
        }
        let AgentServiceIoOperation::Submit(replay) =
            original_intent_io(&original.correlation, &original.signed_payload, true)
        else {
            unreachable!()
        };
        assert_eq!(replay.correlation, original.correlation);
        assert_eq!(replay.signed_payload, original.signed_payload);
        client.submit(replay).map_err(|error| error.to_string())
    }
}

#[cfg(any(test, feature = "test_tier_required"))]
impl crate::viewer::ViewerRuntimeLiveServer {
    pub fn test_replay_original_hosted_settlement(
        client: crate::world_service::client::RemoteWorldServiceClient,
        original: SubmitIntentRequest<crate::world_service::WorldServicePayloadV1>,
    ) -> Result<SubmitObservation<serde_json::Value>, String> {
        use crate::world_service::client::WorldServicePort;
        original.validate().map_err(|error| error.to_string())?;
        if !matches!(&original.signed_payload, crate::world_service::WorldServicePayloadV1::Scheduler(s) if matches!(s.request.operation, SchedulerOperationV1::SettleLease {..}))
        {
            return Err("settlement operation required".into());
        }
        assert!(
            matches!(
                original_settlement_io(
                    &original.correlation,
                    &original.signed_payload,
                    true,
                    true,
                    false
                ),
                AgentServiceIoOperation::Lookup { .. }
            ),
            "existing settlement first reconciles by Lookup"
        );
        assert!(
            matches!(
                original_settlement_io(
                    &original.correlation,
                    &original.signed_payload,
                    true,
                    false,
                    false
                ),
                AgentServiceIoOperation::Submit(_)
            ),
            "new settlement may first Submit"
        );
        assert!(
            matches!(
                original_settlement_io(
                    &original.correlation,
                    &original.signed_payload,
                    false,
                    true,
                    true
                ),
                AgentServiceIoOperation::Lookup { .. }
            ),
            "dispatch consumes an authorized Submit"
        );
        let response = client
            .lookup(
                LookupIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    key: original.correlation.key.clone(),
                },
                original.signed_payload.clone(),
            )
            .map_err(|e| e.to_string())?;
        if !original_settlement_requires_replay(&response, &original.correlation)? {
            return Err("unpublished settlement required".into());
        }
        let AgentServiceIoOperation::Submit(replay) = original_settlement_io(
            &original.correlation,
            &original.signed_payload,
            true,
            true,
            true,
        ) else {
            return Err("retained settlement was not submitted".into());
        };
        assert_eq!(replay.correlation, original.correlation);
        assert_eq!(replay.signed_payload, original.signed_payload);
        client.submit(replay).map_err(|e| e.to_string())
    }
}
