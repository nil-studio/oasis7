//! Canonical DATA equivalence/persistent-copy tests; no accepted owner is minted.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use serde_json::json;
fn limits() -> AuthorizationRootCacheLimitsV1 {
    AuthorizationRootCacheLimitsV1 {
        max_wire_bytes: 8 << 20,
        max_depth: 64,
        max_map_keys: 4096,
        max_key_bytes: 4096,
    }
}
#[test]
fn original_seven_field_bytes_and_sha_survive_persistent_changed_paths() {
    let memory = oasis7_proto::memory_budget::MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let work = FormalBoundedWorkV1::new(20_000_000, 128 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let mut world = crate::world::World::new_with_state(crate::WorldState::default());
    world.capability_grants_v2.insert(
        "outer-key-not-json-id".into(),
        json!({"id":"different","fraction":0.5,"array":[0,-1,u64::MAX]}),
    );
    world
        .capability_revocation_state
        .world_service_results
        .insert("excluded".into(), json!({"large":"correlation data"}));
    let cache = world
        .prepare_iws_authorization_projection_data_v1(limits(), &mut common, MemoryClass::Ordinary)
        .unwrap();
    let expected = world.compute_capability_authorization_root().unwrap();
    assert_eq!(cache.hash(&mut common).unwrap().as_str(), expected);
    let authority =
        crate::world::incremental_authorization_root_projection::AuthorityRevocationViewV1(
            &world.capability_revocation_state,
        );
    let original =
        crate::world::incremental_authorization_root_projection::AuthorizationRootProjectionV1 {
            grants: &world.capability_grants_v2,
            revocation: &authority,
            invocation_contexts: &world.capability_invocation_contexts,
            budget_accounts: &world.capability_budget_accounts,
            nonce_records: &world.capability_nonce_records,
            receipts: &world.capability_authorization_receipts,
            effect_receipt_links: &world.capability_effect_receipt_links,
        };
    let expected_bytes = oasis7_wasm_abi::encode_canonical_cbor(&original).unwrap();
    let _output = memory
        .reserve(
            2 * (expected_bytes.len() + std::mem::size_of::<Vec<u8>>()),
            MemoryClass::Ordinary,
        )
        .unwrap();
    let mut actual = Vec::with_capacity(expected_bytes.len());
    cache.write_hint(&mut actual, &mut common).unwrap();
    assert_eq!(actual, expected_bytes);
    let raw = memory
        .reserve(
            actual.capacity() + std::mem::size_of::<Vec<u8>>(),
            MemoryClass::Ordinary,
        )
        .unwrap()
        .retain(actual);
    let restored = ProvisionalAuthorizationProjectionV1::read_hint_data(
        &raw,
        limits(),
        &mut common,
        MemoryClass::Ordinary,
    )
    .unwrap();
    assert_eq!(restored.hash(&mut common).unwrap().as_str(), expected);
    let next_value = json!({"id":"still-different","fraction":1.0,"array":[true,null,"变化"]});
    let next = restored
        .replace_serialized(
            &[
                AuthorizationProjectionPathV1::Field("grants"),
                AuthorizationProjectionPathV1::Field("outer-key-not-json-id"),
            ],
            &next_value,
            &mut common,
        )
        .unwrap();
    world
        .capability_grants_v2
        .insert("outer-key-not-json-id".into(), next_value);
    assert_eq!(
        next.hash(&mut common).unwrap().as_str(),
        world.compute_capability_authorization_root().unwrap()
    );
    assert_eq!(
        restored.hash(&mut common).unwrap().as_str(),
        expected,
        "persistent update cannot mutate its qualified baseline data"
    );
    let removed = next
        .remove(
            &[
                AuthorizationProjectionPathV1::Field("grants"),
                AuthorizationProjectionPathV1::Field("outer-key-not-json-id"),
            ],
            &mut common,
        )
        .unwrap();
    world.capability_grants_v2.remove("outer-key-not-json-id");
    assert_eq!(
        removed.hash(&mut common).unwrap().as_str(),
        world.compute_capability_authorization_root().unwrap()
    );
}
#[test]
fn canonical_cache_cancellation_and_foreign_pool_never_produce_data() {
    let memory = oasis7_proto::memory_budget::MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let work = FormalBoundedWorkV1::new(10000, 1 << 20).unwrap();
    let mut stop = || true;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let world = crate::world::World::new_with_state(crate::WorldState::default());
    assert!(
        world
            .prepare_iws_authorization_projection_data_v1(
                limits(),
                &mut common,
                MemoryClass::Ordinary
            )
            .is_err()
    );
    let other = oasis7_proto::memory_budget::MemoryBudget::new(8 << 20, 1 << 20).unwrap();
    let raw = other
        .reserve(1, MemoryClass::Ordinary)
        .unwrap()
        .retain(vec![0xf6]);
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    assert!(
        ProvisionalAuthorizationProjectionV1::read_hint_data(
            &raw,
            limits(),
            &mut common,
            MemoryClass::Ordinary
        )
        .is_err()
    );
}
