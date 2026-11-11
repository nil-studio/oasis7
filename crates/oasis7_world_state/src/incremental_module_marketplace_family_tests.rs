use super::*;
#[path = "incremental_module_marketplace_family_test_support.rs"]
mod support;
use support::*;
fn receive(
    memory: &MemoryBudget,
    pages: &Pages,
    parent: &ExecutionParent,
    prepared: &PreparedTransition,
    proof: &ReservedValue<ModuleMarketplaceFamilyQualificationV1>,
) -> Result<ReservedValue<ModuleMarketplaceFamilyQualificationV1>, WorldStateOwnerError> {
    let verifier = TransitionVerifier {
        parent,
        operation: [3; 32],
        source: pages,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 16 << 20,
    };
    accept_module_marketplace_family_transition_v1(
        proof,
        &verifier,
        &prepared.record,
        &mut context(
            memory,
            pages,
            &prepared.record.after,
            &mut io(),
            &mut SemanticKeyBudget {
                bytes_remaining: 128 << 20,
            },
        ),
        &mut scan(),
        &mut work(),
    )
}
#[test]
fn market3_real_gen_full_qualification_and_actual_bid_graph_insert_identity_delete() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 131072);
    let roots = install(&memory, &pages);
    let proof = qualify_module_marketplace_family_v1(
        &roots,
        &mut context(
            &memory,
            &pages,
            &roots,
            &mut io(),
            &mut SemanticKeyBudget {
                bytes_remaining: 128 << 20,
            },
        ),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.record_counts(), [1, 1, 1]);
    let expected = outer(
        &memory,
        &pages,
        &roots,
        ModuleMarketplaceFieldV1::ArtifactBids,
        " original/bids ",
    );
    let bidder = "大买方".repeat(16384);
    let inserted = ModuleArtifactBidState {
        order_id: 11,
        bidder_agent_id: bidder.clone(),
        price_kind: ResourceKind::Data,
        price_amount: i64::MAX,
        bid_at: u64::MAX,
    };
    for case in 0..4 {
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits())
                .unwrap();
        let guard = tx.guard();
        let change = match case {
            0 => ModuleArtifactBidChangeV1::Insert {
                ordinal: 1,
                bid: &inserted,
            },
            1 => ModuleArtifactBidChangeV1::UpdateIdentity {
                ordinal: 1,
                expected_order_id: 11,
                expected_bidder_agent_id: &bidder,
                after_order_id: 99,
                after_bidder_agent_id: "new-original-bidder",
            },
            2 => ModuleArtifactBidChangeV1::DeleteMatching {
                order_id: 11,
                bidder_agent_id: &bidder,
                expected_matches: 3,
            },
            _ => ModuleArtifactBidChangeV1::Delete {
                ordinal: 3,
                expected_order_id: 22,
                expected_bidder_agent_id: "other-bidder",
            },
        };
        change_bid(&memory, &pages, &mut tx, &expected, change, &guard, &live);
        let guard = tx.guard();
        let prepared = tx.seal(&guard, &live, 1).unwrap();
        persist(&memory, &pages, &prepared.new_nodes);
        receive(&memory, &pages, &parent, &prepared, &proof).unwrap();
        qualify_module_marketplace_family_v1(
            &prepared.record.after,
            &mut context(
                &memory,
                &pages,
                &prepared.record.after,
                &mut io(),
                &mut SemanticKeyBudget {
                    bytes_remaining: 128 << 20,
                },
            ),
            &mut scan(),
            &mut work(),
        )
        .unwrap();
    }
}
#[test]
fn market3_scalar_receiving_skips_unchanged_large_bidder_and_seller_text() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 131072);
    let roots = install(&memory, &pages);
    let proof = qualify_module_marketplace_family_v1(
        &roots,
        &mut context(
            &memory,
            &pages,
            &roots,
            &mut io(),
            &mut SemanticKeyBudget {
                bytes_remaining: 128 << 20,
            },
        ),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    let owner = ModuleMarketplaceFieldV1::ArtifactBids;
    let reference = outer(&memory, &pages, &roots, owner, " original/bids ");
    let root = child(&reference, owner);
    let mut point_io = io();
    let mut point_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = context(&memory, &pages, &roots, &mut point_io, &mut point_keys);
    let (key, _) = ctx
        .key(
            owner.descriptor().unwrap(),
            WorldFieldRecordKindV1::Entry,
            &(("bid", 0u64), "bidder_agent_id"),
        )
        .unwrap();
    let missing = root.get(key.as_bytes(), &pages).unwrap().unwrap();
    drop(ctx);
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits()).unwrap();
    let guard = tx.guard();
    let mut rq = io();
    let mut wq = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut wk = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    change_module_bid_price_amount_overlay_v1(
        " original/bids ",
        0,
        0,
        42,
        &mut WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut rq,
            },
            key_work: &mut rk,
            semantic_bytes: 1 << 20,
            record_bytes: 8 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut MemberRecordWriteContext {
            sink: &mut Writer(&pages),
            memory: &memory,
            io: &mut wq,
            key_work: &mut wk,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 4 << 20,
                record_bytes: 8 << 20,
            },
            cancelled: || false,
        },
        &guard,
        &live,
    )
    .unwrap();
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    persist(&memory, &pages, &prepared.new_nodes);
    // Logical inherited values only; a missing page still precludes physical WorldReady.
    pages.remove(&missing);
    receive(&memory, &pages, &parent, &prepared, &proof).unwrap();
    let owner = ModuleMarketplaceFieldV1::ArtifactListings;
    let mut listing = child(
        &outer(&memory, &pages, &roots, owner, " original/listing "),
        owner,
    );
    let mut pq = io();
    let mut pk = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = context(&memory, &pages, &roots, &mut pq, &mut pk);
    let (key, _) = ctx
        .key(
            owner.descriptor().unwrap(),
            WorldFieldRecordKindV1::Entry,
            "seller_agent_id",
        )
        .unwrap();
    let missing = listing.get(key.as_bytes(), &pages).unwrap().unwrap();
    drop(ctx);
    edit_child(
        &memory,
        &pages,
        owner,
        &mut listing,
        "price_amount",
        Some(MemberPayloadRefV1::I64(19)),
    );
    let parent = support::parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits()).unwrap();
    publish(
        &memory,
        &pages,
        &mut tx,
        owner,
        " original/listing ",
        listing,
    );
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    persist(&memory, &pages, &prepared.new_nodes);
    pages.remove(&missing);
    receive(&memory, &pages, &parent, &prepared, &proof).unwrap();
}
#[test]
fn market3_visual_conditional_columns_and_listing_shapes_close_actual_deltas() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 131072);
    let roots = install(&memory, &pages);
    let proof = qualify_module_marketplace_family_v1(
        &roots,
        &mut context(
            &memory,
            &pages,
            &roots,
            &mut io(),
            &mut SemanticKeyBudget {
                bytes_remaining: 128 << 20,
            },
        ),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    for case in 0..5 {
        let owner = if case < 3 {
            ModuleMarketplaceFieldV1::VisualEntities
        } else {
            ModuleMarketplaceFieldV1::ArtifactListings
        };
        let name = if case < 3 {
            " original/visual "
        } else {
            " original/listing "
        };
        let mut root = child(&outer(&memory, &pages, &roots, owner, name), owner);
        let valid = matches!(case, 1 | 3);
        match case {
            0 => edit_child(
                &memory,
                &pages,
                owner,
                &mut root,
                "label_present",
                Some(MemberPayloadRefV1::Bool(false)),
            ),
            1 => {
                edit_child(
                    &memory,
                    &pages,
                    owner,
                    &mut root,
                    "label_present",
                    Some(MemberPayloadRefV1::Bool(false)),
                );
                edit_child(&memory, &pages, owner, &mut root, "label_text", None);
            }
            2 => edit_child(
                &memory,
                &pages,
                owner,
                &mut root,
                "anchor_kind",
                Some(MemberPayloadRefV1::U64(0)),
            ),
            3 => edit_child(
                &memory,
                &pages,
                owner,
                &mut root,
                "price_amount",
                Some(MemberPayloadRefV1::I64(99)),
            ),
            _ => edit_child(&memory, &pages, owner, &mut root, "order_id", None),
        }
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits())
                .unwrap();
        publish(&memory, &pages, &mut tx, owner, name, root);
        let guard = tx.guard();
        let prepared = tx.seal(&guard, &live, 1).unwrap();
        persist(&memory, &pages, &prepared.new_nodes);
        assert_eq!(
            receive(&memory, &pages, &parent, &prepared, &proof).is_ok(),
            valid,
            "case={case}"
        );
    }

    let owner = ModuleMarketplaceFieldV1::VisualEntities;
    let mut visual = child(
        &outer(&memory, &pages, &roots, owner, " original/visual "),
        owner,
    );
    let mut pq = io();
    let mut pk = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = context(&memory, &pages, &roots, &mut pq, &mut pk);
    let (key, _) = ctx
        .key(
            owner.descriptor().unwrap(),
            WorldFieldRecordKindV1::Entry,
            "label_text",
        )
        .unwrap();
    let missing = visual.get(key.as_bytes(), &pages).unwrap().unwrap();
    drop(ctx);
    edit_child(
        &memory,
        &pages,
        owner,
        &mut visual,
        "anchor_x_cm",
        Some(MemberPayloadRefV1::I64(42)),
    );
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits()).unwrap();
    publish(&memory, &pages, &mut tx, owner, " original/visual ", visual);
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    persist(&memory, &pages, &prepared.new_nodes);
    // A separate physical lease must reject this missing inherited page.
    pages.remove(&missing);
    receive(&memory, &pages, &parent, &prepared, &proof).unwrap();
}

