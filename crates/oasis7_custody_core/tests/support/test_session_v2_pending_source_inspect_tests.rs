//! Core payer/typed issuer fixture only, not real Worker/Native SQL or139 IO.
use super::*;
use oasis7_custody_core::execution_pending_source_inspect_v2::*;
fn after_first_rf() -> (SessionFixture, PendingFenceDispatchV2) {
    let (mut f, first) = first();
    let dp = pin(&f, AccountMachinePurposeV2::ExecutionFenceDispatchV2);
    let authority = f
        .state
        .pending_first_fence_read_authority_v2(first.proof(), &dp, &floor(), UnixMillis::new(13008))
        .unwrap();
    let signature = signed(&authority.signing_bytes().unwrap());
    let rp = pin(&f, AccountMachinePurposeV2::ExecutionFenceReadV2);
    f.state
        .record_pending_first_fence_read_v2(
            authority,
            signature,
            rp,
            floor(),
            UnixMillis::new(13009),
        )
        .unwrap();
    (f, first)
}
fn dispatch(
    f: &mut SessionFixture,
    first: PendingFenceDispatchV2,
    until: u64,
) -> PendingSourceDispatchV2 {
    let i = issuer(f);
    let authority = f
        .state
        .pending_source_dispatch_authority_v2(
            first,
            i,
            PendingSourceDispatchTargetV2 {
                object_id: "ca".repeat(32),
                handoff_until: UnixMillis::new(until),
            },
            &scope(&f.pin),
            UnixMillis::new(13010),
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(authority.binding().expires_at.0, until);
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&f.state).unwrap()).unwrap();
    assert_eq!(
        cold, f.state,
        "first Source claim is itself valid durable state"
    );
    let signature = signed(&authority.signing_bytes().unwrap());
    let p = pin(
        f,
        AccountMachinePurposeV2::ExecutionPendingSourceInspectDispatchV2,
    );
    f.state
        .record_pending_source_dispatch_v2(authority, signature, p, floor(), UnixMillis::new(13010))
        .unwrap()
}
#[test]
fn session_v2_pending_source_inspect_fixed_four_leaves_no_second_fee_cold() {
    let (mut f, first) = after_first_rf();
    let budget = remaining(&f);
    let first = dispatch(&mut f, first, 17400);
    let q = first.request();
    let raw = q.wire_bytes().unwrap();
    assert!(PendingSourceInspectDispatchRequestV2::decode(&raw).unwrap() == *q);
    let proof = q.proof.clone();
    let dp = pin(
        &f,
        AccountMachinePurposeV2::ExecutionPendingSourceInspectDispatchV2,
    );
    let authority = f
        .state
        .pending_source_native_leaf_authority_v2(&proof, &dp, &floor(), UnixMillis::new(13011))
        .unwrap();
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&f.state).unwrap()).unwrap();
    assert_eq!(
        cold, f.state,
        "received Account2 + Native1 claims are durable before signing"
    );
    let signature = signed(&authority.signing_bytes().unwrap());
    let np = pin(
        &f,
        AccountMachinePurposeV2::ExecutionPendingSourceNativeLeafV2,
    );
    let leaf = f
        .state
        .record_pending_source_native_leaf_v2(
            authority,
            signature,
            np.clone(),
            floor(),
            UnixMillis::new(13012),
        )
        .unwrap();
    leaf.verify(&np, &floor(), scope(&f.pin).http_origin, 13012)
        .unwrap();
    let native = PendingSourceNativeRequestV2 {
        leaf,
        request: q.request.clone(),
    };
    assert!(PendingSourceNativeRequestV2::decode(&native.wire_bytes().unwrap()).unwrap() == native);
    assert_eq!(
        remaining(&f),
        budget,
        "the original134 master already paid these four leaves"
    );
    let saved = f.state.clone();
    assert!(
        f.state
            .pending_source_native_leaf_authority_v2(&proof, &dp, &floor(), UnixMillis::new(13013))
            .is_err()
    );
    assert_eq!(f.state, saved);
    let value = serde_json::to_value(&saved).unwrap();
    let cold: AccountProvisionApprovalState = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(cold, saved);
    for field in [
        "source_inspect_dispatch",
        "source_inspect_native_leaf",
        "source_inspect_current_until",
    ] {
        let mut bad = value.clone();
        bad["test_sessions_v2"][&f.session_id]["execution_pending_start"][field] =
            serde_json::Value::Null;
        assert!(
            serde_json::from_value::<AccountProvisionApprovalState>(bad).is_err(),
            "cannot delete committed Source cause: {field}"
        );
    }
    let row = value
        .get("test_sessions_v2")
        .unwrap()
        .get(&f.session_id)
        .unwrap();
    assert!(
        row.get("execution_book").unwrap().is_null(),
        "this is neither Book birth nor nonce consumption"
    );
}
#[test]
fn session_v2_pending_source_inspect_original_private_handoff_never_extended() {
    let (mut f, first) = after_first_rf();
    let first = dispatch(&mut f, first, 13011);
    let dp = pin(
        &f,
        AccountMachinePurposeV2::ExecutionPendingSourceInspectDispatchV2,
    );
    let before = f.state.clone();
    assert!(
        f.state
            .pending_source_native_leaf_authority_v2(
                &first.request().proof,
                &dp,
                &floor(),
                UnixMillis::new(13011)
            )
            .is_err()
    );
    assert_eq!(
        f.state, before,
        "expiry cannot refresh or claim the unsent Native subtree"
    );
}

