//! Native pure/codec evidence only; no Ready RPC or actual material claim.
use super::*;
use BftOriginalRetentionTxnSnapshotV1 as Snapshot;
use serde_cbor::Value;

struct Fixture {
    binding: BftSignerBindingV3,
    role: BftPrivateRetentionAttestorV1,
    snapshot: BftFirstReserveSnapshotV1,
    ready: BftCurrentRetentionV1,
}
impl Fixture {
    fn verify(&self) -> Result<(), BftSignerErrorV3> {
        self.ready.verify(&self.binding, &self.role, &self.snapshot)
    }
    fn choice(&mut self) -> &mut BftPrivateRetentionSelectionV1 {
        &mut self.ready.selection_index.selections[0]
    }
}
fn fixture() -> Fixture {
    let binding = binding();
    let mut snapshot = retained(&binding).current;
    let marker = &snapshot.current.marker;
    let role = BftPrivateRetentionAttestorV1 {
        schema_version: 1,
        provider_public_key: public(33),
        source: marker.source.clone(),
        creation_marker_digest: marker.digest().unwrap(),
        coverage_start_height: marker.initial_height,
        allow_authenticated_successor_origins: false,
    };
    let original = snapshot.reservation.as_ref().unwrap();
    let material = original
        .ledger
        .receipt
        .body
        .head
        .retained_material_bytes
        .unwrap();
    let (mut ready, after) = BftCurrentRetentionV1::prepare_first(
        original,
        &snapshot.current.signer.state,
        &hash(90),
        128,
        material + 65536,
        1,
    )
    .unwrap();
    ready.ledger.receipt.signature_hex = crypto::sign_preimage(
        &Key(33),
        fresh_coverage::RECEIPT_DOMAIN,
        &ready.ledger.receipt.body,
    )
    .unwrap();
    snapshot.current.signer.state = after;
    Fixture {
        binding,
        role,
        snapshot,
        ready,
    }
}

#[test]
fn ready_verifies_original_states_separately_from_new_current_lease() {
    let mut f = fixture();
    f.verify().unwrap();
    let original = f.snapshot.reservation.clone();
    let activation = f.snapshot.current.original.clone();
    let signed = crypto::encode_bounded(&f.ready.ledger.receipt, 16384).unwrap();
    f.snapshot.current.signer.lease_generation += 1;
    f.snapshot.current.signer.lease_token_digest = hash(99);
    f.verify().unwrap();
    assert_eq!(f.snapshot.reservation, original);
    assert_eq!(f.snapshot.current.original, activation);
    assert_eq!(
        crypto::encode_bounded(&f.ready.ledger.receipt, 16384).unwrap(),
        signed
    );
    // A current Ready checkpoint cannot be used as another first predecessor.
    assert!(
        BftCurrentRetentionV1::prepare_first(
            f.snapshot.reservation.as_ref().unwrap(),
            &f.snapshot.current.signer.state,
            &hash(90),
            128,
            1024 * 1024 * 1024,
            2
        )
        .is_err()
    );
    f.role.provider_public_key = public(32);
    assert!(f.verify().is_err());
}

#[test]
fn ready_rejects_changed_fact_choice_predecessor_and_original_signatures() {
    let corruptions: &[fn(&mut Fixture)] = &[
        |f| f.snapshot.current.original = None,
        |f| f.snapshot.reservation = None,
        |f| f.snapshot.current.signer.state.state_digest = hash(4),
        |f| f.snapshot.current.signer.lease_generation = 0,
        |f| f.ready.selection_index.last_sequence = 2,
        |f| f.choice().key.header_id = hash(4),
        |f| f.choice().source.handoff_digest = hash(4),
        |f| f.choice().reference.admission_digest = hash(4),
        |f| f.choice().sequence = 2,
        |f| f.choice().local_identity_bytes = 0,
        |f| f.ready.ledger.map = f.snapshot.reservation.as_ref().unwrap().ledger.map.clone(),
        |f| f.ready.ledger.receipt.signature_hex.push('0'),
        |f| {
            f.snapshot
                .current
                .original
                .as_mut()
                .unwrap()
                .receipt
                .signature_hex
                .push('0')
        },
        |f| {
            f.snapshot
                .reservation
                .as_mut()
                .unwrap()
                .ledger
                .receipt
                .signature_hex
                .push('0')
        },
        |f| {
            let BftPrivateRetentionSelectionOperationV1::ReadyOriginalLocal {
                expected_head_digest,
                ..
            } = &mut f.choice().operation;
            *expected_head_digest = hash(4);
        },
        |f| {
            let BftPrivateRetentionSelectionOperationV1::ReadyOriginalLocal {
                expected_state_digest,
                ..
            } = &mut f.choice().operation;
            *expected_state_digest = hash(4);
        },
    ];
    for corrupt in corruptions {
        let mut f = fixture();
        corrupt(&mut f);
        assert!(f.verify().is_err());
    }
    let mut f = fixture();
    f.ready
        .ledger
        .receipt
        .body
        .head
        .selection_commitment
        .as_mut()
        .unwrap()
        .root = hash(4);
    assert!(f.verify().is_err());
}

