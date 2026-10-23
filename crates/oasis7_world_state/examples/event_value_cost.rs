//! Opt-in Runtime Event Value boundary cost; synthetic inputs, not socket latency.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use oasis7_world_state::{
        CapabilityAuthorizationEvent, SnapshotMeta, WorldEvent, WorldEventBody,
    };
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::hint::black_box;
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    use std::time::Instant;

    struct CountingAllocator;
    static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
    static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
    // SAFETY: All allocation operations delegate unchanged to the system allocator.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOCATIONS.fetch_add(1, Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Relaxed);
            // SAFETY: The caller supplies the layout required by GlobalAlloc.
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: The caller supplies the matching allocation and layout.
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            ALLOCATIONS.fetch_add(1, Relaxed);
            ALLOCATED_BYTES.fetch_add(size as u64, Relaxed);
            // SAFETY: The caller supplies a live allocation, matching layout and valid size.
            unsafe { System.realloc(ptr, layout, size) }
        }
    }
    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    fn sample(label: &str, event: &WorldEvent, iterations: u64) -> serde_json::Value {
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            oasis7_world_state::codec::world_event_json_value(event).unwrap()
        );
        let mut rows = Vec::new();
        for round in 0..6 {
            for codec in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let allocations = ALLOCATIONS.load(Relaxed);
                let allocated_bytes = ALLOCATED_BYTES.load(Relaxed);
                let start = Instant::now();
                for _ in 0..iterations {
                    black_box(if codec {
                        oasis7_world_state::codec::world_event_json_value(black_box(event))
                    } else {
                        serde_json::to_value(black_box(event))
                    })
                    .unwrap();
                }
                let nanos = start.elapsed().as_nanos();
                let allocations = ALLOCATIONS.load(Relaxed) - allocations;
                let allocated_bytes = ALLOCATED_BYTES.load(Relaxed) - allocated_bytes;
                rows.push(
                    serde_json::json!({"round":round,"codec":codec,"nanoseconds":nanos,
                "allocations":allocations,"allocated_bytes":allocated_bytes}),
                );
            }
        }
        serde_json::json!({"label":label,"iterations":iterations,"samples":rows})
    }
    pub(super) fn run() {
        let small = WorldEvent {
            id: 1,
            time: 1,
            caused_by: None,
            body: WorldEventBody::SnapshotCreated(SnapshotMeta { journal_len: 1 }),
        };
        let private = WorldEvent {
            body: WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::WorldServiceIntentRecorded {
                    key: "correlation".into(),
                    record: serde_json::json!({"request":{"signed_payload":{"kind":"agent_chat",
                    "payload":{"message":"private"}}}}),
                },
            ),
            ..small.clone()
        };
        let large = WorldEvent {
            body: WorldEventBody::CapabilityAuthorization(
                CapabilityAuthorizationEvent::WorldServiceIntentRecorded {
                    key: "correlation".into(),
                    record: serde_json::json!({"payload":"x".repeat(1 << 20)}),
                },
            ),
            ..small.clone()
        };
        println!(
            "{}",
            serde_json::json!({"schema":1,
        "scope":"same-process synthetic Value conversion only; allocation counters include realloc requests, not peak RSS; not socket latency",
        "cases":[sample("small_public", &small, 100_000),
                 sample("private_canonical_input", &private, 50_000),
                 sample("large_event", &large, 100)]})
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    native::run();
}

#[cfg(target_arch = "wasm32")]
fn main() {
    panic!("event_value_cost requires a native target");
}
