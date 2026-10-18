//! Runtime cognition economy transition, idempotency and crash-prefix tests.

use super::super::*;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn quote(id: &str, resource: &str, amount: u64) -> CognitionLeaseQuoteV1 {
    CognitionLeaseQuoteV1::new(id, resource, amount)
}

fn authority_quote(id: &str, amount: u64, world_binding: &str) -> CognitionLeaseQuoteV1 {
    quote(id, "cognition_units", amount).with_authority(
        "payer-a",
        "cognition_units.v1",
        "provider_cognition",
        "agent_turn",
        COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
        "authority-context-a",
        world_binding,
    )
}

fn request(key: &str, amount: u64) -> CognitionLeaseRequestV1 {
    CognitionLeaseRequestV1::new(
        key,
        "account-agent-a",
        "agent-a",
        "session-a",
        "turn-a",
        "request-a",
        "request-digest-a",
        quote("quote-a", "cognition_units", amount),
    )
}

fn temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-cognition-economy-{prefix}-{nanos}"))
}

fn economy_digest_for_test<T: Serialize>(domain: &str, payload: &T) -> String {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(domain, payload))
        .expect("economy test payload is canonically encodable");
    format!("blake3:{}", blake3::hash(&bytes))
}

fn refresh_event_digest_for_test(event: &mut CognitionEconomyEventV1) {
    let mut value = serde_json::to_value(&*event).expect("economy event serializes");
    value
        .as_object_mut()
        .expect("economy event is an object")
        .remove("event_digest");
    event.event_digest = economy_digest_for_test("oasis7.cognition.economy.event.v1", &value);
}

fn refresh_head_digest_for_test(economy: &mut CognitionEconomyStateV1) {
    economy.head_digest = economy_digest_for_test(
        "oasis7.cognition.economy.journal-head.v1",
        &(economy.head_seq, &economy.journal),
    );
}

fn operation_key_for_test(lease_id: &str, operation: &str) -> String {
    economy_digest_for_test(
        "oasis7.cognition.economy.operation.v1",
        &(lease_id, operation),
    )
}

fn request_fingerprint_for_test(request: &CognitionLeaseRequestV1) -> String {
    economy_digest_for_test(
        "oasis7.cognition.economy.request.v1",
        &(
            &request.schema_version,
            &request.account_id,
            &request.agent_id,
            &request.agent_session_id,
            &request.agent_turn_id,
            &request.decision_request_id,
            &request.request_digest,
            &request.quote,
        ),
    )
}

fn receipt_id_for_test(lease_id: &str, operation_key: &str) -> String {
    economy_digest_for_test(
        "oasis7.cognition.economy.receipt-id.v1",
        &(lease_id, operation_key),
    )
}

#[test]
fn reserve_is_runtime_authoritative_and_idempotent() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 10)
        .expect("seed explicit balance");
    let first = economy
        .reserve(request("stable-key", 7), 4)
        .expect("reserve");
    assert_eq!(
        economy.available_balance("account-agent-a", "cognition_units"),
        3
    );
    assert_eq!(
        economy.reserved_balance("account-agent-a", "cognition_units"),
        7
    );
    assert_eq!(economy.journal.len(), 1);

    let replay = economy
        .reserve(request("stable-key", 7), 5)
        .expect("replay exact reserve");
    assert_eq!(replay, first);
    assert_eq!(
        economy.available_balance("account-agent-a", "cognition_units"),
        3
    );
    assert_eq!(
        economy.reserved_balance("account-agent-a", "cognition_units"),
        7
    );
    assert_eq!(economy.journal.len(), 1, "replay must not append a debit");

    let conflicting = economy.reserve(request("stable-key", 6), 5);
    assert_eq!(
        conflicting
            .expect_err("same key with changed quote must conflict")
            .code(),
        "cognition_idempotency_conflict"
    );
    assert_eq!(
        economy.available_balance("account-agent-a", "cognition_units"),
        3
    );
    assert_eq!(
        economy.reserved_balance("account-agent-a", "cognition_units"),
        7
    );
}

