//! Existing lawful Core Native10 producer + real SDK signatures. Synthetic
//! storage/World/Native signer fixtures only: not Native45, CF or Native HTTP.
use super::*;
use oasis7_agent_api::game::{TaskContextAuthorityModeV1, TaskContextIdentityV1};
use oasis7_client_api::world_service::{self as ws, task_context::*, task_context_projection::*};
use oasis7_custody_core::provisioning::{
    ReadPrepareHistoryReplyV2, SessionSourceFundingV1, SessionSourceReservation,
};
use oasis7_custody_core::{context_body::ContextBodySlotKindV1, context_body_v2::*, envelope::*};
#[path = "test_session_v2_read_history_delivery_fixture.rs"]
mod delivery_fixture;
use super::super::read_history_http_fixture as http_fixture;
use delivery_fixture::delivery;
fn public_result(
    r: &ReadyPrepare,
    q: &NativeExecutionPrepareRequestV2,
) -> ExecutionPreparedPublicResultV2 {
    let delivery = delivery(&r.f, q);
    let c = &q.proof.certificate;
    let scope = ContextBodyScopeV2 {
        api_version: 2,
        format: ContextBodyFormatV2::FullSignedResponseV2,
        identity_scope: c.session.scope.role_scope.clone(),
        world: c.world.clone(),
        session_id: c.session_id.clone(),
        context_operation_id: c.stage_operation_id.clone(),
        prepare_operation_id: q
            .original
            .request
            .original
            .request
            .client_operation_id
            .clone(),
        session_intent_digest: c.session_intent_digest.clone(),
        native_binding_digest: c.native_binding.digest().unwrap(),
        slot: ContextBodySlotKindV1::Prepared,
        version: DecimalU64(1),
        write_deadline: c.execution_deadline,
        context: delivery.payload.private_body.payload.identity.clone(),
        public_projection_digest: public_projection_digest_v2(&delivery.payload.projection)
            .unwrap(),
        body: ContextBodyCommitmentV2::from_signed_body(&delivery.payload.private_body).unwrap(),
    };
    let body = validate_fullsigned_prepared_storage_v2(
        scope.clone(),
        &c.world,
        &q.original.request,
        &delivery,
    )
    .unwrap();
    // Mechanical protected-storage envelope fixture; no cipher/DO success claim.
    let record = ContextBodyRecordV2 {
        scope: scope.clone(),
        envelope: KeyEnvelopeV1 {
            context: scope.envelope_context().unwrap(),
            ciphertext: "ab".repeat(body.canonical_bytes().len() + AES_GCM_TAG_BYTES),
            value_nonce: "01".repeat(12),
            wrapped_dek: "cd".repeat(48),
            wrap_nonce: "02".repeat(12),
            wrapping_key_version: DecimalU64(1),
        },
    };
    let reference = body
        .reference(
            AttestedContextEvidenceV2::Prepared {
                request: Box::new(q.original.request.clone()),
                projection: Box::new(delivery.payload.projection.clone()),
            },
            &record,
            &c.world,
        )
        .unwrap();
    let ns = super::scope(&r.f.pin);
    let receipt = NativeExecutionPrepareReceiptV2 {
        api_version: 2,
        native_key_id: ns.native_root_key_id.into(),
        proof_digest: q.proof.digest().unwrap(),
        native_attempt_id: c.ports[6].attempt_id.clone(),
        current_attempt_id: c.ports[7].attempt_id.clone(),
        prepare_attempt_id: c.ports[8].attempt_id.clone(),
        result_digest: prepared_response_digest_v2(&delivery).unwrap(),
        public_projection_digest: scope.public_projection_digest.clone(),
        completed_at: DecimalU64(14_012),
    };
    let signature_hex = encode_hex(key().sign(&receipt.signing_bytes(q).unwrap()).to_bytes());
    let result = ExecutionPreparedPublicResultV2 {
        api_version: 2,
        proof_digest: q.proof.digest().unwrap(),
        reference,
        native_receipt: SignedNativeExecutionPrepareReceiptV2 {
            receipt,
            signature_hex,
        },
        storage: record.storage_fact().unwrap(),
    };
    result
        .verify_native(q, ns.native_root_key_id, ns.native_root_public_key)
        .unwrap();
    result
}
#[test]
fn read_history_returns_exact_strong_public_observation_after_original_native_window() {
    let (mut r, paid) = ready_new();
    let observed = private(&mut r, &paid);
    let authority = native_authority(&mut r, &paid, &observed);
    let ns = scope(&r.f.pin);
    let (k, _, pin, floor) = issuer(
        AccountMachinePurposeV2::NativeExecutionPrepareV2,
        ns.http_origin,
    );
    let certificate = authority.certificate().clone();
    let proof = SignedAccountNativeExecutionPrepareV2 {
        signature_hex: encode_hex(k.sign(&certificate.signing_bytes().unwrap()).to_bytes()),
        certificate,
    };
    let dispatch =
        r.f.state
            .record_test_session_v2_native_prepare(
                &r.f.session_id,
                authority,
                proof,
                &pin,
                &floor,
                UnixMillis::new(14_011),
            )
            .unwrap();
    let q = dispatch.request().clone();
    let result = public_result(&r, &q);
    let native = r.f.pin.clone();
    let ns = scope(&native);
    r.f.state
        .archive_test_session_v2_prepared_public_result(
            &r.f.session_id,
            result.clone(),
            &ns,
            UnixMillis::new(14_012),
        )
        .unwrap();
    assert!(
        q.proof.certificate.expires_at.0 < 31_002,
        "original Native dispatch window is over, not extended by Read"
    );
    assert!(
        q.proof.certificate.execution_deadline.0 < 31_002,
        "Read is a sibling after execution expires, not a renewed Execution grant"
    );
    let (body, pop) = http_fixture::read_pop(
        &mut r.f,
        "read-exact-public",
        &q.proof.certificate.stage_operation_id,
        31_000,
    );
    let ingress = "read-exact-public-ingress";
    charge(
        &mut r.f,
        &body,
        &pop,
        ingress,
        SessionPurposeV1::ReconcileRead,
        31_002,
    )
    .unwrap();
    let token =
        r.f.state
            .charge_test_session_v2_prepare_history_owner_attempt(
                &r.f.session_id,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop,
                },
                UnixMillis::new(31_002),
            )
            .unwrap()
            .unwrap();
    let source = "read-public-history-source";
    let nonce = oasis7_custody_core::approval::bytes_digest(source.as_bytes());
    r.f.state
        .reserve_test_session_v2_source(
            &r.f.session_id,
            SessionSourceReservation {
                operation_id: source,
                nonce: &nonce,
                funding: SessionSourceFundingV1::ReconcileRead,
            },
            UnixMillis::new(31_003),
        )
        .unwrap();
    complete_execution_source(&mut r.f, source, nonce, 31_003);
    let nonce =
        r.f.state
            .test_session_v2_lifetime_nonce(&r.f.session_id, source)
            .unwrap();
    let lf = lifetime(&r.f, nonce, 31_004);
    let policy = Policy::new(r.f.state.draft());
    let context = r.f.context.clone();
    let provision = policy.current(&context);
    let current = TestSessionLiveCurrent {
        provision: &provision,
        native_scope: &ns,
        lifetime: &lf,
        private_sid_generation: DecimalU64(1),
        source_operation_id: source,
        source_ticket: 3,
        observed_at: UnixMillis::new(31_004),
    };
    let reply =
        r.f.state
            .finish_test_session_v2_prepare_history(
                &r.f.session_id,
                token,
                SessionProofRequest {
                    ingress_attempt_id: ingress,
                    original_bytes: &body,
                    possession: &pop,
                },
                &current,
                UnixMillis::new(31_004),
                &mut StrictCrypto,
            )
            .unwrap();
    let ReadPrepareHistoryReplyV2::HistoricalPublicObservation { observation } = reply else {
        panic!("known strong fact must be historical public observation")
    };
    assert!(serde_json::to_value(&*observation).unwrap() == serde_json::to_value(&result).unwrap());
    let cold: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&r.f.state).unwrap()).unwrap();
    let reply = cold
        .test_session_v2_prepare_history_paid_reply(
            &r.f.session_id,
            SessionProofRequest {
                ingress_attempt_id: ingress,
                original_bytes: &body,
                possession: &pop,
            },
            &ns,
            UnixMillis::new(31_005),
        )
        .unwrap();
    let ReadPrepareHistoryReplyV2::HistoricalPublicObservation { observation } = reply else {
        panic!("cold callback must preserve exact fact")
    };
    assert!(serde_json::to_value(&*observation).unwrap() == serde_json::to_value(&result).unwrap());
}
