use super::*;
use oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalReadAnchorV1;
const MAX_ANCHORED_SNAPSHOT_BYTES: usize = 256 * 1024 * 1024;
/// A parent root is trusted only after the exact snapshot bytes are checked
/// against an independently verified, v2-profile formal anchor. The private
/// fields prevent APIs from manufacturing trust with a raw digest.
#[derive(Debug, Clone)]
pub struct VerifiedFormalRetainedHistoryParentV1 {
    head: Option<oasis7_formal_protocol::controlled_authority::formal::FormalHeadRefV1>,
    controller_registry: Option<crate::GovernanceMainTokenControllerRegistry>,
    root: FormalRetainedHistoryRootV1,
    effect_world:
        Option<oasis7_formal_protocol::controlled_authority::formal::FormalWorldIdentityV1>,
    after_snapshot_root: String,
    logical_tick: Option<u64>,
    tick_consensus_archived_count: u64,
    tick_consensus_records: Vec<crate::TickConsensusRecord>,
}

impl VerifiedFormalRetainedHistoryParentV1 {
    pub fn verify(
        anchor: &VerifiedFormalReadAnchorV1,
        snapshot_cbor: &[u8],
    ) -> Result<Self, String> {
        if anchor.authority().definition().profile_version != 2 {
            return Err("formal retained history requires a qualified profile-v2 anchor".into());
        }
        if snapshot_cbor.is_empty() || snapshot_cbor.len() > MAX_ANCHORED_SNAPSHOT_BYTES {
            return Err("anchored snapshot byte bound".into());
        }
        let expected_snapshot_root = &anchor.head().record().after_state_root;
        let actual_snapshot_root = blake3::hash(snapshot_cbor).to_hex().to_string();
        if &actual_snapshot_root != expected_snapshot_root {
            return Err("snapshot bytes differ from qualified parent after_state_root".into());
        }
        let envelope: SnapshotRetentionEnvelopeV1 =
            serde_cbor::from_slice(snapshot_cbor).map_err(|error| error.to_string())?;
        let root = envelope
            .state
            .retained_history_root
            .ok_or("qualified parent snapshot has no retained-history descriptor")?;
        root.validate()?;
        Ok(Self {
            head: Some(anchor.head().head()),
            controller_registry: envelope.state.governance_main_token_controller_registry,
            root,
            effect_world: Some(anchor.head().record().world.clone()),
            after_snapshot_root: actual_snapshot_root,
            logical_tick: Some(envelope.state.time),
            tick_consensus_archived_count: envelope.tick_consensus_archived_record_count,
            tick_consensus_records: envelope.tick_consensus_records,
        })
    }

    pub fn tick_consensus_archived_count(&self) -> u64 {
        self.tick_consensus_archived_count
    }
    pub fn tick_consensus_records(&self) -> &[crate::TickConsensusRecord] {
        &self.tick_consensus_records
    }
    pub fn effect_world(
        &self,
    ) -> Option<&oasis7_formal_protocol::controlled_authority::formal::FormalWorldIdentityV1> {
        self.effect_world.as_ref()
    }
    /// Exact canonical position and record identity from the qualified anchor.
    /// Execution height and logical tick are not canonical commit positions.
    pub fn head(
        &self,
    ) -> Option<&oasis7_formal_protocol::controlled_authority::formal::FormalHeadRefV1> {
        self.head.as_ref()
    }
    pub fn controller_registry(&self) -> Option<&crate::GovernanceMainTokenControllerRegistry> {
        self.controller_registry.as_ref()
    }
    pub fn logical_tick(&self) -> Option<u64> {
        self.logical_tick
    }

    pub fn root(&self) -> &FormalRetainedHistoryRootV1 {
        &self.root
    }

    pub fn after_snapshot_root(&self) -> &str {
        &self.after_snapshot_root
    }

