//! CPU work oracles for the software Memory backend. The transaction fixture
//! is not ActualWorld source admission, a durable backend or physical custody.
use super::*;
use crate::protected_bft_v3::private_retention_memory::hash_probe::{Counts, Snapshot};

fn probe(memory: &BftPrivateRetentionMemoryBackendV1) -> Arc<Counts> {
    memory.inner.lock().unwrap().hash_probe.clone()
}
fn delta(after: Snapshot, before: Snapshot) -> Snapshot {
    Snapshot {
        object_payload_bytes: after.object_payload_bytes - before.object_payload_bytes,
        canonical_hasher_feed_bytes: after.canonical_hasher_feed_bytes
            - before.canonical_hasher_feed_bytes,
    }
}
#[test]
fn private_retention_memory_fresh_reads_and_transfer_do_not_rehash_published_originals() {
    let (mut memory, mut request) = backend();
    let transfer = Some(
        super::super::super::archive_transfer::BftSuccessorArchiveTransferLedgerV1::provisioned(),
    );
    // Provision the explicit original transfer capability in this test fixture
    // before its first actual retention transaction; never reset a live counter.
    memory
        .inner
        .lock()
        .unwrap()
        .signer
        .successor_archive_transfer = transfer;
    request.before.successor_archive_transfer = transfer;
    request.after.signer.successor_archive_transfer = transfer;
    let counts = probe(&memory);
    let initial = counts.snapshot();
    let committed = memory.compare_and_swap_private_retention(&request).unwrap();
    assert_eq!(committed, request.after);
    let published = counts.snapshot();
    assert!(published.object_payload_bytes > initial.object_payload_bytes);
    assert!(
        published.canonical_hasher_feed_bytes > initial.canonical_hasher_feed_bytes,
        "first actual publication must fully validate original payload/descriptor"
    );
    let input = committed.inputs.values().next().unwrap();
    let reference = input.descriptor().unwrap(); // outside any backend probe scope
    let generation = committed.signer.lease_generation;
    let token = &committed.signer.lease_token_digest;
    let mut observations = Vec::new();
    assert_eq!(
        memory.read_for_lease(generation, token).unwrap(),
        committed.signer
    );
    observations.push(("read_for_lease", delta(counts.snapshot(), published)));
    let current = memory
        .read_private_retention_current(generation, token)
        .unwrap();
    assert_eq!(current.signer, committed.signer);
    assert_eq!(current.retention.as_ref(), Some(&committed.retention));
    observations.push(("read_current", delta(counts.snapshot(), published)));
    let sealed = memory
        .read_private_retention_sealed_current(generation, token)
        .unwrap();
    assert_eq!(sealed.current, current);
    observations.push(("read_sealed_current", delta(counts.snapshot(), published)));
    let loaded = memory
        .load_private_retention_input(&reference.source.binding_digest, &reference)
        .unwrap();
    assert!(
        Arc::ptr_eq(&loaded, input),
        "the fresh lookup must still return the actual immutable owner"
    );
    observations.push(("read_input", delta(counts.snapshot(), published)));
    let charged = memory
        .reserve_successor_archive_transfer(
            generation,
            token,
            committed.signer.state.revision,
            &committed.signer.state.state_digest,
            0,
            1,
        )
        .unwrap();
    assert_eq!(charged.state, committed.signer.state);
    assert_eq!(charged.lease_generation, generation);
    assert_eq!(
        charged
            .successor_archive_transfer
            .unwrap()
            .reserved_response_bytes,
        1
    );
    observations.push(("reserve_transfer", delta(counts.snapshot(), published)));
    assert_eq!(
        counts.snapshot(),
        published,
        "fresh lease/current/input existence and transfer checks must preserve published owners without scanning original bytes again; actual cumulative work: {observations:?}"
    );
}
#[test]
fn private_retention_memory_hash_probe_counts_mismatch_but_not_length_or_non_input_hash() {
    let (memory, request) = backend();
    let counts = probe(&memory);
    let input = request.after.inputs.values().next().unwrap();
    let mut changed = input.as_ref().clone();
    changed.objects[0].bytes[0] ^= 1;
    let before = counts.snapshot();
    {
        let _scope = memory.lock().unwrap();
        assert!(changed.validate().is_err());
    }
    let observed = delta(counts.snapshot(), before);
    assert_eq!(
        observed.object_payload_bytes,
        changed.objects[0].bytes.len() as u64,
        "a digest mismatch is counted after actual BLAKE3, before comparison"
    );
    assert_eq!(observed.canonical_hasher_feed_bytes, 0);
    let mut wrong_length = input.as_ref().clone();
    wrong_length.objects[0].reference.bytes += 1;
    let before = counts.snapshot();
    {
        let _scope = memory.lock().unwrap();
        assert!(wrong_length.validate().is_err());
    }
    assert_eq!(
        counts.snapshot(),
        before,
        "the original length short-circuit performs no payload hash"
    );
    {
        let _scope = memory.lock().unwrap();
        input.objects[0]
            .reference
            .validate_bytes(&input.objects[0].bytes)
            .unwrap();
    }
    assert_eq!(
        counts.snapshot(),
        before,
        "generic archive hashing is not an Input object scan"
    );
}
#[test]
fn private_retention_memory_hash_probe_restores_nested_scope_and_isolates_concurrent_backends() {
    let (first, request) = backend();
    let (second, _) = backend();
    let a = probe(&first);
    let b = probe(&second);
    let input = request.after.inputs.values().next().unwrap().clone();
    let payload = input
        .objects
        .iter()
        .map(|object| object.bytes.len() as u64)
        .sum::<u64>();
    let encoded = input.canonical_bytes().unwrap().len() as u64;
    {
        let _outer = first.lock().unwrap();
        input.descriptor().unwrap();
        {
            let _inner = second.lock().unwrap();
            input.descriptor().unwrap();
        }
        input.descriptor().unwrap();
    }
    assert_eq!(
        a.snapshot(),
        Snapshot {
            object_payload_bytes: 2 * payload,
            canonical_hasher_feed_bytes: 4 * encoded
        }
    );
    assert_eq!(
        b.snapshot(),
        Snapshot {
            object_payload_bytes: payload,
            canonical_hasher_feed_bytes: 2 * encoded
        }
    );
    let before_a = a.snapshot();
    let before_b = b.snapshot();
    input.descriptor().unwrap();
    assert_eq!(a.snapshot(), before_a);
    assert_eq!(b.snapshot(), before_b);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let workers = [(first, 1), (second, 3)]
        .into_iter()
        .map(|(memory, repeats)| {
            let input = input.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let _scope = memory.lock().unwrap();
                barrier.wait();
                for _ in 0..repeats {
                    input.descriptor().unwrap();
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(
        delta(a.snapshot(), before_a),
        Snapshot {
            object_payload_bytes: payload,
            canonical_hasher_feed_bytes: 2 * encoded
        }
    );
    assert_eq!(
        delta(b.snapshot(), before_b),
        Snapshot {
            object_payload_bytes: 3 * payload,
            canonical_hasher_feed_bytes: 6 * encoded
        }
    );
}
