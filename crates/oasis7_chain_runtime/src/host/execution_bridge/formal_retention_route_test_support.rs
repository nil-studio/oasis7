//! Test-only observation of the actual executor boundary; never supplies history.
use oasis7_distfs::controlled_authority::formal::{
    FormalCompletionBranchV1, VerifiedAuthorityV1, VerifiedFormalCommitV1,
    endpoint::FormalCandidateMaterialV1, verify_formal_candidate, verify_record_material,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
#[derive(Clone)]
pub(crate) struct Candidate {
    pub(crate) height: u64,
    pub(crate) profile_two: bool,
    pub(crate) strong_verified: bool,
    pub(crate) record_four_present: bool,
    pub(crate) degraded: bool,
    pub(crate) in_progress: bool,
    pub(crate) live_task_pin_scope: bool,
}
#[derive(Clone, Default)]
pub(crate) struct Report {
    pub(crate) native_begins: usize,
    pub(crate) overflow: bool,
    pub(crate) candidates: Vec<Candidate>,
}
fn reports() -> &'static Mutex<BTreeMap<PathBuf, Report>> {
    static REPORTS: OnceLock<Mutex<BTreeMap<PathBuf, Report>>> = OnceLock::new();
    REPORTS.get_or_init(Default::default)
}
pub(crate) struct Capture {
    root: PathBuf,
}
pub(crate) fn capture(root: PathBuf) -> Capture {
    assert!(
        reports()
            .lock()
            .unwrap()
            .insert(root.clone(), Report::default())
            .is_none()
    );
    Capture { root }
}
impl Capture {
    pub(crate) fn report(&self) -> Report {
        reports().lock().unwrap().get(&self.root).unwrap().clone()
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        reports().lock().unwrap().remove(&self.root);
    }
}
pub(crate) fn observe_native_begin(records: &Path) {
    let Some(root) = records.parent().and_then(Path::parent) else {
        return;
    };
    if let Some(report) = reports().lock().unwrap().get_mut(root) {
        report.native_begins += 1;
    }
}
pub(crate) fn observe_candidate(
    root: &Path,
    attempt: &Path,
    height: u64,
    candidate: &FormalCandidateMaterialV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
) {
    if !reports().lock().unwrap().contains_key(root) {
        return;
    }
    // Verify the actual candidate and its complete installed-role material;
    // the actual HTTP fixture additionally waits for the qualified publication.
    let strong_verified = candidate.candidate.verify_original(authority).is_ok()
        && verify_record_material(&candidate.candidate.record, &candidate.material).is_ok()
        && verify_formal_candidate(
            &candidate.candidate,
            candidate.frozen.as_ref(),
            authority,
            Some(parent),
            None,
            FormalCompletionBranchV1::Normal,
        )
        .is_ok();
    let records = attempt.join("records");
    let observed = Candidate {
        height,
        profile_two: authority.definition().profile_version == 2,
        strong_verified,
        record_four_present: records.join("00000000000000000004.json").is_file(),
        degraded: records
            .join(crate::execution_config::EXECUTION_BRIDGE_RETENTION_DEGRADED_MARKER)
            .exists(),
        in_progress: records
            .join(crate::execution_config::EXECUTION_BRIDGE_RETENTION_IN_PROGRESS_MARKER)
            .exists(),
        live_task_pin_scope: attempt
            .join("storage/pin_scopes/execution_bridge_live_tasks_v1")
            .exists(),
    };
    if let Some(report) = reports().lock().unwrap().get_mut(root) {
        if report.candidates.len() >= 32 {
            report.overflow = true;
        } else {
            report.candidates.push(observed);
        }
    }
}