    /// Candidate execution changes the snapshot hash. Its inherited archive
    /// descriptor must still be the exact parent descriptor until the retention
    /// plan is applied; the formal executor separately binds the new Snapshot.
    pub fn verify_inherited_descriptor(&self, snapshot_cbor: &[u8]) -> Result<(), String> {
        if snapshot_cbor.is_empty() || snapshot_cbor.len() > MAX_ANCHORED_SNAPSHOT_BYTES {
            return Err("candidate snapshot byte bound".into());
        }
        let envelope: SnapshotRetentionEnvelopeV1 =
            serde_cbor::from_slice(snapshot_cbor).map_err(|error| error.to_string())?;
        let candidate_root = envelope
            .state
            .retained_history_root
            .ok_or("candidate snapshot lost the parent retained-history descriptor")?;
        candidate_root.validate()?;
        if self
            .logical_tick
            .is_some_and(|tick| envelope.state.time < tick)
        {
            return Err("candidate snapshot precedes qualified parent logical tick".into());
        }
        if candidate_root != self.root {
            return Err("candidate snapshot changed retained-history root before planning".into());
        }
        Ok(())
    }
}

impl VerifiedFormalRetainedHistoryParentV1 {
    pub fn index_view(&self) -> FormalRetainedHistoryIndexViewV1 {
        FormalRetainedHistoryIndexViewV1 {
            root: self.root.clone(),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl VerifiedFormalRetainedHistoryParentV1 {
    /// Synthetic token for execution tests only; this does not test qualification.
    pub fn for_test_governance_snapshot(
        root: FormalRetainedHistoryRootV1,
        snapshot_cbor: &[u8],
        world: oasis7_formal_protocol::controlled_authority::formal::FormalWorldIdentityV1,
        head: oasis7_formal_protocol::controlled_authority::formal::FormalHeadRefV1,
    ) -> Self {
        let envelope: SnapshotRetentionEnvelopeV1 = serde_cbor::from_slice(snapshot_cbor).unwrap();
        let mut parent = Self::for_test_snapshot_at_tick(root, snapshot_cbor, envelope.state.time);
        parent.effect_world = Some(world);
        parent.head = Some(head);
        parent.controller_registry = envelope.state.governance_main_token_controller_registry;
        parent
    }

    pub fn for_test(root: FormalRetainedHistoryRootV1) -> Self {
        Self {
            head: None,
            controller_registry: None,
            effect_world: None,
            after_snapshot_root: "0".repeat(64),
            logical_tick: None,
            tick_consensus_archived_count: 0,
            tick_consensus_records: Vec::new(),
            root,
        }
    }

    pub fn for_test_snapshot_at_tick(
        root: FormalRetainedHistoryRootV1,
        snapshot_cbor: &[u8],
        tick: u64,
    ) -> Self {
        Self {
            head: None,
            controller_registry: None,
            effect_world: None,
            after_snapshot_root: blake3::hash(snapshot_cbor).to_hex().to_string(),
            root,
            logical_tick: Some(tick),
            tick_consensus_archived_count: serde_cbor::from_slice::<SnapshotRetentionEnvelopeV1>(
                snapshot_cbor,
            )
            .map(|s| s.tick_consensus_archived_record_count)
            .unwrap_or(0),
            tick_consensus_records: serde_cbor::from_slice::<SnapshotRetentionEnvelopeV1>(
                snapshot_cbor,
            )
            .map(|s| s.tick_consensus_records)
            .unwrap_or_default(),
        }
    }

    pub fn for_test_snapshot(root: FormalRetainedHistoryRootV1, snapshot_cbor: &[u8]) -> Self {
        Self {
            head: None,
            controller_registry: None,
            effect_world: None,
            after_snapshot_root: blake3::hash(snapshot_cbor).to_hex().to_string(),
            logical_tick: None,
            tick_consensus_archived_count: serde_cbor::from_slice::<SnapshotRetentionEnvelopeV1>(
                snapshot_cbor,
            )
            .map(|s| s.tick_consensus_archived_record_count)
            .unwrap_or(0),
            tick_consensus_records: serde_cbor::from_slice::<SnapshotRetentionEnvelopeV1>(
                snapshot_cbor,
            )
            .map(|s| s.tick_consensus_records)
            .unwrap_or_default(),
            root,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotRetentionEnvelopeV1 {
    state: SnapshotRetentionStateEnvelopeV1,
    #[serde(default)]
    tick_consensus_archived_record_count: u64,
    #[serde(default)]
    tick_consensus_records: Vec<crate::TickConsensusRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotRetentionStateEnvelopeV1 {
    time: u64,
    #[serde(default)]
    retained_history_root: Option<FormalRetainedHistoryRootV1>,
    #[serde(default)]
    governance_main_token_controller_registry: Option<crate::GovernanceMainTokenControllerRegistry>,
}
