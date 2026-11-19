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

    let writer = service(&binding, &snapshot, &slot, &marker, &role, false)
        .unwrap()
        .with_first_reservation(
            with_reader(vec![original.clone(), repriced.clone()]),
            Duration::from_secs(20),
        )
        .unwrap();
    let (provider, server) = native_service(&binding, writer);
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
    let reserved = lease
        .reserve_first_private_retention(&role, &marker, &activation, &original)
        .unwrap();
    let receipt_generation = reserved.receipt.body.original_lease_generation;
    let state = lease.checkpoint().unwrap().state;
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
    assert!(lease.start_height_timer(2).is_err());
    drop(lease);
    server.join().unwrap();

    let reader = service(&binding, &snapshot, &slot, &marker, &role, false)
        .unwrap()
        .with_first_reservation(with_reader(Vec::new()), Duration::from_secs(20))
        .unwrap();
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
    assert!(
        lease
            .reserve_first_private_retention(&role, &marker, &activation, &repriced)
            .is_err()
    );
    drop(lease);
    server.join().unwrap();
}
