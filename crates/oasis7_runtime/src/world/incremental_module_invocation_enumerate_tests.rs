//! Real GEN and current overlay identities compared with independent old routes.
use super::*;
use crate::world::incremental_module_invocation::{
    CurrentInvocationEnumerationWorkV1, collect_current_module_invocation_ids_v1,
};
use oasis7_world_state::incremental_module_instance_fields::{
    ModuleInstanceFieldV1, ModuleInstanceKeyV1, visit_module_instance_fields,
};
fn enumeration_work() -> CurrentInvocationEnumerationWorkV1 {
    let scan = || StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 512 << 20,
        leaves_remaining: 100_000,
    };
    CurrentInvocationEnumerationWorkV1 {
        instance_scan: scan(),
        registry_scan: scan(),
        pending_entries: 10000,
        comparison_bytes_remaining: 2_000_000_000,
        max_keys: 256,
        max_key_bytes: 1024,
        max_text_bytes: 1024,
    }
}
fn ordinary_work() -> CurrentInvocationEnumerationWorkV1 {
    let mut work = enumeration_work();
    work.instance_scan.nodes_remaining = 0;
    work.registry_scan.nodes_remaining = 0;
    work
}
#[test]
fn real_current_invocation_enumeration_matches_legacy_and_rejects_partial_instance() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let before = initialized_world();
    let expected_before: Vec<_> = before
        .collect_active_module_invocations()
        .unwrap()
        .into_iter()
        .map(|v| v.instance_id)
        .collect();
    let fixture = genesis::build(&before, &memory, workspace);
    let mut after = initialized_world();
    // The inactive policy instance suppresses its active registry entry in both oracles.
    after.state.module_instances.remove("main#stable");
    let mut added = before.state.module_instances["main#stable"].clone();
    added.instance_id = "a-new".into();
    after.state.module_instances.insert("a-new".into(), added);
    after.module_registry.active.remove("main");
    let global =
        after.module_registry.records[&crate::ModuleRegistry::record_key("main", "1")].clone();
    after
        .module_registry
        .active
        .insert("global-z".into(), "1".into());
    after
        .module_registry
        .records
        .insert(crate::ModuleRegistry::record_key("global-z", "1"), global);
    let expected_after: Vec<_> = after
        .collect_active_module_invocations()
        .unwrap()
        .into_iter()
        .map(|v| v.instance_id)
        .collect();
    let mut cas = fixture.cas.clone();
    let mut io = genesis::io();
    let mut keys = genesis::keys();
    let mut rows_before = Vec::new();
    let mut rows_after = Vec::new();
    let work =
        || oasis7_world_state::incremental_runtime_artifact_config::ArtifactConfigWriteWorkV1 {
            cells_remaining: 1000000,
            json: oasis7_world_state::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 1000000,
                json_nodes_remaining: 1000000,
                max_json_depth: 64,
                max_pending_node_bytes: 16 << 20,
            },
            max_pending_node_bytes: 16 << 20,
        };
    for (world, rows) in [(&before, &mut rows_before), (&after, &mut rows_after)] {
        world
            .prepare_incremental_artifact_config_genesis_v1(
                &mut genesis::member(&mut cas, &memory, &mut io, &mut keys, || false),
                &mut work(),
                &mut |row| {
                    rows.push(row);
                    Ok(())
                },
            )
            .unwrap();
    }
    let mut instance_writes = Vec::new();
    {
        let mut mutation = BodyMutationContext {
            sink: &mut cas,
            memory: &memory,
            io: &mut io,
            cancelled: || false,
        };
        for (id, instance) in &after.state.module_instances {
            visit_module_instance_fields(instance,|field,value|->Result<(),crate::world::incremental_body::BodyMutationError>{let value=crate::world::incremental_module_instance_genesis::write_module_instance_scalar(field,value,&mut mutation,1024)?;instance_writes.push((ModuleInstanceKeyV1::new(id,field).unwrap().as_bytes().to_vec(),Some(value)));Ok(())}).unwrap();
        }
        for id in before.state.module_instances.keys() {
            if !after.state.module_instances.contains_key(id) {
                for field in ModuleInstanceFieldV1::ALL {
                    instance_writes.push((
                        ModuleInstanceKeyV1::new(id, field)
                            .unwrap()
                            .as_bytes()
                            .to_vec(),
                        None,
                    ));
                }
            }
        }
    }
    drop(before);
    drop(after);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [119; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 16,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut vi = genesis::io();
    let mut vk = genesis::keys();
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut vi,
        },
        key_work: &mut vk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let directory =
        crate::world::incremental_module_invocation::build_accepted_invocation_directory_v1(
            &fixture.runtime,
            &fixture.head,
            fixture.current.raw().instances(),
            fixture.current.runtime().config(),
            &reader,
            &mut ctx,
            &mut enumeration_work(),
            &live,
        )
        .unwrap();
    let guard = ctx.transaction.guard();
    let actual = collect_current_module_invocation_ids_v1(
        &directory,
        fixture.current.raw().instances(),
        fixture.current.runtime().config(),
        &reader,
        &mut ctx,
        &mut ordinary_work(),
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(&*actual, &expected_before);
    drop(actual);
    for (key, value) in instance_writes {
        ctx.transaction
            .write(StateNamespaceV1::Business, &key, value)
            .unwrap();
    }
    for row in &rows_before {
        if !rows_after
            .iter()
            .any(|next| next.mutation.key == row.mutation.key)
        {
            ctx.transaction
                .write(row.namespace, &row.mutation.key, None)
                .unwrap();
        }
    }
    for row in &rows_after {
        ctx.transaction
            .write(row.namespace, &row.mutation.key, row.mutation.after)
            .unwrap();
    }
    let guard = ctx.transaction.guard();
    let actual = collect_current_module_invocation_ids_v1(
        &directory,
        fixture.current.raw().instances(),
        fixture.current.runtime().config(),
        &reader,
        &mut ctx,
        &mut ordinary_work(),
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(&*actual, &expected_after);
    drop(actual);
    drop(ctx);
    // Each independent transaction must reject the corrupt current row and
    // poison sealing, including an inactive row that would hide global fallback.
    for removed in [
        Some(ModuleInstanceFieldV1::Active),
        Some(ModuleInstanceFieldV1::InstanceId),
        Some(ModuleInstanceFieldV1::Version),
        None,
    ] {
        let mut tx = ExecutionTransaction::begin(
            fixture.parent.clone(),
            [120; 32],
            &reader,
            &memory,
            TransactionLimits {
                max_reads: 2048,
                max_writes: 1024,
                max_facts: 16,
                max_pending_node_bytes: 16 << 20,
            },
        )
        .unwrap();
        let live = tx.parent_fence();
        let (namespace, key) = if let Some(field) = removed {
            (
                StateNamespaceV1::Business,
                ModuleInstanceKeyV1::new("policy#inactive", field)
                    .unwrap()
                    .as_bytes()
                    .to_vec(),
            )
        } else {
            use oasis7_world_state::{
                incremental_runtime_artifact_config::ArtifactConfigFieldV1,
                incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
            };
            (
                StateNamespaceV1::Artifacts,
                WorldFieldKeyV1::new(
                    ArtifactConfigFieldV1::RegistryActive.descriptor(),
                    WorldFieldRecordKindV1::Metadata,
                    None,
                )
                .unwrap()
                .as_bytes()
                .to_vec(),
            )
        };
        tx.write(namespace, &key, None).unwrap();
        let mut vi = genesis::io();
        let mut vk = genesis::keys();
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut vk,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        let guard = ctx.transaction.guard();
        assert!(
            collect_current_module_invocation_ids_v1(
                &directory,
                fixture.current.raw().instances(),
                fixture.current.runtime().config(),
                &reader,
                &mut ctx,
                &mut ordinary_work(),
                &guard,
                &live
            )
            .is_err(),
            "removed {removed:?}"
        );
        drop(ctx);
        let guard = tx.guard();
        assert!(tx.seal(&guard, &live, 1).is_err(), "removed {removed:?}");
    }
}
