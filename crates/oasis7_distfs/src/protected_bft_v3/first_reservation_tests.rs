//! Native TCP command + software transaction fixture. Reader/price fixture
//! pins explicit H values and material; real Node issuer and backend separate.
use super::*;

struct Source {
    allowed: Vec<Arc<BftPrivateRetentionAdmissionInputV1>>,
    reads: Arc<AtomicUsize>,
    retained: String,
    hold: Arc<AtomicUsize>,
}

struct TransportHold(Arc<AtomicUsize>);
impl Drop for TransportHold {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl BftFirstReserveSourceV1 for Source {
    fn read_authorized_original(
        &self,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
        _deadline: std::time::Instant,
    ) -> Result<BftAuthorizedOriginalReserveInputV1, BftSignerErrorV3> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let input = self
            .allowed
            .iter()
            .find(|input| input.descriptor().as_ref() == Ok(reference))
            .ok_or_else(|| invalid("fixture original input authorization absent"))?;
        self.hold.fetch_add(1, Ordering::SeqCst);
        Ok(BftAuthorizedOriginalReserveInputV1 {
            input: input.clone(),
            transport_hold: Box::new(TransportHold(self.hold.clone())),
        })
    }

    fn verify_actual_first_price(
        &self,
        binding: &BftSignerBindingV3,
        marker: &BftPrivateRetentionCreationMarkerV1,
        covered: &BftSignerStateV3,
        input: &BftPrivateRetentionAdmissionInputV1,
        _deadline: std::time::Instant,
    ) -> Result<BftFirstReservationPriceV1, BftSignerErrorV3> {
        assert_eq!(self.hold.load(Ordering::SeqCst), 1);
        assert_eq!(covered.revision, 1);
        assert_eq!(covered.private_retention.as_ref().unwrap().entry_count, 0);
        assert_eq!(binding.digest().unwrap(), input.source.binding_digest);
        assert_eq!(marker.ledger_origin_digest, input.key.ledger_origin_digest);
        Ok(BftFirstReservationPriceV1 {
            publication_bytes: 512,
            private_pin_bytes: 17,
            original_selection_bytes: 22,
            retained_h_dependencies: [self.retained.clone()].into(),
        })
    }
}

fn original_input(
    marker: &BftPrivateRetentionCreationMarkerV1,
) -> (BftPrivateRetentionAdmissionInputV1, String) {
    use BftPrivateRetentionInputObjectKindV1 as Kind;
    let record = |kind, bytes: &[u8]| BftPrivateRetentionInputObjectV1 {
        kind,
        reference: BftSuccessorWindowArchiveRefV1 {
            digest: blake3::hash(bytes).to_hex().to_string(),
            bytes: bytes.len() as u64,
        },
        bytes: bytes.to_vec(),
    };
    let retained = record(Kind::Dependency, b"old H");
    let retained_digest = retained.reference.digest.clone();
    let mut objects = vec![
        record(Kind::OriginalData, b"data"),
        record(Kind::PublicationMetadata, b"original-plan"),
        retained,
        record(Kind::Dependency, b"new-depend"),
    ];
    objects.sort_by(|a, b| a.reference.digest.cmp(&b.reference.digest));
    (
        BftPrivateRetentionAdmissionInputV1 {
            schema_version: 1,
            source: marker.source.clone(),
            key: BftPrivateRetentionKeyV1 {
                ledger_origin_digest: marker.ledger_origin_digest.clone(),
                height: marker.initial_height,
                header_id: hash(77),
            },
            // Historical H bytes are already retained. The exact first price is
            // 512 plan + 17 pin + 22 selection + 8 Data + 13 metadata + 10 new node.
            publication_reservation_bytes: 582,
            objects,
        },
        retained_digest,
    )
}

#[test]
fn native_first_reserve_prices_exact_original_inventory_and_cold_retries() {
    for scenario in ["legacy", "envelope", "lost-ack", "mismatched-original"] {
        first_reserve_and_cold_retry(scenario);
    }
}

