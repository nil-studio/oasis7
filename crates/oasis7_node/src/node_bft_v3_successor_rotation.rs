//! Pure wire-3 automatic successor window rotation, retention continuity,
//! 129-nonempty-commit progression, and cold-resume state verification.
use std::collections::BTreeSet;

use oasis7_distfs::protected_bft_v3::{
    BftSignerBindingV3, BftSuccessorWindowArchiveKindV1, BftSuccessorWindowArchiveManifestV1,
    BftSuccessorWindowArchiveUsageV1, BftSuccessorWindowCutV1, BftSuccessorWindowSourceV1,
    successor_source_from_binding_v3,
};
use serde::{Deserialize, Serialize};

use crate::{NodeConsensusAction, NodeError, compute_consensus_action_root};

pub const BFT_V3_WINDOW_PREPARE_OFFSET: u64 = 32;
pub const BFT_V3_WINDOW_MAX_COMMITS: u64 = 64;

fn err(reason: &'static str) -> NodeError {
    NodeError::Consensus {
        reason: reason.to_owned(),
    }
}
fn add(a: u64, b: u64) -> Result<u64, NodeError> {
    a.checked_add(b)
        .ok_or_else(|| err("wire-3 window overflow"))
}
fn digest(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftWindowHeadPinV1 {
    pub height: u64,
    pub header_id: String,
    pub state_root: String,
    pub manifest_hash: String,
}
impl BftWindowHeadPinV1 {
    fn valid(&self) -> bool {
        self.height > 0
            && digest(&self.header_id)
            && digest(&self.state_root)
            && digest(&self.manifest_hash)
    }
    fn from_cut(c: &BftSuccessorWindowCutV1) -> Self {
        Self {
            height: c.height,
            header_id: c.header_id.clone(),
            state_root: c.semantic_state_root.clone(),
            manifest_hash: c.post_manifest_hash.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPendingTransitionWindowPinV1 {
    pub source_height: u64,
    pub activation_height: u64,
    pub new_validator_set_hash: String,
    pub target_authority_epoch: u64,
}
impl BftPendingTransitionWindowPinV1 {
    fn validate(&self) -> Result<(), NodeError> {
        if self.source_height == 0
            || self.activation_height < add(self.source_height, 2)?
            || self.target_authority_epoch == 0
            || !digest(&self.new_validator_set_hash)
        {
            return Err(err("wire-3 pending transition pin invalid"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftWindowSchedulePinV1 {
    pub authority_epoch: u64,
    pub active_set_hash: String,
    pub pending_transition: Option<BftPendingTransitionWindowPinV1>,
}
impl BftWindowSchedulePinV1 {
    fn validate(&self) -> Result<(), NodeError> {
        if self.authority_epoch == 0 || !digest(&self.active_set_hash) {
            return Err(err("wire-3 schedule pin invalid"));
        }
        self.pending_transition
            .as_ref()
            .map_or(Ok(()), BftPendingTransitionWindowPinV1::validate)
    }
    fn advance(
        &self,
        h: u64,
        rep: Option<&BftPendingTransitionWindowPinV1>,
    ) -> Result<Self, NodeError> {
        let nh = add(h, 1)?;
        if let Some(pin) = &self.pending_transition {
            pin.validate()?;
            if nh == pin.activation_height && rep.is_none() {
                return Ok(Self {
                    authority_epoch: pin.target_authority_epoch,
                    active_set_hash: pin.new_validator_set_hash.clone(),
                    pending_transition: None,
                });
            }
            if nh >= pin.activation_height || rep != Some(pin) {
                return Err(err("wire-3 cross-window transition dropped or unactivated"));
            }
            return Ok(self.clone());
        }
        if let Some(np) = rep {
            np.validate()?;
            if np.source_height != h || np.target_authority_epoch != self.authority_epoch {
                return Err(err("wire-3 new transition height/epoch invalid"));
            }
            return Ok(Self {
                pending_transition: Some(np.clone()),
                ..self.clone()
            });
        }
        Ok(self.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftWindowCommittedStepV1 {
    pub head: BftWindowHeadPinV1,
    pub action_root: String,
    pub driver_install_digest: String,
    pub schedule: BftWindowSchedulePinV1,
    pub actions: Vec<NodeConsensusAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSealedSuccessorWindowV1 {
    pub cut: BftSuccessorWindowCutV1,
    pub public_manifest_digest: String,
    pub execution_manifest_digest: String,
    pub resulting_usage: BftSuccessorWindowArchiveUsageV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BftWindowRotationPhaseV1 {
    ActiveWindow,
    Prepared {
        prepared_at_height: u64,
        target_segment_sequence: u64,
    },
    Sealed(Box<BftSealedSuccessorWindowV1>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftAuthenticatedSuccessorInstallV1 {
    pub source: BftSuccessorWindowSourceV1,
    pub minimum_sequence: u64,
    pub checkpoint_digest: String,
    pub lease_generation: u64,
    pub cut_head: BftWindowHeadPinV1,
    pub successor_schedule_height: u64,
    pub successor_schedule: BftWindowSchedulePinV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorRotationMachineV1 {
    pub source: BftSuccessorWindowSourceV1,
    pub active_window_index: u64,
    pub committed_in_window: u64,
    pub lease_generation_floor: u64,
    pub minimum_checkpoint_digest: String,
    pub public_archive_root: Option<String>,
    pub execution_archive_root: Option<String>,
    pub archive_usage: BftSuccessorWindowArchiveUsageV1,
    pub last_head: Option<BftWindowHeadPinV1>,
    pub last_driver_install_digest: Option<String>,
    pub active_schedule: BftWindowSchedulePinV1,
    pub committed_action_ids: BTreeSet<u64>,
    pub phase: BftWindowRotationPhaseV1,
}

impl BftSuccessorRotationMachineV1 {
    pub fn from_source(
        source: BftSuccessorWindowSourceV1,
        initial_epoch: u64,
        initial_set_hash: String,
        lease_generation: u64,
    ) -> Result<Self, NodeError> {
        let origin = source
            .origin_digest()
            .map_err(|_| err("wire-3 source invalid"))?;
        let _ = add(source.handoff_position, 1)?;
        let active_schedule = BftWindowSchedulePinV1 {
            authority_epoch: initial_epoch,
            active_set_hash: initial_set_hash,
            pending_transition: None,
        };
        active_schedule.validate()?;
        if lease_generation == 0 {
            return Err(err("wire-3 initial lease generation invalid"));
        }
        Ok(Self {
            source,
            active_window_index: 0,
            committed_in_window: 0,
            lease_generation_floor: lease_generation,
            minimum_checkpoint_digest: origin,
            public_archive_root: None,
            execution_archive_root: None,
            archive_usage: BftSuccessorWindowArchiveUsageV1::default(),
            last_head: None,
            last_driver_install_digest: None,
            active_schedule,
            committed_action_ids: BTreeSet::new(),
            phase: BftWindowRotationPhaseV1::ActiveWindow,
        })
    }

    pub fn from_binding(binding: &BftSignerBindingV3, lease_gen: u64) -> Result<Self, NodeError> {
        let src = successor_source_from_binding_v3(binding).map_err(|_| err("wire-3 source"))?;
        let s = binding.initial_schedule();
        Self::from_source(
            src,
            s.authority_epoch(),
            s.active_set().set_hash().into(),
            lease_gen,
        )
    }

    fn offset(&self) -> Result<u64, NodeError> {
        self.active_window_index
            .checked_mul(BFT_V3_WINDOW_MAX_COMMITS)
            .ok_or_else(|| err("wire-3 overflow"))
    }
    pub fn window_start_height(&self) -> Result<u64, NodeError> {
        add(add(self.source.handoff_position, 1)?, self.offset()?)
    }
    pub fn total_nonempty_commits(&self) -> Result<u64, NodeError> {
        add(self.offset()?, self.committed_in_window)
    }
    pub fn next_expected_height(&self) -> Result<u64, NodeError> {
        add(self.window_start_height()?, self.committed_in_window)
    }
    pub fn can_propose_in_current_window(&self) -> bool {
        self.committed_in_window < BFT_V3_WINDOW_MAX_COMMITS
            && !matches!(self.phase, BftWindowRotationPhaseV1::Sealed(_))
    }
    pub fn should_prepare_successor(&self) -> bool {
        self.committed_in_window >= BFT_V3_WINDOW_PREPARE_OFFSET
            && matches!(self.phase, BftWindowRotationPhaseV1::ActiveWindow)
    }

    pub fn verify_cold_resume(
        mut self,
        expected_source: &BftSuccessorWindowSourceV1,
        durable_minimum_sequence: u64,
        durable_minimum_digest: &str,
        current_lease_generation: u64,
        durable_driver_head: Option<&BftWindowHeadPinV1>,
    ) -> Result<Self, NodeError> {
        let origin = expected_source
            .origin_digest()
            .map_err(|_| err("wire-3 source"))?;
        self.archive_usage
            .validate()
            .map_err(|_| err("wire-3 usage"))?;
        self.active_schedule.validate()?;
        let first_h = add(self.source.handoff_position, 1)?;
        let start_h = self.window_start_height()?;
        let total = self.total_nonempty_commits()?;
        let idx = self.active_window_index;
        let roots_ok = (idx == 0) == self.public_archive_root.is_none()
            && (idx == 0) == self.execution_archive_root.is_none()
            && self.public_archive_root.as_deref().is_none_or(digest)
            && self.execution_archive_root.as_deref().is_none_or(digest);
        if &self.source != expected_source
            || !roots_ok
            || current_lease_generation == 0
            || self.lease_generation_floor == 0
            || current_lease_generation < self.lease_generation_floor
            || !digest(durable_minimum_digest)
            || durable_minimum_sequence != idx
            || durable_minimum_digest != self.minimum_checkpoint_digest
            || self.committed_in_window > BFT_V3_WINDOW_MAX_COMMITS
            || self.archive_usage.segments != idx
            || (idx == 0) != (self.minimum_checkpoint_digest == origin)
            || self.committed_action_ids.contains(&0)
            || (self.committed_action_ids.len() as u64) < total
        {
            return Err(err("wire-3 cold resume pins or window invariants rejected"));
        }
        let head_ok = match (self.last_head.as_ref(), durable_driver_head) {
            (None, None) => total == 0 && self.last_driver_install_digest.is_none(),
            (Some(h), Some(drv)) => {
                total > 0
                    && h.valid()
                    && h == drv
                    && h.height == add(first_h, total - 1)?
                    && self
                        .last_driver_install_digest
                        .as_deref()
                        .is_some_and(digest)
            }
            _ => false,
        };
        if !head_ok {
            return Err(err("wire-3 cold resume durable head mismatch"));
        }
        validate_phase(&self, start_h)?;
        self.lease_generation_floor = current_lease_generation;
        Ok(self)
    }

    pub fn record_nonempty_commit(
        &mut self,
        step: BftWindowCommittedStepV1,
    ) -> Result<(), NodeError> {
        let exp_h = self.next_expected_height()?;
        if !self.can_propose_in_current_window()
            || !step.head.valid()
            || step.head.height != exp_h
            || !digest(&step.action_root)
            || !digest(&step.driver_install_digest)
            || step.schedule.authority_epoch != self.active_schedule.authority_epoch
            || step.schedule.active_set_hash != self.active_schedule.active_set_hash
            || step.actions.is_empty()
            || step.actions.iter().any(|a| {
                a.payload_cbor.is_empty() || self.committed_action_ids.contains(&a.action_id)
            })
            || compute_consensus_action_root(&step.actions)? != step.action_root
        {
            return Err(err("wire-3 committed step rejected"));
        }
        let next_sched = self
            .active_schedule
            .advance(step.head.height, step.schedule.pending_transition.as_ref())?;
        self.committed_in_window = add(self.committed_in_window, 1)?;
        let _ = self.total_nonempty_commits()?;
        self.last_head = Some(step.head);
        self.last_driver_install_digest = Some(step.driver_install_digest);
        self.active_schedule = next_sched;
        self.committed_action_ids
            .extend(step.actions.iter().map(|a| a.action_id));
        Ok(())
    }

    pub fn prepare_successor_window(&mut self) -> Result<u64, NodeError> {
        if self.committed_in_window < BFT_V3_WINDOW_PREPARE_OFFSET {
            return Err(err("wire-3 prepare refused before offset 32"));
        }
        let seg = add(self.active_window_index, 1)?;
        match &self.phase {
            BftWindowRotationPhaseV1::ActiveWindow => {
                let h = self
                    .last_head
                    .as_ref()
                    .ok_or_else(|| err("wire-3 missing head"))?
                    .height;
                self.phase = BftWindowRotationPhaseV1::Prepared {
                    prepared_at_height: h,
                    target_segment_sequence: seg,
                };
                Ok(seg)
            }
            BftWindowRotationPhaseV1::Prepared {
                target_segment_sequence,
                ..
            } => Ok(*target_segment_sequence),
            BftWindowRotationPhaseV1::Sealed(_) => Err(err("wire-3 window already sealed")),
        }
    }

    pub fn seal_window(
        &mut self,
        cut: BftSuccessorWindowCutV1,
        pub_m: &BftSuccessorWindowArchiveManifestV1,
        exec_m: &BftSuccessorWindowArchiveManifestV1,
    ) -> Result<BftSealedSuccessorWindowV1, NodeError> {
        let first_h = self.window_start_height()?;
        if self.committed_in_window != BFT_V3_WINDOW_MAX_COMMITS
            || matches!(self.phase, BftWindowRotationPhaseV1::Sealed(_))
        {
            return Err(err("wire-3 seal requires 64 unsealed commits"));
        }
        cut.validate().map_err(|_| err("wire-3 cut"))?;
        pub_m
            .validate()
            .map_err(|_| err("wire-3 public manifest"))?;
        exec_m
            .validate()
            .map_err(|_| err("wire-3 execution manifest"))?;
        let seg = add(self.active_window_index, 1)?;
        let last_h = add(first_h, BFT_V3_WINDOW_MAX_COMMITS - 1)?;
        let head = self
            .last_head
            .as_ref()
            .ok_or_else(|| err("wire-3 missing head"))?;
        if head.height != last_h
            || head != &BftWindowHeadPinV1::from_cut(&cut)
            || self.last_driver_install_digest.as_deref()
                != Some(cut.driver_install_digest.as_str())
            || pub_m.source != self.source
            || exec_m.source != self.source
            || pub_m.kind != BftSuccessorWindowArchiveKindV1::Public
            || exec_m.kind != BftSuccessorWindowArchiveKindV1::Execution
            || pub_m.segment_sequence != seg
            || exec_m.segment_sequence != seg
            || pub_m.usage.segments != seg
            || pub_m.first_height != first_h
            || pub_m.last_height != last_h
            || exec_m.first_height != first_h
            || exec_m.last_height != last_h
            || pub_m.predecessor_root != self.public_archive_root
            || exec_m.predecessor_root != self.execution_archive_root
            || pub_m.usage != exec_m.usage
            || !exec_m.entries.iter().any(|e| e.object == cut.snapshot_ref)
            || !exec_m.entries.iter().any(|e| e.object == cut.journal_ref)
        {
            return Err(err(
                "wire-3 seal cut or archive manifest continuity mismatch",
            ));
        }
        pub_m
            .usage
            .validate_after(&self.archive_usage)
            .map_err(|_| err("wire-3 usage regression"))?;
        let sealed = BftSealedSuccessorWindowV1 {
            cut,
            public_manifest_digest: pub_m.digest().map_err(|_| err("wire-3 public digest"))?,
            execution_manifest_digest: exec_m.digest().map_err(|_| err("wire-3 exec digest"))?,
            resulting_usage: pub_m.usage,
        };
        self.phase = BftWindowRotationPhaseV1::Sealed(Box::new(sealed.clone()));
        Ok(sealed)
    }

    pub fn adopt_successor_window(
        &mut self,
        inst: &BftAuthenticatedSuccessorInstallV1,
    ) -> Result<(), NodeError> {
        let seg = add(self.active_window_index, 1)?;
        let BftWindowRotationPhaseV1::Sealed(sealed) = &self.phase else {
            return Err(err("wire-3 adopt requires sealed window"));
        };
        let next_start = add(sealed.cut.height, 1)?;
        if inst.source != self.source
            || inst.minimum_sequence != seg
            || !digest(&inst.checkpoint_digest)
            || inst.checkpoint_digest == self.minimum_checkpoint_digest
            || inst.lease_generation < self.lease_generation_floor
            || inst.cut_head != BftWindowHeadPinV1::from_cut(&sealed.cut)
            || inst.successor_schedule_height != next_start
            || inst.successor_schedule != self.active_schedule
        {
            return Err(err(
                "wire-3 successor install unauthenticated or mismatched",
            ));
        }
        let s = (**sealed).clone();
        self.active_window_index = seg;
        self.committed_in_window = 0;
        self.lease_generation_floor = inst.lease_generation;
        self.minimum_checkpoint_digest = inst.checkpoint_digest.clone();
        self.public_archive_root = Some(s.public_manifest_digest);
        self.execution_archive_root = Some(s.execution_manifest_digest);
        self.archive_usage = s.resulting_usage;
        self.phase = BftWindowRotationPhaseV1::ActiveWindow;
        let _ = self.window_start_height()?;
        Ok(())
    }

    pub fn retain_uncommitted_pending(
        &self,
        pending: &[NodeConsensusAction],
    ) -> Result<Vec<NodeConsensusAction>, NodeError> {
        let mut seen = BTreeSet::new();
        pending
            .iter()
            .filter(|a| !self.committed_action_ids.contains(&a.action_id))
            .map(|a| {
                a.validate().map_err(|_| err("wire-3 pending invalid"))?;
                if a.payload_cbor.is_empty() || !seen.insert(a.action_id) {
                    return Err(err("wire-3 pending empty or duplicate"));
                }
                Ok(a.clone())
            })
            .collect()
    }
}

fn validate_phase(m: &BftSuccessorRotationMachineV1, start_h: u64) -> Result<(), NodeError> {
    let seg = add(m.active_window_index, 1)?;
    let max_h = add(start_h, BFT_V3_WINDOW_MAX_COMMITS - 1)?;
    match &m.phase {
        BftWindowRotationPhaseV1::ActiveWindow => Ok(()),
        BftWindowRotationPhaseV1::Prepared {
            prepared_at_height,
            target_segment_sequence,
        } => {
            let min_h = add(start_h, BFT_V3_WINDOW_PREPARE_OFFSET - 1)?;
            if m.committed_in_window < BFT_V3_WINDOW_PREPARE_OFFSET
                || *target_segment_sequence != seg
                || !(min_h..=max_h).contains(prepared_at_height)
            {
                return Err(err("wire-3 cold resume prepared phase invalid"));
            }
            Ok(())
        }
        BftWindowRotationPhaseV1::Sealed(s) => {
            s.cut.validate().map_err(|_| err("wire-3 sealed cut"))?;
            s.resulting_usage
                .validate_after(&m.archive_usage)
                .map_err(|_| err("wire-3 sealed usage"))?;
            if m.committed_in_window != BFT_V3_WINDOW_MAX_COMMITS
                || s.cut.height != max_h
                || m.last_head.as_ref() != Some(&BftWindowHeadPinV1::from_cut(&s.cut))
                || m.last_driver_install_digest.as_deref()
                    != Some(s.cut.driver_install_digest.as_str())
                || !digest(&s.public_manifest_digest)
                || !digest(&s.execution_manifest_digest)
            {
                return Err(err("wire-3 cold resume sealed phase mismatch"));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use oasis7_distfs::protected_bft_v3::{
        BftSuccessorWindowArchiveEntryV1, BftSuccessorWindowArchiveRefV1,
    };
    use oasis7_proto::distributed_bft::v2::BftScopeV2;

    use super::*;

    type Artifacts = (
        BftSuccessorWindowCutV1,
        BftSuccessorWindowArchiveManifestV1,
        BftSuccessorWindowArchiveManifestV1,
        BftAuthenticatedSuccessorInstallV1,
    );

    fn h(t: &str, i: u64) -> String {
        blake3::hash(format!("{t}:{i}").as_bytes())
            .to_hex()
            .to_string()
    }
    fn entry(id: &str, obj: &str, seg: u64, bytes: u64) -> BftSuccessorWindowArchiveEntryV1 {
        let object = BftSuccessorWindowArchiveRefV1 {
            digest: h(obj, seg),
            bytes,
        };
        BftSuccessorWindowArchiveEntryV1 {
            identity_digest: h(id, seg),
            object,
        }
    }
    fn source() -> BftSuccessorWindowSourceV1 {
        let scope = BftScopeV2 {
            world_id: "w".into(),
            chain_id: "c".into(),
            genesis_hash: h("g", 0),
        };
        BftSuccessorWindowSourceV1 {
            scope,
            binding_digest: h("b", 0),
            handoff_position: 100,
            handoff_record_id: h("hr", 0),
            handoff_snapshot_root: h("hs", 0),
            target_digest: h("t", 0),
            activation_digest: h("a", 0),
            material_identity: h("m", 0),
        }
    }
    fn act(id: u64) -> NodeConsensusAction {
        NodeConsensusAction::from_payload(id, "p1", format!("secret-op-{id}").into_bytes()).unwrap()
    }
    fn mk_step(
        height: u64,
        id: u64,
        ep: u64,
        set: &str,
        pend: Option<BftPendingTransitionWindowPinV1>,
    ) -> BftWindowCommittedStepV1 {
        let actions = vec![act(id)];
        let action_root = compute_consensus_action_root(&actions).unwrap();
        let head = BftWindowHeadPinV1 {
            height,
            header_id: h("hdr", height),
            state_root: h("st", height),
            manifest_hash: h("mf", height),
        };
        let schedule = BftWindowSchedulePinV1 {
            authority_epoch: ep,
            active_set_hash: set.into(),
            pending_transition: pend,
        };
        BftWindowCommittedStepV1 {
            head,
            action_root,
            driver_install_digest: h("drv", height),
            schedule,
            actions,
        }
    }
    fn commit_range(
        m: &mut BftSuccessorRotationMachineV1,
        ids: std::ops::RangeInclusive<u64>,
        set: &str,
    ) {
        for i in ids {
            m.record_nonempty_commit(mk_step(100 + i, i, 1, set, None))
                .unwrap();
        }
    }
    fn artifacts(m: &BftSuccessorRotationMachineV1, seg: u64, lease: u64) -> Artifacts {
        let head = m.last_head.clone().unwrap();
        let ea = entry("ea", "snap", seg, 4096);
        let eb = entry("eb", "jrnl", seg, 2048);
        let cut = BftSuccessorWindowCutV1 {
            height: head.height,
            header_id: head.header_id.clone(),
            certificate_digest: h("qc", head.height),
            execution_commitment_digest: h("ec", head.height),
            snapshot_ref: ea.object.clone(),
            journal_ref: eb.object.clone(),
            semantic_state_root: head.state_root.clone(),
            post_manifest_hash: head.manifest_hash.clone(),
            business_result_root: h("bz", head.height),
            driver_record_digest: h("dr", head.height),
            driver_install_digest: m.last_driver_install_digest.clone().unwrap(),
        };
        let delta = BftSuccessorWindowArchiveUsageV1 {
            segments: 1,
            objects: 2,
            total_bytes: 6144,
        };
        let usage = m.archive_usage.checked_add(delta).unwrap();
        let mut ee = vec![ea, eb];
        ee.sort_by(|a, b| a.identity_digest.cmp(&b.identity_digest));
        let pm = BftSuccessorWindowArchiveManifestV1 {
            public_index: None,
            schema_version: 1,
            source: m.source.clone(),
            kind: BftSuccessorWindowArchiveKindV1::Public,
            segment_sequence: seg,
            predecessor_root: m.public_archive_root.clone(),
            first_height: m.window_start_height().unwrap(),
            last_height: head.height,
            entries: vec![entry("pub", "po", seg, 1024)],
            usage,
        };
        let em = BftSuccessorWindowArchiveManifestV1 {
            kind: BftSuccessorWindowArchiveKindV1::Execution,
            predecessor_root: m.execution_archive_root.clone(),
            entries: ee,
            ..pm.clone()
        };
        let inst = BftAuthenticatedSuccessorInstallV1 {
            source: m.source.clone(),
            minimum_sequence: seg,
            checkpoint_digest: h("ck", seg),
            lease_generation: lease,
            successor_schedule_height: head.height + 1,
            cut_head: head,
            successor_schedule: m.active_schedule.clone(),
        };
        (cut, pm, em, inst)
    }

    #[test]
    fn progresses_129_nonempty_commits_across_two_automatic_rotations() {
        let src = source();
        let s1 = h("set", 1);
        let mut m =
            BftSuccessorRotationMachineV1::from_source(src.clone(), 1, s1.clone(), 3).unwrap();
        let queued: Vec<_> = (1..=132).map(act).collect();
        for i in 1..=129_u64 {
            m.record_nonempty_commit(mk_step(src.handoff_position + i, i, 1, &s1, None))
                .unwrap();
            if m.should_prepare_successor() {
                assert_eq!(
                    m.prepare_successor_window().unwrap(),
                    m.active_window_index + 1
                );
            }
            if m.committed_in_window == BFT_V3_WINDOW_MAX_COMMITS {
                assert!(!m.can_propose_in_current_window());
                let seg = m.active_window_index + 1;
                let (cut, pm, em, inst) = artifacts(&m, seg, 3 + seg);
                m.seal_window(cut, &pm, &em).unwrap();
                m.adopt_successor_window(&inst).unwrap();
            }
        }
        assert_eq!(m.active_window_index, 2);
        assert_eq!(m.total_nonempty_commits().unwrap(), 129);
        assert_eq!(m.window_start_height().unwrap(), 229);
        assert_eq!(m.committed_in_window, 1);
        assert_eq!(m.archive_usage.segments, 2);
        assert_eq!(m.archive_usage.total_bytes, 12_288);
        assert_eq!(m.retain_uncommitted_pending(&queued).unwrap().len(), 3);
    }

    #[test]
    fn refuses_invalid_rotations_preserves_h_plus_two_and_verifies_cold_resume() {
        let src = source();
        let s1 = h("set", 1);
        let s2 = h("set", 2);
        let mut m =
            BftSuccessorRotationMachineV1::from_source(src.clone(), 1, s1.clone(), 5).unwrap();
        assert!(
            m.record_nonempty_commit(mk_step(102, 1, 1, &s1, None))
                .is_err()
        );
        let mut empty = mk_step(101, 1, 1, &s1, None);
        empty.actions.clear();
        empty.action_root = compute_consensus_action_root(&empty.actions).unwrap();
        assert!(m.record_nonempty_commit(empty).is_err());
        commit_range(&mut m, 1..=31, &s1);
        assert!(m.prepare_successor_window().is_err());
        commit_range(&mut m, 32..=32, &s1);
        assert_eq!(m.prepare_successor_window().unwrap(), 1);
        let (c32, p32, e32, _) = artifacts(&m, 1, 5);
        assert!(m.seal_window(c32, &p32, &e32).is_err());
        commit_range(&mut m, 33..=63, &s1);
        let mut pin = BftPendingTransitionWindowPinV1 {
            source_height: 164,
            activation_height: 166,
            new_validator_set_hash: s2.clone(),
            target_authority_epoch: 2,
        };
        let invalid_epoch = mk_step(164, 64, 1, &s1, Some(pin.clone()));
        assert!(m.record_nonempty_commit(invalid_epoch).is_err());
        pin.target_authority_epoch = 1;
        m.record_nonempty_commit(mk_step(164, 64, 1, &s1, Some(pin)))
            .unwrap();
        assert!(
            m.record_nonempty_commit(mk_step(165, 65, 1, &s1, None))
                .is_err()
        );
        let (cut, pm, em, inst) = artifacts(&m, 1, 8);
        m.seal_window(cut, &pm, &em).unwrap();
        let mut tampered = m.clone();
        let BftWindowRotationPhaseV1::Sealed(s) = &mut tampered.phase else {
            unreachable!()
        };
        s.cut.header_id = h("forged sealed head", 164);
        let minimum = &m.minimum_checkpoint_digest;
        let cold = tampered.verify_cold_resume(&src, 0, minimum, 5, m.last_head.as_ref());
        assert!(cold.is_err());
        let mut stale = inst.clone();
        stale.lease_generation = 4;
        assert!(m.adopt_successor_window(&stale).is_err());
        let mut forged = inst.clone();
        forged.cut_head.header_id = h("forged", 9);
        assert!(m.adopt_successor_window(&forged).is_err());
        let mut dropped = inst.clone();
        dropped.successor_schedule.pending_transition = None;
        assert!(m.adopt_successor_window(&dropped).is_err());
        m.adopt_successor_window(&inst).unwrap();
        commit_range(&mut m, 65..=65, &s1);
        assert_eq!(m.active_schedule.authority_epoch, 1);
        assert_eq!(m.active_schedule.active_set_hash, s2);
        let saved = m.clone();
        let d_min = saved.minimum_checkpoint_digest.clone();
        let d_head = saved.last_head.clone();
        assert!(
            saved
                .clone()
                .verify_cold_resume(&src, 1, &d_min, 7, d_head.as_ref())
                .is_err()
        );
        let mut resumed = saved
            .verify_cold_resume(&src, 1, &d_min, 9, d_head.as_ref())
            .unwrap();
        resumed
            .record_nonempty_commit(mk_step(166, 66, 1, &s2, None))
            .unwrap();
        assert_eq!(resumed.total_nonempty_commits().unwrap(), 66);
    }
}
