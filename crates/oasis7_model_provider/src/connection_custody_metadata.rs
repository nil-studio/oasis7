//! Non-secret local metadata, not authority, a probe, readiness or a key loan.
use super::*;
#[derive(Clone)]
pub struct CustodyConnectionMetadata {
    pub connection_id: String,
    pub account_id: String,
    pub session_id: String,
    pub preset: ProviderPreset,
    pub version: u64,
    pub generation: u64,
    pub expires_at: u64,
}
/// Native catalog must not form IdentityService -> manager -> IdentityService
/// cycles that would keep memory-only provider keys alive after shutdown.
#[derive(Clone)]
pub struct WeakConnectionManager {
    shared: std::sync::Weak<Shared>,
}
impl WeakConnectionManager {
    pub fn upgrade(&self) -> Option<ConnectionManager> {
        self.shared
            .upgrade()
            .map(|shared| ConnectionManager { shared })
    }
}
impl ConnectionManager {
    pub fn downgrade(&self) -> WeakConnectionManager {
        WeakConnectionManager {
            shared: std::sync::Arc::downgrade(&self.shared),
        }
    }
    pub fn custody_connection_for_preset(
        &self,
        account: &str,
        session: &str,
        preset: &str,
    ) -> Result<CustodyConnectionMetadata, ConnectionError> {
        let now = self.shared.clock.now_ms();
        let id = {
            let state = self
                .shared
                .state
                .lock()
                .map_err(|_| ConnectionError::Unavailable)?;
            let mut candidates = state.connections.iter().filter(|(_, saved)| {
                saved.owner.account_id == account
                    && saved.owner.session_id == session
                    && saved.preset.id == preset
                    && saved.key.as_ref().is_some_and(|key| !key.is_empty())
                    && saved.active_version.is_none()
                    && saved.binding.is_none()
                    && now >= state.last_now
                    && now < saved.expires
                    && now < saved.absolute
                    && matches!(saved.state, ConnectionState::Draft | ConnectionState::Ready)
            });
            let id = candidates
                .next()
                .ok_or(ConnectionError::Unavailable)?
                .0
                .clone();
            if candidates.next().is_some() {
                return Err(ConnectionError::Conflict);
            }
            id
        };
        self.custody_connection_metadata(account, session, &id)
    }
    /// Native has already authenticated this exact account/session locally.
    /// No SessionAuthority or SDK BindingAuthority callback is made while the
    /// Native identity lock may be held. This method cannot mint RequestPermit.
    pub fn custody_connection_metadata(
        &self,
        account: &str,
        session: &str,
        connection: &str,
    ) -> Result<CustodyConnectionMetadata, ConnectionError> {
        let now = self.shared.clock.now_ms();
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| ConnectionError::Unavailable)?;
        let saved = state
            .connections
            .get(connection)
            .ok_or(ConnectionError::Unavailable)?;
        if saved.owner.account_id != account
            || saved.owner.session_id != session
            || saved.key.as_ref().is_none_or(|key| key.is_empty())
            || saved.active_version.is_some()
            || saved.binding.is_some()
            || !matches!(saved.state, ConnectionState::Draft | ConnectionState::Ready)
            || now < state.last_now
            || now >= saved.expires
            || now >= saved.absolute
        {
            return Err(ConnectionError::Unavailable);
        }
        Ok(CustodyConnectionMetadata {
            connection_id: connection.into(),
            account_id: saved.owner.account_id.clone(),
            session_id: saved.owner.session_id.clone(),
            preset: saved.preset.clone(),
            version: saved.version,
            generation: saved.generation,
            expires_at: saved.expires.min(saved.absolute),
        })
    }
}
