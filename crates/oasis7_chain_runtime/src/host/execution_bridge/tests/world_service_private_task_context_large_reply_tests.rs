//! Real World-produced large private reply. No arbitrary padding/unknown fields
//! or hand-built signed snapshot. Native/Identity transport qualification is a
//! separate consumer step; this test never grants a paid Account operation.
use super::*;
#[test]
fn context_public_projection_v2_large_business_snapshot_export() {
    let f = active_context_read_fixture_with_business_history(|driver, world, owner, public| {
        // Authenticated SDK payloads run through the actual canonical driver.
        // Never insert a mailbox entry/state or pad a goal string by hand.
        let mut accepted = 0u64;
        loop {
            let cell = driver
                .execution_world
                .state()
                .agents
                .get("agent-a")
                .unwrap();
            if serde_json::to_vec(cell).unwrap().len() >= 36 * 1024 {
                break;
            }
            assert!(
                accepted < 64,
                "bounded actual chat history: cell_json_bytes={} accepted_messages={} mailbox_entries={}",
                serde_json::to_vec(cell).unwrap().len(),
                accepted,
                cell.mailbox.len()
            );
            let replaces = cell.intent.as_ref().map(|i| i.intent_id.clone());
            accepted += 1;
            let q = request(
                world,
                super::canonical_agent_chat::owner_chat(owner, public, accepted, replaces),
            );
            let key = correlation::key_digest(&q.correlation.key).unwrap();
            let height = driver
                .state
                .last_applied_committed_height
                .checked_add(1)
                .unwrap();
            commit(driver, height, q);
            let result: CanonicalIntentResultV1 = serde_json::from_value(
                driver
                    .execution_world
                    .capability_revocation_state()
                    .world_service_results[&key]
                    .clone(),
            )
            .unwrap();
            assert!(
                result.rejected.is_none(),
                "actual SDK business request must be accepted"
            );
            assert_eq!(result.committed_height, height);
            assert_eq!(
                driver.execution_world.state().agents["agent-a"]
                    .intent
                    .as_ref()
                    .unwrap()
                    .status,
                "accepted"
            );
        }
        assert!(accepted > 0);
        assert!(
            driver.execution_world.state().agents["agent-a"]
                .mailbox
                .len()
                > 2
        );
    });
    let record = f.store.get_task_context(&f.get).unwrap();
    let request = TaskContextPrepareAttestationRequestV2 {
        original: record.prepare_proof,
        observation_nonce: "a6".repeat(32),
    };
    let delivery: WorldContextAttestationV2<TaskContextPreparedDeliveryV2> =
        call(&f, CONTEXT_PREPARE_ATTESTED_PATH_V2, &request);
    verify_prepared_delivery_v2(&request, &delivery, &f.signer.public_key_hex).unwrap();
    let body = serde_json::to_vec(&delivery.payload.private_body).unwrap();
    let wire = serde_json::to_vec(&delivery).unwrap();
    assert!(
        body.len() > 32 * 1024,
        "actual old private signed reply crosses legacy transport cap"
    );
    assert!(wire.len() > 32 * 1024 && wire.len() <= MAX_ATTESTED_CONTEXT_WIRE_BYTES_V2);
    let committed =
        ContextBodyCommitmentV2::from_signed_body(&delivery.payload.private_body).unwrap();
    assert!(committed.canonical_bytes.0 <= 64 * 1024);
    let access_read = sign_read_request(
        VIEW_JSON_PATH,
        ReadWorldViewRequest {
            observation_nonce: Some("a7".repeat(32)),
            formal_anchor: None,
            contract_version: 1,
            world: f.world.clone(),
            scope_id: format!("agent:{}", f.fence.agent_id),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        },
        &hex::encode([73; 32]),
    )
    .unwrap();
    if let Ok(path) = std::env::var("OASIS7_NATIVE45_LARGE_PREPARED_FIXTURE") {
        std::fs::write(path,serde_json::to_vec(&serde_json::json!({
            "label":"actual-world-business-large-signed-reply-not-native-or-paid-account-authority",
            "world":{"world_id":f.world.world_id,"genesis_digest":f.world.genesis_digest,"service_public_key":f.signer.public_key_hex},
            "request":request,"delivery":delivery,"access":{"read":access_read,"fence":f.fence},
            "private_signed_json_bytes":body.len(),"delivery_json_bytes":wire.len(),
            "canonical_private_bytes":committed.canonical_bytes,
        })).unwrap()).unwrap();
    }
    drop(f.driver);
    fs::remove_dir_all(f.dir).unwrap();
}
