//! Real qualified GEN plus typed Current membership changes; no World recovery.
use super::*;
use oasis7_world_state::{
    incremental_authorization_mapsets::AuthorizationMembershipV1,
    incremental_field_member::{MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_world_collection_controls::{
        DataAccessMembershipChangeV1, WorldCollectionControlsWorkV1,
        change_current_data_access_membership_v1, read_current_data_access_group_size_v1,
    },
};
#[test]
fn current_data_access_group_size_tracks_typed_membership_deltas() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = transfer_resource_world();
    world
        .state
        .data_access_permissions
        .insert("empty".into(), Default::default());
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [141; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: oasis7_formal_protocol::incremental_execution::EXECUTION_MAX_READS,
            max_writes: 1024,
            max_facts: 64,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut io = genesis::io();
    let mut keys = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut work = WorldCollectionControlsWorkV1 {
        records_remaining: 100_000,
        index_steps_remaining: 100_000,
        max_record_bytes: 16 << 20,
    };
    let mut pending_entries = 100_000;
    let size = |work: &mut WorldCollectionControlsWorkV1,
                pending_entries: &mut usize,
                tx: &mut ExecutionTransaction<'_, Reader<'_>>,
                read: &mut _,
                subject| {
        let guard = tx.guard();
        read_current_data_access_group_size_v1(
            fixture.current.native().controls(),
            &fixture.roots,
            subject,
            tx,
            read,
            work,
            pending_entries,
            &guard,
            &live,
        )
        .unwrap()
    };
    assert_eq!(
        size(
            &mut work,
            &mut pending_entries,
            &mut tx,
            &mut read,
            "sender"
        ),
        Some(1)
    );
    assert_eq!(
        size(&mut work, &mut pending_entries, &mut tx, &mut read, "empty"),
        Some(0)
    );
    assert_eq!(
        size(
            &mut work,
            &mut pending_entries,
            &mut tx,
            &mut read,
            "receiver"
        ),
        None
    );
    let mut sink = fixture.cas.clone();
    let mut output_io = genesis::io();
    let mut output_keys = genesis::keys();
    let mut writer = MemberRecordWriteContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut output_io,
        key_work: &mut output_keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 16,
            record_bytes: 16 << 20,
        },
        cancelled: || false,
    };
    for (subject, member, expected, after, count) in [
        (
            "sender",
            "far",
            AuthorizationMembershipV1::MemberAbsent,
            true,
            Some(2),
        ),
        (
            "sender",
            "far",
            AuthorizationMembershipV1::Present,
            true,
            Some(2),
        ),
        (
            "sender",
            "receiver",
            AuthorizationMembershipV1::Present,
            false,
            Some(1),
        ),
        (
            "sender",
            "far",
            AuthorizationMembershipV1::Present,
            false,
            None,
        ),
        (
            "receiver",
            "sender",
            AuthorizationMembershipV1::GroupAbsent,
            true,
            Some(1),
        ),
        (
            "receiver",
            "sender",
            AuthorizationMembershipV1::Present,
            false,
            None,
        ),
        (
            "empty",
            "unused",
            AuthorizationMembershipV1::MemberAbsent,
            false,
            None,
        ),
        (
            "sender",
            "receiver",
            AuthorizationMembershipV1::GroupAbsent,
            true,
            Some(1),
        ),
    ] {
        let guard = tx.guard();
        change_current_data_access_membership_v1(
            DataAccessMembershipChangeV1 {
                subject,
                member,
                expected,
                after,
            },
            fixture.current.native().controls(),
            &fixture.roots,
            &mut tx,
            &mut read,
            &mut writer,
            &mut work,
            &mut pending_entries,
            &guard,
            &live,
        )
        .unwrap();
        assert_eq!(
            size(&mut work, &mut pending_entries, &mut tx, &mut read, subject),
            count
        );
    }
    assert_eq!(
        size(&mut work, &mut pending_entries, &mut tx, &mut read, "empty"),
        None
    );
    assert_eq!(
        size(
            &mut work,
            &mut pending_entries,
            &mut tx,
            &mut read,
            "sender"
        ),
        Some(1)
    );
    assert_eq!(tx.ordered_fact_count(), 0);
    let guard = tx.guard();
    tx.seal(&guard, &live, 1).unwrap();
}
