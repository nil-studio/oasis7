//! Same-Txn and independent generic+typed tests, not DistFS evidence.
use super::body_tests::*;
use super::*;
use crate::{
    incremental_field_member::*, incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1, incremental_record_tree as tree,
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
        TransitionVerifier,
    },
    incremental_state::{StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[test]
fn ordinary_same_transaction_creates_changes_reads_and_independently_receives() {
    let memory = MemoryBudget::new(192 << 20, 0).unwrap();
    let _index = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let _bookkeeping = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages {
        nodes: Rc::new(RefCell::new(BTreeMap::new())),
    };
    let (roots, _) = seed(&memory, &mut pages);
    let (family, initial) = family_in_pool(&roots, &pages, &memory, MemoryClass::Ordinary).unwrap();
    let json_address = ArtifactConfigAddressV1 {
        section: "subscriptions",
        ordinal: Some(0),
        inner_ordinal: None,
        member: None,
        column: "filters",
    };
    // Target bootstrap is a separate public setup operation, before the single
    // continuous ordinary Txn budgets below. No session is reset inside that Txn.
    let parent_json = json_in_pool(
        &initial,
        json_address,
        &pages,
        &memory,
        MemoryClass::Ordinary,
    );
    let parent = ExecutionParent {
        commit: [7; 32],
        roots,
        authority: AuthorityBinding {
            token: [9; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let source = pages.clone();
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut state_cancel = || false;
    let state_cancel = RefCell::new(&mut state_cancel);
    let state_error = RefCell::new(None);
    let state = tree::Source {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &state_io,
        cancelled: &state_cancel,
        error: &state_error,
        labels: genesis::LABELS,
    };
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [4; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 4096,
            max_writes: 64,
            max_facts: 16,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let mut value_io = io();
    let mut write_io = io();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        },
        key_work: &mut read_keys,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        take_state_error: || state_error.borrow_mut().take(),
        cancelled: || false,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut pages,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut write_keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let address = ArtifactConfigAddressV1 {
        section: "record",
        ordinal: None,
        inner_ordinal: None,
        member: None,
        column: "registered_at",
    };
    let guard = overlay.transaction.guard();
    let first = change_qualified_artifact_config_cell_v1(
        ArtifactConfigCellChangeV1 {
            request: ArtifactConfigCellRequestV1 {
                field: ArtifactConfigFieldV1::RegistryRegisteredAt,
                key: ArtifactConfigRecordKeyV1::Member("raw external key"),
                address,
                family: &family,
                qualification: &initial,
                max_payload_bytes: 1 << 20,
            },
            expected: ArtifactConfigScalarRefV1::Unsigned(u64::MAX),
            after: ArtifactConfigScalarRefV1::Unsigned(0),
            max_pending_node_bytes: 1 << 20,
        },
        &mut overlay,
        &mut write,
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    let guard = overlay.transaction.guard();
    let observed = read_qualified_artifact_config_overlay_v1(
        ArtifactConfigCellRequestV1 {
            field: ArtifactConfigFieldV1::RegistryManifest,
            key: ArtifactConfigRecordKeyV1::Member("raw external key"),
            address,
            family: &family,
            qualification: &first,
            max_payload_bytes: 1 << 20,
        },
        &mut overlay,
        &guard,
        &live,
        &mut std::io::sink(),
    )
    .unwrap();
    assert_eq!(*observed, ArtifactConfigValueV1::Unsigned(0));
    let new_record = ModuleRecord {
        manifest: tests::module_fixture(),
        registered_at: 77,
        registered_by: "new 原值".to_owned(),
        audit_event_id: None,
    };
    let guard = overlay.transaction.guard();
    let created = change_qualified_artifact_config_record_v1(
        ArtifactConfigRecordChangeV1 {
            field: ArtifactConfigFieldV1::RegistryManifest,
            key: ArtifactConfigRecordKeyV1::Member("different raw key"),
            family: &family,
            expected: None,
            after: Some(ArtifactConfigBodyRefV1::Module(&new_record)),
        },
        &mut overlay,
        &mut write,
        &mut write_work(),
        &guard,
        &live,
    )
    .unwrap();
    assert!(created.changed);
    let created = created.qualification.unwrap();
    let guard = overlay.transaction.guard();
    let changed = change_qualified_artifact_config_cell_v1(
        ArtifactConfigCellChangeV1 {
            request: ArtifactConfigCellRequestV1 {
                field: ArtifactConfigFieldV1::RegistryRegisteredBy,
                key: ArtifactConfigRecordKeyV1::Member("different raw key"),
                address,
                family: &family,
                qualification: &created,
                max_payload_bytes: 1 << 20,
            },
            expected: ArtifactConfigScalarRefV1::Unsigned(77),
            after: ArtifactConfigScalarRefV1::Unsigned(78),
            max_pending_node_bytes: 1 << 20,
        },
        &mut overlay,
        &mut write,
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    let guard = overlay.transaction.guard();
    assert!(
        change_qualified_artifact_config_cell_v1(
            ArtifactConfigCellChangeV1 {
                request: ArtifactConfigCellRequestV1 {
                    field: ArtifactConfigFieldV1::RegistryAuditEventId,
                    key: ArtifactConfigRecordKeyV1::Member("different raw key"),
                    address,
                    family: &family,
                    qualification: &changed,
                    max_payload_bytes: 1 << 20
                },
                expected: ArtifactConfigScalarRefV1::Unsigned(78),
                after: ArtifactConfigScalarRefV1::Unsigned(78),
                max_pending_node_bytes: 1 << 20
            },
            &mut overlay,
            &mut write,
            &guard,
            &live
        )
        .unwrap()
        .is_none()
    );
    let guard = overlay.transaction.guard();
    let active = change_qualified_artifact_config_record_v1(
        ArtifactConfigRecordChangeV1 {
            field: ArtifactConfigFieldV1::RegistryActive,
            key: ArtifactConfigRecordKeyV1::Member("independent active"),
            family: &family,
            expected: None,
            after: Some(ArtifactConfigBodyRefV1::Active("unregistered raw version")),
        },
        &mut overlay,
        &mut write,
        &mut write_work(),
        &guard,
        &live,
    )
    .unwrap()
    .qualification
    .unwrap();
    let guard = overlay.transaction.guard();
    let deleted = change_qualified_artifact_config_record_v1(
        ArtifactConfigRecordChangeV1 {
            field: ArtifactConfigFieldV1::RegistryActive,
            key: ArtifactConfigRecordKeyV1::Member("independent active"),
            family: &family,
            expected: Some(&active),
            after: None,
        },
        &mut overlay,
        &mut write,
        &mut write_work(),
        &guard,
        &live,
    )
    .unwrap();
    assert!(deleted.changed && deleted.qualification.is_none());
    use crate::incremental_json_records::{
        JsonChangeWorkV1, JsonPathSegmentV1, JsonReadWorkV1, JsonSubtreeEditV1,
    };
    let json_value = serde_json::json!({"items":[true,true],"metadata":"keep"});
    let mut json_write = write_work().json;
    let mut json_delta = JsonChangeWorkV1 {
        json: JsonReadWorkV1 {
            cells_remaining: 10_000,
            json_nodes_remaining: 1000,
            max_depth: 16,
            max_payload_bytes: 1 << 20,
        },
        paths_remaining: 100_000,
        max_pending_pairs: 64,
    };
    let guard = overlay.transaction.guard();
    let json1 = change_qualified_artifact_config_json_v1(
        ArtifactConfigJsonChangeV1 {
            request: ArtifactConfigCellRequestV1 {
                field: ArtifactConfigFieldV1::RegistryManifest,
                key: ArtifactConfigRecordKeyV1::Member("raw external key"),
                address: json_address,
                family: &family,
                qualification: &first,
                max_payload_bytes: 165,
            },
            json: &parent_json,
            path: &[],
            edit: JsonSubtreeEditV1::Replace {
                expected: parent_json.root(),
                after: &json_value,
            },
        },
        &mut overlay,
        &mut write,
        &mut json_write,
        &mut json_delta,
        &guard,
        &live,
    )
    .unwrap();
    assert!(json1.changed);
    let appended = [
        serde_json::Value::Bool(true),
        serde_json::Value::Null,
        serde_json::json!({}),
    ];
    let guard = overlay.transaction.guard();
    let json2 = change_qualified_artifact_config_json_v1(
        ArtifactConfigJsonChangeV1 {
            request: ArtifactConfigCellRequestV1 {
                field: ArtifactConfigFieldV1::RegistryAuditEventId,
                key: ArtifactConfigRecordKeyV1::Member("raw external key"),
                address: json_address,
                family: &family,
                qualification: json1.body.as_ref().unwrap(),
                max_payload_bytes: 165,
            },
            json: json1.json.as_ref().unwrap(),
            path: &[JsonPathSegmentV1::Name("items")],
            edit: JsonSubtreeEditV1::Append {
                expected_len: 2,
                values: &appended,
            },
        },
        &mut overlay,
        &mut write,
        &mut json_write,
        &mut json_delta,
        &guard,
        &live,
    )
    .unwrap();
    let guard = overlay.transaction.guard();
    let json3 = change_qualified_artifact_config_json_v1(
        ArtifactConfigJsonChangeV1 {
            request: ArtifactConfigCellRequestV1 {
                field: ArtifactConfigFieldV1::RegistryRegisteredBy,
                key: ArtifactConfigRecordKeyV1::Member("raw external key"),
                address: json_address,
                family: &family,
                qualification: json2.body.as_ref().unwrap(),
                max_payload_bytes: 165,
            },
            json: json2.json.as_ref().unwrap(),
            path: &[JsonPathSegmentV1::Name("items")],
            edit: JsonSubtreeEditV1::Truncate {
                expected_len: 5,
                len: 3,
            },
        },
        &mut overlay,
        &mut write,
        &mut json_write,
        &mut json_delta,
        &guard,
        &live,
    )
    .unwrap();
    let guard = overlay.transaction.guard();
    let noop = change_qualified_artifact_config_json_v1(
        ArtifactConfigJsonChangeV1 {
            request: ArtifactConfigCellRequestV1 {
                field: ArtifactConfigFieldV1::RegistryManifest,
                key: ArtifactConfigRecordKeyV1::Member("raw external key"),
                address: json_address,
                family: &family,
                qualification: json3.body.as_ref().unwrap(),
                max_payload_bytes: 165,
            },
            json: json3.json.as_ref().unwrap(),
            path: &[JsonPathSegmentV1::Name("items")],
            edit: JsonSubtreeEditV1::Truncate {
                expected_len: 3,
                len: 3,
            },
        },
        &mut overlay,
        &mut write,
        &mut json_write,
        &mut json_delta,
        &guard,
        &live,
    )
    .unwrap();
    assert!(!noop.changed && noop.body.is_none() && noop.json.is_none());
    let guard = overlay.transaction.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(sealed.record.writes.len(), 8);
    for node in &sealed.new_nodes {
        pages
            .put_node(
                *blake3::hash(&node.bytes).as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    verifier.verify(&sealed.record).unwrap();
    let mut receiver = WorldFieldReadContextV1 {
        root: sealed.record.after.state_root(StateNamespaceV1::Artifacts),
        state: &state,
        values: ValueReadContext {
            source: &source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        },
        key_work: &mut read_keys,
        semantic_bytes: 1 << 20,
        record_bytes: 2 << 20,
        take_state_error: || state_error.borrow_mut().take(),
        cancelled: || false,
    };
    let accepted = accept_artifact_config_transition_with_json_v1(
        &family,
        &verifier,
        &sealed.record,
        &mut receiver,
        &mut StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 10_000,
        },
        &mut ArtifactConfigReadWorkV1 {
            cells_remaining: 100_000,
            max_payload_bytes: 1 << 20,
            json: crate::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 10_000,
                json_nodes_remaining: 1000,
                max_depth: 16,
                max_payload_bytes: 1 << 20,
            },
        },
        &mut ArtifactConfigJsonParentsV1 {
            scopes: &[&parent_json],
            probes_remaining: 64,
            delta: JsonChangeWorkV1 {
                json: JsonReadWorkV1 {
                    cells_remaining: 10_000,
                    json_nodes_remaining: 1000,
                    max_depth: 16,
                    max_payload_bytes: 1 << 20,
                },
                paths_remaining: 100_000,
                max_pending_pairs: 64,
            },
        },
    )
    .unwrap();
    accepted
        .validate_binding(&sealed.record.after.digest().unwrap())
        .unwrap();
    qualify_family(&sealed.record.after, &pages).unwrap();
    let expected_json = serde_json::json!({"items":[true,true,true],"metadata":"keep"});
    let expected = crate::incremental_json_records::prepare_json_subtree_v1(
        genesis::json_profile(),
        &expected_json,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut write_work().json,
    )
    .unwrap();
    let cold = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let (_, actual_body) =
        family_in_pool(&sealed.record.after, &pages, &cold, MemoryClass::Recovery).unwrap();
    let actual = json_in_pool(
        &actual_body,
        json_address,
        &pages,
        &cold,
        MemoryClass::Recovery,
    );
    assert_eq!(actual.root(), &expected);
}
