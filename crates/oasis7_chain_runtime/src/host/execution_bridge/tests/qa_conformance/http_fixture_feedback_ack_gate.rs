//! Actual signed ACK HTTP boundaries; no canonical receipt is manufactured.
use super::*;
impl WorldGate {
    pub(in super::super) fn feedback_ack_trace(&self) -> Vec<serde_json::Value> {
        self.feedback_ack_typed_trace.lock().unwrap().clone()
    }
    pub(super) fn observe_feedback_ack_typed_request(&self, path: &str, bytes: &[u8]) {
        let operation = if path == SUBMIT_PATH {
            "submit"
        } else if path == LOOKUP_PATH {
            "lookup"
        } else {
            return;
        };
        let body = crate::feedback_submit_api::extract_http_json_body(bytes).unwrap();
        let request: serde_json::Value = serde_json::from_slice(body).unwrap();
        let mut trace = self.feedback_ack_typed_trace.lock().unwrap();
        assert!(trace.len() < 4096);
        trace.push(serde_json::json!({"operation":operation,"request":request}));
    }

    fn ack_request(
        &self,
        path: &str,
        bytes: &[u8],
    ) -> Option<(
        std::path::PathBuf,
        SubmitIntentRequest<WorldServicePayloadV1>,
    )> {
        if path != SUBMIT_PATH {
            return None;
        }
        let root = self.root.lock().unwrap().clone()?;
        let body = crate::feedback_submit_api::extract_http_json_body(bytes).ok()?;
        let request: SubmitIntentRequest<WorldServicePayloadV1> =
            serde_json::from_slice(body).ok()?;
        if !matches!(
            &request.signed_payload,
            WorldServicePayloadV1::FeedbackAck(_)
        ) {
            return None;
        }
        Some((root, request))
    }
    pub(super) fn pause_feedback_ack_before(&self, path: &str, bytes: &[u8]) -> bool {
        let Some((root, request)) = self.ack_request(path, bytes) else {
            return false;
        };
        if !root.join("world-feedback-ack-before-arm").exists()
            || root.join("world-feedback-ack-release").exists()
        {
            return false;
        }
        fs::write(
            root.join("world-feedback-ack-before-started"),
            serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !root
            .join("world-feedback-ack-process-exit-confirmed")
            .exists()
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            root.join("world-feedback-ack-process-exit-confirmed")
                .exists(),
            "parent must confirm entire application exit before abandoning original ACK dispatch"
        );
        fs::write(
            root.join("world-feedback-ack-before-abandoned"),
            b"canonical_dispatch=false",
        )
        .unwrap();
        true
    }
    pub(in super::super) fn capture_feedback_ack_response(&self, path: &str, bytes: &[u8]) -> bool {
        self.ack_request(path, bytes).is_some_and(|(root, _)| {
            root.join("world-feedback-ack-after-arm").exists()
                && !root.join("world-feedback-ack-release").exists()
        })
    }
    pub(in super::super) fn hold_feedback_ack_after_commit(&self, path: &str, bytes: &[u8]) {
        let Some((root, request)) = self.ack_request(path, bytes) else {
            return;
        };
        if !root.join("world-feedback-ack-after-arm").exists()
            || root.join("world-feedback-ack-release").exists()
        {
            return;
        }
        fs::write(
            root.join("world-feedback-ack-after-started"),
            serde_json::to_vec(&request).unwrap(),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !root
            .join("world-feedback-ack-process-exit-confirmed")
            .exists()
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            root.join("world-feedback-ack-process-exit-confirmed")
                .exists(),
            "actual committed ACK response held until parent confirms process exit"
        );
        fs::write(
            root.join("world-feedback-ack-after-dropped"),
            b"canonical_committed=true,response_delivered=false",
        )
        .unwrap();
    }
}
