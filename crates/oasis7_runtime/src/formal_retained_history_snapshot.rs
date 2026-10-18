use super::*;
use oasis7_distfs::controlled_authority::formal::VerifiedFormalReadAnchorV1;
const MAX_ANCHORED_SNAPSHOT_BYTES: usize = 256 * 1024 * 1024;
/// A parent root is trusted only after the exact snapshot bytes are checked
/// against an independently verified, v2-profile formal anchor. The private
/// fields prevent APIs from manufacturing trust with a raw digest.
#[derive(Debug, Clone)]
pub struct VerifiedFormalRetainedHistoryParentV1 {
    root: FormalRetainedHistoryRootV1,
    effect_world: Option<oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1>,
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
            root,
            effect_world: Some(anchor.head().record().world.clone()),
            after_snapshot_root: actual_snapshot_root,
            logical_tick: Some(envelope.state.time),
            tick_consensus_archived_count: envelope.tick_consensus_archived_record_count,
            tick_consensus_records: envelope.tick_consensus_records,
        })
    }

    pub(crate) fn tick_consensus_archived_count(&self) -> u64 {
        self.tick_consensus_archived_count
    }
    pub(crate) fn tick_consensus_records(&self) -> &[crate::TickConsensusRecord] {
        &self.tick_consensus_records
    }
    pub(crate) fn effect_world(
        &self,
    ) -> Option<&oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1> {
        self.effect_world.as_ref()
    }
    pub(crate) fn logical_tick(&self) -> Option<u64> {
        self.logical_tick
    }

    pub(crate) fn root(&self) -> &FormalRetainedHistoryRootV1 {
        &self.root
    }

    pub(crate) fn after_snapshot_root(&self) -> &str {
        &self.after_snapshot_root
    }

    /// Candidate execution changes the snapshot hash. Its inherited archive
    /// descriptor must still be the exact parent descriptor until the retention
    /// plan is applied; the formal executor separately binds the new Snapshot.
    pub(crate) fn verify_inherited_descriptor(&self, snapshot_cbor: &[u8]) -> Result<(), String> {
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
    pub(crate) fn index_view(&self) -> FormalRetainedHistoryIndexViewV1 {
        FormalRetainedHistoryIndexViewV1 {
            root: self.root.clone(),
        }
    }
}

#[cfg(test)]
impl VerifiedFormalRetainedHistoryParentV1 {
    pub(crate) fn for_test(root: FormalRetainedHistoryRootV1) -> Self {
        Self {
            effect_world: None,
            after_snapshot_root: "0".repeat(64),
            logical_tick: None,
            tick_consensus_archived_count: 0,
            tick_consensus_records: Vec::new(),
            root,
        }
    }

    pub(crate) fn for_test_snapshot_at_tick(
        root: FormalRetainedHistoryRootV1,
        snapshot_cbor: &[u8],
        tick: u64,
    ) -> Self {
        Self {
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

    pub(crate) fn for_test_snapshot(
        root: FormalRetainedHistoryRootV1,
        snapshot_cbor: &[u8],
    ) -> Self {
        Self {
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
}
