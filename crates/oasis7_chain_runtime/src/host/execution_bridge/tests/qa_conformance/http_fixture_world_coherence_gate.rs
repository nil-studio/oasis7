//! One real authenticated Changes request held after the caller's View capture.
use super::*;
#[derive(Default)]
pub(super) struct CoherenceGateState {
    expected: Option<EventCursor>,
    owner: Option<thread::ThreadId>,
    released: bool,
    returned: bool,
    joined: bool,
}
impl WorldGate {
    pub(in super::super) fn arm_coherence_changes(&self, cursor: EventCursor) {
        let mut state = self.coherence.lock().unwrap();
        assert!(state.expected.is_none());
        state.expected = Some(cursor);
    }
    pub(in super::super) fn release_coherence_changes(&self) {
        self.coherence.lock().unwrap().released = true;
    }
    pub(super) fn pause_coherence_changes(&self, root: &Path, bytes: &[u8]) {
        let body = crate::feedback_submit_api::extract_http_json_body(bytes).unwrap();
        let Ok(request) =
            serde_json::from_slice::<wire::SignedReadRequest<ReadWorldChangesRequest>>(body)
        else {
            return;
        };
        if oasis7::world_service::authority::verify_read_request(CHANGES_PATH, &request).is_err()
            || request.request.validate().is_err()
        {
            return;
        }
        {
            let mut state = self.coherence.lock().unwrap();
            if state.owner.is_some() || state.expected.as_ref() != Some(&request.request.cursor) {
                return;
            }
            state.owner = Some(thread::current().id());
        }
        write_new_json(&root.join("coherence-changes-original.json"), &request).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.coherence.lock().unwrap().released && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            self.coherence.lock().unwrap().released,
            "bounded real Changes gate was not released"
        );
    }
    pub(super) fn coherence_handler_returned(&self, current: thread::ThreadId) {
        let mut state = self.coherence.lock().unwrap();
        if state.owner == Some(current) {
            state.returned = true;
        }
    }
    pub(super) fn coherence_worker_joined(&self, current: thread::ThreadId) {
        let mut state = self.coherence.lock().unwrap();
        if state.owner == Some(current) {
            state.joined = true;
        }
    }
    pub(in super::super) fn coherence_join_proof(&self) -> bool {
        let state = self.coherence.lock().unwrap();
        state.returned && state.joined
    }
}