#[test]
fn session_v2_pending_source_paid_outer_is_history_not_source_or_book() {
    use oasis7_custody_core::execution_pending_source_result_v2::*;
    let (mut f, first) = after_first_rf();
    let first = dispatch(&mut f, first, 17400);
    let proof = first.request().proof.clone();
    let request = first.request().request.clone();
    let dp = pin(
        &f,
        AccountMachinePurposeV2::ExecutionPendingSourceInspectDispatchV2,
    );
    let authority = f
        .state
        .pending_source_native_leaf_authority_v2(&proof, &dp, &floor(), UnixMillis::new(13011))
        .unwrap();
    let signature = signed(&authority.signing_bytes().unwrap());
    let np = pin(
        &f,
        AccountMachinePurposeV2::ExecutionPendingSourceNativeLeafV2,
    );
    let leaf = f
        .state
        .record_pending_source_native_leaf_v2(
            authority,
            signature,
            np,
            floor(),
            UnixMillis::new(13012),
        )
        .unwrap();
    let q = PendingSourceNativeRequestV2 { leaf, request };
    let proof_original = &q.request.approval.proof;
    let fact = ApprovalVerificationV1 {
        version: 1,
        key_id: scope(&f.pin).native_root_key_id.into(),
        identity_namespace: proof_original.identity_namespace.clone(),
        purpose: VerificationPurposeV1::InspectOrigin,
        verify_nonce: q.request.verify_nonce.clone(),
        proof_digest: q.request.proof_digest.clone(),
        approval_request_id: proof_original.approval_request_id.clone(),
        audience: q.request.expected_audience.clone(),
        intent_digest: q.request.expected_intent_digest.clone(),
        manager_security_version: proof_original.manager_security_version,
        manager_policy_version: proof_original.manager_policy_version,
        origin_credential_version: proof_original.origin_credential_version,
        identity_instance_epoch: proof_original.identity_instance_epoch,
        credential_version: proof_original.passkey_credential_version,
        observed_at: DecimalU64(13013),
        proof_expires_at: proof_original.expires_at,
        source_session_expires_at: f.context.source.session_expires_at,
        source_session_idle_expires_at: f.context.source.session_idle_expires_at,
        proof_state: ProofStateV1::Active,
        origin_state: OriginStateV1::Active,
    };
    let inner = sign_verification(fact);
    assert_eq!(
        hex_bytes(key().verifying_key().to_bytes()),
        scope(&f.pin).native_root_public_key,
        "fixture Native root must be the actual fixed public pin"
    );
    let result = PendingSourceInspectResultV2 {
        api_version: 2,
        key_id: scope(&f.pin).native_root_key_id.into(),
        dispatch_digest: proof.digest().unwrap(),
        native_leaf_digest: q.leaf.digest().unwrap(),
        native_request_digest: q.digest().unwrap(),
        inner,
        completed_at: DecimalU64(13013),
    };
    let signature_hex = hex_bytes(
        key()
            .sign(&SignedPendingSourceInspectResultV2::signing_bytes(&result).unwrap())
            .to_bytes(),
    );
    let reply = SignedPendingSourceInspectResultV2 {
        result,
        signature_hex,
    };
    reply
        .verify_for(
            &q,
            scope(&f.pin).native_root_key_id,
            scope(&f.pin).native_root_public_key,
        )
        .unwrap();
    assert!(
        SignedPendingSourceInspectResultV2::decode(
            &serde_json::to_vec(&reply.result.inner).unwrap()
        )
        .is_err(),
        "ordinary signed inner never upgrades to paid outer"
    );
    let mut other = reply.clone();
    other.result.native_leaf_digest = "ed".repeat(32);
    other.signature_hex = hex_bytes(
        key()
            .sign(&SignedPendingSourceInspectResultV2::signing_bytes(&other.result).unwrap())
            .to_bytes(),
    );
    assert!(
        other
            .verify_for(
                &q,
                scope(&f.pin).native_root_key_id,
                scope(&f.pin).native_root_public_key
            )
            .is_err(),
        "even strong Native signature cannot substitute another paid leaf"
    );
    let history = f
        .state
        .archive_pending_source_inspect_result_history_v2(
            first,
            reply,
            &scope(&f.pin),
            UnixMillis::new(13014),
        )
        .unwrap();
    assert_eq!(history.session_id(), f.session_id);
    assert_eq!(history.result_digest().len(), 64);
    for checks in f
        .state
        .test_session_v2_native_restore_checks(&f.session_id, &scope(&f.pin))
        .unwrap()
    {
        StrictCrypto.verify(&checks).unwrap();
    }
    let value = serde_json::to_value(&f.state).unwrap();
    let cold: AccountProvisionApprovalState = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(cold, f.state);
    let row = value
        .get("test_sessions_v2")
        .unwrap()
        .get(&f.session_id)
        .unwrap();
    assert!(row.get("execution_book").unwrap().is_null());
    let source_id = row
        .get("execution_pending_start")
        .unwrap()
        .get("source_operation_id")
        .unwrap()
        .as_str()
        .unwrap();
    let source = row
        .get("sources")
        .unwrap()
        .get(source_id)
        .unwrap()
        .get("source")
        .unwrap();
    assert!(
        source.get("verification").unwrap().is_null(),
        "paid result history has not admitted Source"
    );
    let mut bad = value;
    bad["test_sessions_v2"][&f.session_id]["execution_pending_start"]["source_inspect_result"]["reply"]
        ["signature_hex"] = serde_json::json!("00".repeat(64));
    let cold: AccountProvisionApprovalState = serde_json::from_value(bad).unwrap();
    assert!(
        cold.test_session_v2_native_restore_checks(&f.session_id, &scope(&f.pin))
            .is_err(),
        "unified history verification requires the externally pinned Root outer"
    );
}

