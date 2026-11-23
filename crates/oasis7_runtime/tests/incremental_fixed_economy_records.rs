#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_field_member::*, incremental_field_read::*,
    incremental_fixed_economy_records::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_250_000,
        ..fixture::physical()
    }
}
fn state_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(v) => ValueTreeError::Memory(v),
        PageStoreError::ResourceLimited(v) => ValueTreeError::ResourceLimited(v),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(v) => ValueTreeError::Io(v),
        _ => ValueTreeError::Invalid("fixed economy test state"),
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        leaves_remaining: 1000,
        read_bytes_remaining: 64 << 20,
    }
}
fn source() -> WorldState {
    let mut value = WorldState::default();
    value.main_token_supply.total_supply = u64::MAX;
    value.main_token_supply.circulating_supply = 0;
    value.main_token_supply.total_issued = 1;
    value.main_token_supply.total_burned = u64::MAX;
    value.reward_asset_config.points_per_credit = 0;
    value.reward_asset_config.credits_per_power_unit = u64::MAX;
    value.reward_asset_config.max_redeem_power_per_epoch = i64::MIN;
    value.reward_asset_config.min_redeem_power_unit = i64::MAX;
    value.protocol_power_reserve.epoch_index = u64::MAX;
    value.protocol_power_reserve.available_power_units = i64::MIN;
    value.protocol_power_reserve.redeemed_power_units = i64::MAX;
    value.reward_signature_governance_policy.require_mintsig_v2 = true;
    value
        .reward_signature_governance_policy
        .allow_mintsig_v1_fallback = false;
    value
        .reward_signature_governance_policy
        .require_redeem_signature = true;
    value
        .reward_signature_governance_policy
        .require_redeem_signer_match_node_id = false;
    value
}
fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 1024,
        payload_bytes: 165,
        record_bytes: 4096,
    }
}
fn seed(store: &IncrementalPageStore, memory: &MemoryBudget) -> RootSetV1 {
    let _caller = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut writer = store.value_session(physical());
    let mut rows = Vec::with_capacity(8);
    let mut work = FixedEconomyWorkV1 {
        cells_remaining: 31,
        max_pending_node_bytes: 2 << 20,
    };
    prepare_fixed_economy_genesis_v1(
        &source(),
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: limits(),
            cancelled: || false,
        },
        &mut work,
        &mut |row| {
            assert!(rows.len() < 8);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(work.cells_remaining, 0);
    assert_eq!(rows.len(), 8);
    let mut roots = RootSetV1::empty("fixed-economy".into(), [0x63; 32]).unwrap();
    for namespace in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut writes = rows
            .iter()
            .filter(|row| row.namespace == namespace)
            .map(|row| StateWriteV1 {
                key: row.mutation.key.clone(),
                before: None,
                after: row.mutation.after,
            })
            .collect::<Vec<_>>();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
        let delta = charge.retain(
            StateDeltaV1::build(
                StateRootV1::empty(namespace).unwrap(),
                writes,
                &state,
                2 << 20,
            )
            .unwrap(),
        );
        writer.put_state_pages(&delta, memory).unwrap();
        *roots
            .state
            .iter_mut()
            .find(|root| root.namespace == namespace)
            .unwrap() = delta.after.clone();
    }
    roots
}
macro_rules! read_ctx {
    ($root:expr,$state:expr,$values:expr,$memory:expr,$class:expr,$io:expr,$keys:expr) => {
        WorldFieldReadContextV1 {
            root: $root,
            state: $state,
            values: ValueReadContext {
                source: $values,
                memory: $memory,
                class: $class,
                io: $io,
            },
            key_work: $keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || $state.take_read_error().map(state_error),
            cancelled: || false,
        }
    };
}
// Independent literal field names/codecs/values, not supplied by the producer.
fn expected() -> [(
    FixedEconomyFieldV1,
    FixedEconomyColumnV1,
    &'static str,
    FixedEconomyValueV1,
); 15] {
    use FixedEconomyColumnV1 as C;
    use FixedEconomyFieldV1 as F;
    use FixedEconomyValueV1 as V;
    [
        (
            F::MainTokenSupply,
            C::TotalSupply,
            "total_supply",
            V::U64(u64::MAX),
        ),
        (
            F::MainTokenSupply,
            C::CirculatingSupply,
            "circulating_supply",
            V::U64(0),
        ),
        (
            F::MainTokenSupply,
            C::TotalIssued,
            "total_issued",
            V::U64(1),
        ),
        (
            F::MainTokenSupply,
            C::TotalBurned,
            "total_burned",
            V::U64(u64::MAX),
        ),
        (
            F::RewardAssetConfig,
            C::PointsPerCredit,
            "points_per_credit",
            V::U64(0),
        ),
        (
            F::RewardAssetConfig,
            C::CreditsPerPowerUnit,
            "credits_per_power_unit",
            V::U64(u64::MAX),
        ),
        (
            F::RewardAssetConfig,
            C::MaxRedeemPowerPerEpoch,
            "max_redeem_power_per_epoch",
            V::I64(i64::MIN),
        ),
        (
            F::RewardAssetConfig,
            C::MinRedeemPowerUnit,
            "min_redeem_power_unit",
            V::I64(i64::MAX),
        ),
        (
            F::ProtocolPowerReserve,
            C::EpochIndex,
            "epoch_index",
            V::U64(u64::MAX),
        ),
        (
            F::ProtocolPowerReserve,
            C::AvailablePowerUnits,
            "available_power_units",
            V::I64(i64::MIN),
        ),
        (
            F::ProtocolPowerReserve,
            C::RedeemedPowerUnits,
            "redeemed_power_units",
            V::I64(i64::MAX),
        ),
        (
            F::RewardSignatureGovernancePolicy,
            C::RequireMintsigV2,
            "require_mintsig_v2",
            V::Bool(true),
        ),
        (
            F::RewardSignatureGovernancePolicy,
            C::AllowMintsigV1Fallback,
            "allow_mintsig_v1_fallback",
            V::Bool(false),
        ),
        (
            F::RewardSignatureGovernancePolicy,
            C::RequireRedeemSignature,
            "require_redeem_signature",
            V::Bool(true),
        ),
        (
            F::RewardSignatureGovernancePolicy,
            C::RequireRedeemSignerMatchNodeId,
            "require_redeem_signer_match_node_id",
            V::Bool(false),
        ),
    ]
}
fn descriptor(field: FixedEconomyFieldV1) -> FieldDescriptor {
    use FixedEconomyFieldV1 as F;
    let (name, namespace) = match field {
        F::MainTokenSupply => ("main_token_supply", StateNamespaceV1::Business),
        F::RewardAssetConfig => ("reward_asset_config", StateNamespaceV1::Business),
        F::ProtocolPowerReserve => ("protocol_power_reserve", StateNamespaceV1::Business),
        F::RewardSignatureGovernancePolicy => (
            "reward_signature_governance_policy",
            StateNamespaceV1::Authorization,
        ),
    };
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name,
        shape: FieldShape::Value,
        destination: FieldDestination::State(namespace),
    }
}
fn literal_key(field: FixedEconomyFieldV1, name: Option<&str>) -> (WorldFieldKeyV1, Vec<u8>) {
    let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
    match name {
        None => semantic.push(0),
        Some(name) => {
            semantic.extend_from_slice(&[5, 0, 1]);
            semantic.extend_from_slice(&(name.len() as u64).to_be_bytes());
            semantic.extend_from_slice(name.as_bytes());
        }
    }
    (
        WorldFieldKeyV1::new(
            descriptor(field),
            WorldFieldRecordKindV1::Entry,
            Some(*blake3::hash(&semantic).as_bytes()),
        )
        .unwrap(),
        semantic,
    )
}
#[test]
fn fixed_economy_all_original_primitives_have_root_only_cold_literal_oracles() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let canonical = {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory).canonical_bytes().unwrap()
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let _caller = memory.reserve(128 << 10, MemoryClass::Recovery).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let roots = RootSetV1::from_canonical_bytes(&canonical).unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            &roots.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Recovery,
            &mut io,
            &mut keys
        );
        let family =
            qualify_fixed_economy_family_v1(&roots, &mut ctx, &mut scan(), &mut scan(), &mut 1000)
                .unwrap();
        family.validate_binding(&roots.digest().unwrap()).unwrap();
        assert!(family.validate_binding(&[0; 32]).is_err());
        for field in FixedEconomyFieldV1::ALL {
            ctx.root = roots
                .state
                .iter()
                .find(|root| root.namespace == field.namespace())
                .unwrap();
            let (outer_key, _) = literal_key(field, None);
            assert!(
                ctx.root
                    .get(outer_key.as_bytes(), &state)
                    .unwrap()
                    .is_some()
            );
            let proof =
                open_qualified_fixed_economy_record_v1(field, &roots, &family, &mut ctx).unwrap();
            assert_eq!(
                proof.root().leaf_count,
                if field == FixedEconomyFieldV1::ProtocolPowerReserve {
                    5
                } else {
                    6
                }
            );
            for (_, column, name, want) in expected()
                .into_iter()
                .filter(|(owner, _, _, _)| *owner == field)
            {
                assert_eq!(
                    read_qualified_fixed_economy_cell_v1(
                        FixedEconomyCellRequestV1 {
                            column,
                            qualification: &proof
                        },
                        &mut ctx
                    )
                    .unwrap(),
                    want
                );
                let (key, semantic) = literal_key(field, Some(name));
                let _scratch = memory.reserve(512 << 10, MemoryClass::Recovery).unwrap();
                let root = proof.root().get(key.as_bytes(), &state).unwrap().unwrap();
                let record = ctx.values.load_root(&root).unwrap();
                let mut bytes = Vec::with_capacity(4096);
                ctx.values
                    .read_range(&record, 0, record.0.logical_len, &mut bytes, || false)
                    .unwrap();
                let offset = FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len() + semantic.len();
                assert_eq!(
                    &bytes[FIELD_MEMBER_HEADER_BYTES
                        ..FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()],
                    key.as_bytes()
                );
                assert_eq!(
                    &bytes[FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()..offset],
                    semantic
                );
                let (codec, data) = match want {
                    FixedEconomyValueV1::U64(value) => (2, value.to_be_bytes().to_vec()),
                    FixedEconomyValueV1::I64(value) => (3, value.to_be_bytes().to_vec()),
                    FixedEconomyValueV1::Bool(value) => (4, vec![u8::from(value)]),
                };
                assert_eq!(bytes[FIELD_MEMBER_DOMAIN.len() + 2], codec);
                assert_eq!(&bytes[offset..], data);
            }
        }
        eprintln!(
            "fixed economy complete{compression:?}: state={:?} values={:?}",
            state.remaining_io(),
            values.remaining_io()
        );
    }
}

#[path = "support/incremental_fixed_economy_operations.rs"]
mod operations;

#[path = "support/incremental_fixed_economy_faults.rs"]
mod faults;

#[path = "support/incremental_fixed_economy_pages.rs"]
mod pages;

#[path = "support/incremental_fixed_economy_schema.rs"]
mod schema;

#[path = "support/incremental_fixed_economy_genesis.rs"]
mod genesis;
