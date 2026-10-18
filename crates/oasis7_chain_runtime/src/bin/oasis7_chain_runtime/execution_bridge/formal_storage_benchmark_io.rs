use super::*;
use std::{
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(super) struct Scratch(PathBuf);
impl Scratch {
    pub(super) fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-storage-capacity-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self(path)
    }
    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Domain {
    root: PathBuf,
    optimized: PathBuf,
    samples: BTreeSet<String>,
    raw_written: u64,
    raw_write_ns: u128,
}
pub(super) struct Corpus {
    domains: [Domain; 2],
}
impl Corpus {
    pub(super) fn new(root: &Path) -> Self {
        let domain = |role: &str| {
            let path = root.join(format!("source-{role}"));
            let optimized = root.join(format!("optimized-{role}"));
            for dir in [&path, &path.join("formal-objects-v1"), &optimized] {
                fs::DirBuilder::new().mode(0o700).create(dir).unwrap();
            }
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path.join(".formal-endpoint.lock"))
                .unwrap()
                .sync_all()
                .unwrap();
            Domain {
                root: path,
                optimized,
                samples: BTreeSet::new(),
                raw_written: 0,
                raw_write_ns: 0,
            }
        };
        Self {
            domains: [domain("primary"), domain("replica")],
        }
    }
    pub(super) fn primary_objects(&self) -> PathBuf {
        self.domains[0].root.join("formal-objects-v1")
    }
    fn common(&mut self, bytes: &[u8]) {
        for domain in &mut self.domains {
            domain.put(bytes);
        }
    }
    pub(super) fn capture(
        &mut self,
        candidate: &FormalCandidateMaterialV1,
        proof: &FormalQualifiedCommitProofV1,
    ) {
        verify_record_material(&candidate.candidate.record, &candidate.material).unwrap();
        for material in std::iter::once(&candidate.material).chain(candidate.frozen_material.iter())
        {
            for object in &material.objects {
                self.common(&object.bytes);
            }
        }
        let envelope = super::super::super::super::formal_business_package::envelope(
            &candidate.material,
            true,
        )
        .unwrap()
        .unwrap();
        for node in envelope.delta.node_writes.iter().chain(
            envelope
                .retained_history_delta
                .iter()
                .flat_map(|d| d.node_writes.iter()),
        ) {
            assert_eq!(blake3_hex(&node.bytes), node.digest);
            self.common(&node.bytes);
        }
        let (primary, replica) = self.domains.split_at_mut(1);
        for (domain, role, key, final_receipt, complete) in [
            (
                &mut primary[0],
                EndpointRole::Primary,
                5,
                &proof.decision.primary,
                &proof.primary_complete,
            ),
            (
                &mut replica[0],
                EndpointRole::Replica,
                6,
                &proof.decision.replica,
                &proof.replica_complete,
            ),
        ] {
            let body = FormalPrepareReceiptBodyV1 {
                final_receipt: final_receipt.body.clone(),
                candidate_material_digest: formal_candidate_material_digest(candidate).unwrap(),
            };
            let prepare = FormalPrepareReceiptV1 {
                signature_hex: fixtures::sign(
                    formal_prepare_receipt_signing_bytes(&body).unwrap(),
                    key,
                ),
                body,
            };
            let mut states = vec![
                FormalRequestStateV1::Prepared {
                    candidate: candidate.clone(),
                    receipt: Some(prepare.clone()),
                },
                FormalRequestStateV1::Decided {
                    candidate: candidate.clone(),
                    prepare: prepare.clone(),
                    receipt: final_receipt.clone(),
                },
                FormalRequestStateV1::Ready {
                    candidate: candidate.clone(),
                    prepare,
                    decision: proof.decision.clone(),
                    complete: Some(complete.clone()),
                },
                FormalRequestStateV1::Qualified {
                    candidate: candidate.clone(),
                    proof: proof.clone(),
                },
            ];
            if let Some(frozen) = &candidate.frozen {
                states.push(FormalRequestStateV1::Frozen {
                    input: frozen.reservation.clone(),
                    material: candidate.frozen_material.clone().unwrap(),
                    receipt: Some(if role == EndpointRole::Primary {
                        frozen.primary.clone()
                    } else {
                        frozen.replica.clone()
                    }),
                    certificate: Some(frozen.clone()),
                });
            }
            for state in states {
                let bytes = to_cbor(&state).unwrap();
                let decoded: FormalRequestStateV1 = serde_cbor::from_slice(&bytes).unwrap();
                assert_eq!(state, decoded);
                domain.put(&bytes);
            }
            fs::File::open(domain.root.join("formal-objects-v1"))
                .unwrap()
                .sync_all()
                .unwrap();
        }
    }
    pub(super) fn compare(&self) -> Vec<serde_json::Value> {
        self.domains.iter().enumerate().map(|(index,domain)| {
            let before=inventory_storage(&domain.root.join("formal-objects-v1")); assert!(before.complete);
            let started=Instant::now();
            eprintln!("STORAGE_CAPACITY_PHASE=shadow_start domain={index}");
            let options=FormalStorageShadowOptions { optimized:true, max_logical_bytes:64*1024*1024 };
            let mut previous_phase=String::new();
            let shadow=loop {
                let progress=advance_formal_storage_shadow(&domain.root,&domain.optimized.join("formal-objects-v1"),&options).unwrap();
                if progress.phase != previous_phase {
                    eprintln!("STORAGE_CAPACITY_PHASE=shadow_{} domain={index} processed={} total={}",progress.phase,progress.processed_records,progress.total_records);
                    previous_phase=progress.phase.clone();
                }
                if progress.complete { break progress.report.expect("complete shadow report"); }
            };
            let build_ns=started.elapsed().as_nanos();
            let after=inventory_storage(&domain.root.join("formal-objects-v1"));
            assert_eq!(serde_json::to_value(&before).unwrap(),serde_json::to_value(&after).unwrap());
            let source=FileFormalArchiveSourceV1::new(domain.root.clone()).unwrap();
            let optimized=FileFormalArchiveSourceV1::new(domain.optimized.clone()).unwrap();
            let mut raw_full=Vec::new();let mut target_full=Vec::new();let mut raw_range=Vec::new();let mut target_range=Vec::new();
            for id in &domain.samples {
                let timer=Instant::now();let bytes=source.read_object(id).unwrap().unwrap();raw_full.push(timer.elapsed().as_nanos());
                let timer=Instant::now();let rebuilt=optimized.read_object(id).unwrap().unwrap();target_full.push(timer.elapsed().as_nanos());assert_eq!(bytes,rebuilt);
                let offset=bytes.len() as u64/2; let limit=1024u32;
                let timer=Instant::now();let raw=source.read_range(id,offset,limit).unwrap().unwrap();raw_range.push(timer.elapsed().as_nanos());
                let timer=Instant::now();let target=optimized.read_range(id,offset,limit).unwrap().unwrap();target_range.push(timer.elapsed().as_nanos());
                raw.validate(id,offset,limit).unwrap();target.validate(id,offset,limit).unwrap();assert_eq!(raw.bytes,target.bytes);
                assert_eq!(target.bytes,bytes[offset as usize..(offset as usize+limit as usize).min(bytes.len())]);
            }
            serde_json::json!({"domain":if index==0{"primary"}else{"replica"},"source_unchanged":true,"raw_written_bytes":domain.raw_written,"raw_put_and_dedup_verification_io_ns":domain.raw_write_ns,"shadow_build_and_all_object_readback_ns":build_ns,"shadow":shadow,
                "latency_scope":"warm-cache observations after full shadow readback; deterministic 128 lowest digest sample; nanoseconds; no OS cache flush",
                "source_full":percentiles(raw_full),"optimized_full":percentiles(target_full),"source_range":percentiles(raw_range),"optimized_range":percentiles(target_range)})
        }).collect()
    }
}
impl Domain {
    fn put(&mut self, bytes: &[u8]) {
        let id = blake3_hex(bytes);
        let path = self.root.join("formal-objects-v1").join(&id);
        let timer = Instant::now();
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(bytes).unwrap();
                file.sync_all().unwrap();
                self.raw_written += bytes.len() as u64;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                assert_eq!(fs::read(path).unwrap(), bytes)
            }
            Err(e) => panic!("raw capacity corpus write: {e}"),
        }
        self.raw_write_ns += timer.elapsed().as_nanos();
        self.samples.insert(id);
        if self.samples.len() > 128 {
            self.samples.pop_last();
        }
    }
}
fn percentiles(mut values: Vec<u128>) -> serde_json::Value {
    values.sort_unstable();
    assert!(!values.is_empty());
    let n = values.len();
    serde_json::json!({"count":n,"p50_ns":values[(n-1)/2],"p95_ns":values[(95*n).div_ceil(100).saturating_sub(1).min(n-1)],"p99_ns":values[(99*n).div_ceil(100).saturating_sub(1).min(n-1)],"max_ns":values[n-1]})
}