#[test]
fn settlement_release_and_refund_preserve_conservation_and_terminality() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 20)
        .expect("seed explicit balance");
    let lease = economy
        .reserve(request("settle-key", 8), 1)
        .expect("reserve");
    let settlement = economy.settle(&lease.lease_id, 5, 2).expect("settle");
    assert_eq!(settlement.status, CognitionLeaseStatusV1::Settled);
    assert_eq!(settlement.consumed_amount, 5);
    assert_eq!(settlement.refunded_amount, 3);
    assert_eq!(
        economy.available_balance("account-agent-a", "cognition_units"),
        15
    );
    assert_eq!(
        economy.reserved_balance("account-agent-a", "cognition_units"),
        0
    );

    let settlement_replay = economy
        .settle(&lease.lease_id, 5, 99)
        .expect("settle replay");
    assert_eq!(settlement_replay, settlement);
    let changed_settlement = economy.settle(&lease.lease_id, 4, 99);
    assert_eq!(
        changed_settlement
            .expect_err("changed settlement retry must conflict")
            .code(),
        "cognition_settlement_idempotency_conflict"
    );
    let refund = economy
        .refund(&lease.lease_id, 3)
        .expect("refund settled lease");
    assert_eq!(refund.status, CognitionLeaseStatusV1::Refunded);
    assert_eq!(refund.refunded_amount, 5);
    assert_eq!(
        economy.available_balance("account-agent-a", "cognition_units"),
        20
    );
    assert_eq!(
        economy.reserved_balance("account-agent-a", "cognition_units"),
        0
    );
    assert_eq!(
        economy.refund(&lease.lease_id, 4).expect("refund replay"),
        refund
    );

    let released = economy
        .reserve(request("release-key", 6), 5)
        .expect("second reserve");
    let release = economy.release(&released.lease_id, 6).expect("release");
    assert_eq!(release.status, CognitionLeaseStatusV1::Released);
    assert_eq!(release.released_amount, 6);
    assert_eq!(release.refunded_amount, 0);
    assert_eq!(
        economy.available_balance("account-agent-a", "cognition_units"),
        20
    );
    assert_eq!(
        economy
            .release(&released.lease_id, 7)
            .expect("release replay"),
        release
    );
}

#[test]
fn settled_refund_allows_unrelated_active_reservation() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 20)
        .expect("seed balance");
    let settled_lease = economy
        .reserve(request("settled-refund-key", 5), 1)
        .expect("reserve settled lease");
    let settled = economy
        .settle(&settled_lease.lease_id, 5, 2)
        .expect("settle lease");
    let active_lease = economy
        .reserve(request("unrelated-active-key", 3), 3)
        .expect("reserve unrelated active lease");

    let refund = economy
        .refund_settled(
            &settled_lease.lease_id,
            5,
            &settled.receipt_id,
            "provider_timeout",
            4,
        )
        .expect("unrelated reservation must not block refund");
    assert_eq!(refund.operation, "refund");
    assert_eq!(
        economy.reserved_balance("account-agent-a", "cognition_units"),
        active_lease.reserved_amount
    );
    assert_eq!(
        economy.available_balance("account-agent-a", "cognition_units"),
        17
    );
}

#[test]
fn settled_refund_fails_closed_when_its_balance_backing_is_missing() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 10)
        .expect("seed balance");
    let lease = economy
        .reserve(request("missing-refund-balance-key", 8), 1)
        .expect("reserve");
    let settled = economy.settle(&lease.lease_id, 5, 2).expect("settle");

    let mut missing_balance = economy.clone();
    missing_balance.balances.remove("account-agent-a");
    let before = missing_balance.clone();
    let error = missing_balance
        .refund_settled(
            &lease.lease_id,
            5,
            &settled.receipt_id,
            "provider_timeout",
            3,
        )
        .expect_err("refund must not create a missing balance");
    assert_eq!(error.code(), "cognition_refund_balance_missing");
    assert_eq!(missing_balance, before);

    let encoded = serde_json::to_value(&missing_balance).expect("encode malformed snapshot");
    let recovery_error = CognitionEconomyStateV1::from_snapshot_json(encoded)
        .expect_err("missing terminal balance must fail closed during recovery");
    assert_eq!(
        recovery_error.code(),
        "cognition_economy_lease_balance_missing"
    );
}

#[test]
fn quote_digest_and_expiry_are_immutable_runtime_inputs() {
    let mut forged = quote("quote-forged", "cognition_units", 2);
    forged.amount = 3;
    assert_eq!(
        forged
            .validate()
            .expect_err("mutated quote must fail digest validation")
            .code(),
        "cognition_quote_invalid"
    );

    let expired = CognitionLeaseRequestV1::new(
        "expired-key",
        "account-agent-a",
        "agent-a",
        "session-a",
        "turn-a",
        "request-a",
        "request-digest-a",
        quote("expired", "cognition_units", 1).with_valid_until_tick(2),
    );
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 5)
        .expect("seed balance");
    assert_eq!(
        economy
            .reserve(expired, 3)
            .expect_err("expired quote must not reserve")
            .code(),
        "cognition_quote_expired"
    );
    assert_eq!(
        economy.available_balance("account-agent-a", "cognition_units"),
        5
    );
    assert_eq!(economy.journal.len(), 0);
}

