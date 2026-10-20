//! Original authenticated Resume bytes held before canonical execution.
use super::*;
#[derive(Default)]
pub(super) struct RejectedResumeGate {
    candidate: Option<SubmitIntentRequest<WorldServicePayloadV1>>,
    owner: Option<thread::ThreadId>,
    release: bool,
    returned: bool,
    joined: bool,
}
impl WorldGate {
    pub(super) fn pause_rejected_resume(
        &self,
        root: &Path,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) {
        if !root.join("resume-rejected-arm").exists() {
            return;
        }
        let WorldServicePayloadV1::Scheduler(signed) = &request.signed_payload else {
            return;
        };
        if !matches!(
            signed.request.operation,
            SchedulerOperationV1::ResumeWake { .. }
        ) {
            return;
        }
        request.validate().unwrap();
        assert_eq!(
            correlation::derive_correlation(
                request.correlation.key.world.clone(),
                &request.signed_payload
            )
            .unwrap(),
            request.correlation
        );
        {
            let mut state = self.rejected_resume.lock().unwrap();
            assert!(
                state.candidate.is_none(),
                "one exact original Resume Submit"
            );
            state.candidate = Some(request.clone());
            state.owner = Some(thread::current().id());
        }
        write_new_json(&root.join("resume-rejected-original.json"), request).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.rejected_resume.lock().unwrap().release && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(self.rejected_resume.lock().unwrap().release);
    }
    pub(in super::super) fn rejected_resume_candidate(
        &self,
    ) -> Option<SubmitIntentRequest<WorldServicePayloadV1>> {
        self.rejected_resume.lock().unwrap().candidate.clone()
    }
    pub(in super::super) fn release_rejected_resume(&self) {
        self.rejected_resume.lock().unwrap().release = true;
    }
    pub(super) fn rejected_resume_returned(&self, id: thread::ThreadId) {
        let mut s = self.rejected_resume.lock().unwrap();
        if s.owner == Some(id) {
            s.returned = true;
        }
    }
    pub(super) fn rejected_resume_joined(&self, id: thread::ThreadId) {
        let mut s = self.rejected_resume.lock().unwrap();
        if s.owner == Some(id) {
            s.joined = true;
        }
    }
    pub(in super::super) fn rejected_resume_join_proof(&self) -> bool {
        let s = self.rejected_resume.lock().unwrap();
        s.returned && s.joined
    }
}
