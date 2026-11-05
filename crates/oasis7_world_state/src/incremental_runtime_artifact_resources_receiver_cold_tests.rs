use super::*;
#[path = "incremental_runtime_artifact_resources_receiver_test_support.rs"]
mod support;
use oasis7_agent_api::world_resource::{ChainResourceDelta, ChainResourceManifest};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use support::*;
#[test]
fn runtime_artifact_resources_cold_accepts_original_none_option_fields() {
    let memory = MemoryBudget::new(192 << 20, 16 << 20).unwrap();
    let _source = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let manifest = ChainResourceManifest::empty_at_height("world", 7, 0);
    let builtin = Builtins(Default::default());
    let (pages, roots) = seed(&memory, &manifest, None, &builtin);
    let family =
        qualify(&pages, &roots, &memory).expect("original None fields emitted by GEN must qualify");
    assert_eq!(family.record_counts(), [1, 0, 1]);
    let delta = ChainResourceDelta::empty_for_manifest("world", &manifest.manifest_hash, 0, 0);
    let (pages, roots) = seed(&memory, &manifest, Some(&delta), &builtin);
    let family = qualify(&pages, &roots, &memory).expect("delta None option fields must qualify");
    assert_eq!(family.record_counts(), [1, 1, 1]);
}

#[test]
fn runtime_artifact_resources_cold_rejects_wrong_delta_variant_tag() {
    use crate::incremental_typed_record_test_support::Writer;
    use crate::{incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1};
    use oasis7_agent_api::{
        world_action::MaterialLedgerId, world_resource::ChainResourceDeltaEntry,
    };
    use oasis7_formal_protocol::{incremental_state::*, incremental_value::*};
    let memory = MemoryBudget::new(192 << 20, 16 << 20).unwrap();
    let _source = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
    let mut manifest = ChainResourceManifest::empty_at_height("world", 7, 0);
    manifest.genesis_ref = Some("original genesis".into());
    manifest.created_at_block_hash = Some("original block".into());
    let mut delta = ChainResourceDelta::empty_for_manifest("world", &manifest.manifest_hash, 0, 0);
    delta.action_id = Some(1);
    delta.event_id = Some(2);
    delta.commit_block_hash = Some("block".into());
    delta.entries.push(ChainResourceDeltaEntry::MaterialLedger {
        ledger_id: MaterialLedgerId::World,
        material_kind: "iron".into(),
        delta: 1,
        resulting_balance: 1,
    });
    let builtin = Builtins(Default::default());
    let (pages, mut roots) = seed(&memory, &manifest, Some(&delta), &builtin);
    qualify(&pages, &roots, &memory)
        .expect("original all-Some baseline must qualify before forging a tag");
    let mut vi = io();
    let mut kw = keys();
    let mut ctx = WorldFieldReadContextV1 {
        root: &roots.state[5],
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut vi,
        },
        key_work: &mut kw,
        semantic_bytes: 2 << 20,
        record_bytes: 4 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let outer = inspect_runtime_artifact_record_root_v1(
        RuntimeArtifactFieldV1::LatestChainResourceDelta,
        &mut ctx,
    )
    .unwrap()
    .unwrap();
    let outer_key = ctx
        .key(
            RuntimeArtifactFieldV1::LatestChainResourceDelta.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(),
        )
        .unwrap()
        .0;
    let old_outer = ctx.lookup(&outer_key).unwrap().unwrap();
    let mut ctx = WorldFieldReadContextV1 {
        root: outer.root(),
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut vi,
        },
        key_work: &mut kw,
        semantic_bytes: 2 << 20,
        record_bytes: 4 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let material = read_runtime_artifact_record_reference_v1(
        RuntimeArtifactRecordV1::ChainResourceDelta,
        RuntimeArtifactRecordV1::DeltaMaterialLedger,
        &outer,
        &("entries", 0u64),
        &mut ctx,
    )
    .unwrap()
    .unwrap();
    let wrong = rewrite(
        &memory,
        &pages,
        material.root(),
        RuntimeArtifactRecordV1::DeltaMaterialLedger,
        &"variant",
        MemberPayloadRefV1::U64(0),
    );
    let reference = root_ref(
        RuntimeArtifactRecordV1::DeltaMaterialLedger.schema(),
        &("entries", 0u64),
        wrong,
        &memory,
    );
    let delta_root = rewrite(
        &memory,
        &pages,
        outer.root(),
        RuntimeArtifactRecordV1::ChainResourceDelta,
        &("entries", 0u64),
        MemberPayloadRefV1::RecordRoot(&reference),
    );
    let reference = root_ref(
        RuntimeArtifactFieldV1::LatestChainResourceDelta.schema(),
        &(),
        delta_root,
        &memory,
    );
    let mut out = Writer(&pages);
    let mut wi = io();
    let mut wk = keys();
    let row = write_member_record_v1(
        RuntimeArtifactFieldV1::LatestChainResourceDelta.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::RecordRoot(&reference),
        &mut MemberRecordWriteContext {
            sink: &mut out,
            memory: &memory,
            io: &mut wi,
            key_work: &mut wk,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 2 << 20,
                payload_bytes: 2 << 20,
                record_bytes: 4 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let change = StateDeltaV1::build(
        roots.state[5].clone(),
        vec![StateWriteV1 {
            key: outer_key.as_bytes().to_vec(),
            before: Some(old_outer),
            after: Some(row.value_root),
        }],
        &pages,
        8 << 20,
    )
    .unwrap();
    for n in &change.new_nodes {
        let id = *blake3::Hash::from_hex(&n.digest).unwrap().as_bytes();
        out.put_node(
            id,
            memory
                .reserve(n.bytes.len(), MemoryClass::Ordinary)
                .unwrap()
                .retain(n.bytes.clone()),
        )
        .unwrap();
    }
    roots.state[5] = change.after;
    assert!(
        matches!(
            qualify(&pages, &roots, &memory),
            Err(ValueTreeError::Invalid(_))
        ),
        "MaterialLedger record schema must reject the RuntimeResource variant tag 0"
    );
}

#[path = "incremental_runtime_artifact_resources_receiver_general_tests.rs"]
mod general_tests;

#[path = "incremental_runtime_artifact_resources_current_manifest_tests.rs"]
mod current_manifest_tests;