#[test]
fn boundary_amount_validation_returns_error_without_panicking() {
    let amount = u64::MAX;
    let request = request("max-amount-key", amount);
    let lease = CognitionLeaseV1 {
        schema_version: COGNITION_LEASE_SCHEMA_VERSION.to_string(),
        lease_id: request.derived_lease_id(),
        idempotency_key: request.idempotency_key.clone(),
        account_id: request.account_id.clone(),
        agent_id: request.agent_id.clone(),
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        request_digest: request.request_digest.clone(),
        quote: request.quote.clone(),
        reserved_amount: amount,
        settled_amount: amount,
        released_amount: 0,
        refunded_amount: amount,
        compensated_amount: 0,
        net_amount: amount,
        status: CognitionLeaseStatusV1::Settled,
        reserved_at_tick: 1,
        closed_at_tick: Some(2),
        receipt_id: Some(format!("blake3:{}", "0".repeat(64))),
    };
    let lease_result = std::panic::catch_unwind(|| lease.validate());
    assert!(
        lease_result.is_ok(),
        "lease boundary validation must return an error instead of panicking"
    );
    assert!(lease_result.unwrap().is_err());

    let mut receipt = CognitionReceiptV1 {
        schema_version: COGNITION_RECEIPT_SCHEMA_VERSION.to_string(),
        receipt_id: format!("blake3:{}", "1".repeat(64)),
        receipt_digest: String::new(),
        lease_id: lease.lease_id,
        idempotency_key: lease.idempotency_key,
        account_id: lease.account_id,
        agent_id: lease.agent_id,
        agent_session_id: lease.agent_session_id,
        agent_turn_id: lease.agent_turn_id,
        decision_request_id: lease.decision_request_id,
        request_digest: lease.request_digest,
        quote: lease.quote,
        reserved_amount: amount,
        operation: "settle".to_string(),
        consumed_amount: amount,
        released_amount: 0,
        refunded_amount: amount,
        net_amount: amount,
        parent_receipt_id: None,
        reason: None,
        status: CognitionLeaseStatusV1::Settled,
        issued_at_tick: 2,
    };
    receipt.receipt_digest = receipt.recompute_digest();
    let receipt_result = std::panic::catch_unwind(|| receipt.validate());
    assert!(
        receipt_result.is_ok(),
        "receipt boundary validation must return an error instead of panicking"
    );
    assert!(receipt_result.unwrap().is_err());
}

#[test]
fn validation_rejects_receipt_amount_mismatch_with_lease_transition() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 20)
        .expect("seed balance");
    let lease = economy
        .reserve(request("amount-linkage-key", 8), 1)
        .expect("reserve");
    economy.settle(&lease.lease_id, 5, 2).expect("settle");

    let stored_lease = economy
        .leases
        .get_mut(&lease.lease_id)
        .expect("stored lease");
    stored_lease.settled_amount = 4;
    stored_lease.refunded_amount = 4;

    assert!(
        economy.validate().is_err(),
        "receipt amounts must match the lease transition"
    );
}

#[test]
fn validation_rejects_duplicate_terminal_journal_transition() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 20)
        .expect("seed balance");
    let lease = economy
        .reserve(request("duplicate-event-key", 8), 1)
        .expect("reserve");
    economy.settle(&lease.lease_id, 5, 2).expect("settle");

    let mut duplicate = economy.journal.last().cloned().expect("settle event");
    duplicate.journal_seq = economy.journal.len() as u64 + 1;
    duplicate.parent_event_digest = economy
        .journal
        .last()
        .expect("settle event")
        .event_digest
        .clone();
    refresh_event_digest_for_test(&mut duplicate);
    economy.journal.push(duplicate);
    economy.head_seq = economy.journal.len() as u64;
    refresh_head_digest_for_test(&mut economy);

    assert!(
        economy.validate().is_err(),
        "a lease must have one canonical terminal journal transition"
    );
}

