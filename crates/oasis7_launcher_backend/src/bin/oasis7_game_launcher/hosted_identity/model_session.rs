//! Reuse the hosted identity authority for model sessions, without exporting a cookie.
use super::{IdentityService, digest, now_ms, reject, service, store::StoreError};
use oasis7_model_provider::connection::Principal;

impl IdentityService {
    pub(crate) fn model_principal(
        &mut self,
        headers: &str,
        writing: bool,
    ) -> Result<Principal, StoreError> {
        let now = now_ms();
        if !self.clock.observe(now) {
            return Err(reject("clock_unhealthy"));
        }
        let token = service::cookie_value(headers, "__Host-oasis7_sid")
            .ok_or_else(|| reject("session_expired"))?;
        if writing {
            let csrf = digest(&self.config.session_key, "csrf", token);
            self.validate_json_write(headers, &csrf)?;
        }
        let (account, session) = self.session(Some(token), now)?;
        Ok(Principal {
            account_id: account.account_id,
            session_id: session.session_id,
            auth_ref: service::session_key(&self.config.session_key, token),
        })
    }
    /// Read existing account/player ownership only; never allocates or grants.
    pub(crate) fn model_account_owns_player(&mut self, account_id: &str, player_id: &str) -> bool {
        let key = format!("binding:{account_id}:{}", self.config.world_scope);
        self.store
            .transaction(|tx| {
                let account =
                    tx.get::<super::model::AccountRecord>(&format!("account:{account_id}"))?;
                let binding = tx.get::<super::model::PlayerBinding>(&key)?;
                Ok(account.is_some_and(|a| {
                    a.value.account_id == account_id && a.value.status == "active"
                }) && binding.is_some_and(|b| {
                    b.value.account_id == account_id
                        && b.value.world_identity_scope == self.config.world_scope
                        && b.value.player_id == player_id
                }))
            })
            .unwrap_or(false)
    }
    pub(crate) fn model_session_current(&mut self, principal: &Principal) -> bool {
        let now = now_ms();
        if !self.clock.observe(now) {
            return false;
        }
        let epoch = self.config.rp.identity_instance_epoch;
        self.store
            .transaction(|tx| service::valid_session(tx, &principal.auth_ref, epoch, now))
            .is_ok_and(|(account, session)| {
                account.account_id == principal.account_id
                    && session.session_id == principal.session_id
            })
    }
}

#[cfg(test)]
#[path = "model_session_tests.rs"]
pub(super) mod model_session_tests;

#[cfg(test)]
impl IdentityService {
    /// Shared real SQLite fixture, compiled only into the launcher test binary.
    pub(crate) fn model_executor_test_fixture() -> model_session_tests::Fixture {
        model_session_tests::fixture()
    }
}
