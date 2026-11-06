use super::*;
use crate::incremental_projection::WORLD_STATE_FIELDS;
use crate::incremental_semantic_key::SemanticKeyBudget;
use crate::incremental_typed_record_test_support::{Pages, Writer, io, source_admission};
use crate::incremental_world_native::recover_world_native_record_v1;
use crate::incremental_world_rule_composite_family::test_support as work;
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
struct Seed {
    memory: MemoryBudget,
    pages: Pages,
    roots: ReservedValue<RootSetV1>,
}
fn seed(source: &crate::WorldState) -> Seed {
    let memory = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
    let mut phase = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let root_charge = phase.split(4096).unwrap();
    let mut pages = Pages::with_limit(&memory, 131_072);
    let mut rows = Vec::with_capacity(18);
    let mut records = 1000;
    prepare_world_native_industry_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut records,
        &mut work::write_work(),
        &mut |row| {
            assert!(rows.len() < 18);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(rows.len(), 18);
    assert_eq!(records, 982);
    let mut roots = RootSetV1::empty("native-industry".into(), [7; 32]).unwrap();
    for namespace in [
        StateNamespaceV1::Authorization,
        StateNamespaceV1::Business,
        StateNamespaceV1::Effects,
    ] {
        let profile = match namespace {
            StateNamespaceV1::Authorization => ProductValidationReceiptsFieldV1::profile(),
            StateNamespaceV1::Business => ProductProfilesFieldV1::profile(),
            StateNamespaceV1::Effects => ProductValidationAttemptsFieldV1::profile(),
            _ => unreachable!(),
        }
        .unwrap();
        let mut root = TypedTreeBuilderV1::empty(namespace, &memory).unwrap();
        for row in rows.iter().filter(|row| row.namespace == namespace) {
            let parsed = WorldFieldKeyV1::from_bytes(&row.mutation.key).unwrap();
            let descriptor = *WORLD_STATE_FIELDS
                .iter()
                .find(|field| field.name == parsed.name)
                .unwrap();
            let key = WorldFieldKeyV1::new(descriptor, parsed.kind, parsed.semantic_key).unwrap();
            assert_eq!(key.as_bytes(), row.mutation.key.as_slice());
            assert!(row.mutation.before.is_none());
            append(
                &profile,
                &mut root,
                &key,
                row.mutation.after.unwrap(),
                &mut MemberRecordWriteContext {
                    sink: &mut Writer(&pages),
                    memory: &memory,
                    io: &mut io(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 64 << 20,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 2 << 20,
                        record_bytes: 3 << 20,
                    },
                    cancelled: || false,
                },
                &work::write_work(),
            )
            .unwrap();
        }
        *roots
            .state
            .iter_mut()
            .find(|root| root.namespace == namespace)
            .unwrap() = root.root().clone();
    }
    Seed {
        memory,
        pages,
        roots: root_charge.retain(roots),
    }
}
fn check<F: WorldNativeFieldSpecV1>(seed: &Seed, key: &F::Key, expected: &F::Value)
where
    F::Value: PartialEq + std::fmt::Debug,
{
    let namespace = F::profile().unwrap().namespace().unwrap();
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut context = work::context(
        &seed.pages,
        &seed.roots,
        &seed.memory,
        &mut quota,
        &mut keys,
    );
    context.root = seed.roots.state_root(namespace);
    let mut read = work::read_work();
    read.max_tree_heap = 2 << 20;
    let value =
        recover_world_native_record_v1::<F>(key, &mut context, &mut work::scan(), &mut read)
            .unwrap()
            .unwrap();
    assert_eq!(value.value().value(), expected);
}
#[test]
fn native_industry_genesis_outer_keys_and_all_original_values_cold_roundtrip() {
    let _source = source_admission();
    let mut source = crate::WorldState::default();
    fixture::populate_world(&mut source);
    let seed = seed(&source);
    check::<ProductValidationReceiptsFieldV1>(
        &seed,
        &u64::MAX,
        &source.product_validation_receipts[&u64::MAX],
    );
    check::<ProductProfilesFieldV1>(
        &seed,
        &"outside-product".into(),
        &source.product_profiles["outside-product"],
    );
    check::<LatestProductValidationFieldV1>(&seed, &(), &source.latest_product_validation);
    check::<RecipeProfilesFieldV1>(
        &seed,
        &"outside-recipe".into(),
        &source.recipe_profiles["outside-recipe"],
    );
    check::<FactoryProfilesFieldV1>(
        &seed,
        &"outside-factory".into(),
        &source.factory_profiles["outside-factory"],
    );
    check::<FactoryConstructionReceiptsFieldV1>(
        &seed,
        &"outside-construction".into(),
        &source.factory_construction_receipts["outside-construction"],
    );
    check::<ProductValidationAttemptsFieldV1>(
        &seed,
        &u64::MAX,
        &source.product_validation_attempts[&u64::MAX],
    );
    check::<FactoryRecycleReceiptsFieldV1>(
        &seed,
        &"outside-recycle".into(),
        &source.factory_recycle_receipts["outside-recycle"],
    );
    check::<IndustryProgressFieldV1>(&seed, &(), &source.industry_progress);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut context = work::context(
        &seed.pages,
        &seed.roots,
        &seed.memory,
        &mut quota,
        &mut keys,
    );
    assert!(
        recover_world_native_record_v1::<ProductValidationReceiptsFieldV1>(
            &123,
            &mut context,
            &mut work::scan(),
            &mut work::read_work()
        )
        .unwrap()
        .is_none()
    );
    context.root = seed.roots.state_root(StateNamespaceV1::Business);
    assert!(
        recover_world_native_record_v1::<ProductProfilesFieldV1>(
            &"inner-product-id".into(),
            &mut context,
            &mut work::scan(),
            &mut work::read_work()
        )
        .unwrap()
        .is_none()
    );
}
#[test]
fn native_latest_option_none_and_some_empty_keep_distinct_original_value() {
    let _source = source_admission();
    for value in [
        None,
        Some(crate::LastProductValidationState {
            product_id: String::new(),
            tradable: false,
        }),
    ] {
        let mut source = crate::WorldState::default();
        fixture::populate_world(&mut source);
        source.latest_product_validation = value;
        let seed = seed(&source);
        check::<LatestProductValidationFieldV1>(&seed, &(), &source.latest_product_validation);
    }
}
