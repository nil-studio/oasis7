use super::{archive::WorldEvent, kernel_host::WorldKernel};
use oasis7_runtime::World as RuntimeWorld;
use oasis7_simulator::WorldEventKind;

#[test]
fn original_composite_snapshot_extensions_survive_exact_native_split_rejoin() {
    let mut snapshot = WorldKernel::new().snapshot();
    snapshot.runtime_snapshot = Some(RuntimeWorld::new().snapshot());
    snapshot.chain_resource_manifest.world_id = "native-archive-owner".to_string();
    snapshot.chain_resource_manifest.refresh_hashes();
    let json_before = serde_json::to_vec(&snapshot).unwrap();
    let original = snapshot.clone();
    let (pure, extensions) = snapshot.into_kernel_parts();
    let rebuilt = super::persist::WorldSnapshot::from_kernel_parts(pure, extensions);
    assert_eq!(original, rebuilt);
    assert_eq!(serde_json::to_vec(&rebuilt).unwrap(), json_before);
}

#[test]
fn native_composite_event_roundtrip_keeps_rejection_and_host_field() {
    let event = WorldEvent {
        id: 7,
        time: 4,
        kind: WorldEventKind::ActionRejected {
            reason: oasis7_simulator::RejectReason::RuleDenied {
                notes: vec!["native-owner-rejection".to_string()],
            },
        },
        runtime_event: None,
    };
    let json = serde_json::to_vec(&event).unwrap();
    let copy: WorldEvent = serde_json::from_slice(&json).unwrap();
    assert_eq!(event, copy);
    assert_eq!(json, serde_json::to_vec(&copy).unwrap());
}