fn map(value: &mut Value) -> &mut BTreeMap<Value, Value> {
    let Value::Map(map) = value else {
        panic!("expected map")
    };
    map
}
fn field<'a>(value: &'a mut Value, name: &str) -> &'a mut Value {
    map(value).get_mut(&Value::Text(name.into())).unwrap()
}
fn rejected(value: Value) {
    let bytes = serde_cbor::to_vec(&value).unwrap();
    // Assert semantic rejection separately from map-order canonical rejection.
    if let Ok(decoded) = serde_cbor::from_slice::<codec::Stored>(&bytes) {
        assert!(decoded.encode().is_err());
        let canonical = serde_cbor::to_vec(&decoded).unwrap();
        assert!(codec::Stored::decode(&canonical).is_err());
    }
    assert!(codec::Stored::decode(&bytes).is_err());
}

#[test]
fn stored_one_rejects_new_heads_in_all_current_and_original_positions() {
    let ready = fixture().ready;
    for position in 0..3 {
        for replace in [false, true] {
            let mut legacy = retained(&binding());
            let head = match position {
                0 => legacy
                    .current
                    .current
                    .signer
                    .state
                    .private_retention
                    .as_mut()
                    .unwrap(),
                1 => {
                    &mut legacy
                        .current
                        .current
                        .original
                        .as_mut()
                        .unwrap()
                        .receipt
                        .body
                        .head
                }
                _ => {
                    &mut legacy
                        .current
                        .reservation
                        .as_mut()
                        .unwrap()
                        .ledger
                        .receipt
                        .body
                        .head
                }
            };
            if replace {
                *head = ready.ledger.receipt.body.head.clone();
            } else {
                head.selection_commitment =
                    ready.ledger.receipt.body.head.selection_commitment.clone();
            }
            let signer = legacy.current.current.signer.clone();
            if position == 0 {
                assert!(codec::Stored::new("codec", signer.clone(), None).is_err());
            }
            assert!(codec::Stored::new("codec", signer, Some(legacy)).is_err());
        }
    }
    let mut legacy = retained(&binding());
    legacy.current.reservation.as_mut().unwrap().ledger.map = ready.ledger.map;
    assert!(
        codec::Stored::new("codec", legacy.current.current.signer.clone(), Some(legacy)).is_err()
    );
}

#[test]
fn stored_one_and_head_versions_reject_missing_null_and_unknown_fields() {
    let legacy = retained(&binding());
    let old =
        codec::Stored::new("codec", legacy.current.current.signer.clone(), Some(legacy)).unwrap();
    let old_value = serde_cbor::value::to_value(&old).unwrap();
    let normalized: codec::Stored =
        serde_cbor::from_slice(&serde_cbor::to_vec(&old_value).unwrap()).unwrap();
    assert_eq!(normalized.encode().unwrap(), old.encode().unwrap());
    for version in [0, 2, 3] {
        let mut bad = old_value.clone();
        *field(&mut bad, "schema_version") = Value::Integer(version);
        rejected(bad);
    }
    for value in [
        Value::Null,
        serde_cbor::value::to_value(fixture().ready).unwrap(),
    ] {
        let mut bad = old_value.clone();
        let envelope = field(field(field(&mut bad, "body"), "retention"), "value");
        map(envelope).insert(Value::Text("current_retention".into()), value);
        rejected(bad);
    }
    let head = fixture().ready.ledger.receipt.body.head;
    for (count, sequence, encoded_bytes) in [
        (0, 1, 1),
        (2, 1, 1),
        (1, 0, 1),
        (1, 2, 1),
        (1, 1, 0),
        (1, 1, 1024 * 1024 + 1),
    ] {
        let mut invalid = head.clone();
        let commitment = invalid.selection_commitment.as_mut().unwrap();
        commitment.count = count;
        commitment.sequence = sequence;
        commitment.encoded_bytes = encoded_bytes;
        assert!(invalid.digest().is_err());
    }
    for version in [2, 3] {
        let mut value = serde_cbor::value::to_value(&head).unwrap();
        *field(&mut value, "schema_version") = Value::Integer(version);
        *field(&mut value, "selection_commitment") = Value::Null;
        assert!(
            serde_cbor::from_slice::<BftPrivateRetentionHeadV1>(
                &serde_cbor::to_vec(&value).unwrap()
            )
            .is_err()
        );
    }
    let mut missing = head;
    missing.selection_commitment = None;
    assert!(missing.digest().is_err());
    missing.schema_version = 1;
    assert!(missing.digest().is_err());
}

