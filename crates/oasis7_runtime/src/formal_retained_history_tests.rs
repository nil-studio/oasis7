use super::*;
use oasis7_distfs::controlled_authority::formal::business_index::{
    BusinessIndexDeltaV1, BusinessIndexLeafKindV1, BusinessIndexLeafV1, BusinessIndexNodeReader,
};
use std::collections::BTreeMap;

#[derive(Default)]
struct MemoryNodes(BTreeMap<String, Vec<u8>>);

impl BusinessIndexNodeReader for MemoryNodes {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(digest).cloned())
    }
}

fn apply_metadata_round(
    root: BusinessIndexRootV1,
    updates: Vec<BusinessIndexLeafV1>,
    nodes: &mut MemoryNodes,
) -> BusinessIndexRootV1 {
    let delta = BusinessIndexDeltaV1::build(root, updates, nodes).unwrap();
    let BusinessIndexDeltaV1 {
        new, node_writes, ..
    } = delta;
    for write in node_writes {
        nodes.0.insert(write.digest, write.bytes);
    }
    new
}

fn canonical_chat_result(
    nonce: u64,
    private_key: &str,
    public_key: &str,
) -> crate::service_admission::CanonicalIntentResultV1 {
    let mut chat = oasis7_proto::viewer::AgentChatRequest {
        agent_id: "agent".into(),
        message: format!("intent retry {nonce}"),
        player_id: Some("owner".into()),
        public_key: Some(public_key.into()),
        auth: None,
        intent_tick: Some(2),
        intent_seq: Some(nonce),
        world_id: Some("world".into()),
        reorg_epoch: Some(7),
        authority_scope: Some("player_agent_chat".into()),
        replaces_intent_id: None,
        canonical_authority: Some(oasis7_proto::viewer::CanonicalAgentChatAuthorityV1 {
            branch_id: "main".into(),
            agent_identity_generation: 1,
        }),
    };
    chat.auth = Some(
        oasis7_proto::player_auth::sign_agent_chat_auth_proof(&chat, nonce, public_key, private_key).unwrap(),
    );
    let payload = crate::service_admission::WorldServicePayloadV1::AgentChat(chat);
    let world = crate::service_admission::WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "fixture".into(),
    };
    let correlation =
        crate::service_admission::derive_correlation(world, &payload).unwrap();
    crate::service_admission::CanonicalIntentResultV1 {
        request: crate::service_admission::SubmitIntentRequest {
            contract_version: 1,
            correlation,
            deadline_unix_ms: None,
            signed_payload: payload,
        },
        action_id: nonce,
        committed_height: nonce,
        receipt: serde_json::json!({ "intent_id": "intent" }),
        rejected: None,
    }
}

fn retained_world_service_leaf(
    result: &crate::service_admission::CanonicalIntentResultV1,
) -> (String, BusinessIndexLeafV1) {
    let correlation_key_digest =
        crate::service_admission::correlation::key_digest(&result.request.correlation.key)
            .unwrap();
    let canonical_result_cbor = serde_cbor::to_vec(result).unwrap();
    let canonical_result_digest = blake3::hash(&canonical_result_cbor).to_hex().to_string();
    let leaf_key = retained_key_digest("world-service-result", &correlation_key_digest).unwrap();
    let value = RetainedHistoryLeafV1::WorldServiceResult(RetainedWorldServiceResultV1 {
        correlation_key_digest: correlation_key_digest.clone(),
        canonical_result_cbor,
        canonical_result_digest,
    });
    (
        correlation_key_digest,
        retained_leaf(leaf_key, &value).unwrap(),
    )
}

fn one_leaf_root(key: &str) -> (FormalRetainedHistoryRootV1, MemoryNodes) {
    let reader = MemoryNodes::default();
    let leaf = retained_leaf(
        key.to_owned(),
        &RetainedHistoryLeafV1::AgentIntentReorgEpochs(DistinctReorgEpochSummaryV1::Single(7)),
    )
    .unwrap();
    let delta =
        BusinessIndexDeltaV1::build(BusinessIndexRootV1::empty().unwrap(), vec![leaf], &reader)
            .unwrap();
    let mut nodes = MemoryNodes::default();
    for write in delta.node_writes {
        nodes.0.insert(write.digest, write.bytes);
    }
    (
        FormalRetainedHistoryRootV1::from_index_root(delta.new).unwrap(),
        nodes,
    )
}

#[test]
fn sparse_read_distinguishes_authenticated_absence_from_missing_cas() {
    let present_key = "1".repeat(64);
    let absent_key = "2".repeat(64);
    let (root, nodes) = one_leaf_root(&present_key);
    let parent = VerifiedFormalRetainedHistoryParentV1::for_test(root);

    assert_eq!(parent.read_leaf(&absent_key, &nodes).unwrap(), None);
    assert!(matches!(
        parent.read_leaf(&absent_key, &MemoryNodes::default()),
        Err(RetainedHistoryReadError::Unavailable(_))
    ));
}

