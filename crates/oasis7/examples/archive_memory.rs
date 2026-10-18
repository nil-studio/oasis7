//! Repeat in fresh processes; allocator bytes are live requested bytes, not RSS.
use oasis7::chain_resource_schema::ChainResourceDerivationContext;
use oasis7::simulator::{WorldKernel as Host, WorldSnapshot};
use oasis7_simulator::{Action, GeoPos, LocationProfile, WorldKernel as Native};
use sha2::{Digest, Sha256};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

struct Meter;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
fn allocated(size: usize) {
    let live = LIVE.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(live, Relaxed);
    ALLOCATED.fetch_add(size, Relaxed);
    CALLS.fetch_add(1, Relaxed);
}
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forward the caller-provided valid layout to System unchanged.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            allocated(layout.size());
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        // SAFETY: ptr and its original layout are forwarded to the same allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, size: usize) -> *mut u8 {
        // SAFETY: preserve the caller allocation, old layout and valid new size.
        let result = unsafe { System.realloc(ptr, old, size) };
        if !result.is_null() {
            LIVE.fetch_sub(old.size(), Relaxed);
            allocated(size);
        }
        result
    }
}
#[global_allocator]
static METER: Meter = Meter;
fn measure<T>(stage: &str, f: impl FnOnce() -> T) -> T {
    let before = LIVE.load(Relaxed);
    PEAK.store(before, Relaxed);
    ALLOCATED.store(0, Relaxed);
    CALLS.store(0, Relaxed);
    let start = Instant::now();
    let value = f();
    let micros = start.elapsed().as_micros();
    let live = LIVE.load(Relaxed);
    let peak = PEAK.load(Relaxed);
    let allocated = ALLOCATED.load(Relaxed);
    let calls = CALLS.load(Relaxed);
    let rss_kib = std::process::Command::new("/bin/ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok());
    let rss = rss_kib
        .map(|k| (k * 1024).to_string())
        .unwrap_or_else(|| "null".into());
    println!(
        "{{\"stage\":\"{stage}\",\"micros\":{micros},\"before\":{before},\"live\":{live},\"peak\":{peak},\"allocated\":{allocated},\"calls\":{calls},\"rss_bytes\":{rss}}}"
    );
    value
}
fn encoded<T: serde::Serialize>(stage: &str, value: impl FnOnce() -> T) {
    let (bytes, hash) = measure(stage, || {
        let bytes = serde_cbor::to_vec(&value()).unwrap();
        (bytes.len(), blake3::hash(&bytes))
    });
    println!("{{\"encoding_stage\":\"{stage}\",\"cbor_bytes\":{bytes},\"blake3\":\"{hash}\"}}");
}
fn append(kernel: &mut Native, start: usize, count: usize) {
    for i in start..start + count {
        kernel.submit_action(Action::RegisterAgent {
            agent_id: format!("agent-{i:06}"),
            location_id: "origin".into(),
        });
        kernel.step().unwrap();
    }
}
trait FixtureKernel {
    fn execute(&mut self, action: Action);
    fn modules(&self) -> usize;
}
impl FixtureKernel for Native {
    fn execute(&mut self, action: Action) {
        let deployment = matches!(action, Action::DeployModuleArtifact { .. });
        self.submit_action(action);
        let event = self.step().unwrap();
        if deployment {
            assert!(matches!(
                event.kind,
                oasis7_simulator::WorldEventKind::ModuleArtifactDeployed {
                    bytes_len: 1048576,
                    ..
                }
            ));
        }
    }
    fn modules(&self) -> usize {
        self.model().module_artifacts.len()
    }
}
impl FixtureKernel for Host {
    fn execute(&mut self, action: Action) {
        let deployment = matches!(action, Action::DeployModuleArtifact { .. });
        self.submit_action(action);
        let event = self.step().unwrap();
        if deployment {
            assert!(matches!(
                event.kind,
                oasis7_simulator::WorldEventKind::ModuleArtifactDeployed {
                    bytes_len: 1048576,
                    ..
                }
            ));
        }
    }
    fn modules(&self) -> usize {
        self.model().module_artifacts.len()
    }
}
fn create<K: FixtureKernel + Default>() -> K {
    let mut kernel = K::default();
    kernel.execute(Action::RegisterLocation {
        location_id: "origin".into(),
        name: "Origin".into(),
        pos: GeoPos::new(0, 0, 0),
        profile: LocationProfile::default(),
    });

    for i in 0..1000 {
        kernel.execute(Action::RegisterAgent {
            agent_id: format!("agent-{i:06}"),
            location_id: "origin".into(),
        });
    }
    for i in 0..4 {
        let bytes = vec![i as u8; 1024 * 1024];
        kernel.execute(Action::DeployModuleArtifact {
            publisher_agent_id: "agent-000000".into(),
            wasm_hash: hex::encode(Sha256::digest(&bytes)),
            wasm_bytes: bytes,
            module_id_hint: Some(format!("module-{i}")),
        });
    }
    assert_eq!(kernel.modules(), 4);
    kernel
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("host");
    let extended = args.get(2).is_some_and(|v| v == "extended");
    if mode == "host" {
        drop(measure("host_create", create::<Host>));
    }
    let mut source = measure("create", create::<Native>);
    let origin = source.kernel_snapshot();
    let prefix = source.kernel_journal_snapshot();
    let native_snapshot_bytes = serde_cbor::to_vec(&origin).unwrap().len();
    let native_journal_bytes = serde_cbor::to_vec(&prefix).unwrap().len();
    println!(
        "{{\"native_snapshot_cbor_bytes\":{native_snapshot_bytes},\"native_journal_cbor_bytes\":{native_journal_bytes}}}"
    );
    append(&mut source, 1000, 1000);
    let journal = source.kernel_journal_snapshot();
    drop(source);
    println!(
        "{{\"mode\":\"{mode}\",\"extended\":{extended},\"agents\":1000,\"locations\":1,\"modules\":4,\"events\":{},\"append_agents\":1000,\"wasm_bytes\":4194304}}",
        prefix.events.len()
    );
    if mode == "native" {
        let mut kernel = measure("recover", || {
            Native::from_kernel_snapshot(origin.clone(), prefix.clone()).unwrap()
        });
        encoded("stationary_snapshot_encode", || kernel.kernel_snapshot());
        measure("append", || append(&mut kernel, 1000, 1000));
        encoded("snapshot_encode", || kernel.kernel_snapshot());
        encoded("journal_encode", || kernel.kernel_journal_snapshot());
        encoded("direct_codec_encode", || &kernel);
        drop(kernel);
        measure("replay", || {
            std::hint::black_box(Native::replay_from_kernel_snapshot(origin, journal).unwrap());
        });
    } else {
        let empty = Host::new().snapshot();
        let (_, mut extensions) = empty.into_kernel_parts();
        let context = ChainResourceDerivationContext::simulator_default(origin.time);
        extensions.chain_resource_manifest = oasis7_simulator::from_simulator_state(
            context,
            &origin.model,
            &origin.config,
            &origin.chunk_runtime,
            &prefix.events,
        );
        extensions.latest_chain_resource_delta = oasis7_simulator::latest_from_simulator_journal(
            context,
            &extensions.chain_resource_manifest,
            &prefix.events,
        );
        if extended {
            let mut runtime = oasis7::runtime::World::new();
            runtime.submit_action(oasis7::runtime::Action::RegisterAgent {
                agent_id: "historical-agent".into(),
                pos: GeoPos::new(1, 2, 3),
            });
            runtime.step().unwrap();
            extensions.runtime_snapshot = Some(runtime.snapshot());
            extensions.player_gameplay = Some(serde_json::from_value(serde_json::json!({
                "stage_id":"first_session_loop", "stage_status":"active", "execution_state":"accepted",
                "goal_id":"archive", "goal_kind":"create_first_world_feedback", "goal_title":"Goal",
                "objective":"Retain historical objective", "progress_detail":"Recorded", "next_step_hint":"Resume"
            })).unwrap());
            extensions.chain_resource_manifest.world_id = "external-archive".into();
            extensions.chain_resource_manifest.refresh_hashes();
        }
        let snapshot = WorldSnapshot::from_kernel_parts(origin, extensions);
        let host_journal =
            |journal: oasis7_simulator::KernelJournal| oasis7::simulator::WorldJournal {
                version: journal.version,
                events: journal
                    .events
                    .into_iter()
                    .map(|e| oasis7::simulator::WorldEvent::from_kernel_parts(e, None))
                    .collect(),
            };
        let prefix = host_journal(prefix);
        let mut journal = host_journal(journal);
        let mut prefix = prefix;
        if extended {
            let mut runtime = oasis7::runtime::World::new();
            runtime.submit_action(oasis7::runtime::Action::RegisterAgent {
                agent_id: "historical-agent".into(),
                pos: GeoPos::new(1, 2, 3),
            });
            runtime.step().unwrap();
            let event = runtime.journal().events.last().unwrap().clone();
            prefix.events[0].runtime_event = Some(event.clone());
            journal.events[0].runtime_event = Some(event);
        }
        println!(
            "{{\"host_snapshot_cbor_bytes\":{},\"host_journal_cbor_bytes\":{}}}",
            serde_cbor::to_vec(&snapshot).unwrap().len(),
            serde_cbor::to_vec(&prefix).unwrap().len()
        );
        let mut kernel = measure("recover", || {
            Host::from_snapshot(snapshot.clone(), prefix.clone()).unwrap()
        });
        encoded("stationary_snapshot_encode", || kernel.snapshot());
        measure("append", || {
            for i in 1000..2000 {
                kernel.submit_action(Action::RegisterAgent {
                    agent_id: format!("agent-{i:06}"),
                    location_id: "origin".into(),
                });
                kernel.step().unwrap();
            }
        });
        encoded("snapshot_encode", || kernel.snapshot());
        encoded("journal_encode", || kernel.journal_snapshot());
        encoded("direct_codec_encode", || &kernel);
        drop(kernel);
        measure("replay", || {
            std::hint::black_box(Host::replay_from_snapshot(snapshot, journal).unwrap());
        });
    }
}