fn current(snapshot: &mut Snapshot) -> &mut BftCurrentRetentionV1 {
    snapshot.current_retention.as_mut().unwrap()
}
fn choice(snapshot: &mut Snapshot) -> &mut BftPrivateRetentionSelectionV1 {
    &mut current(snapshot).selection_index.selections[0]
}
fn ready_record(snapshot: &Snapshot) -> &Original {
    &snapshot.original_operations[&format!(
        "first-ready:{}",
        snapshot
            .current_retention
            .as_ref()
            .unwrap()
            .ledger
            .receipt
            .body
            .operation_digest
    )]
}
fn store_receipt(operation: &mut Original, receipt: &BftPrivateRetentionReceiptV1) {
    operation.signed_receipt_bytes = crypto::encode_bounded(receipt, 16384).unwrap();
    operation.original_receipt_digest =
        Original::receipt_digest_from_original_bytes(&operation.signed_receipt_bytes);
}

fn envelope_fixture() -> (Fixture, Snapshot, Snapshot) {
    let f = fixture();
    let mut before = retained(&f.binding);
    let mut reserve = before.original_operations.remove("original-1").unwrap();
    before.current.current.signer.lease_token_digest = reserve.original_lease_token_digest.clone();
    let original = &before.current.reservation.as_ref().unwrap().ledger.receipt;
    reserve.operation_id = format!("first-reserve:{}", original.body.operation_digest);
    store_receipt(&mut reserve, original);
    before
        .original_operations
        .insert(reserve.operation_id.clone(), reserve.clone());
    let mut next = before.current.clone();
    next.current.signer.state = f.snapshot.current.signer.state.clone();
    next.current.reserved_response_bytes += 100;
    let mut operation = reserve;
    let body = &f.ready.ledger.receipt.body;
    operation.operation_id = format!("first-ready:{}", body.operation_digest);
    operation.before_revision = body.before_revision;
    operation.before_state_digest = body.before_state_digest.clone();
    operation.after_revision = body.after_revision;
    operation.after_state_digest = body.after_state_digest.clone();
    // Byte identity only. The actual Ready command and pre-sign price belong
    // to the authenticated service adapter, not this pure transaction fixture.
    operation.signed_request_bytes = b"distinct-original-ready-request".to_vec();
    operation.original_request_digest =
        Original::request_digest_from_original_bytes(&operation.signed_request_bytes);
    store_receipt(&mut operation, &f.ready.ledger.receipt);
    operation.cumulative_reserved_response_bytes = next.current.reserved_response_bytes;
    let after = before
        .prepare_first_ready_operation(&f.binding, &f.role, next, f.ready.clone(), operation)
        .unwrap();
    (f, before, after)
}