#[test]
fn market3_bid_relation_changes_cannot_forge_counts_membership_or_avl_links() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 131072);
    let roots = install(&memory, &pages);
    let proof = qualify_module_marketplace_family_v1(
        &roots,
        &mut context(
            &memory,
            &pages,
            &roots,
            &mut io(),
            &mut SemanticKeyBudget {
                bytes_remaining: 128 << 20,
            },
        ),
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    let owner = ModuleMarketplaceFieldV1::ArtifactBids;
    for case in 0..4 {
        let mut root = child(
            &outer(&memory, &pages, &roots, owner, " original/bids "),
            owner,
        );
        match case {
            0 => edit_child(
                &memory,
                &pages,
                owner,
                &mut root,
                "length",
                Some(MemberPayloadRefV1::U64(3)),
            ),
            1 => edit_child(
                &memory,
                &pages,
                owner,
                &mut root,
                &(("bid", 1u64), "present"),
                None,
            ),
            2 => edit_child(
                &memory,
                &pages,
                owner,
                &mut root,
                &(("bid", 1u64), "bidder_agent_id"),
                Some(MemberPayloadRefV1::Text("forged-unindexed-bidder")),
            ),
            _ => edit_child(
                &memory,
                &pages,
                owner,
                &mut root,
                &(("bid", 1u64), "index_node"),
                Some(MemberPayloadRefV1::FixedU64x7([
                    1,
                    u64::MAX,
                    1,
                    1,
                    0,
                    0,
                    u64::MAX,
                ])),
            ),
        }
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits())
                .unwrap();
        publish(&memory, &pages, &mut tx, owner, " original/bids ", root);
        let guard = tx.guard();
        let prepared = tx.seal(&guard, &live, 1).unwrap();
        persist(&memory, &pages, &prepared.new_nodes);
        assert!(
            receive(&memory, &pages, &parent, &prepared, &proof).is_err(),
            "case={case}"
        );
    }
}

