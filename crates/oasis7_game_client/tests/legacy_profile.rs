//! External-crate consumer: ordinary dependencies only, no server profile/DTO.
use oasis7_agent_api::{legacy_economy::*, legacy_scheduler::*};
use oasis7_client_api::{legacy_world_service::*, world_service::*};
use oasis7_game_client::{
    legacy_profile::*,
    world_service_client::{ClientWorldProfile, WorldServicePort},
};
use serde_json::{Value, json};

// Compile the actual independent transport with the concrete payload on both
// sides of durable Submit/Lookup. Never execute networking in this codec test.
#[allow(dead_code)]
fn submit_lookup(
    client: &LegacyWorldServiceClient,
    request: SubmitIntentRequest<WorldServicePayloadV1>,
    lookup: LookupIntentRequest,
) {
    let original = request.signed_payload.clone();
    let _ = client.submit(request);
    let _ = client.lookup(lookup, original);
}

#[test]
fn original_runtime_fixture_preserves_all_legacy_operations_and_results() {
    let baseline: Value = serde_json::from_str(include_str!(
        "../../oasis7_client_api/tests/fixtures/legacy_scheduler_baseline.json"
    ))
    .unwrap();
    let records = baseline["records"].as_array().unwrap();
    assert_eq!(records.len(), 19); // 10 kinds + terminal/replan + all 9 status variants
    for record in records {
        let original = record["json"].as_str().unwrap();
        let payload: WorldServicePayloadV1 = serde_json::from_str(original).unwrap();
        assert_eq!(serde_json::to_string(&payload).unwrap(), original);
        let WorldServicePayloadV1::Scheduler(signed) = &payload else {
            panic!("scheduler fixture")
        };
        verify_read_request("scheduler", signed).unwrap();
        // Construct the published concrete profile and sign it independently,
        // retaining the fixture's operation identity rather than replacing it.
        let constructed = SchedulerIntentV1 {
            task_prefix_binding_revision: 0,
            agent_id: signed.request.agent_id.clone(),
            request_id: signed.request.request_id.clone(),
            delegation_generation: signed.request.delegation_generation,
            captured_base_binding: signed.request.captured_base_binding.clone(),
            operation: signed.request.operation.clone(),
        };
        let constructed = WorldServicePayloadV1::Scheduler(
            sign_read_request("scheduler", constructed, &"11".repeat(32)).unwrap(),
        );
        assert_eq!(serde_json::to_string(&constructed).unwrap(), original);
        assert_eq!(
            serde_json::from_slice::<WorldServicePayloadV1>(
                &serde_json::to_vec(&constructed).unwrap()
            )
            .unwrap(),
            constructed
        );

        match &signed.request.operation {
            SchedulerOperationV1::ReserveLease(request) => request.validate().unwrap(),
            SchedulerOperationV1::AdmitContinuation(proposal) => proposal.validate().unwrap(),
            SchedulerOperationV1::ResumeWake {
                proposal,
                resume,
                current_context,
                ..
            } => {
                proposal.validate().unwrap();
                resume.validate().unwrap();
                current_context.validate().unwrap();
            }
            SchedulerOperationV1::HandoffWake {
                disposition: CognitionWakeDispositionV1::Replan { proposal, .. },
                ..
            } => proposal.validate().unwrap(),
            _ => (),
        }

        assert_eq!(
            hex::encode(signing_bytes("scheduler", &signed.request).unwrap()),
            record["signing_hex"]
        );
        assert_eq!(
            request_digest("intent", &payload).unwrap(),
            record["digest"]
        );
        let correlation: RequestCorrelation =
            serde_json::from_value(record["correlation"].clone()).unwrap();
        assert_eq!(LegacyClientProfile::<oasis7_game_client::read_projection::WorldReadProjectionV1>::derive_correlation(correlation.key.world.clone(), &payload).unwrap(), correlation);
        let key = request_digest("request-key", &correlation.key).unwrap();
        assert_eq!(key, record["key"]);
        let result: CanonicalIntentResultV1 =
            serde_json::from_str(record["result"].as_str().unwrap()).unwrap();
        assert_eq!(serde_json::to_string(&result).unwrap(), record["result"]);
        let value = serde_json::to_value(&result).unwrap();
        LegacyClientProfile::<oasis7_game_client::read_projection::WorldReadProjectionV1>::validate_canonical_result(&key,&value).unwrap();
        assert!(validate_canonical_result("wrong-key", &value).is_err());
        let mut invalid = value.clone();
        invalid["committed_height"] = json!(0);
        assert!(validate_canonical_result(&key, &invalid).is_err());
        let mut invalid = value;
        invalid["request"]["signed_payload"]["payload"]["signature_hex"] = json!("00");
        assert!(validate_canonical_result(&key, &invalid).is_err());
    }
}

#[test]
fn published_lease_and_continuation_pure_helpers_preserve_defaults_and_reject_invalid_input() {
    let digest = format!("blake3:{}", "1".repeat(64));
    let quote =
        CognitionLeaseQuoteV1::new("quote", "cognition_units", 12).with_valid_until_tick(30);
    let request = CognitionLeaseRequestV1::new(
        "idem", "account", "agent", "session", "turn", "decision", &digest, quote,
    );
    request.validate().unwrap();
    let again: CognitionLeaseRequestV1 =
        serde_json::from_slice(&serde_json::to_vec(&request).unwrap()).unwrap();
    assert_eq!(again.derived_lease_id(), request.derived_lease_id());
    let mut historical = serde_json::to_value(&request.quote).unwrap();
    for field in [
        "payer_id",
        "resource_version",
        "purpose",
        "scope",
        "policy_revision",
        "authority_context",
        "world_binding",
        "valid_until_tick",
    ] {
        historical.as_object_mut().unwrap().remove(field);
    }
    let mut historical: CognitionLeaseQuoteV1 = serde_json::from_value(historical).unwrap();
    assert_eq!(historical.payer_id, "");
    assert_eq!(historical.valid_until_tick, None);
    historical.refresh_digest();
    historical.validate().unwrap();
    let mut invalid = request;
    invalid.quote.amount = 0;
    invalid.quote.refresh_digest();
    assert_eq!(
        invalid.validate().unwrap_err().code(),
        "cognition_quote_invalid"
    );
    let condition: WakeConditionV1 = serde_json::from_value(
        json!({"schema_version":"wake-condition.v1","kind":"at_or_after_tick","logical_tick":20}),
    )
    .unwrap();
    WakeConditionValidator::validate(std::slice::from_ref(&condition)).unwrap();
    assert_eq!(
        WakeConditionValidator::next_wake_tick_at(std::slice::from_ref(&condition), 25).unwrap(),
        Some(25)
    );
    let mut invalid = condition;
    invalid.receipt_id = Some("extra".into());
    assert!(WakeConditionValidator::validate(&[invalid]).is_err());
    let resume = CognitionContinuationResumeRequestV1 {
        agent_session_id: "session".into(),
        agent_turn_id: "turn".into(),
        decision_request_id: "decision".into(),
        request_digest: "invalid".into(),
        context_digest: digest,
    };
    assert_eq!(
        resume.validate().unwrap_err().code(),
        "cognition_resume_identity_invalid"
    );
}