#[test]
fn ready_envelope_preserves_originals_and_exact_retry_across_new_lease() {
    let (f, before, after) = envelope_fixture();
    assert_eq!(after.current.reservation, before.current.reservation);
    assert_eq!(
        after.current.current.original,
        before.current.current.original
    );
    for (id, original) in &before.original_operations {
        assert_eq!(after.original_operations.get(id), Some(original));
    }
    let operation = ready_record(&after).clone();
    let mut cold = after.clone();
    cold.current.current.signer.lease_generation += 1;
    cold.current.current.signer.lease_token_digest = hash(99);
    let retry = |snapshot: &Snapshot, operation| {
        snapshot.prepare_first_ready_operation(
            &f.binding,
            &f.role,
            snapshot.current.clone(),
            f.ready.clone(),
            operation,
        )
    };
    f.ready.verify(&f.binding, &f.role, &cold.current).unwrap();
    assert_eq!(
        cold.lookup_original(&operation.operation_id, &operation.signed_request_bytes)
            .unwrap(),
        Some(&operation)
    );
    assert_eq!(retry(&cold, operation.clone()).unwrap(), cold);
    assert_eq!(
        cold.prepare_single_atomic_operation(cold.current.clone(), operation.clone())
            .unwrap(),
        cold
    );
    let mut substituted = operation.clone();
    substituted.signed_request_bytes.push(1);
    substituted.original_request_digest =
        Original::request_digest_from_original_bytes(&substituted.signed_request_bytes);
    substituted.original_lease_generation = 2;
    substituted.original_lease_token_digest = hash(99);
    assert!(retry(&cold, substituted.clone()).is_err());
    assert!(
        cold.lookup_original(&operation.operation_id, &substituted.signed_request_bytes)
            .is_err()
    );
    substituted.operation_id.push('x');
    assert!(retry(&cold, substituted.clone()).is_err());
    assert!(
        cold.prepare_single_atomic_operation(cold.current.clone(), substituted)
            .is_err()
    );
    let (mut replacement, state) = BftCurrentRetentionV1::prepare_first(
        before.current.reservation.as_ref().unwrap(),
        &before.current.current.signer.state,
        &hash(91),
        129,
        f.ready
            .ledger
            .receipt
            .body
            .head
            .retained_material_bytes
            .unwrap(),
        2,
    )
    .unwrap();
    replacement.ledger.receipt.signature_hex = crypto::sign_preimage(
        &Key(33),
        fresh_coverage::RECEIPT_DOMAIN,
        &replacement.ledger.receipt.body,
    )
    .unwrap();
    let mut next = cold.current.clone();
    next.current.signer.state = state;
    replacement.verify(&f.binding, &f.role, &next).unwrap();
    let mut new_choice = operation.clone();
    new_choice.operation_id = format!(
        "first-ready:{}",
        replacement.ledger.receipt.body.operation_digest
    );
    new_choice.original_lease_generation = 2;
    new_choice.original_lease_token_digest = hash(99);
    new_choice.after_state_digest = next.current.signer.state.state_digest.clone();
    store_receipt(&mut new_choice, &replacement.ledger.receipt);
    assert_eq!(
        cold.prepare_first_ready_operation(&f.binding, &f.role, next, replacement, new_choice)
            .unwrap_err()
            .0,
        "first Ready replaces original selection/operation"
    );
    let charged = cold
        .prepare_original_response_charge(2, &hash(99), 17)
        .unwrap();
    assert_eq!(charged.original_operations, cold.original_operations);
    assert_eq!(charged.current_retention, cold.current_retention);
    let mut expected = cold.clone();
    expected.current.current.reserved_response_bytes += 17;
    assert_eq!(charged, expected);
    assert_eq!(cold.current_retention, after.current_retention);
}

#[test]
fn ready_envelope_rejects_partial_history_records_and_replaced_choices() {
    let (f, before, after) = envelope_fixture();
    let corruptions: &[fn(&mut Snapshot)] = &[
        |s| s.current_retention = None,
        |s| s.current.current.original = None,
        |s| s.current.reservation = None,
        |s| s.original_operations.clear(),
        |s| s.current.current.signer.state.state_digest = hash(4),
        |s| current(s).selection_index.last_sequence = 2,
        |s| choice(s).key.header_id = hash(4),
        |s| choice(s).source.handoff_digest = hash(4),
        |s| choice(s).reference.admission_digest = hash(4),
        |s| choice(s).sequence = 2,
        |s| current(s).ledger.map = s.current.reservation.as_ref().unwrap().ledger.map.clone(),
        |s| {
            let BftPrivateRetentionSelectionOperationV1::ReadyOriginalLocal {
                expected_head_digest,
                ..
            } = &mut choice(s).operation;
            *expected_head_digest = hash(4);
        },
        |s| {
            current(s)
                .ledger
                .receipt
                .body
                .head
                .selection_commitment
                .as_mut()
                .unwrap()
                .root = hash(4)
        },
    ];
    for corrupt in corruptions {
        let mut wrong = after.clone();
        corrupt(&mut wrong);
        assert!(wrong.validate().is_err());
    }
    for prefix in ["first-reserve:", "first-ready:"] {
        let mut missing = after.clone();
        missing
            .original_operations
            .retain(|id, _| !id.starts_with(prefix));
        assert!(missing.validate().is_err());
    }
    let corrupt_records: &[fn(&mut Original)] = &[
        |r| r.signed_request_bytes.push(1),
        |r| {
            r.signed_receipt_bytes.push(1);
            r.original_receipt_digest =
                Original::receipt_digest_from_original_bytes(&r.signed_receipt_bytes);
        },
        |r| r.before_state_digest = hash(4),
        |r| r.after_state_digest = hash(4),
        |r| r.original_lease_generation = 2,
        |r| r.input_refs[0].admission_digest = hash(4),
    ];
    for prefix in ["first-reserve:", "first-ready:"] {
        for corrupt in corrupt_records {
            let mut wrong = after.clone();
            corrupt(
                wrong
                    .original_operations
                    .values_mut()
                    .find(|r| r.operation_id.starts_with(prefix))
                    .unwrap(),
            );
            assert!(wrong.validate().is_err());
        }
    }
    let mut missing = before;
    missing.original_operations.clear();
    assert!(
        missing
            .prepare_first_ready_operation(
                &f.binding,
                &f.role,
                after.current.clone(),
                f.ready,
                ready_record(&after).clone()
            )
            .is_err()
    );
}

