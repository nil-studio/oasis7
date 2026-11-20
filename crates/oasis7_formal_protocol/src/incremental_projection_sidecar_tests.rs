use super::*;
use crate::controlled_authority::{
    formal::bounded_context::{FormalBoundedContextV1, FormalBoundedWorkV1},
    replicated_protocol::{ArtifactObject, ClosedRecord},
};
use crate::incremental_projection_sidecar::*;

impl PaidProjectionOriginalReaderV1 for Objects {
    type Error = std::io::Error;
    fn allocation_budget(&self) -> &MemoryBudget {
        &self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        self.class
    }
    fn read_projection_original_bounded_v1(
        &mut self,
        id: &[u8; 32],
        max: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> std::io::Result<Arc<ReservedValue<Vec<u8>>>> {
        common
            .consume_read_bytes(max as u64)
            .map_err(std::io::Error::other)?;
        self.read_original(id, max, common.memory())
    }
}

fn prepared(
    bad_record: bool,
) -> (
    Setup,
    ReservedValue<CheckedCommitCore>,
    ClosedRecord,
    Objects,
) {
    let s = setup();
    let (mut objects, pages) = sources(&s, &s.history);
    let core = verify_accepted_commit_core(
        &AcceptedCoreRequest {
            head: &s.commit,
            original_authority: &s.authority,
            parent: None,
            max_pending_node_bytes: 4 << 20,
        },
        &mut objects,
        &pages,
        &s.memory,
        &mut budget(),
        || false,
    )
    .unwrap();
    assert_eq!(core.candidate_record_id(), s.commit.verified_record_id());
    assert_eq!(core.body(), &s.history.body);
    let sidecar = ProjectionSidecarV1 {
        record: if bad_record {
            [0; 32]
        } else {
            s.commit.verified_record_id()
        },
        commit_body: core.catalog().entry.commit,
        roots: core.body().roots,
        retained_history_root_json: b"null".to_vec(),
    };
    let bytes = sidecar
        .canonical_bytes_with_work(&s.memory, &mut |_, _| Ok(()))
        .unwrap();
    let side_id = *blake3::hash(&bytes).as_bytes();
    let body_id = core.catalog().entry.commit;
    // This fixture supplies the exact body/sidecar subgraph read by this DATA
    // verifier. It does not claim execution-artifact or full graph qualification.
    let mut graph = vec![
        ArtifactObject {
            content_hash: hex::encode(body_id),
            bytes: objects.values[&body_id].to_vec(),
            references: vec![hex::encode(side_id)],
        },
        ArtifactObject {
            content_hash: hex::encode(side_id),
            bytes: bytes.to_vec(),
            references: vec![],
        },
    ];
    graph.sort_by(|left, right| left.content_hash.cmp(&right.content_hash));
    objects.values.insert(side_id, Arc::new(bytes));
    objects.reads = 0;
    let record = s.commit.record();
    let material = ClosedRecord {
        payload_digest: record.payload_digest.clone(),
        before_state_root: record.before_state_root.clone(),
        after_state_root: record.after_state_root.clone(),
        roots: record.artifact_roots.clone(),
        objects: graph,
    };
    (s, core, material, objects)
}

fn work() -> FormalBoundedWorkV1 {
    FormalBoundedWorkV1::new(1_000_000, 16 << 20).unwrap()
}

#[test]
fn sidecar_reads_real_originals_and_retains_exact_head_core_and_work_binding() {
    let (s, core, material, mut objects) = prepared(false);
    let meter = work();
    let mut stop = || false;
    let before = s.memory.used();
    let mut common = FormalBoundedContextV1::new(&s.memory, &meter, &mut stop);
    let paid =
        read_projection_sidecar_bounded_v1(&s.commit, &core, &material, &mut objects, &mut common)
            .unwrap();
    assert_eq!(
        objects.reads, 2,
        "body and sidecar must both come from the original source"
    );
    assert_eq!(paid.retained_history_root_json(), b"null");
    assert_eq!(paid.record_id(), s.commit.verified_record_id());
    assert_eq!(paid.body_id(), core.catalog().entry.commit);
    assert_eq!(paid.roots_id(), core.body().roots);
    paid.validate_context(&s.commit, &core, &mut common)
        .unwrap();
    assert!(s.memory.used() > before);
    let other_work = work();
    assert!(
        paid.validate_context(
            &s.commit,
            &core,
            &mut FormalBoundedContextV1::new(&s.memory, &other_work, &mut stop),
        )
        .is_err()
    );
    let other_pool = MemoryBudget::new(32 << 20, 0).unwrap();
    assert!(
        paid.validate_context(
            &s.commit,
            &core,
            &mut FormalBoundedContextV1::new(&other_pool, &meter, &mut stop),
        )
        .is_err()
    );
    drop(paid);
    assert_eq!(s.memory.used(), before);
    assert_eq!(other_pool.used(), 0);
}

#[test]
fn sidecar_rejects_substitution_duplicates_and_wrong_context_without_retaining_output() {
    for mode in 0..5 {
        let (s, core, mut material, mut objects) = prepared(mode == 0);
        match mode {
            1 => material.payload_digest = "00".repeat(32),
            2 => {
                let side = material
                    .objects
                    .iter()
                    .find(|v| v.bytes.starts_with(PROJECTION_SIDECAR_DOMAIN_V1))
                    .unwrap()
                    .clone();
                material.objects.push(side);
            }
            3 => {
                let id = core.catalog().entry.commit;
                let mut altered = objects.values[&id].to_vec();
                altered[0] ^= 1;
                let fee = s
                    .memory
                    .reserve(altered.capacity() + 64, MemoryClass::Ordinary)
                    .unwrap();
                objects.values.insert(id, Arc::new(fee.retain(altered)));
            }
            4 => {
                objects.values.remove(&core.catalog().entry.commit);
            }
            _ => {}
        }
        let before = s.memory.used();
        let meter = work();
        let mut stop = || false;
        let error = read_projection_sidecar_bounded_v1(
            &s.commit,
            &core,
            &material,
            &mut objects,
            &mut FormalBoundedContextV1::new(&s.memory, &meter, &mut stop),
        )
        .err()
        .expect("invalid side data must not produce a verified owner");
        match mode {
            0 => assert!(matches!(error, ProjectionSidecarReadErrorV1::Context(_))),
            1 => {
                assert!(matches!(
                    error,
                    ProjectionSidecarReadErrorV1::Invalid("projection side graph record binding")
                ));
                assert_eq!(objects.reads, 0);
            }
            2 => assert!(matches!(
                error,
                ProjectionSidecarReadErrorV1::Invalid("projection side uniqueness/bound")
            )),
            3 => assert!(matches!(
                error,
                ProjectionSidecarReadErrorV1::Invalid("projection Original content hash")
            )),
            _ => match error {
                ProjectionSidecarReadErrorV1::Original(e) => {
                    assert_eq!(e.kind(), std::io::ErrorKind::NotFound)
                }
                other => panic!("original source error was replaced: {other:?}"),
            },
        }
        assert_eq!(s.memory.used(), before);
    }
}

#[test]
fn sidecar_resource_failures_prevent_original_io() {
    for mode in 0..4 {
        let (s, core, material, mut objects) = prepared(false);
        if mode == 2 {
            objects.class = MemoryClass::Recovery;
        }
        if mode == 3 {
            objects.memory = MemoryBudget::new(32 << 20, 0).unwrap();
        }
        let meter =
            FormalBoundedWorkV1::new(if mode == 1 { 0 } else { 1_000_000 }, 16 << 20).unwrap();
        let mut stop = || mode == 0;
        let before = s.memory.used();
        let error = read_projection_sidecar_bounded_v1(
            &s.commit,
            &core,
            &material,
            &mut objects,
            &mut FormalBoundedContextV1::new(&s.memory, &meter, &mut stop),
        )
        .err()
        .expect("resource failure must not produce an owner");
        if mode < 2 {
            assert!(matches!(error, ProjectionSidecarReadErrorV1::Context(_)));
        } else {
            assert!(matches!(
                error,
                ProjectionSidecarReadErrorV1::Invalid("projection side Original pool/class")
            ));
        }
        assert_eq!(objects.reads, 0);
        assert_eq!(s.memory.used(), before);
    }
}

#[test]
fn sidecar_codec_preserves_opaque_json_bytes_and_rejects_size_and_work_failures() {
    let memory = MemoryBudget::new(64 << 10, 0).unwrap();
    let json = "{\"original\":\"原文\"}".as_bytes().to_vec();
    let mut sidecar = ProjectionSidecarV1 {
        record: [1; 32],
        commit_body: [2; 32],
        roots: [3; 32],
        retained_history_root_json: json.clone(),
    };
    let bytes = sidecar
        .canonical_bytes_with_work(&memory, &mut |_, _| Ok(()))
        .unwrap();
    let mut expected = b"oasis7.iws.projection-side.v1\0".to_vec();
    expected.extend_from_slice(&(98u32 + json.len() as u32).to_be_bytes());
    expected.extend_from_slice(&[1; 32]);
    expected.extend_from_slice(&[2; 32]);
    expected.extend_from_slice(&[3; 32]);
    expected.extend_from_slice(&(json.len() as u16).to_be_bytes());
    expected.extend_from_slice(&json);
    assert_eq!(*bytes, expected);
    drop(bytes);
    assert_eq!(memory.used(), 0);
    for size in [0, PROJECTION_SIDECAR_MAX_JSON_BYTES + 1] {
        sidecar.retained_history_root_json = vec![b' '; size];
        assert!(matches!(
            sidecar.canonical_bytes_with_work(&memory, &mut |_, _| Ok(())),
            Err(ExecutionError::Invalid("projection side JSON bound"))
        ));
    }
    sidecar.retained_history_root_json = vec![b' '; PROJECTION_SIDECAR_MAX_JSON_BYTES];
    let bytes = sidecar
        .canonical_bytes_with_work(&memory, &mut |_, _| Ok(()))
        .unwrap();
    assert!(bytes.len() <= PROJECTION_SIDECAR_MAX_BYTES);
    drop(bytes);
    let mut calls = 0;
    let error = sidecar
        .canonical_bytes_with_work(&memory, &mut |_, _| {
            calls += 1;
            if calls == 3 {
                Err(ExecutionError::Invalid("sidecar work stopped"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert!(matches!(
        error,
        ExecutionError::Invalid("sidecar work stopped")
    ));
    assert_eq!(calls, 3);
    assert_eq!(memory.used(), 0);
}