fn first_reserve_and_cold_retry(scenario: &str) {
    let use_envelope = scenario != "legacy";
    let uncertain = matches!(scenario, "lost-ack" | "mismatched-original");
    let binding = binding();
    let (marker, role) = configured_source(&binding);
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let slot = Arc::new(Mutex::new(FreshSlot {
        marker: marker.clone(),
        original: None,
        reservation: None,
        reservation_hold: None,
        reserved_response_bytes: 0,
        mismatched_readback: false,
        lose_commit_acknowledgement: false,
        activation_signs: Arc::new(AtomicUsize::new(0)),
        original_envelope: None,
    }));
    let (original, retained) = original_input(&marker);
    let mut repriced = original.clone();
    repriced.publication_reservation_bytes += 1;
    let mut unauthorized = original.clone();
    unauthorized.key.header_id = hash(78);
    let original = Arc::new(original);
    let repriced = Arc::new(repriced);
    let unauthorized = Arc::new(unauthorized);
    let reads = Arc::new(AtomicUsize::new(0));
    let hold = Arc::new(AtomicUsize::new(0));
    slot.lock().unwrap().reservation_hold = Some(hold.clone());
    let with_reader = |allowed| {
        Box::new(Source {
            allowed,
            reads: reads.clone(),
            retained: retained.clone(),
            hold: hold.clone(),
        })
    };
    let configure = |service: BftSignerServiceV3, allowed| {
        if use_envelope {
            service.with_retained_first_reservation(with_reader(allowed), Duration::from_secs(20))
        } else {
            service.with_first_reservation(with_reader(allowed), Duration::from_secs(20))
        }
        .unwrap()
    };

    let unsupported = service(&binding, &snapshot, &slot, &marker, &role, true).unwrap();
    let (provider, server) = native_service(&binding, unsupported);
    let mut lease = provider.acquire(&binding).unwrap();
    let activation = lease.provision_private_retention(&role, &marker).unwrap();
    assert!(
        lease
            .reserve_first_private_retention(&role, &marker, &activation, &original)
            .is_err()
    );
    let covered = lease.checkpoint().unwrap().state;
    assert_eq!(covered.revision, 1);
    drop(lease);
    server.join().unwrap();

    // Opting in on a legacy-only backend must fail before reading originals,
    // charging a response or falling back to the old transaction primitive.
    if use_envelope {
        let unsupported = configure(
            service(&binding, &snapshot, &slot, &marker, &role, false).unwrap(),
            vec![original.clone()],
        );
        let (provider, server) = native_service(&binding, unsupported);
        let mut lease = provider.acquire(&binding).unwrap();
        let spend = slot.lock().unwrap().reserved_response_bytes;
        assert!(
            lease
                .reserve_first_private_retention(&role, &marker, &activation, &original)
                .unwrap_err()
                .to_string()
                .contains("envelope unsupported")
        );
        assert_eq!(slot.lock().unwrap().reserved_response_bytes, spend);
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        drop(lease);
        server.join().unwrap();
        // Opaque historical bytes stand for an already verified operation;
        // this fixture tests preservation, not original signature admission.
        let request = b"original request\0\xff".to_vec();
        let receipt = b" original receipt\n\xfe".to_vec();
        let operation = BftRetainedOriginalOperationV1 {
            schema_version: 1,
            operation_id: "historical-raw".into(),
            source: marker.source.clone(),
            ledger_origin_digest: marker.ledger_origin_digest.clone(),
            original_request_digest:
                BftRetainedOriginalOperationV1::request_digest_from_original_bytes(&request),
            signed_request_bytes: request,
            before_revision: 0,
            before_state_digest: marker.initial_state_digest.clone(),
            after_revision: covered.revision,
            after_state_digest: covered.state_digest.clone(),
            original_lease_generation: activation.receipt.body.original_lease_generation,
            original_lease_token_digest: hash(90),
            input_refs: vec![original.descriptor().unwrap()],
            original_receipt_digest:
                BftRetainedOriginalOperationV1::receipt_digest_from_original_bytes(&receipt),
            signed_receipt_bytes: receipt,
            cumulative_reserved_response_bytes: 1,
        };
        slot.lock().unwrap().original_envelope = Some(OriginalEnvelope {
            operations: [(operation.operation_id.clone(), operation)].into(),
            lose_ack: scenario == "lost-ack",
            mismatch_readback: scenario == "mismatched-original",
            ..Default::default()
        });
    }
    let historical = slot
        .lock()
        .unwrap()
        .original_envelope
        .as_ref()
        .map(|e| e.operations.clone());
    let writer = configure(
        service(&binding, &snapshot, &slot, &marker, &role, false).unwrap(),
        vec![original.clone(), repriced.clone()],
    );
    let (provider, server) = native_service_expected(&binding, writer, uncertain);
    let mut lease = provider.acquire(&binding).unwrap();
    assert!(
        lease
            .reserve_first_private_retention(&role, &marker, &activation, &unauthorized)
            .unwrap_err()
            .to_string()
            .contains("authorization absent")
    );
    assert!(
        lease
            .reserve_first_private_retention(&role, &marker, &activation, &repriced)
            .unwrap_err()
            .to_string()
            .contains("price differs")
    );
    assert!(slot.lock().unwrap().reservation.is_none());
    assert_eq!(lease.checkpoint().unwrap().state, covered);
    let answer = lease.reserve_first_private_retention(&role, &marker, &activation, &original);
    let reserved = slot
        .lock()
        .unwrap()
        .reservation
        .as_ref()
        .unwrap()
        .ledger
        .clone();
    if uncertain {
        assert!(
            answer
                .unwrap_err()
                .to_string()
                .contains("commit/readback uncertain")
        );
        assert!(lease.checkpoint().is_err());
        assert!(
            lease
                .reserve_first_private_retention(&role, &marker, &activation, &original)
                .is_err()
        );
    } else {
        assert_eq!(answer.unwrap(), reserved);
    }
    let receipt_generation = reserved.receipt.body.original_lease_generation;
    let state = snapshot.lock().unwrap().state.clone();
    assert_eq!(state.revision, 2);
    assert_eq!(
        state.private_retention.as_ref(),
        Some(&reserved.receipt.body.head)
    );
    let (original_spend, reservation) = {
        let persisted = slot.lock().unwrap();
        (
            persisted.reserved_response_bytes,
            persisted.reservation.clone().unwrap(),
        )
    };
    assert_eq!(reservation.input, original);
    assert_eq!(reservation.ledger, reserved);
    assert_eq!(hold.load(Ordering::SeqCst), 0);
    if let Some(expected) = &historical {
        let persisted = slot.lock().unwrap();
        let envelope = persisted.original_envelope.as_ref().unwrap();
        assert_eq!(&envelope.operations, expected);
        assert_eq!(envelope.commits, 1);
    }
    assert!(lease.start_height_timer(2).is_err());
    drop(lease);
    server.join().unwrap();

    if uncertain {
        let fenced = snapshot.lock().unwrap().clone();
        assert!(!fenced.lease_token_digest.is_empty());
        let reader = configure(
            service(&binding, &snapshot, &slot, &marker, &role, false).unwrap(),
            Vec::new(),
        );
        let (provider, server) = native_service(&binding, reader);
        assert!(provider.acquire(&binding).is_err());
        server.join().unwrap();
        assert_eq!(*snapshot.lock().unwrap(), fenced);
        // An independent test backend fences the departed old writer. A
        // production service must never clear an uncertain owner by itself.
        Backend {
            binding: binding.digest().unwrap(),
            snapshot: snapshot.clone(),
            fresh: Some(slot.clone()),
        }
        .release_lease(fenced.lease_generation, &fenced.lease_token_digest)
        .unwrap();
        let mut persisted = slot.lock().unwrap();
        let envelope = persisted.original_envelope.as_mut().unwrap();
        envelope.lose_ack = false;
        envelope.mismatch_readback = false;
    }
    let reader = configure(
        service(&binding, &snapshot, &slot, &marker, &role, false).unwrap(),
        Vec::new(),
    );
    let (provider, server) = native_service(&binding, reader);
    let mut lease = provider.acquire(&binding).unwrap();
    assert_eq!(
        lease
            .reserve_first_private_retention(&role, &marker, &activation, &original)
            .unwrap(),
        reserved
    );
    let checkpoint = lease.checkpoint().unwrap();
    assert_eq!(checkpoint.state, state);
    assert!(checkpoint.lease_generation > receipt_generation);
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    assert_eq!(
        slot.lock().unwrap().activation_signs.load(Ordering::SeqCst),
        2
    );
    assert!(slot.lock().unwrap().reserved_response_bytes > original_spend);
    if let Some(expected) = &historical {
        let persisted = slot.lock().unwrap();
        let envelope = persisted.original_envelope.as_ref().unwrap();
        assert_eq!(&envelope.operations, expected);
        assert_eq!(envelope.commits, 2);
    }
    assert!(
        lease
            .reserve_first_private_retention(&role, &marker, &activation, &repriced)
            .is_err()
    );
    drop(lease);
    server.join().unwrap();
}