fn envelope(value: &mut Value) -> &mut Value {
    field(field(field(value, "body"), "retention"), "value")
}

#[test]
fn stored_two_roundtrips_complete_originals_and_rejects_missing_history_and_downgrade() {
    let (_, _, after) = envelope_fixture();
    let stored = codec::Stored::new(
        "codec",
        after.current.current.signer.clone(),
        Some(after.clone()),
    )
    .unwrap();
    let bytes = stored.encode().unwrap();
    let decoded = codec::Stored::decode(&bytes).unwrap();
    assert_eq!(decoded, stored);
    assert_eq!(decoded.encode().unwrap(), bytes);
    let value = serde_cbor::value::to_value(&stored).unwrap();
    let normalized: codec::Stored =
        serde_cbor::from_slice(&serde_cbor::to_vec(&value).unwrap()).unwrap();
    assert_eq!(normalized.encode().unwrap(), bytes);
    for version in [0, 1, 3] {
        let mut bad = value.clone();
        *field(&mut bad, "schema_version") = Value::Integer(version);
        rejected(bad);
    }
    let mut downgrade = value.clone();
    *field(&mut downgrade, "schema_version") = Value::Integer(1);
    map(envelope(&mut downgrade)).remove(&Value::Text("current_retention".into()));
    rejected(downgrade);
    for null in [false, true] {
        for path in ["current_retention", "original", "reservation"] {
            let mut bad = value.clone();
            let target = match path {
                "original" => field(field(envelope(&mut bad), "current"), "current"),
                "reservation" => field(envelope(&mut bad), "current"),
                _ => envelope(&mut bad),
            };
            if null {
                *field(target, path) = Value::Null;
            } else {
                map(target).remove(&Value::Text(path.into()));
            }
            rejected(bad);
        }
    }
    for id in after.original_operations.keys() {
        let mut bad = value.clone();
        map(field(envelope(&mut bad), "original_operations")).remove(&Value::Text(id.clone()));
        rejected(bad);
    }
    for position in 0..2 {
        for replace in [false, true] {
            let mut bad = value.clone();
            let first = field(envelope(&mut bad), "current");
            let ledger = if position == 0 {
                field(field(first, "current"), "original")
            } else {
                field(field(first, "reservation"), "ledger")
            };
            let head = field(field(field(ledger, "receipt"), "body"), "head");
            let ready = &after
                .current_retention
                .as_ref()
                .unwrap()
                .ledger
                .receipt
                .body
                .head;
            if replace {
                *head = serde_cbor::value::to_value(ready).unwrap();
            } else {
                map(head).insert(
                    Value::Text("selection_commitment".into()),
                    serde_cbor::value::to_value(ready.selection_commitment.as_ref().unwrap())
                        .unwrap(),
                );
            }
            rejected(bad);
        }
    }
    let mut bad = value.clone();
    let ready_map = after.current_retention.as_ref().unwrap().ledger.map.clone();
    *field(
        field(field(envelope(&mut bad), "current"), "reservation"),
        "ledger",
    ) = serde_cbor::value::to_value(BftPrivateRetentionLedgerV1 {
        map: ready_map,
        receipt: after
            .current
            .reservation
            .as_ref()
            .unwrap()
            .ledger
            .receipt
            .clone(),
    })
    .unwrap();
    rejected(bad);
    let mut bad = value;
    map(field(envelope(&mut bad), "current_retention"))
        .insert(Value::Text("unknown".into()), Value::Null);
    rejected(bad);
    assert!(codec::Stored::new("codec", after.current.current.signer, None).is_err());
    let mut state = serde_cbor::value::to_value(
        codec::Stored::new("codec", initial(&binding()), None).unwrap(),
    )
    .unwrap();
    *field(&mut state, "schema_version") = Value::Integer(2);
    rejected(state);
}
