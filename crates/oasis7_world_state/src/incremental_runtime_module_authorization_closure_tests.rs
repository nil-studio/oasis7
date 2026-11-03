//! Local child closure tests, not a full family/physical recovery claim.
#[path = "incremental_runtime_module_authorization_family_tests.rs"]
mod family_tests;
#[path = "incremental_runtime_module_authorization_output_tests.rs"]
mod output_tests;
use super::*;
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_record_tree::{self as tree, RecordTreeWritePolicyV1},
    incremental_semantic_key::SemanticKeyBudget,
    incremental_world_field_key::WorldFieldRecordKindV1,
};
use oasis7_formal_protocol::{
    incremental_state::{StatePageWriter, StateRootV1, StateScanBudget},
    incremental_value::{
        ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError,
    },
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};
type Nodes = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
#[derive(Clone)]
struct Pages(Rc<RefCell<Nodes>>);
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.borrow_mut().insert(id, Arc::new(bytes));
        Ok(())
    }
}
impl StatePageWriter for Pages {}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let pages = self.0.borrow();
        let bytes = pages
            .get(id)
            .ok_or(ValueTreeError::Invalid("authorization test page absent"))?;
        if bytes.len() > max {
            return Err(ValueTreeError::Invalid("authorization test page bound"));
        }
        Ok(Arc::new(
            memory.reserve(bytes.len(), class)?.retain(bytes.to_vec()),
        ))
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    }
}
fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    }
}
fn writer<'a>(
    pages: &'a mut Pages,
    memory: &'a MemoryBudget,
    io: &'a mut ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> MemberRecordWriteContext<'a, Pages, impl FnMut() -> bool> {
    MemberRecordWriteContext {
        sink: pages,
        memory,
        io,
        key_work: keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    }
}
fn work() -> ModuleAuthorizationWriteWorkV1 {
    ModuleAuthorizationWriteWorkV1 {
        cells_remaining: 10_000,
        max_pending_node_bytes: 1 << 20,
    }
}
fn qualify(
    field: ModuleAuthorizationFieldV1,
    root: &StateRootV1,
    pages: &Pages,
) -> Result<ReservedValue<ModuleAuthorizationBodyQualificationV1>, ValueTreeError> {
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let mut state_io = io();
    let state_io = RefCell::new(&mut state_io);
    let mut stop = || false;
    let cancel = RefCell::new(&mut stop);
    let error = RefCell::new(None);
    let state = tree::Source {
        source: pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &state_io,
        cancelled: &cancel,
        error: &error,
        labels: genesis::LABELS,
    };
    let mut io = io();
    let mut keys = keys();
    qualify_module_authorization_body_v1(
        field,
        &mut WorldFieldReadContextV1 {
            root,
            state: &state,
            values: ValueReadContext {
                source: pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 3 << 20,
            take_state_error: || error.borrow_mut().take(),
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 100_000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 10_000,
        },
        &mut ModuleAuthorizationReadWorkV1 {
            cells_remaining: 10_000,
            comparison_bytes_remaining: 4 << 20,
            max_payload_bytes: 2 << 20,
        },
    )
}
fn policy() -> RecordTreeWritePolicyV1 {
    RecordTreeWritePolicyV1 {
        max_pending_node_bytes: 1 << 20,
        labels: genesis::LABELS,
    }
}
fn replace(
    root: StateRootV1,
    field: ModuleAuthorizationFieldV1,
    address: ModuleAuthorizationAddressV1,
    before: MemberPayloadRefV1<'_>,
    after: Option<MemberPayloadRefV1<'_>>,
    pages: &mut Pages,
    memory: &MemoryBudget,
) -> StateRootV1 {
    let source = pages.clone();
    let mut write_io = io();
    let mut key_work = keys();
    let mut write = writer(pages, memory, &mut write_io, &mut key_work);
    let old = write_member_record_v1(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        &address.semantic(field),
        before,
        &mut write,
    )
    .unwrap();
    let new = after.map(|value| {
        write_member_record_v1(
            genesis::body_descriptor(),
            WorldFieldRecordKindV1::Entry,
            &address.semantic(field),
            value,
            &mut write,
        )
        .unwrap()
        .value_root
    });
    tree::edit(
        root,
        &old.field_key,
        (Some(old.value_root), new),
        &mut ValueReadContext {
            source: &source,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut io(),
        },
        &mut write,
        &mut || false,
        policy(),
    )
    .unwrap()
}
#[test]
fn saved_grant_and_rule_children_recover_without_source_and_reject_local_schema_faults() {
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages(Rc::new(RefCell::new(BTreeMap::new())));
    let grant = CapabilityGrant {
        name: "not the outer key".into(),
        effect_kinds: vec!["".into(), "x.*".into(), "x.*".into()],
        expiry: Some(0),
    };
    let root = genesis::body(
        genesis::Body::Grant(&grant),
        &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
        &mut work(),
    )
    .unwrap();
    drop(grant);
    assert_eq!(root.leaf_count, 8);
    let proof = qualify(ModuleAuthorizationFieldV1::Capabilities, &root, &pages).unwrap();
    assert_eq!(proof.root(), &root);
    assert_eq!(proof.field(), ModuleAuthorizationFieldV1::Capabilities);
    assert_eq!(proof.allocation_class(), MemoryClass::Ordinary);
    let absent = replace(
        root.clone(),
        ModuleAuthorizationFieldV1::Capabilities,
        ModuleAuthorizationAddressV1::fixed("expiry"),
        MemberPayloadRefV1::U64(0),
        Some(MemberPayloadRefV1::Unit),
        &mut pages,
        &memory,
    );
    assert_ne!(absent.node_root, root.node_root);
    qualify(ModuleAuthorizationFieldV1::Capabilities, &absent, &pages).unwrap();
    let missing = replace(
        root.clone(),
        ModuleAuthorizationFieldV1::Capabilities,
        ModuleAuthorizationAddressV1::fixed("expiry"),
        MemberPayloadRefV1::U64(0),
        None,
        &mut pages,
        &memory,
    );
    assert!(matches!(
        qualify(ModuleAuthorizationFieldV1::Capabilities, &missing, &pages),
        Err(ValueTreeError::Invalid(_))
    ));
    let extra = write_member_record_v1(
        genesis::body_descriptor(),
        WorldFieldRecordKindV1::Entry,
        "unknown original address",
        MemberPayloadRefV1::Text("extra"),
        &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
    )
    .unwrap();
    let mut extra_root = tree::append(
        root.clone(),
        &extra.field_key,
        extra.value_root,
        &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
        policy(),
    )
    .unwrap();
    // Defeat an expected-count-only shortcut; actual authenticated traversal
    // still has to reject this extra original leaf, not trust the supplied count.
    extra_root.leaf_count = root.leaf_count;
    assert!(matches!(
        qualify(
            ModuleAuthorizationFieldV1::Capabilities,
            &extra_root,
            &pages
        ),
        Err(ValueTreeError::Invalid(_))
    ));
    let rule = PolicyRule {
        when: PolicyWhen {
            effect_kind: Some("".into()),
            origin_kind: Some(OriginKind::Module),
            cap_name: None,
        },
        decision: PolicyDecision::Allow,
    };
    let root = genesis::body(
        genesis::Body::Rule(&rule),
        &mut writer(&mut pages, &memory, &mut io(), &mut keys()),
        &mut work(),
    )
    .unwrap();
    drop(rule);
    assert_eq!(root.leaf_count, 7);
    qualify(ModuleAuthorizationFieldV1::PolicyRules, &root, &pages).unwrap();
    let reason = replace(
        root.clone(),
        ModuleAuthorizationFieldV1::PolicyRules,
        ModuleAuthorizationAddressV1::fixed("decision.reason"),
        MemberPayloadRefV1::Unit,
        Some(MemberPayloadRefV1::Text("")),
        &mut pages,
        &memory,
    );
    assert!(matches!(
        qualify(ModuleAuthorizationFieldV1::PolicyRules, &reason, &pages),
        Err(ValueTreeError::Invalid(_))
    ));
    let origin = replace(
        root,
        ModuleAuthorizationFieldV1::PolicyRules,
        ModuleAuthorizationAddressV1::fixed("when.origin_kind"),
        MemberPayloadRefV1::Text("module"),
        Some(MemberPayloadRefV1::Text("MODULE")),
        &mut pages,
        &memory,
    );
    assert!(matches!(
        qualify(ModuleAuthorizationFieldV1::PolicyRules, &origin, &pages),
        Err(ValueTreeError::Invalid(_))
    ));
}
