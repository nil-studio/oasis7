//! Native pure/codec evidence only; no Ready RPC or actual material claim.
use super::*;
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