use std::sync::Arc;
struct Recovery<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
}
struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl ValueNodeReader for Recovery<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Recovery || !memory.same_pool(self.memory) {
            return Err(ValueTreeError::Invalid("phase recovery source"));
        }
        let bytes = ValueNodeReader::read_node(self.pages, id, max, memory, MemoryClass::Ordinary)?;
        let charge = memory.reserve(bytes.len() + 128, class)?;
        Ok(Arc::new(charge.retain(bytes.to_vec())))
    }
}
impl MerkleNodeReader for Recovery<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        ValueNodeReader::read_node(self, &id, max, self.memory, MemoryClass::Recovery)
            .map(|n| Some(Node(n)))
            .map_err(|e| e.to_string())
    }
}

#[test]
fn market3_recovery_requires_explicit_same_root_pool_class_readmission() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 131072);
    let roots = install(&memory, &pages);
    let recovery = Recovery {
        pages: &pages,
        memory: &memory,
    };
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 128 << 20,
    };
    let proof = qualify_module_marketplace_family_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
            state: &recovery,
            values: ValueReadContext {
                source: &recovery,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 8 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        &mut work(),
    )
    .unwrap();
    assert_eq!(proof.allocation_class(), MemoryClass::Recovery);
    assert_eq!(proof.record_counts(), [1; 3]);
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let tx =
        ExecutionTransaction::begin(parent.clone(), [3; 32], &pages, &memory, limits()).unwrap();
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    assert!(receive(&memory, &pages, &parent, &prepared, &proof).is_err());
    let promoted =
        readmit_module_marketplace_family_ordinary_v1(&proof, &roots, &memory, || false).unwrap();
    receive(&memory, &pages, &parent, &prepared, &promoted).unwrap();
    assert!(
        readmit_module_marketplace_family_ordinary_v1(&promoted, &roots, &memory, || false)
            .is_err()
    );
    let foreign = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    assert!(
        readmit_module_marketplace_family_ordinary_v1(&proof, &roots, &foreign, || false).is_err()
    );
    let mut wrong = roots.clone();
    wrong.simulation_tick += 1;
    assert!(
        readmit_module_marketplace_family_ordinary_v1(&proof, &wrong, &memory, || false).is_err()
    );
    let mut polls = 0;
    assert!(matches!(
        readmit_module_marketplace_family_ordinary_v1(&proof, &roots, &memory, || {
            polls += 1;
            polls > 1
        }),
        Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled))
    ));
    let held = memory
        .reserve((480 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        readmit_module_marketplace_family_ordinary_v1(&proof, &roots, &memory, || false),
        Err(WorldStateOwnerError::Memory(_))
    ));
    drop(held);
}