#[test]
fn authority_identity_is_bound_into_quote_and_lease_idempotency() {
    let first = CognitionLeaseRequestV1::new(
        "authority-key",
        "payer-a",
        "agent-a",
        "session-a",
        "turn-a",
        "request-a",
        "request-digest-a",
        authority_quote("authority-quote", 4, "world-a"),
    );
    let second = CognitionLeaseRequestV1::new(
        "authority-key",
        "payer-a",
        "agent-a",
        "session-a",
        "turn-a",
        "request-a",
        "request-digest-a",
        authority_quote("authority-quote", 4, "world-b"),
    );
    assert_ne!(first.derived_lease_id(), second.derived_lease_id());

    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("payer-a", "cognition_units", 8)
        .expect("seed authority payer balance");
    economy.reserve(first, 1).expect("reserve first authority");
    assert_eq!(
        economy
            .reserve(second, 1)
            .expect_err("world authority change must conflict")
            .code(),
        "cognition_idempotency_conflict"
    );

    let mut unsupported_policy = authority_quote("unsupported-policy", 1, "world-a");
    unsupported_policy.policy_revision = "dynamic_provider_metering.v1".to_string();
    unsupported_policy.refresh_digest();
    assert_eq!(
        unsupported_policy
            .validate()
            .expect_err("deferred policy revisions must fail closed")
            .code(),
        "cognition_quote_invalid"
    );
}

#[test]
fn recovery_rejects_recomputed_lease_and_receipt_payer_mismatch() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("payer-a", "cognition_units", 8)
        .expect("seed payer balance");
    let request = CognitionLeaseRequestV1::new(
        "recovery-payer-mismatch-key",
        "payer-a",
        "agent-a",
        "session-a",
        "turn-a",
        "request-a",
        "request-digest-a",
        authority_quote("recovery-payer-mismatch-quote", 4, "world-a"),
    );
    let lease = economy.reserve(request, 1).expect("reserve");
    let old_lease_id = lease.lease_id.clone();
    let old_reserve_receipt_id = lease.receipt_id.clone().expect("reserve receipt");

    let mut forged_lease = economy.leases.remove(&old_lease_id).expect("stored lease");
    forged_lease.quote.payer_id = "payer-b".to_string();
    forged_lease.quote.refresh_digest();
    let forged_request = CognitionLeaseRequestV1::new(
        forged_lease.idempotency_key.clone(),
        forged_lease.account_id.clone(),
        forged_lease.agent_id.clone(),
        forged_lease.agent_session_id.clone(),
        forged_lease.agent_turn_id.clone(),
        forged_lease.decision_request_id.clone(),
        forged_lease.request_digest.clone(),
        forged_lease.quote.clone(),
    );
    let forged_lease_id = forged_request.derived_lease_id();
    forged_lease.lease_id = forged_lease_id.clone();
    let forged_reserve_operation_key = operation_key_for_test(&forged_lease_id, "reserve");
    let forged_reserve_receipt_id =
        receipt_id_for_test(&forged_lease_id, &forged_reserve_operation_key);
    forged_lease.receipt_id = Some(forged_reserve_receipt_id.clone());
    economy
        .leases
        .insert(forged_lease_id.clone(), forged_lease.clone());

    let idempotency = economy
        .idempotency
        .get_mut(&forged_lease.idempotency_key)
        .expect("idempotency record");
    idempotency.lease_id = forged_lease_id.clone();
    idempotency.request_fingerprint = request_fingerprint_for_test(&forged_request);

    let mut forged_receipt = economy
        .receipts
        .remove(&old_reserve_receipt_id)
        .expect("reserve receipt");
    forged_receipt.lease_id = forged_lease_id.clone();
    forged_receipt.receipt_id = forged_reserve_receipt_id.clone();
    forged_receipt.quote = forged_lease.quote.clone();
    forged_receipt.receipt_digest = forged_receipt.recompute_digest();
    economy
        .receipts
        .insert(forged_reserve_receipt_id.clone(), forged_receipt);

    let old_reserve_operation_key = operation_key_for_test(&old_lease_id, "reserve");
    let mut forged_operation = economy
        .operations
        .remove(&old_reserve_operation_key)
        .expect("reserve operation");
    forged_operation.operation_key = forged_reserve_operation_key.clone();
    forged_operation.operation_digest = economy_digest_for_test(
        "oasis7.cognition.economy.operation.v1",
        &forged_reserve_operation_key,
    );
    forged_operation.lease_id = forged_lease_id.clone();
    forged_operation.receipt_id = forged_reserve_receipt_id.clone();
    economy
        .operations
        .insert(forged_reserve_operation_key.clone(), forged_operation);

    let event = economy.journal.first_mut().expect("reserve event");
    event.lease_id = forged_lease_id;
    event.operation_key = forged_reserve_operation_key;
    event.receipt_id = forged_reserve_receipt_id;
    refresh_event_digest_for_test(event);
    refresh_head_digest_for_test(&mut economy);

    let encoded = serde_json::to_value(&economy).expect("encode recomputed forged snapshot");
    let error = CognitionEconomyStateV1::from_snapshot_json(encoded)
        .expect_err("payer/account mismatch must fail closed after digest recomputation");
    assert_eq!(error.code(), "cognition_lease_invalid");
}

