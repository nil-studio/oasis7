//! Completeness attack: retain the signed T+1 candidate but omit its original Some promise.
//! All authority and execution inputs come from the real native-provider fixture.
use super::*;
use std::cell::Cell;
thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }
pub(super) fn requested() -> bool {
    ENABLED.with(Cell::get)
}
#[test]
fn wire3_successor_pending_original_some_cannot_be_omitted_as_none() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            ENABLED.with(|enabled| enabled.set(true));
            install_case_with_retention(
                false,
                false,
                66,
                CandidateRetentionProfile::PublicationReserved,
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
pub(super) struct AttackInput<'a> {
    pub verifier: &'a ActualBftSuccessorWindowDriverVerifierV1,
    pub archive: &'a LocalBftSuccessorArchiveStoreV1,
    pub body: &'a BftSuccessorWindowCheckpointBodyV1,
    pub original_state: &'a BftSignerStateV3,
    pub manifest: &'a BftSuccessorWindowArchiveManifestV1,
    pub public_manifest: &'a BftSuccessorWindowArchiveManifestV1,
    pub public_ref: &'a BftSuccessorWindowArchiveRefV1,
    pub metadata_ref: &'a BftSuccessorWindowArchiveRefV1,
    pub signed_checkpoint: &'a [u8],
}
pub(super) fn exercise(
    input: AttackInput<'_>,
    (omitted_metadata, removed): (Vec<u8>, Vec<BftSuccessorWindowArchiveRefV1>),
) {
    const LIMIT: usize = 512 * 1024 * 1024;
    let deadline = || Instant::now() + Duration::from_secs(120);
    input
        .verifier
        .verify_closure(
            input.body,
            input.original_state,
            input.public_ref,
            deadline(),
        )
        .expect("unaltered real Some fixture must independently verify before the omission attack");
    assert_eq!(
        input.manifest.kind,
        BftSuccessorWindowArchiveKindV1::Execution
    );
    assert_eq!(
        input.manifest.last_height + 1,
        input.original_state.journal.as_ref().unwrap().height()
    );
    assert!(
        !removed.is_empty(),
        "attack must omit real original Some objects"
    );
    let original_state = oasis7_cas::to_canonical_cbor(input.original_state).unwrap();
    let original_checkpoint = input.signed_checkpoint.to_vec();
    let original_metadata = input
        .archive
        .load_object(input.metadata_ref, LIMIT)
        .unwrap();
    assert_ne!(omitted_metadata, original_metadata);
    assert_eq!(
        input.archive.load_object(input.public_ref, LIMIT).unwrap(),
        original_state
    );
    let originals = input
        .manifest
        .entries
        .iter()
        .map(|entry| {
            let bytes = input.archive.load_object(&entry.object, LIMIT).unwrap();
            (entry.object.clone(), blake3::hash(&bytes))
        })
        .collect::<Vec<_>>();
    let replacement = input
        .archive
        .persist_object(&omitted_metadata, LIMIT)
        .unwrap();
    let mut manifest = input.manifest.clone();
    manifest
        .entries
        .retain(|entry| !removed.contains(&entry.object));
    let metadata_entry = manifest
        .entries
        .iter_mut()
        .find(|entry| &entry.object == input.metadata_ref)
        .expect("original metadata entry must be retained and replaced coherently");
    metadata_entry.object = replacement.clone();
    // Keep metadata's semantic identity; every other entry keeps its original digest identity.
    manifest
        .entries
        .sort_by(|a, b| a.identity_digest.cmp(&b.identity_digest));
    let old_objects = input
        .manifest
        .entries
        .iter()
        .map(|entry| entry.object.bytes)
        .sum::<u64>();
    let new_objects = manifest
        .entries
        .iter()
        .map(|entry| entry.object.bytes)
        .sum::<u64>();
    let old_manifest_bytes = oasis7_cas::to_canonical_cbor(input.manifest).unwrap().len() as u64;
    let base = input
        .manifest
        .usage
        .total_bytes
        .checked_sub(old_objects)
        .unwrap()
        .checked_sub(old_manifest_bytes)
        .unwrap()
        .checked_add(new_objects)
        .unwrap();
    manifest.usage.objects = manifest
        .usage
        .objects
        .checked_sub(removed.len() as u64)
        .unwrap();
    let mut stable = false;
    for _ in 0..16 {
        let total = base
            .checked_add(oasis7_cas::to_canonical_cbor(&manifest).unwrap().len() as u64)
            .unwrap();
        if total == manifest.usage.total_bytes {
            stable = true;
            break;
        }
        manifest.usage.total_bytes = total;
    }
    assert!(
        stable,
        "caller-selected manifest usage must reach the genuine codec fixed point"
    );
    manifest.validate().unwrap();
    let manifest_bytes = oasis7_cas::to_canonical_cbor(&manifest).unwrap();
    let manifest_ref = input
        .archive
        .persist_object(&manifest_bytes, LIMIT)
        .unwrap();
    let mut public_manifest = input.public_manifest.clone();
    public_manifest.usage = manifest.usage;
    public_manifest.validate().unwrap();
    let mut body = input.body.clone();
    body.execution_archive_root = manifest.digest().unwrap();
    body.execution_manifest_ref = manifest_ref;
    body.evidence_refs = vec![replacement];
    body.public_archive_root = public_manifest.digest().unwrap();
    body.usage = manifest.usage;
    body.validate().unwrap();
    manifest.validate_for_checkpoint(&body).unwrap();
    assert_eq!(body.cut, input.body.cut);
    assert_eq!(body.current_journal_ref, input.body.current_journal_ref);
    assert_eq!(
        body.current_journal_digest,
        input.body.current_journal_digest
    );
    assert_eq!(
        body.successor_schedule_cbor,
        input.body.successor_schedule_cbor
    );
    let attacked =
        input
            .verifier
            .verify_closure(&body, input.original_state, input.public_ref, deadline());
    // No signed original, QC, candidate/action/dependency, Snapshot or Journal is
    // rewritten. Only unsigned caller-selected closure/map references changed.
    for (reference, digest) in originals {
        assert_eq!(
            blake3::hash(&input.archive.load_object(&reference, LIMIT).unwrap()),
            digest
        );
    }
    assert_eq!(
        oasis7_cas::to_canonical_cbor(input.original_state).unwrap(),
        original_state
    );
    assert_eq!(input.signed_checkpoint, original_checkpoint);
    input
        .verifier
        .verify_closure(
            input.body,
            input.original_state,
            input.public_ref,
            deadline(),
        )
        .expect("same original fixture remains valid after the caller-selected attack");
    assert!(
        attacked.is_err(),
        "actual verifier accepted omission of the signed T+1 candidate's original Some publication as legacy None"
    );
}
