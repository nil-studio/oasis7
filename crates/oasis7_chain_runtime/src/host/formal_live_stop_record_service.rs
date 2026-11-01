//! Parent-private formal HTTP service implementation.
use super::*;
impl FormalLiveService {
    pub(super) fn stop_record_checked_now(&self) -> Result<u64, String> {
        self.stop_record_clock
            .lock()
            .map_err(|_| "Stop clock lock poisoned".to_owned())?
            .sample()
    }

    pub(super) fn handle_stop_record_control(&self, body: &[u8]) -> HttpReply {
        use oasis7::world_service::task_stop_record_control_v2;
        use oasis7_client_api::world_service::task_stop_record_control_v2::TASK_STOP_RECORD_CONTROL_V2;
        let result = (|| -> Result<_, String> {
            if body.len() > 64 * 1024 {
                return Err("Stop query exceeds bound".into());
            }
            let request: oasis7_client_api::world_service::task_stop_record_control_v2::StopRecordControlRequestV2 = parse_strict(body)?;
            request.validate()?;
            if request.original.expected_world != self.world {
                return Err("Stop query differs from configured World".into());
            }
            if !self.runtime_status().ready {
                return Err("fresh local publication unavailable".into());
            }
            let received = self.stop_record_checked_now()?;
            let first_cut = self
                .published
                .current_for_read()
                .map_err(|e| format!("{e:?}"))?;
            // World is Send but not Sync. Serialize this resident CPU-only
            // read; no guard survives prepare or reaches a pause/network write.
            let prepared = {
                let cached = first_cut
                    .stop_publication
                    .as_ref()
                    .ok_or("qualified local Stop producer unavailable")?;
                let publication = cached
                    .lock()
                    .map_err(|_| "qualified local Stop cache poisoned")?;
                task_stop_record_control_v2::prepare(body, &publication, received)?
            };
            #[cfg(test)]
            if let Some(control) = self.published.test_control_state()? {
                let position = self.published.position()?;
                control.stop_record_prepared(position)?;
            }
            if !self.runtime_status().ready {
                return Err("fresh delivery publication unavailable".into());
            }
            let second_cut = self
                .published
                .current_for_read()
                .map_err(|e| format!("{e:?}"))?;
            let cached = second_cut
                .stop_publication
                .as_ref()
                .ok_or("qualified local Stop delivery producer unavailable")?;
            let publication = cached
                .lock()
                .map_err(|_| "qualified local Stop cache poisoned")?;
            prepared.deliver(
                &publication,
                &hex::encode(self.signer.to_bytes()),
                self.stop_record_checked_now()?,
            )
        })();
        match result {
            Ok(response) => json_reply(200, &response),
            Err(error) => raw_error(
                TASK_STOP_RECORD_CONTROL_V2,
                body,
                "stop_observation_unavailable",
                &error,
                &self.signer,
            ),
        }
    }
}