#[test]
fn receipt_validation_rejects_recomputed_payer_account_mismatch() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 8)
        .expect("seed balance");
    let lease = economy
        .reserve(request("receipt-payer-mismatch-key", 4), 1)
        .expect("reserve");
    let settled = economy.settle(&lease.lease_id, 4, 2).expect("settle");
    let mut forged = settled;
    forged.quote.payer_id = "payer-b".to_string();
    forged.quote.refresh_digest();
    forged.receipt_digest = forged.recompute_digest();
    let error = forged
        .validate()
        .expect_err("receipt payer/account mismatch must fail closed");
    assert_eq!(error.code(), "cognition_receipt_invalid");
}

#[test]
fn settle_requires_positive_bounded_usage_without_mutation() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 8)
        .expect("seed balance");
    let lease = economy
        .reserve(request("usage-key", 4), 1)
        .expect("reserve");

    let before_zero = economy.clone();
    let zero = economy
        .settle(&lease.lease_id, 0, 2)
        .expect_err("zero usage must be rejected");
    assert_eq!(zero.code(), "cognition_settlement_usage_zero");
    assert_eq!(economy, before_zero);

    let before_overuse = economy.clone();
    let overuse = economy
        .settle(&lease.lease_id, 5, 2)
        .expect_err("overuse must be rejected");
    assert_eq!(
        overuse.code(),
        "cognition_settlement_usage_exceeds_reservation"
    );
    assert_eq!(economy, before_overuse);
}

#[test]
fn settlement_at_lease_deadline_is_valid() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 8)
        .expect("seed balance");
    let lease = economy
        .reserve(
            CognitionLeaseRequestV1::new(
                "deadline-boundary-key",
                "account-agent-a",
                "agent-a",
                "session-a",
                "turn-a",
                "request-a",
                "request-digest-a",
                quote("deadline-boundary-quote", "cognition_units", 4).with_valid_until_tick(3),
            ),
            1,
        )
        .expect("reserve before deadline");

    let settled = economy
        .settle(&lease.lease_id, 3, 3)
        .expect("settlement at the inclusive deadline is valid");
    assert_eq!(settled.status, CognitionLeaseStatusV1::Settled);
    assert_eq!(settled.operation, "settle");
    assert_eq!(settled.consumed_amount, 3);
    assert_eq!(settled.refunded_amount, 1);
    assert_eq!(
        economy.available_balance("account-agent-a", "cognition_units"),
        5
    );
    assert_eq!(
        economy.reserved_balance("account-agent-a", "cognition_units"),
        0
    );
    assert_eq!(
        economy.journal.last().expect("settle event").event_kind,
        "settle"
    );
    economy
        .validate()
        .expect("deadline settlement preserves invariants");
}

#[test]
fn late_settlement_atomically_expires_reserved_lease() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 8)
        .expect("seed balance");
    let lease = economy
        .reserve(
            CognitionLeaseRequestV1::new(
                "deadline-late-key",
                "account-agent-a",
                "agent-a",
                "session-a",
                "turn-a",
                "request-a",
                "request-digest-a",
                quote("deadline-late-quote", "cognition_units", 4).with_valid_until_tick(3),
            ),
            1,
        )
        .expect("reserve before deadline");

    let expired = economy
        .settle(&lease.lease_id, 3, 4)
        .expect("late settlement closes the lease through expiry");
    assert_eq!(expired.status, CognitionLeaseStatusV1::Expired);
    assert_eq!(expired.operation, "expire");
    assert_eq!(expired.consumed_amount, 0);
    assert_eq!(expired.released_amount, 4);
    assert_eq!(expired.refunded_amount, 0);
    assert_eq!(expired.reason.as_deref(), Some("quote_expired"));
    assert_eq!(
        economy.available_balance("account-agent-a", "cognition_units"),
        8
    );
    assert_eq!(
        economy.reserved_balance("account-agent-a", "cognition_units"),
        0
    );
    assert_eq!(
        economy.leases[&lease.lease_id].status,
        CognitionLeaseStatusV1::Expired
    );
    assert_eq!(
        economy.receipts.len(),
        2,
        "reserve and expiry receipts are linked"
    );
    assert_eq!(
        economy.journal.len(),
        2,
        "expiry appends one terminal event"
    );
    assert_eq!(
        economy.journal.last().expect("expiry event").event_kind,
        "expire"
    );
    economy
        .validate()
        .expect("atomic expiry preserves invariants");
    let encoded = economy.snapshot_json().expect("encode expired economy");
    let restored = CognitionEconomyStateV1::from_snapshot_json(encoded)
        .expect("decode atomically expired economy");
    assert_eq!(
        restored, economy,
        "expiry receipt and journal replay deterministically"
    );

    let after_expiry = economy.clone();
    let retry = economy
        .settle(&lease.lease_id, 3, 5)
        .expect_err("an expired lease rejects late settlement retries");
    assert_eq!(retry.code(), "cognition_lease_already_closed");
    assert_eq!(
        economy, after_expiry,
        "late retries do not append a second transition"
    );
}

