//! Production cache orchestration with explicit immutable-source doubles.
//! These tests establish resource/owner behavior, not World or issuer authority.
use super::super::super::admission_input::NodeBftAdmissionInputPublisherV1;
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionInputObjectKindV1, BftPrivateRetentionInputObjectV1,
    BftPrivateRetentionKeyV1, BftPrivateRetentionSourceV1, BftSuccessorWindowArchiveRefV1,
};
use std::sync::atomic::AtomicUsize;

struct Source {
    current: Mutex<Option<Arc<Input>>>,
    buffers: Arc<crate::NodeBftPrivateTransportBuffersV1>,
    builds: AtomicUsize,
    checks: AtomicUsize,
}
impl NodeBftAdmissionInputSourceV1 for Source {
    fn original(
        &self,
        reference: &InputRef,
        deadline: Instant,
    ) -> Result<Arc<NodeBftAdmissionInputBytesV1>, String> {
        if Instant::now() >= deadline {
            return Err("test source deadline".into());
        }
        let current = self
            .current
            .lock()
            .unwrap()
            .clone()
            .ok_or("test original missing")?;
        reference
            .validate_input(&current)
            .map_err(|e| e.to_string())?;
        self.builds.fetch_add(1, Ordering::SeqCst);
        NodeBftAdmissionInputBytesV1::from_retained_original(&current, &self.buffers)
    }
    fn revalidate_original(
        &self,
        original: &NodeBftAdmissionInputBytesV1,
        deadline: Instant,
    ) -> Result<bool, String> {
        if Instant::now() >= deadline {
            return Err("test source deadline".into());
        }
        self.checks.fetch_add(1, Ordering::SeqCst);
        let current = self
            .current
            .lock()
            .unwrap()
            .clone()
            .ok_or("test original missing")?;
        Ok(original
            .retained_owner
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(owner, &current)))
    }
}
fn input() -> Arc<Input> {
    let mut objects = [
        (
            BftPrivateRetentionInputObjectKindV1::OriginalData,
            vec![1, 2, 3],
        ),
        (
            BftPrivateRetentionInputObjectKindV1::PublicationMetadata,
            vec![4, 5, 6],
        ),
    ]
    .into_iter()
    .map(|(kind, bytes)| BftPrivateRetentionInputObjectV1 {
        reference: BftSuccessorWindowArchiveRefV1 {
            digest: blake3::hash(&bytes).to_hex().to_string(),
            bytes: bytes.len() as u64,
        },
        kind,
        bytes,
    })
    .collect::<Vec<_>>();
    objects.sort_by(|a, b| a.reference.digest.cmp(&b.reference.digest));
    Arc::new(Input {
        schema_version: 1,
        publication_reservation_bytes: 1024,
        source: BftPrivateRetentionSourceV1 {
            binding_digest: "11".repeat(32),
            handoff_digest: "22".repeat(32),
            material_source_digest: "33".repeat(32),
        },
        key: BftPrivateRetentionKeyV1 {
            ledger_origin_digest: "44".repeat(32),
            height: 7,
            header_id: "55".repeat(32),
        },
        objects,
    })
}
fn setup() -> (
    NodeBftPrivateTransportScopeV1,
    String,
    String,
    Source,
    InputRef,
    Instant,
) {
    let peer = libp2p::identity::Keypair::generate_ed25519()
        .public()
        .to_peer_id()
        .to_string();
    let scope =
        NodeBftPrivateTransportScopeV1::new(vec![peer.clone()], Duration::from_secs(10)).unwrap();
    let nonce = "66".repeat(32);
    let deadline = scope
        .input_deadline(
            peer.clone(),
            nonce.clone(),
            "77".repeat(32),
            "88".repeat(32),
            true,
        )
        .unwrap();
    let original = input();
    let reference = original.descriptor().unwrap();
    let source = Source {
        current: Mutex::new(Some(original)),
        buffers: scope.buffers(),
        builds: AtomicUsize::new(0),
        checks: AtomicUsize::new(0),
    };
    (scope, peer, nonce, source, reference, deadline)
}
#[test]
fn admission_encoded_cache_repeated_chunks_reuse_bytes_but_freshly_check_owner() {
    let (scope, peer, nonce, source, reference, deadline) = setup();
    let first = scope
        .admission_original(&peer, &nonce, &reference, &source, deadline)
        .unwrap();
    for _ in 0..4 {
        let next = scope
            .admission_original(&peer, &nonce, &reference, &source, deadline)
            .unwrap();
        assert!(
            Arc::ptr_eq(&first.bytes, &next.bytes),
            "every next chunk reuses the first encoded original"
        );
    }
    assert_eq!(source.builds.load(Ordering::SeqCst), 1);
    assert_eq!(
        source.checks.load(Ordering::SeqCst),
        5,
        "first build and every subsequent chunk revalidate actual ownership"
    );
    let count = source.checks.load(Ordering::SeqCst);
    assert!(
        scope
            .admission_original(&peer, &nonce, &reference, &source, Instant::now())
            .is_err()
    );
    assert_eq!(source.checks.load(Ordering::SeqCst), count);
    assert_eq!(
        scope
            .cache
            .lock()
            .unwrap()
            .seen
            .get(&(peer, nonce))
            .unwrap()
            .deadline,
        deadline,
        "cache hits cannot renew the original nonce deadline"
    );
}
#[test]
fn admission_encoded_cache_missing_or_replaced_owner_cannot_reuse_old_bytes() {
    let (scope, peer, nonce, source, reference, deadline) = setup();
    let first = scope
        .admission_original(&peer, &nonce, &reference, &source, deadline)
        .unwrap();
    let old = Arc::downgrade(&first.bytes);
    drop(first);
    let replacement = Arc::new((**source.current.lock().unwrap().as_ref().unwrap()).clone());
    *source.current.lock().unwrap() = Some(replacement.clone());
    let second = scope
        .admission_original(&peer, &nonce, &reference, &source, deadline)
        .unwrap();
    assert_eq!(source.builds.load(Ordering::SeqCst), 2);
    assert!(!std::sync::Weak::ptr_eq(
        &old,
        &Arc::downgrade(&second.bytes)
    ));
    assert!(Arc::ptr_eq(
        second.retained_owner.as_ref().unwrap(),
        &replacement
    ));
    let replaced = Arc::downgrade(&second.bytes);
    drop(second);
    *source.current.lock().unwrap() = None;
    assert!(
        scope
            .admission_original(&peer, &nonce, &reference, &source, deadline)
            .err()
            .unwrap()
            .contains("missing")
    );
    assert!(
        scope.cache.lock().unwrap().entries.is_empty(),
        "a missing source must discard its encoded entry, not fall back"
    );
    *source.current.lock().unwrap() = Some(replacement);
    let third = scope
        .admission_original(&peer, &nonce, &reference, &source, deadline)
        .unwrap();
    assert_eq!(source.builds.load(Ordering::SeqCst), 3);
    assert!(!std::sync::Weak::ptr_eq(
        &replaced,
        &Arc::downgrade(&third.bytes)
    ));
}
#[test]
fn admission_encoded_cache_idle_eviction_uses_existing_candidate_peak_reservoir() {
    let (scope, peer, nonce, source, reference, deadline) = setup();
    let response = scope
        .admission_original(&peer, &nonce, &reference, &source, deadline)
        .unwrap();
    assert!(scope.buffers().outbound(2 * MAX_PACKAGE).is_err());
    assert!(
        scope
            .cache
            .lock()
            .unwrap()
            .reserve_build(&scope.buffers())
            .is_err(),
        "an active response cannot be evicted for candidate encoding"
    );
    drop(response);
    let mut cache = scope.cache.lock().unwrap();
    let peak = cache.reserve_build(&scope.buffers()).unwrap();
    assert!(cache.entries.is_empty());
    assert_eq!(cache.bytes, 0);
    assert_eq!(
        cache.seen.len(),
        1,
        "eviction never erases original nonce identity"
    );
    assert!(
        scope.buffers().outbound(1).is_err(),
        "candidate and admission use the identical 132MiB pool"
    );
    drop(peak);
    drop(cache);
    assert!(scope.buffers().outbound(2 * MAX_PACKAGE).is_ok());
}
#[test]
fn admission_publisher_clear_is_not_kept_alive_by_an_encoded_cache() {
    let (scope, peer, nonce, _, _, deadline) = setup();
    let publisher = Arc::new(NodeBftAdmissionInputPublisherV1::new(scope.buffers()));
    let reference = publisher
        .reserve()
        .unwrap()
        .publish((*input()).clone())
        .unwrap();
    let response = scope
        .admission_original(&peer, &nonce, &reference, publisher.as_ref(), deadline)
        .unwrap();
    assert!(
        scope.cache.lock().unwrap().entries.is_empty(),
        "already-owned publisher bytes need no second strong cache"
    );
    publisher.clear(&reference).unwrap();
    assert!(
        scope
            .admission_original(&peer, &nonce, &reference, publisher.as_ref(), deadline)
            .is_err()
    );
    assert!(
        publisher.reserve().is_err(),
        "in-flight response still owns its actual bytes"
    );
    drop(response);
    assert!(
        publisher.reserve().is_ok(),
        "clear plus last response drop permits the next original peak reservation"
    );
}

#[test]
fn admission_encoded_cache_rejects_an_original_charged_to_another_pool() {
    let (scope, peer, nonce, mut source, reference, deadline) = setup();
    source.buffers = Arc::new(crate::NodeBftPrivateTransportBuffersV1::default());
    let error = scope
        .admission_original(&peer, &nonce, &reference, &source, deadline)
        .err()
        .unwrap();
    assert!(error.contains("pool differs"));
    let cache = scope.cache.lock().unwrap();
    assert!(cache.entries.is_empty());
    assert_eq!(
        cache.bytes, 0,
        "failed build releases its logical shared reservation"
    );
    drop(cache);
    assert!(scope.buffers().outbound(2 * MAX_PACKAGE).is_ok());
    assert!(
        source.buffers.outbound(2 * MAX_PACKAGE).is_ok(),
        "the rejected encoded original releases its actual foreign permit"
    );
}
