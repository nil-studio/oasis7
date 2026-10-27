//! Production Node/native TCP/provider batching with a deterministic test hook.
//! This test does not execute its 1025 actions in World or attest durability.
use super::*;
#[test]
fn wire3_formal_read_node_test_hook_splits_1025_prequeued_actions_without_loss() {
    thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(batch_case)
        .unwrap()
        .join()
        .unwrap();
}
fn batch_case() {
    let h = qualified_handoff_v3_with_count_and_setup(1, |_| {});
    let root = Scratch::new();
    let store = cas(root.path());
    let actual = driver(&h, root.path(), &store);
    let binding = signer_binding(&actual);
    drop(actual);
    let state = Arc::new(Mutex::new(Cursor {
        binding_digest: binding.digest().unwrap(),
        schedule: binding.initial_schedule().clone(),
        retained: std::collections::BTreeMap::new(),
        history: Vec::new(),
    }));
    let snapshot = Arc::new(Mutex::new(BftSignerBackendSnapshotV3 {
        state: BftSignerStateV3::from_handoff(&binding).unwrap(),
        lease_generation: 1,
        lease_token_digest: String::new(),
        protected_time_floor_ms: 100,
    }));
    let (address, server) = tcp(service(&binding, snapshot.clone()));
    let provider = Arc::new(
        RemoteBftSignerProviderV3::new(
            address,
            Duration::from_secs(10),
            binding.clone(),
            fixtures::key(117),
            BftSignerTrustV3 {
                schema_version: 3,
                provider_public_key: hex::encode(fixtures::key(116).verifying_key().to_bytes()),
                binding_digest: binding.digest().unwrap(),
            },
        )
        .unwrap(),
    );
    let config = NodeConfig::new(
        validator_id(0),
        binding.scope.world_id.clone(),
        NodeRole::Sequencer,
    )
    .unwrap()
    .with_tick_interval(Duration::from_millis(200))
    .unwrap()
    .with_replication(NodeReplicationConfig::new(root.path().join("replication")).unwrap());
    let mut node = NodeRuntime::new(config)
        .with_replication_network_consensus_enabled(false)
        .with_execution_hook(TestHook(state.clone()))
        .with_tendermint_v3(binding.clone(), provider)
        .unwrap();
    for id in 1..=1025 {
        node.submit_consensus_action_payload(10_000 + id, payload("batch-test-hook-input"))
            .unwrap();
    }
    node.start().unwrap();
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut seen = std::collections::BTreeSet::new();
    let mut heights = std::collections::BTreeSet::new();
    while seen.len() < 1025 {
        for batch in node.drain_committed_action_batches() {
            if batch.actions.is_empty() {
                continue;
            }
            assert!(
                batch.actions.len()
                    <= oasis7_consensus::formal_bft_payload_v3::BFT_PRIVATE_ACTIONS_MAX_COUNT_V3
            );
            let bytes = serde_cbor::to_vec(&batch.actions).unwrap();
            assert!(bytes.len() <= oasis7_proto::distributed_bft::BFT_MAX_ACTIONS_CBOR_BYTES_V1);
            assert_eq!(
                oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3(&bytes).unwrap(),
                batch.actions
            );
            heights.insert(batch.height);
            for action in batch.actions {
                assert!((10001..=11025).contains(&action.action_id));
                assert!(seen.insert(action.action_id), "duplicate action completion");
            }
        }
        let status = node.bft_v3_status().unwrap();
        assert!(status.running, "batch Node stopped: {status:?}");
        assert!(
            Instant::now() < deadline,
            "did not deliver all queued actions: {}",
            seen.len()
        );
        thread::sleep(Duration::from_millis(20));
    }
    node.stop().unwrap();
    drop(node);
    server.join().unwrap().unwrap();
    assert!(heights.len() >= 2);
    assert_eq!(seen.len(), 1025);
    let cursor = state.lock().unwrap();
    assert!(cursor.history.len() >= 2);
    assert_eq!(
        cursor
            .history
            .iter()
            .map(|entry| {
                let p = cursor
                    .retained
                    .get(&entry.execution_commitment.height)
                    .unwrap();
                oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3(&p.actions_cbor)
                    .unwrap()
                    .len()
            })
            .sum::<usize>(),
        1025
    );
    let protected = snapshot.lock().unwrap();
    assert_eq!(protected.state.history, cursor.history);
    assert!(protected.state.pending_driver_apply.is_none());
    assert_eq!(
        protected.state.applied_installs.len(),
        protected.state.history.len()
    );
}