#[test]
fn release_and_expire_have_distinct_terminal_receipts() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 12)
        .expect("seed balance");
    let release_lease = economy
        .reserve(
            CognitionLeaseRequestV1::new(
                "release-distinct-key",
                "account-agent-a",
                "agent-a",
                "session-a",
                "turn-a",
                "request-a",
                "request-digest-a",
                quote("release-distinct-quote", "cognition_units", 4).with_valid_until_tick(10),
            ),
            1,
        )
        .expect("reserve release lease");
    let release = economy
        .release(&release_lease.lease_id, 2)
        .expect("release reserved lease");
    assert_eq!(release.operation, "release");
    assert_eq!(release.released_amount, 4);
    assert_eq!(release.refunded_amount, 0);
    assert_eq!(release.net_amount, 0);

    let expire_lease = economy
        .reserve(
            CognitionLeaseRequestV1::new(
                "expire-distinct-key",
                "account-agent-a",
                "agent-a",
                "session-a",
                "turn-b",
                "request-b",
                "request-digest-b",
                quote("expire-distinct-quote", "cognition_units", 3).with_valid_until_tick(4),
            ),
            1,
        )
        .expect("reserve expiring lease");
    let expired = economy
        .expire(&expire_lease.lease_id, 5)
        .expect("expire expired lease");
    assert_eq!(expired.operation, "expire");
    assert_eq!(expired.status, CognitionLeaseStatusV1::Expired);
    assert_eq!(expired.released_amount, 3);
    assert_eq!(expired.refunded_amount, 0);
    assert_eq!(
        economy
            .settle(&expire_lease.lease_id, 1, 6)
            .expect_err("expired lease cannot be settled")
            .code(),
        "cognition_lease_already_closed"
    );
}

#[test]
fn reserve_journal_requires_current_linkage_but_accepts_truly_legacy_prefix() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 8)
        .expect("seed balance");
    let lease = economy
        .reserve(request("reserve-linkage-key", 4), 1)
        .expect("reserve");
    let reserve_receipt_id = lease.receipt_id.clone().expect("reserve receipt");
    let reserve_operation_key = operation_key_for_test(&lease.lease_id, "reserve");

    let mut empty_linkage = economy.clone();
    empty_linkage.journal[0].receipt_id.clear();
    refresh_event_digest_for_test(&mut empty_linkage.journal[0]);
    refresh_head_digest_for_test(&mut empty_linkage);
    let error = empty_linkage
        .validate()
        .expect_err("current reserve records require journal receipt linkage");
    assert_eq!(
        error.code(),
        "cognition_economy_journal_reserve_receipt_missing"
    );

    let mut missing_receipt = economy.clone();
    missing_receipt.receipts.remove(&reserve_receipt_id);
    assert!(
        missing_receipt.validate().is_err(),
        "reserve event cannot outlive its receipt"
    );

    let mut legacy = economy;
    legacy.receipts.remove(&reserve_receipt_id);
    legacy.operations.remove(&reserve_operation_key);
    legacy
        .leases
        .get_mut(&lease.lease_id)
        .expect("legacy lease")
        .receipt_id = None;
    legacy.journal[0].receipt_id.clear();
    refresh_event_digest_for_test(&mut legacy.journal[0]);
    refresh_head_digest_for_test(&mut legacy);
    assert!(
        legacy.validate().is_ok(),
        "legacy reserve prefix remains readable without new linkage"
    );
}

#[test]
fn settled_refund_requires_parent_receipt_reason_and_bounded_amount() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 10)
        .expect("seed balance");
    let lease = economy
        .reserve(request("compensate-key", 8), 1)
        .expect("reserve");
    let settled = economy.settle(&lease.lease_id, 5, 2).expect("settle");
    let before_invalid = economy.clone();
    let invalid = economy
        .refund_settled(
            &lease.lease_id,
            6,
            &settled.receipt_id,
            "provider_timeout",
            3,
        )
        .expect_err("refund cannot exceed parent consumed amount");
    assert_eq!(invalid.code(), "cognition_refund_amount_invalid");
    assert_eq!(economy, before_invalid);

    let refund = economy
        .refund_settled(
            &lease.lease_id,
            5,
            &settled.receipt_id,
            "provider_timeout",
            3,
        )
        .expect("Runtime-authorized compensation");
    assert_eq!(refund.operation, "refund");
    assert_eq!(
        refund.parent_receipt_id.as_deref(),
        Some(settled.receipt_id.as_str())
    );
    assert_eq!(refund.reason.as_deref(), Some("provider_timeout"));
    assert_eq!(refund.consumed_amount, 5);
    assert_eq!(refund.refunded_amount, 5);
    assert_eq!(refund.net_amount, 0);
    assert_eq!(
        economy
            .refund_settled(
                &lease.lease_id,
                5,
                &settled.receipt_id,
                "provider_timeout",
                99,
            )
            .expect("compensation replay"),
        refund
    );
}

