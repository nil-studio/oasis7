use super::*;
use crate::{
    incremental_projection::{FieldDescriptor, FieldShape},
    incremental_typed_record_test_support::Pages,
    incremental_world_field_key::WorldFieldRecordKindV1 as K,
};
use oasis7_formal_protocol::incremental_state::{StateDeltaV1, StatePageWriter, StateWriteV1};
fn metadata(field: FieldDescriptor) -> Vec<u8> {
    WorldFieldKeyV1::new(field, K::Metadata, None)
        .unwrap()
        .as_bytes()
        .to_vec()
}
#[test]
fn world_key_router_binds_registered_owner_shape_namespace_and_dedicated_families() {
    for field in WORLD_STATE_FIELDS
        .iter()
        .filter(|f| f.name != "module_states")
    {
        let FieldDestination::State(ns) = field.destination else {
            continue;
        };
        assert_eq!(
            route_world_key_v1(ns, &metadata(*field)).unwrap(),
            WorldKeyOwnerV1::WorldStateField
        );
    }
    for field in RUNTIME_TYPED_GENESIS_FIELDS_V1 {
        let FieldDestination::State(ns) = field.destination else {
            panic!()
        };
        assert_eq!(
            route_world_key_v1(ns, &metadata(*field)).unwrap(),
            WorldKeyOwnerV1::RuntimeField
        );
        let other = if ns == Ns::Business {
            Ns::Authorization
        } else {
            Ns::Business
        };
        assert!(route_world_key_v1(other, &metadata(*field)).is_err());
    }
    let field = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "invented_field",
        shape: FieldShape::Value,
        destination: FieldDestination::State(Ns::Business),
    };
    assert!(route_world_key_v1(Ns::Business, &metadata(field)).is_err());
    let mut wrong = *RUNTIME_TYPED_GENESIS_FIELDS_V1.first().unwrap();
    wrong.owner = FieldOwner::WorldState;
    assert!(route_world_key_v1(Ns::Artifacts, &metadata(wrong)).is_err());
    wrong.owner = FieldOwner::Runtime;
    wrong.shape = FieldShape::Map;
    assert!(route_world_key_v1(Ns::Artifacts, &metadata(wrong)).is_err());
    let opaque = *WORLD_STATE_FIELDS
        .iter()
        .find(|f| f.name == "module_states")
        .unwrap();
    assert!(route_world_key_v1(Ns::Business, &metadata(opaque)).is_err());
    let key = ModuleStateKeyV1::new("original").unwrap();
    assert_eq!(
        route_world_key_v1(Ns::Business, key.as_bytes()).unwrap(),
        WorldKeyOwnerV1::ModuleState
    );
    assert!(route_world_key_v1(Ns::Allocator, key.as_bytes()).is_err());
    assert!(route_world_key_v1(Ns::Business, b"unknown").is_err());
}
#[test]
fn world_key_router_runtime_sequence_has_only_the_three_original_tags() {
    for tag in 0..=4 {
        let mut key = RUNTIME_SEQUENCE_KEY_DOMAIN_V1.to_vec();
        key.push(tag);
        assert_eq!(
            route_world_key_v1(Ns::Allocator, &key).is_ok(),
            (1..=3).contains(&tag)
        );
        key.push(0);
        assert!(route_world_key_v1(Ns::Allocator, &key).is_err());
    }
    assert_eq!(
        route_world_key_v1(Ns::Allocator, EVENT_SEQUENCE_KEY_V1).unwrap(),
        WorldKeyOwnerV1::Allocator
    );
    assert!(route_world_key_v1(Ns::Business, EVENT_SEQUENCE_KEY_V1).is_err());
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10000,
        leaves_remaining: 1000,
        read_bytes_remaining: 16 << 20,
    }
}
#[test]
fn world_key_router_recovery_requires_explicit_exact_root_same_pool_readmission() {
    struct Recovery<'a>(&'a Pages);
    impl MerkleNodeReader for Recovery<'_> {
        type Node = <Pages as MerkleNodeReader>::Node;
        fn allocation_budget(&self) -> Option<&MemoryBudget> {
            self.0.allocation_budget()
        }
        fn allocation_class(&self) -> Option<MemoryClass> {
            Some(MemoryClass::Recovery)
        }
        fn read_node(&self, _: &str, _: usize) -> Result<Option<Self::Node>, String> {
            Err("empty routing fixture must not read a page".into())
        }
    }
    let memory = MemoryBudget::new(8 << 20, 1 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 16);
    let roots = RootSetV1::empty("routing-recovery".into(), [44; 32]).unwrap();
    let source = qualify_world_key_routing_v1(
        &roots,
        &Recovery(&pages),
        &memory,
        MemoryClass::Recovery,
        &mut scan(),
        || None,
        || false,
    )
    .unwrap();
    assert_eq!(source.class(), MemoryClass::Recovery);
    let ordinary =
        readmit_world_key_routing_ordinary_v1(&source, &roots, &memory, || false).unwrap();
    ordinary.validate_binding(&roots).unwrap();
    assert_eq!(ordinary.class(), MemoryClass::Ordinary);
    assert!(readmit_world_key_routing_ordinary_v1(&ordinary, &roots, &memory, || false).is_err());
    let other = RootSetV1::empty("other-world".into(), [44; 32]).unwrap();
    assert!(readmit_world_key_routing_ordinary_v1(&source, &other, &memory, || false).is_err());
    let foreign = MemoryBudget::new(8 << 20, 1 << 20).unwrap();
    assert!(readmit_world_key_routing_ordinary_v1(&source, &roots, &foreign, || false).is_err());
    assert!(matches!(
        readmit_world_key_routing_ordinary_v1(&source, &roots, &memory, || true),
        Err(ValueTreeError::Cancelled)
    ));
}
#[test]
fn world_key_router_full_scan_rejects_unknown_extra_keys_and_exhausted_work() {
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::with_limit(&memory, 65536);
    let mut roots = RootSetV1::empty("routing".into(), [42; 32]).unwrap();
    let field = RUNTIME_TYPED_GENESIS_FIELDS_V1
        .iter()
        .find(|f| f.name == "manifest")
        .unwrap();
    let phase = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let delta = phase.retain(
        StateDeltaV1::build(
            roots.state_root(Ns::Artifacts).clone(),
            vec![StateWriteV1 {
                key: metadata(*field),
                before: None,
                after: Some([1; 32]),
            }],
            &pages,
            4 << 20,
        )
        .unwrap(),
    );
    pages.put_state_pages(&delta, &memory).unwrap();
    roots.state[5] = delta.after.clone();
    let proof = qualify_world_key_routing_v1(
        &roots,
        &pages,
        &memory,
        MemoryClass::Ordinary,
        &mut scan(),
        || None,
        || false,
    )
    .unwrap();
    proof.validate_binding(&roots).unwrap();
    assert_eq!(proof.count(WorldKeyOwnerV1::RuntimeField), 1);
    let mut exhausted = scan();
    exhausted.nodes_remaining = 0;
    assert!(
        qualify_world_key_routing_v1(
            &roots,
            &pages,
            &memory,
            MemoryClass::Ordinary,
            &mut exhausted,
            || None,
            || false
        )
        .is_err()
    );
    assert!(matches!(
        qualify_world_key_routing_v1(
            &roots,
            &pages,
            &memory,
            MemoryClass::Ordinary,
            &mut scan(),
            || Some(ValueTreeError::ResourceLimited("physical reader")),
            || false
        ),
        Err(ValueTreeError::ResourceLimited("physical reader"))
    ));
    let phase = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let delta = phase.retain(
        StateDeltaV1::build(
            roots.state_root(Ns::Artifacts).clone(),
            vec![StateWriteV1 {
                key: b"unregistered-extra".to_vec(),
                before: None,
                after: Some([2; 32]),
            }],
            &pages,
            4 << 20,
        )
        .unwrap(),
    );
    pages.put_state_pages(&delta, &memory).unwrap();
    roots.state[5] = delta.after.clone();
    assert!(proof.validate_binding(&roots).is_err());
    assert!(
        qualify_world_key_routing_v1(
            &roots,
            &pages,
            &memory,
            MemoryClass::Ordinary,
            &mut scan(),
            || None,
            || false
        )
        .is_err()
    );
    assert!(matches!(
        qualify_world_key_routing_v1(
            &roots,
            &pages,
            &memory,
            MemoryClass::Ordinary,
            &mut scan(),
            || None,
            || true
        ),
        Err(ValueTreeError::Cancelled)
    ));
}