#[test]
fn session_v2_pending_source_cold_cannot_claim_original_after_dispatch_signed() {
    let (mut f, first) = after_first_rf();
    let first = dispatch(&mut f, first, 17400);
    let source_id = first.request().proof.binding.source_operation_id.clone();
    let request_id = first.request().proof.binding.action.request_id.clone();
    let original = serde_json::to_value(&f.state).unwrap();
    let good: AccountProvisionApprovalState = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(
        good, f.state,
        "genuine signed Source dispatch cold positive"
    );
    let mut altered = original.clone();
    let row = altered
        .get_mut("test_sessions_v2")
        .unwrap()
        .get_mut(&f.session_id)
        .unwrap();
    let events = row.get_mut("events").unwrap().as_array_mut().unwrap();
    let claim = events
        .iter()
        .position(|e| {
            let event = e.get("event").unwrap();
            event.get("kind").and_then(|v| v.as_str()) == Some("source_verification_claimed")
                && event.get("source_operation_id").and_then(|v| v.as_str())
                    == Some(source_id.as_str())
        })
        .expect("actual exact Source claim event");
    let signed = events
        .iter()
        .position(|e| {
            let event = e.get("event").unwrap();
            event.get("kind").and_then(|v| v.as_str())
                == Some("execution_pending_source_dispatch_recorded")
                && event.get("request_id").and_then(|v| v.as_str()) == Some(request_id.as_str())
        })
        .expect("actual exact Source signed dispatch event");
    assert_eq!(
        signed,
        claim + 1,
        "producer commits original claim before its separate signing record"
    );
    assert_eq!(
        events[claim].get("at").unwrap(),
        events[signed].get("at").unwrap(),
        "actual same-ms producer order"
    );
    // Ordinals are derived from vector position, not a caller field. Swap only
    // these two same-time events; every later event keeps its original index.
    events.swap(claim, signed);
    let before_row = original
        .get("test_sessions_v2")
        .unwrap()
        .get(&f.session_id)
        .unwrap();
    for field in [
        "execution_pending_start",
        "sources",
        "source_funding",
        "source_oracle_reads",
        "roots",
    ] {
        assert_eq!(
            row.get(field).unwrap(),
            before_row.get(field).unwrap(),
            "all strong originals, claims, windows and fees unchanged: {field}"
        );
    }
    assert!(
        serde_json::from_value::<AccountProvisionApprovalState>(altered).is_err(),
        "original Source claim cannot follow the signed dispatch event, even within the same millisecond"
    );
}
