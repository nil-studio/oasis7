//! Actual private durable import, recovery and retained-material boundaries.
use super::super::super::formal_header::{
    FormalBftHeaderExecutionV3, FormalBftHeaderLedgerExecutionV3, PrivateHeaderEntryV3,
    PrivateHeaderPrefixV3,
};
use super::execution_tests::{certify, private_actions};
use super::*;
use oasis7_runtime::LocalCasStore;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn wire3_formal_read_durable_private_prefix_reopens_after_gc_and_pins_exact_cut() {
    run(recovery);
}
#[test]
fn wire3_formal_read_durable_private_prefix_recovers_original_publication_intent() {
    run(publication_recovery);
}
#[test]
fn wire3_formal_read_durable_private_prefix_refuses_missing_original_h_receipt() {
    run(missing_origin);
}
fn run(case: fn()) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(case)
        .unwrap()
        .join()
        .unwrap();
}
fn fixture(h: &QualifiedH<BftProfileTargetV2>) -> PrivateHeaderPrefixV3 {
    let mut execution = FormalBftHeaderExecutionV3::from_handoff(
        h.anchor.clone(),
        h.target.clone(),
        &h.material,
        h.reader.clone(),
        Vec::new(),
        false,
    )
    .unwrap();
    let mut entries = Vec::new();
    for actions in [
        private_actions(),
        EMPTY_ACTIONS_CBOR.to_vec(),
        EMPTY_ACTIONS_CBOR.to_vec(),
    ] {
        let candidate = execution.build_candidate(&actions).unwrap();
        let certificate = certify(execution.schedule(), &candidate);
        execution.replay_next(&certificate, &actions).unwrap();
        entries.push(PrivateHeaderEntryV3 {
            certificate,
            actions_cbor: actions,
        });
    }
    PrivateHeaderPrefixV3::from_entries(entries).unwrap()
}
fn cas(root: &Path) -> LocalCasStore {
    let path = root.join("cas");
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    LocalCasStore::new(path)
}
fn open(
    h: &QualifiedH<BftProfileTargetV2>,
    root: &Path,
    store: &LocalCasStore,
    initialize: bool,
) -> Result<FormalBftHeaderLedgerExecutionV3, String> {
    FormalBftHeaderLedgerExecutionV3::open_from_handoff(
        &root.join("ledger"),
        store.clone(),
        h.anchor.clone(),
        h.target.clone(),
        h.material.clone(),
        h.reader.clone(),
        initialize,
    )
}
fn target(input: &PrivateHeaderPrefixV3) -> (u64, String) {
    let last = input.entries().last().unwrap();
    (
        last.certificate.execution_commitment.height,
        last.certificate.header.block_id().unwrap(),
    )
}
fn recovery() {
    let h = qualified_handoff_v3();
    let input = fixture(&h);
    let root = Scratch::new();
    let store = cas(root.path());
    let (height, id) = target(&input);
    let cut = open(&h, root.path(), &store, true)
        .unwrap()
        .install_prefix(&input, height, &id)
        .unwrap();
    assert!(
        cut.world()
            .state()
            .agents
            .contains_key("private-wire3-world-agent-4975")
    );
    assert_eq!(
        cut.result_tree().root().unwrap(),
        cut.proof().commitment().result_root
    );
    let digest = cut.durable_record_digest().to_owned();
    let bytes = input.canonical_private_bytes().unwrap();
    assert!(PrivateHeaderPrefixV3::decode_private_canonical(&bytes).unwrap() == input);
    drop(cut);
    let garbage = store.put_bytes(b"unretained-wire3-canary").unwrap();
    assert!(store.prune_unpinned(0).unwrap() > 0);
    assert!(store.get_verified(&garbage).is_err());
    let mut reopened = open(&h, root.path(), &store, false).unwrap();
    assert!(
        reopened
            .verified_cut(height, &hash("wrong-exact-target"))
            .is_err()
    );
    let cut = reopened.verified_cut(height, &id).unwrap();
    assert_eq!(cut.durable_record_digest(), digest);
    assert_eq!(cut.certified_prefix().entries.len(), 3);
    let first = &input.entries()[0].certificate;
    let old = reopened
        .verified_cut(
            first.execution_commitment.height,
            &first.header.block_id().unwrap(),
        )
        .unwrap();
    assert_eq!(old.certified_prefix().entries.len(), 1);
    assert!(
        old.world()
            .state()
            .agents
            .contains_key("private-wire3-world-agent-4975")
    );
    drop(reopened);
    assert!(
        open(&h, root.path(), &store, false)
            .unwrap()
            .install_prefix(&input, height, &id)
            .is_ok()
    );
}
fn publication_recovery() {
    let h = qualified_handoff_v3();
    let input = fixture(&h);
    let (height, id) = target(&input);
    for phase in ["pins", "intent", "link"] {
        let root = Scratch::new();
        let store = cas(root.path());
        let mut ledger = open(&h, root.path(), &store, true).unwrap();
        ledger
            .inject_publication_failure(
                input.entries()[0].certificate.execution_commitment.height,
                phase,
                false,
            )
            .unwrap();
        assert!(ledger.install_prefix(&input, height, &id).is_err());
        let cut = open(&h, root.path(), &store, true)
            .unwrap()
            .install_prefix(&input, height, &id)
            .unwrap_or_else(|e| panic!("recover original {phase}: {e}"));
        assert_eq!(cut.certified_prefix().entries.len(), 3);
    }
}
fn missing_origin() {
    for populated in [false, true] {
        missing_origin_case(populated);
    }
}
fn missing_origin_case(populated: bool) {
    let h = qualified_handoff_v3();
    let input = fixture(&h);
    let root = Scratch::new();
    let store = cas(root.path());
    let (height, id) = target(&input);
    let ledger = open(&h, root.path(), &store, true).unwrap();
    if populated {
        ledger.install_prefix(&input, height, &id).unwrap();
    } else {
        drop(ledger);
    }
    let scope = "formal-bft-header-prefix-origin-v1";
    let shards = store.list_pin_scope_shards(scope).unwrap();
    assert_eq!(shards.len(), 1);
    store.remove_pin_scope_shard(scope, &shards[0]).unwrap();
    assert!(open(&h, root.path(), &store, false).is_err());
    assert!(
        open(&h, root.path(), &store, true).is_err(),
        "install must not heal lost original retention"
    );
    assert!(store.list_pin_scope_shards(scope).unwrap().is_empty());
}