#[test]
fn every_durable_crash_prefix_round_trips_without_scheduler_state() {
    let mut economy = CognitionEconomyStateV1::new();
    economy
        .set_resource_balance("account-agent-a", "cognition_units", 12)
        .expect("seed balance");
    let prefixes = [
        economy.clone(),
        {
            let mut prefix = economy.clone();
            prefix
                .reserve(request("prefix-key", 4), 7)
                .expect("reserve prefix");
            prefix
        },
        {
            let mut prefix = economy.clone();
            let lease = prefix
                .reserve(request("prefix-key", 4), 7)
                .expect("reserve prefix");
            prefix.settle(&lease.lease_id, 3, 8).expect("settle prefix");
            prefix
        },
    ];
    for prefix in prefixes {
        let encoded = prefix.snapshot_json().expect("encode crash prefix");
        let restored = CognitionEconomyStateV1::from_snapshot_json(encoded.clone())
            .expect("decode crash prefix");
        assert_eq!(restored, prefix);
        assert_eq!(restored.snapshot_json().expect("re-encode"), encoded);
        assert!(restored.validate().is_ok());
    }
}

#[test]
fn world_save_load_retains_typed_economy_projection_atomically() {
    let mut world = World::new();
    world
        .set_cognition_resource_balance("account-agent-a", "cognition_units", 9)
        .expect("seed world cognition balance");
    let scheduler_before = world.cognition().get("scheduler_state").cloned();
    let lease = world
        .reserve_cognition_lease(request("world-key", 6))
        .expect("world reserve");
    assert_eq!(
        world.cognition().get("scheduler_state").cloned(),
        scheduler_before
    );

    let dir = temp_dir("persist");
    world.save_to_dir(&dir).expect("save reserved world");
    let restored = World::load_from_dir(&dir).expect("load reserved world");
    assert_eq!(
        restored.cognition_economy().expect("typed economy"),
        world.cognition_economy().expect("typed economy")
    );
    assert_eq!(
        restored
            .cognition_economy()
            .expect("typed economy")
            .reserved_balance("account-agent-a", "cognition_units"),
        6
    );

    let mut restored = restored;
    restored
        .settle_cognition_lease(&lease.lease_id, 4)
        .expect("settle restored lease");
    let round_trip = temp_dir("round-trip");
    restored
        .save_to_dir(&round_trip)
        .expect("save settled world");
    let loaded = World::load_from_dir(&round_trip).expect("load settled world");
    let economy = loaded.cognition_economy().expect("typed settled economy");
    assert_eq!(
        economy.available_balance("account-agent-a", "cognition_units"),
        5
    );
    assert_eq!(
        economy.reserved_balance("account-agent-a", "cognition_units"),
        0
    );
    assert_eq!(economy.receipts.len(), 2);
    assert_eq!(
        economy.leases[&lease.lease_id].status,
        CognitionLeaseStatusV1::Settled
    );
    assert_eq!(economy.journal.len(), 2);
    assert_eq!(
        serde_json::to_value(economy).expect("json economy")["schema_version"],
        Value::String(COGNITION_ECONOMY_SCHEMA_VERSION.to_string())
    );

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&round_trip);
}

fn bound_world_with_identity(agent_id: &str, owner_binding: &str) -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register provider agent");
    world
        .bind_cognition_runtime("provision-world", "main", 0, None, "pending", 0)
        .expect("bind cognition runtime");
    world
        .install_capability_agent_identity(agent_id, owner_binding, 1)
        .expect("install capability identity");
    world
}