#[test]
fn candidate_must_inherit_exact_parent_descriptor() {
    #[derive(Serialize)]
    struct SnapshotEnvelope {
        state: SnapshotStateEnvelope,
    }

    #[derive(Serialize)]
    struct SnapshotStateEnvelope {
        time: u64,
        retained_history_root: Option<FormalRetainedHistoryRootV1>,
    }

    let (root, _) = one_leaf_root(&"1".repeat(64));
    let parent = VerifiedFormalRetainedHistoryParentV1::for_test(root.clone());
    let inherited = serde_cbor::to_vec(&SnapshotEnvelope {
        state: SnapshotStateEnvelope {
            time: 0,
            retained_history_root: Some(root.clone()),
        },
    })
    .unwrap();
    parent.verify_inherited_descriptor(&inherited).unwrap();

    let changed = serde_cbor::to_vec(&SnapshotEnvelope {
        state: SnapshotStateEnvelope {
            time: 0,
            retained_history_root: Some(
                FormalRetainedHistoryRootV1::from_index_root(BusinessIndexRootV1::empty().unwrap())
                    .unwrap(),
            ),
        },
    })
    .unwrap();
    assert!(parent.verify_inherited_descriptor(&changed).is_err());
    assert!(parent.verify_inherited_descriptor(&[]).is_err());
}

#[test]
fn descriptor_digest_commits_to_the_sparse_root() {
    let (mut root, _) = one_leaf_root(&"1".repeat(64));
    root.validate().unwrap();
    let first = if root.root_digest.starts_with('f') {
        "e"
    } else {
        "f"
    };
    root.root_digest.replace_range(0..1, first);
    assert!(root.validate().is_err());
}

#[test]
fn archived_reorg_epoch_summary_preserves_only_query_cardinality() {
    let one = DistinctReorgEpochSummaryV1::from_hot([&Some(8), &Some(8), &None]);
    assert_eq!(one, DistinctReorgEpochSummaryV1::Single(8));
    let multiple = DistinctReorgEpochSummaryV1::merge_hot(&one, [&Some(9)]);
    assert_eq!(
        multiple,
        DistinctReorgEpochSummaryV1::Multiple { max_epoch: 9 }
    );
    assert_eq!(
        DistinctReorgEpochSummaryV1::merge_hot(&multiple, [&Some(3)]),
        DistinctReorgEpochSummaryV1::Multiple { max_epoch: 9 }
    );
    assert_eq!(
        DistinctReorgEpochSummaryV1::merge_hot(&multiple, [&Some(10)]),
        DistinctReorgEpochSummaryV1::Multiple { max_epoch: 10 }
    );
    assert!(multiple.has_multiple());
    assert_eq!(multiple.max_epoch(), Some(9));
    assert_eq!(one.single_epoch(), Some(8));
}