struct Cursor {
    binding_digest: String,
    schedule: BftHeaderHeightScheduleV3,
    retained: std::collections::BTreeMap<u64, NodeBftPrivateCandidateV3>,
    history: Vec<FormalBftCertifiedHeaderV3>,
}
struct TestHook(Arc<Mutex<Cursor>>);
impl TestHook {
    fn check(
        s: &Cursor,
        context: &NodeBftExecutionContextV3,
        p: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        if context != &NodeBftExecutionContextV3::from_schedule(&s.schedule) {
            return Err("test cursor context mismatch".into());
        }
        let bound = p.public_value.validate_binding()?;
        let decoded =
            oasis7_consensus::formal_bft_payload_v3::ValidatedBftPrivatePayloadV3::decode(
                bound,
                &p.actions_cbor,
            )?;
        if decoded.bound_header().commitment().height() != context.height {
            return Err("test cursor height mismatch".into());
        }
        Ok(())
    }
}
impl NodeExecutionHook for TestHook {
    fn on_commit(
        &mut self,
        _: oasis7_node::NodeExecutionCommitContext,
    ) -> Result<oasis7_node::NodeExecutionCommitResult, String> {
        Err("test hook only wire3".into())
    }
    fn configure_bft_v3_execution(
        &mut self,
        b: &BftSignerBindingV3,
    ) -> Result<Option<NodeBftExecutionHeadV3>, String> {
        if b.digest()? != self.0.lock().unwrap().binding_digest {
            return Err("test binding mismatch".into());
        }
        Ok(None)
    }
    fn current_bft_v3_execution_context(
        &mut self,
        schedule: &BftHeaderHeightScheduleV3,
    ) -> Result<NodeBftExecutionContextV3, String> {
        let s = self.0.lock().unwrap();
        if NodeBftExecutionContextV3::from_schedule(schedule)
            != NodeBftExecutionContextV3::from_schedule(&s.schedule)
        {
            return Err("test schedule mismatch".into());
        }
        Ok(NodeBftExecutionContextV3::from_schedule(&s.schedule))
    }
    fn validate_bft_v3_action_payload(&mut self, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn validate_bft_v3_action_batch(
        &mut self,
        _: &BftHeaderHeightScheduleV3,
        _: &[oasis7_node::NodeConsensusAction],
    ) -> Result<(), String> {
        Ok(())
    }
    fn build_bft_v3_candidate(
        &mut self,
        c: NodeBftBuildContextV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        use oasis7_consensus::tendermint_v3::FormalBftExecutionCommitmentV1;
        use oasis7_proto::distributed_bft::v3::{BftBlockHeaderV3, actions_cbor_digest_v3};
        let s = self.0.lock().unwrap();
        let actions_cbor = serde_cbor::to_vec(&c.actions).map_err(|e| e.to_string())?;
        // Independent production decoder is the oracle: old Worker truncation
        // at 4096 passes 1025 here and must fail before retention or signing.
        let actions =
            oasis7_consensus::formal_bft_payload_v3::decode_private_actions_v3(&actions_cbor)?;
        let root =
            oasis7_node::compute_consensus_action_root(&actions).map_err(|e| e.to_string())?;
        let x = &c.execution;
        let commitment = FormalBftExecutionCommitmentV1 {
            domain: oasis7_formal_protocol::bft::BFT_FORMAL_EXECUTION_DOMAIN_V1.into(),
            schema_version: 1,
            profile: "bft".into(),
            profile_version: 1,
            activation_digest: x.activation_digest.clone(),
            scope: x.scope.clone(),
            height: x.height,
            parent_commit_ref: x.parent_commit_ref.clone(),
            parent_tick: x.parent_tick,
            parent_semantic_state_root: x.parent_state_root.clone(),
            authority_epoch: x.authority_epoch,
            active_validator_set_hash: x.active_validator_set_hash.clone(),
            schedule_policy_hash: x.schedule_policy_hash.clone(),
            manifest_hash: x.manifest_hash.clone(),
            post_manifest_hash: x.manifest_hash.clone(),
            action_root: root.clone(),
            event_root: root.clone(),
            execution_state_root: root.clone(),
            state_root: root.clone(),
            projection_root: root.clone(),
            result_root: root,
            transition: None,
            transition_ref: None,
        };
        let p = NodeBftPrivateCandidateV3 {
            public_value: BftHeaderValueV3 {
                header: BftBlockHeaderV3 {
                    protocol_version: 3,
                    execution_commitment_hash: commitment.digest()?,
                    actions_cbor_digest: actions_cbor_digest_v3(&actions_cbor)?,
                },
                execution_commitment: commitment,
            },
            actions_cbor,
        };
        Self::check(&s, &c.execution, &p)?;
        Ok(p)
    }
    fn preview_bft_v3_candidate(
        &mut self,
        c: &NodeBftExecutionContextV3,
        p: &NodeBftPrivateCandidateV3,
    ) -> Result<(), String> {
        Self::check(&self.0.lock().unwrap(), c, p)
    }
    fn retain_bft_v3_private_candidate(
        &mut self,
        c: &NodeBftExecutionContextV3,
        p: &NodeBftPrivateCandidateV3,
    ) -> Result<String, String> {
        let mut s = self.0.lock().unwrap();
        Self::check(&s, c, p)?;
        if s.retained.get(&c.height).is_some_and(|old| old != p) {
            return Err("test original candidate replaced".into());
        }
        s.retained.insert(c.height, p.clone());
        p.public_value.header.block_id()
    }
    fn pending_bft_v3_private_candidate(
        &mut self,
        c: &NodeBftExecutionContextV3,
    ) -> Result<Option<NodeBftPrivateCandidateV3>, String> {
        Ok(self.0.lock().unwrap().retained.get(&c.height).cloned())
    }
    fn load_bft_v3_private_candidate(
        &mut self,
        v: &BftHeaderValueV3,
    ) -> Result<NodeBftPrivateCandidateV3, String> {
        self.0
            .lock()
            .unwrap()
            .retained
            .values()
            .find(|p| &p.public_value == v)
            .cloned()
            .ok_or("test original candidate missing".into())
    }
    fn load_bft_v3_commit_history_page(
        &mut self,
        _: &BftSignerBindingV3,
        after: u64,
        max: usize,
    ) -> Result<Vec<FormalBftCertifiedHeaderV3>, String> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .history
            .iter()
            .filter(|e| e.execution_commitment.height > after)
            .take(max)
            .cloned()
            .collect())
    }
    fn commit_bft_v3_certificate(
        &mut self,
        token: VerifiedBftHeaderCertificateV3,
    ) -> Result<NodeBftExecutionCommitResultV3, String> {
        let mut s = self.0.lock().unwrap();
        if NodeBftExecutionContextV3::from_schedule(token.schedule())
            != NodeBftExecutionContextV3::from_schedule(&s.schedule)
        {
            return Err("test commit schedule mismatch".into());
        }
        let entry = token.certified_header();
        let p = s
            .retained
            .get(&entry.execution_commitment.height)
            .ok_or("test commit input missing")?;
        if p.public_value.header != entry.header
            || p.public_value.execution_commitment != entry.execution_commitment
        {
            return Err("test commit input mismatch".into());
        }
        let c = &entry.execution_commitment;
        let result = NodeBftExecutionCommitResultV3 {
            height: c.height,
            header_id: token.block_id().into(),
            execution_commitment_hash: c.digest()?,
            execution_state_root: c.execution_state_root.clone(),
            state_root: c.state_root.clone(),
            post_manifest_hash: c.post_manifest_hash.clone(),
            driver_install_digest: c.digest()?,
        };
        s.history.push(entry.clone());
        s.schedule = token.next_height_schedule().clone();
        Ok(result)
    }
}
