//! Compatibility is checked with a genuinely signed, dual-qualified entry.
use super::*;
#[test]
fn absent_local_closure_keeps_legacy_qualified_encoding_and_rejects_missing_blob() {
    #[derive(serde::Serialize)]
    enum BeforeLocalExtension<'a> {
        Qualified {
            candidate: &'a FormalCandidateMaterialV1,
            proof: &'a FormalQualifiedCommitProofV1,
        },
    }
    let fixture = Fixture::new();
    let mut coordinator = fixture.coordinator();
    let proof = coordinator.submit(fixture.initial.clone()).unwrap();
    let request = &fixture.initial.candidate.record.request_digest;
    let (primary_state, _) = coordinator.query(request).unwrap();
    let FormalQueryOutcomeV1::Recorded(state) = primary_state else {
        panic!("actual Qualified entry missing");
    };
    let FormalRequestStateV1::Qualified {
        candidate,
        proof: installed,
        data_closure,
    } = &*state
    else {
        panic!("actual Qualified entry missing");
    };
    assert_eq!(installed, &proof);
    assert!(data_closure.is_none());
    let old_bytes = serde_cbor::to_vec(&BeforeLocalExtension::Qualified {
        candidate,
        proof: installed,
    })
    .unwrap();
    assert_eq!(serde_cbor::to_vec(&*state).unwrap(), old_bytes);
    assert_eq!(
        serde_cbor::from_slice::<FormalRequestStateV1>(&old_bytes).unwrap(),
        *state
    );
    drop(coordinator);
    let mut primary = fixture.endpoint(EndpointRole::Primary);
    let old = primary.store.body.clone();
    let mut next = old.clone();
    next.data_closure = Some(h(98));
    let mut guard = primary.guard().unwrap();
    assert!(primary.publish_native(next, guard.as_mut()).is_err());
    drop(guard);
    assert_eq!(primary.store.body, old);
}