#[test]
fn repeated_prune_metadata_updates_preserve_monotonic_retention_state() {
    let private_key = hex::encode([71u8; 32]);
    let public_key =
        crate::service_admission::sign_read_request("fixture", (), &private_key)
            .unwrap()
            .subject_public_key;
    let first_result = canonical_chat_result(1, &private_key, &public_key);
    let second_result = canonical_chat_result(2, &private_key, &public_key);
    let (first_result_key, first_result_leaf) = retained_world_service_leaf(&first_result);
    let (second_result_key, second_result_leaf) = retained_world_service_leaf(&second_result);
    assert_eq!(first_result_leaf.kind, BusinessIndexLeafKindV1::Outcome);
    assert_eq!(second_result_leaf.kind, BusinessIndexLeafKindV1::Outcome);
    let owner_digest = digest_text("owner-public-key", &public_key).unwrap();
    let owner_nonce_key =
        retained_key_digest("owner-chat-nonce-high-water", &owner_digest).unwrap();
    let lookup = AgentChatIntentLookupV1 {
        agent_id: "agent".into(),
        intent_id: "intent".into(),
        owner_public_key_digest: owner_digest.clone(),
        player_id: "owner".into(),
        world_id: "world".into(),
        branch_id: "main".into(),
        identity_generation: 1,
        reorg_epoch: 7,
        authority_scope: "player_agent_chat".into(),
    };
    let refs_key = retained_key_digest("agent-chat-intent", &lookup).unwrap();
    let epoch_key = retained_key_digest("agent-intent-reorg-epochs", &()).unwrap();
    let mut nodes = MemoryNodes::default();
    let mut root = BusinessIndexRootV1::empty().unwrap();

    for round in 1..=2 {
        let result_key = if round == 1 {
            first_result_key.clone()
        } else {
            second_result_key.clone()
        };
        let mut refs = if round == 1 {
            vec![first_result_key.clone()]
        } else {
            vec![first_result_key.clone(), second_result_key.clone()]
        };
        refs.sort();
        let summary = if round == 1 {
            DistinctReorgEpochSummaryV1::Single(7)
        } else {
            DistinctReorgEpochSummaryV1::Multiple { max_epoch: 9 }
        };
        let mut updates = vec![
            if round == 1 {
                first_result_leaf.clone()
            } else {
                second_result_leaf.clone()
            },
            retained_leaf(
                owner_nonce_key.clone(),
                &RetainedHistoryLeafV1::OwnerChatNonceHighWater(
                    RetainedOwnerChatNonceHighWaterV1 {
                        owner_public_key_digest: owner_digest.clone(),
                        high_water_nonce: round as u64,
                        witness_result_key_digest: result_key,
                    },
                ),
            )
            .unwrap(),
            retained_leaf(
                refs_key.clone(),
                &RetainedHistoryLeafV1::AgentChatIntentIndex(RetainedResultRefsV1 {
                    result_key_digests: refs,
                }),
            )
            .unwrap(),
            retained_leaf(
                epoch_key.clone(),
                &RetainedHistoryLeafV1::AgentIntentReorgEpochs(summary.clone()),
            )
            .unwrap(),
        ];
        updates.sort_by(|left, right| left.key_digest.cmp(&right.key_digest));
        assert_eq!(
            updates
                .iter()
                .filter(|leaf| leaf.kind == BusinessIndexLeafKindV1::Outcome)
                .count(),
            1
        );
        assert_eq!(
            updates
                .iter()
                .filter(|leaf| leaf.kind == BusinessIndexLeafKindV1::CommitMetadata)
                .count(),
            3
        );
        root = apply_metadata_round(root, updates, &mut nodes);

        let view = FormalRetainedHistoryIndexViewV1::from_validated_root(
            FormalRetainedHistoryRootV1::from_index_root(root.clone()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            view.owner_chat_nonce_high_water(&public_key, &nodes)
                .unwrap(),
            Some(round as u64)
        );
        let results = view.agent_chat_intent_results(&lookup, &nodes).unwrap();
        assert_eq!(results.len(), round as usize);
        assert!(
            results
                .iter()
                .all(|result| result.receipt["intent_id"] == "intent")
        );
        assert_eq!(view.archived_reorg_epoch_summary(&nodes).unwrap(), summary);
    }
}

#[test]
fn read_leaf_rejects_kind_mismatch_with_decoded_variant() {
    let key_digest = "3".repeat(64);
    let value =
        RetainedHistoryLeafV1::AgentIntentReorgEpochs(DistinctReorgEpochSummaryV1::Single(7));
    let leaf = BusinessIndexLeafV1 {
        key_digest: key_digest.clone(),
        kind: BusinessIndexLeafKindV1::Outcome,
        canonical_value: encode_retained_leaf(&value).unwrap(),
    };
    let mut nodes = MemoryNodes::default();
    let delta =
        BusinessIndexDeltaV1::build(BusinessIndexRootV1::empty().unwrap(), vec![leaf], &nodes)
            .unwrap();
    let BusinessIndexDeltaV1 {
        new, node_writes, ..
    } = delta;
    for write in node_writes {
        nodes.0.insert(write.digest, write.bytes);
    }
    let view = FormalRetainedHistoryIndexViewV1::from_validated_root(
        FormalRetainedHistoryRootV1::from_index_root(new).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        view.read_leaf(&key_digest, &nodes),
        Err(RetainedHistoryReadError::Invalid(_))
    ));
}

#[test]
fn correlation_key_digests_keep_the_world_service_algorithm_prefix() {
    assert!(is_correlation_key_digest(&format!(
        "blake3:{}",
        "a".repeat(64)
    )));
    assert!(!is_correlation_key_digest(&"a".repeat(64)));
    assert!(!is_correlation_key_digest(&format!(
        "blake3:{}",
        "A".repeat(64)
    )));
}

#[test]
fn feedback_lookup_requires_canonical_prefixed_request_digest() {
    let mut lookup = FeedbackPublicationLookupV1 {
        agent_id: "agent".into(),
        agent_session_id: "session".into(),
        agent_turn_id: "turn".into(),
        decision_request_id: "decision".into(),
        request_digest: format!("blake3:{}", "a".repeat(64)),
        feedback_id: "feedback".into(),
        runtime_receipt_id: Some("receipt".into()),
    };
    lookup.validate().unwrap();
    lookup.request_digest = "a".repeat(64);
    assert!(lookup.validate().is_err());
}
