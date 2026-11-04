//! Bounded typed proof requests serialized with the wire-3 worker checkpoint.
//! These proofs convey neither controller authorization nor voting permission.
use super::{error, worker::Worker};
use crate::{NodeError, NodeRuntime};
use oasis7_agent_api::world_action::{
    FINALITY_KEY_ROTATION_MAX_BYTES_V1, FinalityKeyRotationBodyV1, FinalityKeyRotationProofV1,
    FinalityKeyRotationRoleV1,
};
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};
type Proofs = (FinalityKeyRotationProofV1, FinalityKeyRotationProofV1);
const MAX_PENDING: usize = 4;
const MAX_TIMEOUT: Duration = Duration::from_secs(120);
#[derive(Default)]
pub(crate) struct RotationQueue {
    inner: Mutex<QueueState>,
    outstanding: AtomicUsize,
}
#[derive(Default)]
struct QueueState {
    accepting: bool,
    jobs: VecDeque<Job>,
}
struct Job {
    body: FinalityKeyRotationBodyV1,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    reply: Option<mpsc::Sender<Result<Proofs, NodeError>>>,
    owner: Arc<RotationQueue>,
}
impl Job {
    fn live(&self) -> bool {
        !self.cancelled.load(Ordering::Acquire)
            && Instant::now() < self.deadline
            && self.owner.inner.lock().is_ok_and(|state| state.accepting)
    }
    fn reject_conflicting_archive(
        &mut self,
        archive: Option<&oasis7_distfs::protected_bft_v3::BftFinalityRotationProofArchiveV3>,
    ) -> bool {
        if archive.is_some_and(|archive| {
            archive.records.iter().any(|record| {
                record.body.rotation_id == self.body.rotation_id && record.body != self.body
            })
        }) {
            self.finish(Err(error(
                "wire-3 rotation ID already binds a different original body",
            )));
            true
        } else {
            false
        }
    }
    fn finish(&mut self, result: Result<Proofs, NodeError>) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(result);
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.finish(Err(error("wire-3 rotation request stopped or expired")));
        self.owner.outstanding.fetch_sub(1, Ordering::AcqRel);
    }
}
impl RotationQueue {
    pub(super) fn open(&self) -> Result<(), NodeError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| error("wire-3 rotation queue poisoned"))?;
        if self.outstanding.load(Ordering::Acquire) != 0 || !state.jobs.is_empty() {
            return Err(error("wire-3 rotation requests from prior worker remain"));
        }
        state.accepting = true;
        Ok(())
    }
    pub(crate) fn close(&self) {
        let jobs = match self.inner.lock() {
            Ok(mut state) => {
                state.accepting = false;
                std::mem::take(&mut state.jobs)
            }
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.accepting = false;
                std::mem::take(&mut state.jobs)
            }
        };
        // Job destructors may reference the queue; never drop them under its lock.
        drop(jobs);
    }
    fn enqueue(
        self: &Arc<Self>,
        body: FinalityKeyRotationBodyV1,
        deadline: Instant,
        cancelled: Arc<AtomicBool>,
        reply: mpsc::Sender<Result<Proofs, NodeError>>,
    ) -> Result<(), NodeError> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| error("wire-3 rotation queue poisoned"))?;
        if !state.accepting
            || Instant::now() >= deadline
            || self.outstanding.load(Ordering::Acquire) >= MAX_PENDING
        {
            return Err(error(
                "wire-3 rotation request unavailable or capacity exceeded",
            ));
        }
        self.outstanding.fetch_add(1, Ordering::AcqRel);
        state.jobs.push_back(Job {
            body,
            deadline,
            cancelled,
            reply: Some(reply),
            owner: self.clone(),
        });
        Ok(())
    }
}
impl NodeRuntime {
    /// Obtain only the two domain-separated proofs for one immutable decision.
    /// Callers must independently obtain full controller authorization afterward.
    pub fn request_bft_v3_finality_key_rotation_proofs(
        &self,
        body: FinalityKeyRotationBodyV1,
        timeout: Duration,
    ) -> Result<Proofs, NodeError> {
        let started = Instant::now();
        if timeout.is_zero() || timeout > MAX_TIMEOUT {
            return Err(error("wire-3 rotation timeout bounds"));
        }
        let deadline = started
            .checked_add(timeout)
            .ok_or_else(|| error("wire-3 rotation deadline overflow"))?;
        body.validate().map_err(error)?;
        if serde_cbor::to_vec(&body)
            .map_err(|_| error("wire-3 rotation body encoding"))?
            .len()
            > FINALITY_KEY_ROTATION_MAX_BYTES_V1
        {
            return Err(error("wire-3 rotation body capacity"));
        }
        let selected = self
            .bft_v3_config
            .as_ref()
            .ok_or_else(|| error("wire-3 profile not selected"))?;
        if !selected.membership_enabled
            || !self.running.load(Ordering::Acquire)
            || selected.capacity_exhausted.load(Ordering::Acquire)
        {
            return Err(error("wire-3 rotation requires running membership profile"));
        }
        let (reply, response) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        selected
            .rotation_requests
            .enqueue(body, deadline, cancelled.clone(), reply)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let result = response
            .recv_timeout(remaining)
            .map_err(|_| error("wire-3 rotation request stopped or expired"));
        cancelled.store(true, Ordering::Release);
        if Instant::now() >= deadline {
            return Err(error("wire-3 rotation request expired"));
        }
        result?
    }
}
impl Worker {
    pub(super) fn process_rotation_request(&mut self) -> Result<(), NodeError> {
        let job = self
            .selected
            .rotation_requests
            .inner
            .lock()
            .map_err(|_| error("wire-3 rotation queue poisoned"))?
            .jobs
            .pop_front();
        let Some(mut job) = job else {
            return Ok(());
        };
        if !job.live() {
            return Ok(());
        }
        // A locally known conflict is a definite input rejection, not an
        // uncertain provider operation. Reject before any signing RPC and keep
        // the consensus worker alive for the original request and later work.
        if job.reject_conflicting_archive(self.checkpoint.state.finality_rotation_proofs.as_ref()) {
            return Ok(());
        }
        let archived_complete = self
            .checkpoint
            .state
            .finality_rotation_proofs
            .as_ref()
            .is_some_and(|archive| {
                [
                    FinalityKeyRotationRoleV1::OldKeyAuthorization,
                    FinalityKeyRotationRoleV1::NewKeyPossession,
                ]
                .iter()
                .all(|role| {
                    archive
                        .records
                        .iter()
                        .any(|record| record.body == job.body && record.proof.role == *role)
                })
            });
        // Both original results can be recovered at a later parent. A partial
        // archive never authorizes completing a fresh role after that parent.
        if !archived_complete && let Err(reason) = self.validate_rotation_context(&job.body) {
            job.finish(Err(reason));
            return Ok(());
        }
        let old = match self.rotation_proof(&job, FinalityKeyRotationRoleV1::OldKeyAuthorization) {
            Ok(proof) => proof,
            Err(reason) => {
                job.finish(Err(error("wire-3 protected rotation proof failed")));
                return Err(reason);
            }
        };
        if !job.live() {
            return Ok(());
        }
        let new = match self.rotation_proof(&job, FinalityKeyRotationRoleV1::NewKeyPossession) {
            Ok(proof) => proof,
            Err(reason) => {
                job.finish(Err(error("wire-3 protected rotation proof failed")));
                return Err(reason);
            }
        };
        if job.live() {
            job.finish(Ok((old, new)));
        }
        Ok(())
    }
    fn validate_rotation_context(&self, body: &FinalityKeyRotationBodyV1) -> Result<(), NodeError> {
        let a = &body.authorization;
        let binding = &self.selected.binding;
        let scope = self.schedule.scope();
        let active = self
            .schedule
            .active_set()
            .validator(&binding.local_validator_id);
        let epoch_height = body
            .target_epoch
            .checked_mul(self.schedule.policy().epoch_length_ticks);
        if self.schedule.pending_transition().is_some()
            || body.node_id != binding.local_validator_id
            || a.world_id != scope.world_id
            || a.chain_id != scope.chain_id
            || a.genesis_digest != scope.genesis_hash
            || a.parent_position.checked_add(1) != Some(self.schedule.height())
            || a.parent_record_id != self.schedule.parent_commit_ref()
            || a.parent_state_root != self.schedule.parent_state_root()
            || body.decision_height != self.schedule.height()
            || epoch_height != Some(body.activation_height)
            || active.is_none_or(|v| v.public_key_hex != body.old_public_key_hex)
            || [&body.old_public_key_hex, &body.new_public_key_hex]
                .iter()
                .any(|public| {
                    !binding.provisioned_signing_keys.iter().any(|key| {
                        key.validator_id == body.node_id && &key.public_key_hex == *public
                    })
                })
        {
            return Err(error(
                "wire-3 rotation request differs from current installed context",
            ));
        }
        Ok(())
    }
    fn rotation_proof(
        &mut self,
        job: &Job,
        role: FinalityKeyRotationRoleV1,
    ) -> Result<FinalityKeyRotationProofV1, NodeError> {
        let original = self.checkpoint.state.clone();
        let (proof, checkpoint) = self
            .signer()?
            .sign_finality_rotation_proof(
                original.revision,
                &original.state_digest,
                &job.body,
                role,
            )
            .map_err(|_| error("wire-3 rotation provider outcome unknown or rejected"))?;
        let mut unchanged = checkpoint.state.clone();
        unchanged.finality_rotation_proofs = original.finality_rotation_proofs.clone();
        unchanged.revision = original.revision;
        unchanged.state_digest = original.state_digest.clone();
        if unchanged != original
            || checkpoint.state.revision < original.revision
            || checkpoint.state.revision
                > original
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| error("wire-3 rotation revision overflow"))?
        {
            return Err(error(
                "wire-3 rotation proof changed consensus schedule or WAL",
            ));
        }
        proof.validate_for(&job.body, role).map_err(error)?;
        self.checkpoint = checkpoint;
        Ok(proof)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn body() -> FinalityKeyRotationBodyV1 {
        serde_json::from_value(serde_json::json!({
            "schema_version":1,"authorization":{"schema_version":1,"world_id":"w","chain_id":"c",
            "genesis_digest":"0".repeat(64),"parent_position":1,"parent_record_id":"1".repeat(64),
            "parent_state_root":"2".repeat(64),"policy_digest":"3".repeat(64),"controller_account_id":"a",
            "nonce":1,"valid_from_position":2,"valid_through_position":3,"signatures":{}},
            "rotation_id":"r","candidate_id":"c","node_id":"n","expected_generation":1,
            "old_public_key_hex":"4".repeat(64),"new_public_key_hex":"5".repeat(64),
            "decision_height":2,"activation_height":4,"target_epoch":2
        })).unwrap()
    }
    #[test]
    fn typed_rotation_queue_bounds_include_inflight_and_stop_cleans_replies() {
        let queue = Arc::new(RotationQueue::default());
        queue.open().unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut replies = Vec::new();
        for _ in 0..MAX_PENDING {
            let (tx, rx) = mpsc::channel();
            queue
                .enqueue(body(), deadline, Arc::new(AtomicBool::new(false)), tx)
                .unwrap();
            replies.push(rx);
        }
        let inflight = queue.inner.lock().unwrap().jobs.pop_front().unwrap();
        assert_eq!(inflight.body, body());
        let (tx, _) = mpsc::channel();
        assert!(
            queue
                .enqueue(body(), deadline, Arc::new(AtomicBool::new(false)), tx)
                .is_err()
        );
        queue.close();
        assert!(!inflight.live());
        assert_eq!(queue.outstanding.load(Ordering::Acquire), 1);
        drop(inflight);
        for rx in replies {
            assert!(rx.recv().unwrap().is_err());
        }
        assert_eq!(queue.outstanding.load(Ordering::Acquire), 0);
        queue.open().unwrap();
        let (tx, _) = mpsc::channel();
        assert!(
            queue
                .enqueue(body(), Instant::now(), Arc::new(AtomicBool::new(false)), tx)
                .is_err()
        );
        queue.close();
    }
    #[test]
    fn conflicting_original_rotation_is_rejected_before_provider_without_stopping_queue() {
        use oasis7_distfs::protected_bft_v3::{
            BftFinalityRotationProofArchiveV3, BftFinalityRotationProofRecordV3,
        };
        let queue = Arc::new(RotationQueue::default());
        queue.open().unwrap();
        let original = body();
        for role in [
            FinalityKeyRotationRoleV1::OldKeyAuthorization,
            FinalityKeyRotationRoleV1::NewKeyPossession,
        ] {
            // This unit exercises routing only, not signature validity or custody.
            let archive = BftFinalityRotationProofArchiveV3 {
                schema_version: 1,
                records: vec![BftFinalityRotationProofRecordV3 {
                    body: original.clone(),
                    public_key_hex: original.old_public_key_hex.clone(),
                    proof: FinalityKeyRotationProofV1 {
                        schema_version: 1,
                        role,
                        signature_hex: "0".repeat(128),
                    },
                    recorded_revision: 1,
                }],
            };
            let mut changed = original.clone();
            changed.candidate_id = "different-original".into();
            changed.validate().unwrap();
            let (tx, rx) = mpsc::channel();
            queue
                .enqueue(
                    changed,
                    Instant::now() + Duration::from_secs(60),
                    Arc::new(AtomicBool::new(false)),
                    tx,
                )
                .unwrap();
            let mut rejected = queue.inner.lock().unwrap().jobs.pop_front().unwrap();
            assert!(rejected.reject_conflicting_archive(Some(&archive)));
            assert!(rx.recv().unwrap().is_err());
            drop(rejected);
            // The same production rejection seam keeps admission open and the
            // exact original request eligible. No provider call is made here.
            let (tx, _rx) = mpsc::channel();
            queue
                .enqueue(
                    original.clone(),
                    Instant::now() + Duration::from_secs(60),
                    Arc::new(AtomicBool::new(false)),
                    tx,
                )
                .unwrap();
            let mut accepted = queue.inner.lock().unwrap().jobs.pop_front().unwrap();
            assert!(!accepted.reject_conflicting_archive(Some(&archive)));
            assert!(accepted.live());
            drop(accepted);
            assert_eq!(queue.outstanding.load(Ordering::Acquire), 0);
        }
        queue.close();
    }
}
