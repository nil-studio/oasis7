use super::*;
use oasis7_formal_protocol::incremental_state::{StateDeltaV1, StateNodeWriteV1, StateWriteV1};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation, ReservedValue};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
};
struct Page(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for Page {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
struct Pages {
    memory: MemoryBudget,
    _fixture: MemoryReservation,
    nodes: BTreeMap<String, Vec<u8>>,
    denied: BTreeSet<String>,
    reads: RefCell<Vec<String>>,
}
impl Pages {
    fn new() -> Self {
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let fixture = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        Self {
            memory,
            _fixture: fixture,
            nodes: BTreeMap::new(),
            denied: BTreeSet::new(),
            reads: RefCell::new(Vec::new()),
        }
    }
    fn apply(&mut self, root: StateRootV1, writes: Vec<StateWriteV1>) -> StateRootV1 {
        let delta = StateDeltaV1::build(root, writes, self, 8 << 20).unwrap();
        for node in delta.new_nodes {
            self.nodes.insert(node.digest, node.bytes);
        }
        delta.after
    }
}
impl MerkleNodeReader for Pages {
    type Node = Page;
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        self.reads.borrow_mut().push(id.to_owned());
        if self.denied.contains(id) {
            return Err("unchanged subtree opened".into());
        }
        self.nodes
            .get(id)
            .map(|bytes| {
                if bytes.len() > max {
                    return Err("page bound".into());
                }
                let charge = self
                    .memory
                    .reserve(bytes.len(), MemoryClass::Ordinary)
                    .map_err(|e| e.to_string())?;
                Ok(Page(charge.retain(bytes.clone())))
            })
            .transpose()
    }
}
fn write(key: &[u8], before: Option<u8>, after: Option<u8>) -> StateWriteV1 {
    StateWriteV1 {
        key: key.to_vec(),
        before: before.map(|v| [v; 32]),
        after: after.map(|v| [v; 32]),
    }
}
fn work() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 16 << 20,
        leaves_remaining: 1000,
    }
}
#[test]
fn organization_changed_tree_skips_unchanged_branches_and_preserves_exact_edges() {
    let mut pages = Pages::new();
    let before = pages.apply(
        StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
        vec![
            write(b"changed", None, Some(1)),
            write(b"untouched", None, Some(2)),
        ],
    );
    let after = pages.apply(before.clone(), vec![write(b"changed", Some(1), Some(3))]);
    for (id, bytes) in &pages.nodes {
        if matches!(StateNodeWriteV1 { digest: id.clone(), bytes: bytes.clone() }.verified_links().unwrap(), oasis7_formal_protocol::incremental_state::StatePageLinksV1::Value(entry) if entry.key == b"untouched")
        {
            pages.denied.insert(id.clone());
        }
    }
    assert_eq!(pages.denied.len(), 1);
    pages.reads.borrow_mut().clear();
    let mut seen = 0;
    OrganizationChangedTreeContextV1 {
        source: &pages,
        memory: &pages.memory,
        class: MemoryClass::Ordinary,
        budget: &mut work(),
        cancelled: &mut || false,
    }
    .visit(&before, &after, |old, new| {
        assert_eq!(old.unwrap().key, b"changed");
        assert_eq!(old.unwrap().value, [1; 32]);
        assert_eq!(new.unwrap().key, b"changed");
        assert_eq!(new.unwrap().value, [3; 32]);
        seen += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(seen, 1);
    assert!(
        pages
            .reads
            .borrow()
            .iter()
            .all(|id| !pages.denied.contains(id))
    );
    let mut no_work = StateScanBudget {
        nodes_remaining: 0,
        read_bytes_remaining: 0,
        leaves_remaining: 0,
    };
    OrganizationChangedTreeContextV1 {
        source: &pages,
        memory: &pages.memory,
        class: MemoryClass::Ordinary,
        budget: &mut no_work,
        cancelled: &mut || false,
    }
    .visit(&after, &after, |_, _| {
        panic!("identical roots must skip callback")
    })
    .unwrap();
}
#[test]
fn organization_changed_tree_rejects_canonical_leaf_at_wrong_route_and_bad_counts() {
    let mut pages = Pages::new();
    let root = pages.apply(
        StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
        vec![write(b"right", None, Some(1))],
    );
    let mut remapped = BTreeMap::new();
    let mut branches = Vec::new();
    for (id, bytes) in &pages.nodes {
        match (StateNodeWriteV1 {
            digest: id.clone(),
            bytes: bytes.clone(),
        })
        .verified_links()
        .unwrap()
        {
            oasis7_formal_protocol::incremental_state::StatePageLinksV1::Value(_) => {
                let mut bad = bytes.clone();
                let key_start = b"oasis7.iws.state-page.v1\0".len() + 4 + 5;
                bad[key_start..key_start + 5].copy_from_slice(b"wrong");
                let next = blake3::hash(&bad).to_hex().to_string();
                remapped.insert(id.clone(), (next, bad));
            }
            oasis7_formal_protocol::incremental_state::StatePageLinksV1::Branch {
                depth, ..
            } => branches.push((depth, id.clone(), bytes.clone())),
        }
    }
    branches.sort_by_key(|(depth, _, _)| std::cmp::Reverse(*depth));
    for (_, id, mut bytes) in branches {
        let offset = b"oasis7.iws.state-page.v1\0".len() + 4 + 3;
        for side in 0..2 {
            let child = blake3::Hash::from_bytes(
                bytes[offset + side * 32..offset + (side + 1) * 32]
                    .try_into()
                    .unwrap(),
            )
            .to_hex()
            .to_string();
            if let Some((next, _)) = remapped.get(&child) {
                bytes[offset + side * 32..offset + (side + 1) * 32]
                    .copy_from_slice(blake3::Hash::from_hex(next).unwrap().as_bytes());
            }
        }
        let next = blake3::hash(&bytes).to_hex().to_string();
        remapped.insert(id, (next, bytes));
    }
    let mut bad = root.clone();
    bad.node_root = remapped[&root.node_root].0.clone();
    for (id, bytes) in remapped.into_values() {
        pages.nodes.insert(id, bytes);
    }
    let empty = StateRootV1::empty(StateNamespaceV1::Authorization).unwrap();
    assert!(matches!(
        OrganizationChangedTreeContextV1 {
            source: &pages,
            memory: &pages.memory,
            class: MemoryClass::Ordinary,
            budget: &mut work(),
            cancelled: &mut || false
        }
        .visit(&empty, &bad, |_, _| Ok(())),
        Err(ValueTreeError::Invalid("organization diff leaf route"))
    ));
    let mut wrong_count = root.clone();
    wrong_count.leaf_count += 1;
    assert!(matches!(
        OrganizationChangedTreeContextV1 {
            source: &pages,
            memory: &pages.memory,
            class: MemoryClass::Ordinary,
            budget: &mut work(),
            cancelled: &mut || false
        }
        .visit(&root, &wrong_count, |_, _| Ok(())),
        Err(ValueTreeError::Invalid("organization diff leaf count"))
    ));
}
#[test]
fn organization_changed_tree_cancellation_and_limits_never_return_success() {
    let mut pages = Pages::new();
    let empty = StateRootV1::empty(StateNamespaceV1::Authorization).unwrap();
    let root = pages.apply(empty.clone(), vec![write(b"field", None, Some(1))]);
    let entered = Cell::new(false);
    assert!(matches!(
        OrganizationChangedTreeContextV1 {
            source: &pages,
            memory: &pages.memory,
            class: MemoryClass::Ordinary,
            budget: &mut work(),
            cancelled: &mut || entered.get()
        }
        .visit(&empty, &root, |_, _| {
            entered.set(true);
            Ok(())
        }),
        Err(ValueTreeError::Cancelled)
    ));
    assert!(entered.get());
    for limited in [
        StateScanBudget {
            nodes_remaining: 0,
            ..work()
        },
        StateScanBudget {
            read_bytes_remaining: 0,
            ..work()
        },
        StateScanBudget {
            leaves_remaining: 0,
            ..work()
        },
    ] {
        let mut limited = limited;
        assert!(matches!(
            OrganizationChangedTreeContextV1 {
                source: &pages,
                memory: &pages.memory,
                class: MemoryClass::Ordinary,
                budget: &mut limited,
                cancelled: &mut || false
            }
            .visit(&empty, &root, |_, _| Ok(())),
            Err(ValueTreeError::ResourceLimited(_))
        ));
    }
}
fn text(value: &str) -> Vec<u8> {
    let mut b = vec![1];
    b.extend_from_slice(&(value.len() as u64).to_be_bytes());
    b.extend_from_slice(value.as_bytes());
    b
}
fn pair(a: Vec<u8>, b: Vec<u8>) -> Vec<u8> {
    let mut bytes = vec![5];
    bytes.extend(a);
    bytes.extend(b);
    bytes
}
fn encoded(token: Vec<u8>) -> Vec<u8> {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.extend(token);
    bytes
}
#[test]
fn organization_relative_key_decoder_accepts_only_closed_original_shapes() {
    let case = OrganizationRootKeyV1::Case("outer-ignored");
    assert_eq!(
        inspect_organization_relative_key_v1(case, &encoded(pair(vec![0], text("policy_version"))))
            .unwrap(),
        OrganizationRelativeKeyV1::Column {
            record: OrganizationRelativeRecordV1::Case,
            slot: 4
        }
    );
    let grant = pair(pair(vec![0], text("grants")), text(""));
    assert_eq!(
        inspect_organization_relative_key_v1(
            case,
            &encoded(pair(
                pair(grant.clone(), text("scopes")),
                pair(text("object\0x"), text("advance_case"))
            ))
        )
        .unwrap(),
        OrganizationRelativeKeyV1::Scope {
            grant: "",
            object: "object\0x",
            right: "advance_case"
        }
    );
    for wrong in [
        pair(vec![0], text("unknown")),
        pair(
            pair(grant.clone(), text("scopes")),
            pair(text(""), text("unknown")),
        ),
        pair(
            pair(vec![0], text("members")),
            vec![2, 0, 0, 0, 0, 0, 0, 0, 0],
        ),
    ] {
        assert!(inspect_organization_relative_key_v1(case, &encoded(wrong)).is_err());
    }
    assert!(
        inspect_organization_relative_key_v1(OrganizationRootKeyV1::Estate("x"), &encoded(grant))
            .is_err()
    );
}
