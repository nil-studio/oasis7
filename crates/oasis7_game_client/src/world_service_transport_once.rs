//! Direct one-HTTP transport for a separately prepaid single-page consumer.
//! Existing clients retain their configured transport and retry behavior.
use super::*;
impl WorldServiceTransport {
    pub(crate) fn require_one_http(&self) -> Result<(), WorldServiceClientError> {
        #[cfg(unix)]
        if self.executor_gateway.is_some() {
            // An IPC/gateway route has a separate physical graph; do not label
            // it one HTTP or bypass its authenticated executor boundary.
            return Err(WorldServiceClientError::Configuration(
                "Unsupported: one-HTTP lookup requires direct World transport".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn call_once<Q: Serialize, R: Serialize + DeserializeOwned>(
        &self,
        path: &str,
        request: &Q,
    ) -> Result<R, WorldServiceClientError> {
        self.require_one_http()?;
        let mut once = self.clone();
        // Reqwest's default protocol-NACK retry can emit more than one physical
        // HTTP even without an application loop. Disable it only for this lane.
        once.http = reqwest::blocking::Client::builder()
            .timeout(self.config.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map_err(|e| WorldServiceClientError::Configuration(e.to_string()))?;
        once.call(path, request)
    }
}
