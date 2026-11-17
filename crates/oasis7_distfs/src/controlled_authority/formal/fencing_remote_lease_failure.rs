//! Unknown or rejected operations retire this connection, not provider history.
use super::*;
use std::net::Shutdown;

impl RemoteGuard {
    pub(super) fn with_live_lease<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, FencingError>,
    ) -> Result<T, FencingError> {
        if self.failed {
            return Err(FencingError::FencingUnproven);
        }
        // Set before calling: even an unwinding operation cannot leave cached
        // evidence usable. Only a completely validated reply restores this guard.
        self.failed = true;
        match operation(self) {
            Ok(value) => {
                self.failed = false;
                Ok(value)
            }
            Err(error) => {
                // Do not retry an unknown Begin/Finish on a potentially desynced
                // stream. A fresh acquire must recover the provider's real state.
                // A poisoned mutex is only accessed to close, never to repair it.
                let stream = self.stream.get_mut().unwrap_or_else(|e| e.into_inner());
                let _ = stream.shutdown(Shutdown::Both);
                Err(error)
            }
        }
    }
}
