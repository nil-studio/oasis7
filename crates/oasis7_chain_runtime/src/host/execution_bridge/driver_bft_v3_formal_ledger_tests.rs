//! Actual private durable import, recovery and retained-material boundaries.
use super::super::super::formal_header::{
    FormalBftHeaderCandidateV3, FormalBftHeaderExecutionV3, FormalBftHeaderLedgerExecutionV3,
    PrivateHeaderEntryV3, PrivateHeaderPrefixV3,
};
use super::execution_tests::{certify, certify_at_round, private_actions};
use super::*;
use oasis7_runtime::LocalCasStore;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

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
#[test]
fn wire3_formal_read_durable_private_prefix_survives_real_process_kill() {
    run(process_kill);
}
#[test]
fn wire3_formal_read_durable_private_prefix_refuses_lost_pin_or_corrupt_snapshot() {
    run(lost_record_material);
}
fn run(case: fn()) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(case)
        .unwrap()
        .join()
        .unwrap();
}
pub(super) fn fixture(h: &QualifiedH<BftProfileTargetV2>) -> PrivateHeaderPrefixV3 {
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
pub(super) fn cas(root: &Path) -> LocalCasStore {
    let path = root.join("cas");
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    LocalCasStore::new(path)
}
pub(super) fn open(
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
        if initialize {
            h.reader.clone()
        } else {
            Arc::new(EmptyIndexReader)
        },
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
    let truncated = PrivateHeaderPrefixV3::from_entries(input.entries()[..1].to_vec()).unwrap();
    assert!(
        open(&h, root.path(), &store, true)
            .unwrap()
            .install_prefix(&truncated, height, &id)
            .is_err(),
        "a valid short prefix cannot satisfy a pinned later cut"
    );
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
    assert_eq!(old.world().state().time, first.execution_commitment.height);
    assert_ne!(old.world().state().time, cut.world().state().time);
    assert_eq!(
        old.world()
            .execution_state_identity()
            .unwrap()
            .state_root_hash,
        first.execution_commitment.state_root
    );
    assert_eq!(
        old.result_tree().root().unwrap(),
        first.execution_commitment.result_root
    );
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
    let schedule_owner = FormalBftHeaderExecutionV3::from_handoff(
        h.anchor.clone(),
        h.target.clone(),
        &h.material,
        h.reader.clone(),
        Vec::new(),
        false,
    )
    .unwrap();
    let mut changed = input.entries().to_vec();
    let first = &changed[0].certificate;
    changed[0].certificate = certify_at_round(
        schedule_owner.schedule(),
        &FormalBftHeaderCandidateV3 {
            header: first.header.clone(),
            commitment: first.execution_commitment.clone(),
        },
        1,
    );
    let alternate = PrivateHeaderPrefixV3::from_entries(changed).unwrap();
    assert!(
        alternate
            .public_prefix()
            .unwrap()
            .verify(
                &oasis7_formal_protocol::bft::verify_formal_bft_handoff_v2(
                    &h.anchor,
                    h.target.clone()
                )
                .unwrap()
            )
            .is_ok()
    );
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
        let first = input.entries()[0].certificate.execution_commitment.height;
        let name = if phase == "pins" {
            "reservation"
        } else {
            "intent"
        };
        let original_path = root.path().join(format!(
            "ledger/formal-bft-header-prefix-v1/{name}-{first:020}.cbor"
        ));
        let original_bytes = fs::read(&original_path).unwrap();
        assert!(
            open(&h, root.path(), &store, true)
                .unwrap()
                .install_prefix(&alternate, height, &id)
                .is_err(),
            "must preserve original QC round at {phase}"
        );
        assert_eq!(fs::read(&original_path).unwrap(), original_bytes);
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

fn lost_record_material() {
    let h = qualified_handoff_v3();
    let input = fixture(&h);
    let (height, id) = target(&input);
    for fault in ["pin", "snapshot"] {
        let root = Scratch::new();
        let store = cas(root.path());
        open(&h, root.path(), &store, true)
            .unwrap()
            .install_prefix(&input, height, &id)
            .unwrap();
        if fault == "pin" {
            store
                .clear_pin_scope("formal-bft-header-prefix-v1")
                .unwrap();
        } else {
            let reference = &input.entries()[0]
                .certificate
                .execution_commitment
                .execution_state_root;
            fs::write(
                store.blobs_dir().join(format!("{reference}.blob")),
                b"corrupt original Snapshot",
            )
            .unwrap();
        }
        assert!(
            open(&h, root.path(), &store, false).is_err(),
            "verification must reject lost {fault}"
        );
        assert!(
            open(&h, root.path(), &store, true).is_err(),
            "import must not repair lost original {fault}"
        );
        if fault == "pin" {
            assert!(
                store
                    .list_pin_scope_shards("formal-bft-header-prefix-v1")
                    .unwrap()
                    .is_empty()
            );
        } else {
            let reference = &input.entries()[0]
                .certificate
                .execution_commitment
                .execution_state_root;
            assert_eq!(
                fs::read(store.blobs_dir().join(format!("{reference}.blob"))).unwrap(),
                b"corrupt original Snapshot"
            );
        }
    }
}

fn process_kill() {
    use std::os::unix::process::ExitStatusExt;
    let h = qualified_handoff_v3();
    let input = fixture(&h);
    let (height, id) = target(&input);
    for phase in ["init-stage", "reservation-stage", "pins", "intent", "link"] {
        let root = Scratch::new();
        let store = cas(root.path());
        for (digest, bytes) in &h.reader.0 {
            assert_eq!(store.put_bytes(bytes).unwrap(), *digest);
        }
        for (name, bytes) in [
            ("trust.json", serde_json::to_vec(&h.trust).unwrap()),
            ("target.json", serde_json::to_vec(&h.target).unwrap()),
            (
                "H.cbor",
                oasis7_wasm_abi::encode_canonical_cbor(&h.material).unwrap(),
            ),
            ("history.cbor", input.canonical_private_bytes().unwrap()),
        ] {
            let path = root.path().join(name);
            fs::write(&path, bytes).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["wire3_formal_read_durable_process_child", "--nocapture"])
            .env("OASIS7_TEST_HEADER_LEDGER_ROOT", root.path())
            .env("OASIS7_TEST_HEADER_LEDGER_PHASE", phase)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let _control = child.stdin.take().unwrap();
        let ready = root.path().join("ledger/prefix-test-publication-ready");
        let deadline = Instant::now() + Duration::from_secs(60);
        while !ready.exists() {
            if child.try_wait().unwrap().is_some() {
                let out = child.wait_with_output().unwrap();
                panic!(
                    "wire3 {phase} child exited: {} {}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let out = child.wait_with_output().unwrap();
                panic!(
                    "wire3 {phase} child missed durable seam: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            open(&h, root.path(), &store, true).is_err(),
            "live process holds the format lock"
        );
        child.kill().unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.signal(), Some(libc::SIGKILL));
        let directory = root.path().join("ledger/formal-bft-header-prefix-v1");
        let protected = match phase {
            "init-stage" => None,
            "reservation-stage" => None,
            "pins" => Some(directory.join(format!(
                "reservation-{first:020}.cbor",
                first = input.entries()[0].certificate.execution_commitment.height
            ))),
            _ => Some(directory.join(format!(
                "intent-{first:020}.cbor",
                first = input.entries()[0].certificate.execution_commitment.height
            ))),
        };
        let before = protected.as_ref().map(|path| fs::read(path).unwrap());
        let cut = open(&h, root.path(), &store, true)
            .unwrap()
            .install_prefix(&input, height, &id)
            .unwrap_or_else(|e| panic!("recover SIGKILL {phase}: {e}"));
        assert_eq!(cut.certified_prefix().entries.len(), 3);
        if let Some(before) = before {
            let first = input.entries()[0].certificate.execution_commitment.height;
            assert_eq!(
                fs::read(directory.join(format!("record-{first:020}.cbor"))).unwrap(),
                before
            );
        }
        drop(cut);
        let mut reopened = open(&h, root.path(), &store, false).unwrap();
        assert_eq!(
            reopened
                .verified_cut(height, &id)
                .unwrap()
                .certified_prefix()
                .entries
                .len(),
            3
        );
    }
}

#[test]
fn wire3_formal_read_durable_process_child() {
    let Ok(root) = std::env::var("OASIS7_TEST_HEADER_LEDGER_ROOT") else {
        return;
    };
    std::thread::Builder::new().stack_size(32 * 1024 * 1024).spawn(move || {
        let root = PathBuf::from(root);
        let phase = std::env::var("OASIS7_TEST_HEADER_LEDGER_PHASE").unwrap();
        let records = root.join("ledger");
        let ready = records.join("prefix-test-publication-ready");
        if phase == "init-stage" {
            crate::execution_bridge_real_tests::real_execution_bridge::durable_transaction::set_private_publication_pause(
                records.join("formal-bft-header-prefix-v1/init-intent.cbor"), ready.clone(),
            );
        }
        let anchor = crate::formal_bft_read_cli::load_qualified_handoff_anchor(&root.join("trust.json")).unwrap();
        let header_target: BftProfileTargetV2 = serde_json::from_slice(&fs::read(root.join("target.json")).unwrap()).unwrap();
        let material = crate::formal_bft_read_cli::read_qualified_handoff_material(&root.join("H.cbor")).unwrap();
        let store = LocalCasStore::new(root.join("cas"));
        let source = Arc::new(crate::formal_bft_read_cli::CasBusinessIndexReader {store:store.clone()});
        let mut ledger = FormalBftHeaderLedgerExecutionV3::open_from_handoff(
            &records, store, anchor, header_target, material, source, true,
        ).unwrap();
        let input = PrivateHeaderPrefixV3::decode_private_canonical(&fs::read(root.join("history.cbor")).unwrap()).unwrap();
        let first = input.entries()[0].certificate.execution_commitment.height;
        if phase == "reservation-stage" {
            crate::execution_bridge_real_tests::real_execution_bridge::durable_transaction::set_private_publication_pause(
                records.join(format!("formal-bft-header-prefix-v1/reservation-{first:020}.cbor")), ready,
            );
        } else if phase != "init-stage" {
            ledger.inject_publication_failure(first, &phase, true).unwrap();
        }
        let (height, id) = target(&input);
        ledger.install_prefix(&input, height, &id).unwrap();
        panic!("wire3 child reached a cut instead of its durable seam");
    }).unwrap().join().unwrap();
}

#[test]
fn wire3_committed_archive_replays_original_world_without_live_ledger() {
    run(committed_archive);
}

fn committed_archive() {
    use oasis7_distfs::protected_bft_v3::{
        BftSuccessorWindowSourceV1, LocalBftSuccessorArchiveStoreV1,
    };
    use oasis7_formal_protocol::bft::verify_formal_bft_handoff_v2;

    let h = qualified_handoff_v3();
    let handoff = verify_formal_bft_handoff_v2(&h.anchor, h.target.clone()).unwrap();
    let source = BftSuccessorWindowSourceV1 {
        scope: handoff.target().scope.clone(),
        binding_digest: "12".repeat(32),
        handoff_position: handoff.handoff_position(),
        handoff_record_id: handoff.handoff_head().record_id.clone(),
        handoff_snapshot_root: handoff.raw_snapshot_root().into(),
        target_digest: handoff.target_digest().into(),
        activation_digest: handoff.activation_digest().into(),
        material_identity: "34".repeat(32),
    };
    let input = fixture(&h);
    let (later_height, later_id) = target(&input);
    let original_cut = &input.entries()[input.entries().len() - 2].certificate;
    let height = original_cut.execution_commitment.height;
    let id = original_cut.header.block_id().unwrap();
    assert_eq!(height + 1, later_height);

    let root = Scratch::new();
    let store = cas(root.path());
    let mut ledger = open(&h, root.path(), &store, true).unwrap();
    ledger
        .install_prefix(&input, later_height, &later_id)
        .unwrap();
    // The complete live ledger contains a later certified record. The
    // archive and publication plan use only records through the chosen cut.
    let durable = ledger.verified_cut(height, &id).unwrap();
    drop(ledger);
    let destination = Scratch::new();
    let archive = LocalBftSuccessorArchiveStoreV1::new(cas(destination.path())).unwrap();
    let mut original_ledger = open(&h, root.path(), &store, false).unwrap();
    let (reference, original) = original_ledger
        .export_committed_prefix(&archive, &source, height, &id)
        .unwrap();
    let (candidate, expected_header, expected_root) = original_ledger.fixture_archived_candidate(
        &archive,
        &source,
        EMPTY_ACTIONS_CBOR.to_vec(),
        None,
    );
    let (missing, _, _) = original_ledger.fixture_archived_candidate(
        &archive,
        &source,
        EMPTY_ACTIONS_CBOR.to_vec(),
        Some("missing"),
    );
    let (extra, _, _) = original_ledger.fixture_archived_candidate(
        &archive,
        &source,
        EMPTY_ACTIONS_CBOR.to_vec(),
        Some("extra"),
    );
    let altered_metadata = ["snapshot", "journal", "derived", "publication"].map(|kind| {
        let (candidate, _, _) = original_ledger.fixture_archived_candidate(
            &archive,
            &source,
            EMPTY_ACTIONS_CBOR.to_vec(),
            Some(kind),
        );
        (kind, candidate)
    });
    drop(original_ledger);
    assert_eq!(
        original.durable_record_digest,
        durable.durable_record_digest()
    );
    assert_eq!(original.world_tick, durable.world().state().time);

    drop(archive);
    drop(store);
    drop(root);
    let reopened =
        LocalBftSuccessorArchiveStoreV1::new(LocalCasStore::new(destination.path().join("cas")))
            .unwrap();
    let replay = |reference, source, id| {
        FormalBftHeaderLedgerExecutionV3::replay_committed_prefix(
            &reopened,
            reference,
            source,
            h.anchor.clone(),
            h.target.clone(),
            height,
            id,
        )
    };
    assert_eq!(replay(&reference, &source, &id).unwrap(), original);
    let mut wrong_source = source.clone();
    wrong_source.material_identity = "56".repeat(32);
    assert!(replay(&reference, &wrong_source, &id).is_err());
    let wrong_id = "78".repeat(32);
    assert!(replay(&reference, &source, &wrong_id).is_err());
    let mut wrong_root = reference.clone();
    wrong_root.root = wrong_root.manifest.digest.clone();
    assert!(replay(&wrong_root, &source, &id).is_err());

    use super::super::super::formal_dependencies::{
        BUSINESS_INDEX_MAX_NODE_BYTES, MAX_RETAINED_INDEX_TOTAL_BYTES,
    };
    use oasis7_distfs::protected_bft_v3::{
        BftSuccessorWindowArchiveEntryV1, BftSuccessorWindowArchiveRefV1,
    };
    let mut excessive = reopened
        .load_manifest(&reference.manifest, &reference.root)
        .unwrap();
    let oversized_count = MAX_RETAINED_INDEX_TOTAL_BYTES / BUSINESS_INDEX_MAX_NODE_BYTES + 1;
    for item in 0..oversized_count {
        let digest = blake3_hex(format!("committed-archive-capacity-fixture-{item}").as_bytes());
        excessive.entries.push(BftSuccessorWindowArchiveEntryV1 {
            identity_digest: digest.clone(),
            object: BftSuccessorWindowArchiveRefV1 {
                digest,
                bytes: BUSINESS_INDEX_MAX_NODE_BYTES as u64,
            },
        });
    }
    excessive
        .entries
        .sort_by(|a, b| a.identity_digest.cmp(&b.identity_digest));
    excessive.usage.objects += oversized_count as u64;
    excessive.usage.total_bytes += (oversized_count * BUSINESS_INDEX_MAX_NODE_BYTES) as u64;
    let mut excessive_reference = reference.clone();
    excessive_reference.manifest = reopened.persist_manifest(&excessive).unwrap();
    excessive_reference.root = excessive.digest().unwrap();
    assert!(
        replay(&excessive_reference, &source, &id)
            .unwrap_err()
            .contains("retained-index byte capacity")
    );

    let write_config = |name: &str, value: serde_json::Value| {
        let path = destination.path().join(name);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        path
    };
    let trust = write_config("trust.json", serde_json::to_value(&h.trust).unwrap());
    let ledger_config = write_config(
        "ledger.json",
        serde_json::json!({
            "schema_version": 1, "trust_config_path": trust, "target": h.target,
            "handoff_material_path": destination.path().join("absent-handoff"),
            "cas_directory": destination.path().join("absent-live-cas"),
            "ledger_directory": destination.path().join("absent-live-ledger"),
            "exact_height": height, "exact_header_id": id,
        }),
    );
    let candidate_pin: super::super::super::formal_header::OriginalCandidateArchivePinV1 =
        serde_json::from_value(candidate.clone()).unwrap();
    let (_, result) = candidate_pin
        .replay(
            &reopened,
            &reference,
            &source,
            h.anchor.clone(),
            h.target.clone(),
            (height, &id),
        )
        .unwrap();
    let result = serde_json::to_value(result).unwrap();
    assert_eq!(result["header_id"], expected_header);
    assert_eq!(result["semantic_state_root"], expected_root);
    assert!(result["publication_bytes"].as_u64().unwrap() > 0);

    let run_cli = |archive_config: &Path| {
        crate::identity_receipt::dispatch(
            [
                "bft-header",
                "archive-prefix",
                "--config",
                ledger_config.to_str().unwrap(),
                "--archive",
                archive_config.to_str().unwrap(),
            ]
            .into_iter(),
        )
        .unwrap()
    };
    let mut archive_options = serde_json::json!({
        "schema_version": 1,
        "archive_directory": destination.path().join("cas"),
        "source": source,
        "reference": reference,
    });
    run_cli(&write_config(
        "ordinary-archive.json",
        archive_options.clone(),
    ))
    .unwrap();
    archive_options["candidate"] = candidate;
    run_cli(&write_config(
        "candidate-archive.json",
        archive_options.clone(),
    ))
    .unwrap();
    archive_options["candidate"] = missing;
    assert!(
        run_cli(&write_config(
            "missing-candidate.json",
            archive_options.clone()
        ))
        .is_err()
    );
    archive_options["candidate"] = extra;
    let error = run_cli(&write_config(
        "extra-candidate.json",
        archive_options.clone(),
    ))
    .unwrap_err();
    assert!(
        error.contains("closed candidate archive contains unobserved dependencies"),
        "{error}"
    );
    for (kind, candidate) in altered_metadata {
        archive_options["candidate"] = candidate;
        let invalid = write_config("altered-metadata.json", archive_options.clone());
        let error = run_cli(&invalid).unwrap_err();
        assert!(
            error.contains(if kind == "publication" {
                "original publication bound differs from actual preparation"
            } else {
                "original publication metadata differs from actual execution"
            }),
            "{kind}: {error}"
        );
    }
}
