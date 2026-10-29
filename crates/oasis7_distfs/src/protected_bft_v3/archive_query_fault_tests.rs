use super::*;

struct ReserveFault {
    inner: Backend,
    mode: u8,
    reads: Arc<AtomicUsize>,
}
impl BftSignerBackendV3 for ReserveFault {
    fn load_provisioned(
        &mut self,
        b: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner.load_provisioned(b)
    }
    fn acquire_lease(
        &mut self,
        b: &str,
        g: u64,
        t: &str,
    ) -> Result<BftSignerAcquireOutcomeV3, BftSignerErrorV3> {
        self.inner.acquire_lease(b, g, t)
    }
    fn release_lease(
        &mut self,
        g: u64,
        t: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner.release_lease(g, t)
    }
    fn read_for_lease(
        &mut self,
        g: u64,
        t: &str,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.inner.read_for_lease(g, t)
    }
    fn protected_time_ms(&mut self, g: u64, t: &str) -> Result<i64, BftSignerErrorV3> {
        self.inner.protected_time_ms(g, t)
    }
    fn compare_and_swap(
        &mut self,
        g: u64,
        t: &str,
        r: u64,
        d: &str,
        floor: Option<i64>,
        s: BftSignerStateV3,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        self.inner.compare_and_swap(g, t, r, d, floor, s)
    }
    fn reserve_successor_archive_transfer(
        &mut self,
        g: u64,
        t: &str,
        r: u64,
        d: &str,
        c: u64,
        a: u64,
    ) -> Result<BftSignerBackendSnapshotV3, BftSignerErrorV3> {
        let mut after = self
            .inner
            .reserve_successor_archive_transfer(g, t, r, d, c, a)?;
        if self.mode == 0 {
            return Err(BftSignerErrorV3(
                "test transfer committed but reply lost".into(),
            ));
        }
        // Durable State/counter stay correct; only the returned receipt is corrupt.
        if self.mode == 1 {
            after.state.revision += 1;
        }
        Ok(after)
    }
}
#[test]
fn archive_query_tcp_unknown_or_malformed_reserve_never_releases_payload_and_new_owner_keeps_charge()
 {
    for mode in [0, 1] {
        let (binding, snapshot, archive, original, id, digest) = install(true, true);
        let frozen = snapshot.lock().unwrap().state.clone();
        let support =
            BftSuccessorWindowSupportV1::new(archive.clone(), Duration::from_secs(30)).unwrap();
        let service = BftSignerServiceV3::open_with_successor_window(
            binding.clone(),
            Box::new(Handle {
                key: key(33),
                disabled: None,
            }),
            vec![
                Box::new(Handle {
                    key: key(31),
                    disabled: None,
                }),
                Box::new(Handle {
                    key: key(32),
                    disabled: None,
                }),
            ],
            Box::new(ReserveFault {
                inner: Backend {
                    binding: binding.digest().unwrap(),
                    snapshot: snapshot.clone(),
                },
                mode,
                reads: Arc::new(AtomicUsize::new(0)),
            }),
            support,
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            assert!(
                service
                    .serve_tcp_connection(&mut stream)
                    .unwrap_err()
                    .0
                    .contains("poisoned")
            );
        });
        let provider = RemoteBftSignerProviderV3::new(
            address,
            Duration::from_secs(30),
            binding.clone(),
            key(34),
            BftSignerTrustV3 {
                schema_version: 3,
                provider_public_key: public(33),
                binding_digest: binding.digest().unwrap(),
            },
        )
        .unwrap();
        let mut lease = provider.acquire(&binding).unwrap();
        let error = lease
            .read_successor_original_state(&id, &digest, deadline())
            .unwrap_err();
        assert!(error.0.contains("transfer") || error.0.contains("reserve"));
        let charged = counter(&snapshot);
        assert!(charged > 0);
        assert_eq!(snapshot.lock().unwrap().state, frozen);
        assert!(
            lease.checkpoint().is_err(),
            "uncertain reservation must poison subsequent operations"
        );
        drop(lease);
        server.join().unwrap();
        // Explicit backend fencing after an uncertain owner; never reset its ledger.
        {
            let mut durable = snapshot.lock().unwrap();
            durable.lease_generation += 1;
            durable.lease_token_digest.clear();
        }
        let (provider, server) = native_successor(&binding, &snapshot, archive, None);
        let mut lease = provider.acquire(&binding).unwrap();
        assert_eq!(counter(&snapshot), charged);
        assert_eq!(
            lease
                .read_successor_original_state(&id, &digest, deadline())
                .unwrap()
                .original_state(),
            &original
        );
        assert!(counter(&snapshot) > charged);
        assert_eq!(snapshot.lock().unwrap().state, frozen);
        drop(lease);
        server.join().unwrap();
    }
}

#[test]
fn archive_attempt_uses_only_charged_chunks_and_cached_misses_do_no_io() {
    let (binding, snapshot, archive, original, id, digest) = install(true, true);
    let reads = Arc::new(AtomicUsize::new(0));
    let support = BftSuccessorWindowSupportV1::new(archive, Duration::from_secs(30)).unwrap();
    let service = BftSignerServiceV3::open_with_successor_window(
        binding.clone(),
        Box::new(Handle {
            key: key(33),
            disabled: None,
        }),
        vec![
            Box::new(Handle {
                key: key(31),
                disabled: None,
            }),
            Box::new(Handle {
                key: key(32),
                disabled: None,
            }),
        ],
        Box::new(ReserveFault {
            inner: Backend {
                binding: binding.digest().unwrap(),
                snapshot: snapshot.clone(),
            },
            mode: 2,
            reads: reads.clone(),
        }),
        support,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        service.serve_tcp_connection(&mut stream).unwrap();
    });
    let provider = RemoteBftSignerProviderV3::new(
        address,
        Duration::from_secs(30),
        binding.clone(),
        key(34),
        BftSignerTrustV3 {
            schema_version: 3,
            provider_public_key: public(33),
            binding_digest: binding.digest().unwrap(),
        },
    )
    .unwrap();
    let mut lease = provider.acquire(&binding).unwrap();
    let before = reads.load(Ordering::SeqCst);
    let charged = counter(&snapshot);
    let wrong_digest = hash(97);
    for (request, checkpoint) in [
        ("uncached-request", digest.as_str()),
        (id.as_str(), wrong_digest.as_str()),
    ] {
        assert!(
            lease
                .read_successor_original_state(request, checkpoint, deadline())
                .unwrap_err()
                .0
                .contains("refresh checkpoint explicitly")
        );
        assert_eq!(reads.load(Ordering::SeqCst), before);
        assert_eq!(counter(&snapshot), charged);
    }
    assert_eq!(
        lease
            .read_successor_original_state(&id, &digest, deadline())
            .unwrap()
            .original_state(),
        &original
    );
    let encoded = oasis7_cas::to_canonical_cbor(&original).unwrap();
    let chunks = encoded
        .len()
        .div_ceil(BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1);
    // Each chunk does exactly one lease backend read. An automatic Get or
    // Checkpoint would add an uncharged backend read and fail this assertion.
    assert_eq!(reads.load(Ordering::SeqCst) - before, chunks);
    assert!(counter(&snapshot) > charged + encoded.len() as u64);
    drop(lease);
    server.join().unwrap();
}