#[test]
fn provisioning_is_owner_bound_once_and_exactly_idempotent() {
    let mut world = bound_world_with_identity("agent-a", "owner-a");
    let first = world
        .provision_cognition_for_agent("agent-a", "provision-a", "authority-a", 7)
        .expect("provision allowance");
    assert_eq!(first.request.account_id, "owner-a");
    assert_eq!(first.request.owner_binding, "owner-a");
    assert_eq!(
        first.request.resource_version,
        COGNITION_RESOURCE_VERSION_V1
    );
    assert_eq!(
        first.request.policy_revision,
        COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION
    );
    assert!(!first.request.authority_digest.is_empty());
    assert!(!first.request.provisioning_digest.is_empty());

    let before = world.cognition_economy().expect("read provisioned economy");
    assert_eq!(before.available_balance("owner-a", "cognition_units"), 7);
    assert_eq!(before.provisions.len(), 1);
    assert_eq!(before.provision_receipts.len(), 1);
    assert_eq!(before.provision_journal.len(), 1);
    assert_eq!(before.provision_head_seq, 1);

    let replay = world
        .provision_cognition_for_agent("agent-a", "provision-a", "authority-a", 7)
        .expect("exact provision replay");
    assert_eq!(replay, first);
    assert_eq!(
        world.cognition_economy().expect("read replayed economy"),
        before,
        "exact replay must not refill or append journal evidence"
    );
}

#[test]
fn provisioning_rejects_changed_allowance_authority_or_owner() {
    let mut world = bound_world_with_identity("agent-a", "owner-a");
    world
        .provision_cognition_for_agent("agent-a", "provision-a", "authority-a", 7)
        .expect("provision allowance");

    let changed_amount = world
        .provision_cognition_for_agent("agent-a", "provision-a", "authority-a", 8)
        .expect_err("changed amount must conflict");
    assert!(format!("{changed_amount:?}").contains("cognition_provisioning_idempotency_conflict"));

    let changed_authority = world
        .provision_cognition_for_agent("agent-a", "provision-a", "authority-b", 7)
        .expect_err("changed authority must conflict");
    assert!(
        format!("{changed_authority:?}").contains("cognition_provisioning_idempotency_conflict")
    );

    world
        .install_capability_agent_identity("agent-a", "owner-b", 2)
        .expect("rotate owner identity");
    let changed_owner = world
        .provision_cognition_for_agent("agent-a", "provision-a", "authority-a", 7)
        .expect_err("changed owner must conflict");
    assert!(format!("{changed_owner:?}").contains("cognition_provisioning_idempotency_conflict"));
}

#[test]
fn provisioning_persists_across_restart_without_legacy_implicit_seed() {
    let mut world = bound_world_with_identity("agent-a", "owner-a");
    world
        .provision_cognition_for_agent("agent-a", "provision-a", "authority-a", 7)
        .expect("provision allowance");
    let dir = temp_dir("provision-restart");
    world.save_to_dir(&dir).expect("save provisioned world");

    let restored = World::load_from_dir(&dir).expect("restore provisioned world");
    assert_eq!(
        restored.cognition_economy().expect("restored economy"),
        world.cognition_economy().expect("original economy")
    );
    let mut restored_economy = restored.cognition_economy().expect("restored economy");
    let before_replay = restored_economy.clone();
    let replay = restored_economy
        .provision(
            before_replay
                .provisions
                .get("provision-a")
                .expect("restored provision")
                .request
                .clone(),
            0,
        )
        .expect("replay restored provision");
    assert_eq!(
        replay.request.provisioning_digest,
        before_replay
            .provisions
            .get("provision-a")
            .expect("restored provision")
            .request
            .provisioning_digest
    );
    assert_eq!(restored_economy, before_replay);

    let legacy_dir = temp_dir("provision-legacy");
    World::new()
        .save_to_dir(&legacy_dir)
        .expect("save legacy world");
    let snapshot_path = legacy_dir.join("snapshot.json");
    let snapshot_bytes = fs::read(&snapshot_path).expect("read legacy snapshot");
    let mut snapshot: Value = serde_json::from_slice(&snapshot_bytes).expect("decode snapshot");
    snapshot
        .as_object_mut()
        .expect("snapshot object")
        .remove("cognition");
    fs::write(
        &snapshot_path,
        serde_json::to_vec_pretty(&snapshot).expect("encode legacy snapshot"),
    )
    .expect("write legacy snapshot");
    let _ = fs::remove_dir_all(legacy_dir.join(".distfs-state"));
    let _ = fs::remove_file(legacy_dir.join("snapshot.manifest.json"));
    let _ = fs::remove_file(legacy_dir.join("journal.segments.json"));
    let legacy = World::load_from_dir(&legacy_dir).expect("restore legacy world");
    let legacy_economy = legacy.cognition_economy().expect("legacy economy default");
    assert!(legacy_economy.provisions.is_empty());
    assert!(legacy_economy.provision_receipts.is_empty());
    assert!(legacy_economy.provision_journal.is_empty());

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&legacy_dir);
}
