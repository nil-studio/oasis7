use std::collections::BTreeSet;

use super::super::auth::verify_director_capability_grant;
use super::super::protocol::DirectorCapabilityGrant;
use super::*;

#[derive(Debug, Clone)]
pub(super) struct RuntimeRecoveryCursor {
    pub(super) snapshot_hash: String,
    pub(super) snapshot_height: u64,
    pub(super) log_cursor: u64,
    pub(super) stable_batch_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimeSessionRevokeMetadata {
    pub(super) revoke_reason: Option<String>,
    pub(super) revoked_by: Option<String>,
}

pub(super) struct RuntimeSessionRegistrationPlan {
    player_id: String,
    public_key: String,
    replaced_public_key: Option<String>,
    session_epoch: u64,
}

impl RuntimeSessionRegistrationPlan {
    pub(super) fn session_epoch(&self) -> u64 {
        self.session_epoch
    }

    pub(super) fn rotates_key(&self) -> bool {
        self.replaced_public_key.is_some()
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimeSessionPolicy {
    /// Never restore a cached identity authorization from a recovery snapshot.
    #[serde(skip)]
    hosted_authorizations: BTreeMap<String, super::super::auth::HostedAuthContextV2>,
    active_pubkey_by_player: BTreeMap<String, String>,
    revoked_pubkeys_by_player: BTreeMap<String, BTreeSet<String>>,
    session_epoch_by_player: BTreeMap<String, u64>,
    /// Director grant nonces are intentionally process-local and are never persisted.
    #[serde(skip)]
    consumed_director_capability_nonces: BTreeSet<String>,
}

impl RuntimeSessionPolicy {
    pub(super) fn active_session_public_key(&self, player: &str) -> Option<&str> {
        self.active_pubkey_by_player.get(player).map(String::as_str)
    }

    #[cfg(test)]
    pub(super) fn register_session(
        &mut self,
        player_id: &str,
        public_key: &str,
    ) -> Result<u64, String> {
        let plan = self.validate_session_registration(player_id, public_key, false)?;
        let session_epoch = plan.session_epoch();
        self.commit_session_registration(plan);
        Ok(session_epoch)
    }

    pub(super) fn validate_session_registration(
        &self,
        player_id: &str,
        public_key: &str,
        allow_key_rotation: bool,
    ) -> Result<RuntimeSessionRegistrationPlan, String> {
        let player_id = player_id.trim();
        let public_key = public_key.trim();
        if player_id.is_empty() {
            return Err("session_player_id_invalid: player_id cannot be empty".to_string());
        }
        if public_key.is_empty() {
            return Err("session_pubkey_invalid: session_pubkey cannot be empty".to_string());
        }
        if self
            .revoked_pubkeys_by_player
            .get(player_id)
            .is_some_and(|keys| keys.contains(public_key))
        {
            return Err(format!(
                "session_revoked: player {} session_pubkey {} is revoked",
                player_id, public_key
            ));
        }

        let (replaced_public_key, session_epoch) = match self.active_pubkey_by_player.get(player_id)
        {
            Some(active) if active == public_key => (None, self.session_epoch(player_id)),
            Some(active) if allow_key_rotation => (
                Some(active.clone()),
                self.session_epoch(player_id).saturating_add(1).max(1),
            ),
            Some(active) => {
                return Err(format!(
                    "session_key_mismatch: player {} active session_pubkey {} does not match {}",
                    player_id, active, public_key
                ));
            }
            None => (
                None,
                self.session_epoch_by_player
                    .get(player_id)
                    .map_or(1, |epoch| epoch.saturating_add(1).max(1)),
            ),
        };

        Ok(RuntimeSessionRegistrationPlan {
            player_id: player_id.to_string(),
            public_key: public_key.to_string(),
            replaced_public_key,
            session_epoch,
        })
    }

    pub(super) fn commit_session_registration(&mut self, plan: RuntimeSessionRegistrationPlan) {
        if let Some(replaced_public_key) = plan.replaced_public_key {
            self.revoked_pubkeys_by_player
                .entry(plan.player_id.clone())
                .or_default()
                .insert(replaced_public_key);
        }
        self.active_pubkey_by_player
            .insert(plan.player_id.clone(), plan.public_key);
        self.session_epoch_by_player
            .insert(plan.player_id, plan.session_epoch);
    }

    pub(super) fn validate_known_session_key(
        &self,
        player_id: &str,
        public_key: &str,
    ) -> Result<u64, String> {
        let player_id = player_id.trim();
        let public_key = public_key.trim();
        if player_id.is_empty() {
            return Err("session_player_id_invalid: player_id cannot be empty".to_string());
        }
        if public_key.is_empty() {
            return Err("session_pubkey_invalid: session_pubkey cannot be empty".to_string());
        }

        if self
            .revoked_pubkeys_by_player
            .get(player_id)
            .is_some_and(|keys| keys.contains(public_key))
        {
            return Err(format!(
                "session_revoked: player {} session_pubkey {} is revoked",
                player_id, public_key
            ));
        }
        match self.active_pubkey_by_player.get(player_id) {
            Some(active) if active == public_key => {}
            Some(active) => {
                return Err(format!(
                    "session_key_mismatch: player {} active session_pubkey {} does not match {}",
                    player_id, active, public_key
                ));
            }
            None => {
                return Err(format!(
                    "session_not_found: player {} has no active session_pubkey",
                    player_id
                ));
            }
        }
        if super::super::auth::hosted_identity_v2_enabled() {
            self.validate_hosted_authorization_at(
                player_id,
                public_key,
                super::super::auth::now_unix_ms(),
            )?;
        }
        Ok(self.session_epoch(player_id))
    }

    fn validate_hosted_authorization_at(
        &self,
        player: &str,
        public_key: &str,
        now: u64,
    ) -> Result<(), String> {
        let context = self
            .hosted_authorizations
            .get(player)
            .ok_or("hosted_authorization_required: identity must renew authorization")?;
        context.validate_at(now)?;
        if context.player_id != player
            || context.session_public_key != public_key
            || context.world_session_epoch != self.session_epoch(player)
        {
            return Err("hosted_authorization_binding_mismatch".into());
        }
        Ok(())
    }

    pub(super) fn validate_authorization_registration(
        &self,
        context: &super::super::auth::HostedAuthContextV2,
        planned_epoch: u64,
    ) -> Result<(), String> {
        context.validate_at(super::super::auth::now_unix_ms())?;
        if context.world_session_epoch != planned_epoch {
            return Err("hosted_authorization_world_epoch_mismatch".into());
        }
        if let Some(old) = self.hosted_authorizations.get(&context.player_id)
            && old.session_public_key == context.session_public_key
            && (old.account_id != context.account_id
                || old.account_session_id != context.account_session_id
                || old.device_session_id != context.device_session_id
                || old.credential_record_id != context.credential_record_id
                || old.credential_version != context.credential_version
                || old.account_security_epoch != context.account_security_epoch
                || old.checked_at > context.checked_at)
        {
            return Err("hosted_authorization_renewal_binding_mismatch".into());
        }
        Ok(())
    }

    pub(super) fn commit_hosted_authorization(
        &mut self,
        context: super::super::auth::HostedAuthContextV2,
    ) {
        self.hosted_authorizations
            .insert(context.player_id.clone(), context);
    }

    pub(super) fn validate_and_consume_director_capability_grant(
        &mut self,
        grant: &DirectorCapabilityGrant,
        expected_server: &str,
        required_signer_public_key: &str,
        now_unix_ms: u64,
    ) -> Result<(), String> {
        let session_epoch = self.validate_known_session_key(
            grant.player_id.as_str(),
            grant.player_public_key.as_str(),
        )?;
        verify_director_capability_grant(
            grant,
            grant.player_id.as_str(),
            grant.player_public_key.as_str(),
            expected_server,
            session_epoch,
            required_signer_public_key,
            now_unix_ms,
        )?;
        let nonce_key = format!("{}:{}", grant.player_id.trim(), grant.nonce.trim());
        if nonce_key.ends_with(':') || grant.nonce.trim().is_empty() {
            return Err("director capability grant nonce is empty".to_string());
        }
        if !self.consumed_director_capability_nonces.insert(nonce_key) {
            return Err("director capability grant nonce replay".to_string());
        }
        Ok(())
    }

    pub(super) fn revoke_session(
        &mut self,
        player_id: &str,
        session_pubkey: Option<&str>,
    ) -> Result<(String, u64), String> {
        let player_id = player_id.trim();
        if player_id.is_empty() {
            return Err("session_player_id_invalid: player_id cannot be empty".to_string());
        }

        let target = session_pubkey
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| self.active_pubkey_by_player.get(player_id).cloned())
            .ok_or_else(|| {
                format!(
                    "session_not_found: player {} has no active session_pubkey",
                    player_id
                )
            })?;

        let revoked = self
            .revoked_pubkeys_by_player
            .entry(player_id.to_string())
            .or_default()
            .insert(target.clone());
        if self
            .active_pubkey_by_player
            .get(player_id)
            .is_some_and(|active| active == &target)
        {
            self.active_pubkey_by_player.remove(player_id);
        }

        if revoked {
            let next_epoch = self.session_epoch(player_id).saturating_add(1).max(1);
            self.session_epoch_by_player
                .insert(player_id.to_string(), next_epoch);
        }

        Ok((target, self.session_epoch(player_id)))
    }

    pub(super) fn rotate_session(
        &mut self,
        player_id: &str,
        old_session_pubkey: &str,
        new_session_pubkey: &str,
    ) -> Result<u64, String> {
        let player_id = player_id.trim();
        let old_session_pubkey = old_session_pubkey.trim();
        let new_session_pubkey = new_session_pubkey.trim();
        if player_id.is_empty() {
            return Err("session_player_id_invalid: player_id cannot be empty".to_string());
        }
        if old_session_pubkey.is_empty() || new_session_pubkey.is_empty() {
            return Err("session_pubkey_invalid: session_pubkey cannot be empty".to_string());
        }
        if old_session_pubkey == new_session_pubkey {
            return Err("session_rotation_invalid: old/new session_pubkey must differ".to_string());
        }

        if self
            .active_pubkey_by_player
            .get(player_id)
            .is_some_and(|active| active != old_session_pubkey)
        {
            return Err(format!(
                "session_key_mismatch: player {} active session_pubkey does not match {}",
                player_id, old_session_pubkey
            ));
        }
        if self
            .revoked_pubkeys_by_player
            .get(player_id)
            .is_some_and(|keys| keys.contains(new_session_pubkey))
        {
            return Err(format!(
                "session_rotation_invalid: new session_pubkey {} is already revoked",
                new_session_pubkey
            ));
        }

        self.revoked_pubkeys_by_player
            .entry(player_id.to_string())
            .or_default()
            .insert(old_session_pubkey.to_string());
        self.active_pubkey_by_player
            .insert(player_id.to_string(), new_session_pubkey.to_string());
        let next_epoch = self.session_epoch(player_id).saturating_add(1).max(1);
        self.session_epoch_by_player
            .insert(player_id.to_string(), next_epoch);
        Ok(next_epoch)
    }

    pub(super) fn session_epoch(&self, player_id: &str) -> u64 {
        self.session_epoch_by_player
            .get(player_id)
            .copied()
            .unwrap_or(0)
    }
}

pub(super) fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn session_revoke_metadata_key(
    player_id: &str,
    session_pubkey: &str,
) -> (String, String) {
    (
        player_id.trim().to_string(),
        session_pubkey.trim().to_string(),
    )
}

pub(super) fn map_session_policy_error_code(message: &str) -> &'static str {
    if message.contains("hosted_authorization_expired") {
        return "hosted_authorization_expired";
    }
    if message.contains("hosted_authorization_required") {
        return "hosted_authorization_required";
    }
    if message.contains("hosted_authorization") {
        return "hosted_authorization_invalid";
    }
    if message.contains("session_revoked") {
        return "session_revoked";
    }
    if message.contains("session_key_mismatch") {
        return "session_key_mismatch";
    }
    if message.contains("session_not_found") {
        return "session_not_found";
    }
    if message.contains("session_pubkey_invalid")
        || message.contains("session_player_id_invalid")
        || message.contains("session_rotation_invalid")
    {
        return "session_invalid";
    }
    "session_policy_error"
}

pub(super) fn location_id_for_pos(pos: GeoPos) -> String {
    format!("runtime:{}:{}:{}", pos.x_cm, pos.y_cm, pos.z_cm)
}

#[cfg(test)]
mod hosted_authorization_tests {
    use super::*;
    use crate::viewer::auth::test_hosted_authorization_context;

    #[test]
    fn opened_session_stops_accepting_new_operations_after_identity_deadline() {
        let context = test_hosted_authorization_context();
        let mut policy = RuntimeSessionPolicy::default();
        policy
            .register_session(&context.player_id, &context.session_public_key)
            .unwrap();
        policy.commit_hosted_authorization(context.clone());
        assert!(
            policy
                .validate_hosted_authorization_at(
                    &context.player_id,
                    &context.session_public_key,
                    30_999
                )
                .is_ok()
        );
        assert!(
            policy
                .validate_hosted_authorization_at(
                    &context.player_id,
                    &context.session_public_key,
                    31_000
                )
                .is_err()
        );
        assert!(
            policy
                .validate_hosted_authorization_at(&context.player_id, "another-key", 2_000)
                .is_err()
        );
    }

    #[test]
    fn recovery_does_not_restore_cached_identity_authority() {
        let context = test_hosted_authorization_context();
        let mut policy = RuntimeSessionPolicy::default();
        policy
            .register_session(&context.player_id, &context.session_public_key)
            .unwrap();
        policy.commit_hosted_authorization(context.clone());
        let bytes = serde_json::to_vec(&policy).unwrap();
        let restored: RuntimeSessionPolicy = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            restored.active_session_public_key(&context.player_id),
            Some(context.session_public_key.as_str())
        );
        assert!(
            restored
                .validate_hosted_authorization_at(
                    &context.player_id,
                    &context.session_public_key,
                    2_000
                )
                .is_err()
        );
    }

    #[test]
    fn replaced_or_revoked_world_epoch_cannot_reuse_old_identity_authority() {
        let context = test_hosted_authorization_context();
        let mut policy = RuntimeSessionPolicy::default();
        policy
            .register_session(&context.player_id, &context.session_public_key)
            .unwrap();
        policy.commit_hosted_authorization(context.clone());
        policy.revoke_session(&context.player_id, None).unwrap();
        assert!(
            policy
                .validate_hosted_authorization_at(
                    &context.player_id,
                    &context.session_public_key,
                    2_000
                )
                .is_err()
        );
    }

    #[test]
    fn renewal_cannot_change_parent_session_or_security_versions_for_existing_key() {
        let mut context = test_hosted_authorization_context();
        let now = crate::viewer::auth::now_unix_ms();
        context.checked_at = now;
        context.issued_at = now;
        context.expires_at = now + 30_000;
        let mut policy = RuntimeSessionPolicy::default();
        policy
            .register_session(&context.player_id, &context.session_public_key)
            .unwrap();
        policy.commit_hosted_authorization(context.clone());
        assert!(
            policy
                .validate_authorization_registration(&context, 1)
                .is_ok()
        );
        assert!(
            policy
                .validate_authorization_registration(&context, 2)
                .is_err()
        );
        for field in [
            "account_session",
            "device_session",
            "credential",
            "security_epoch",
        ] {
            let mut changed = context.clone();
            match field {
                "account_session" => changed.account_session_id = "another-session".into(),
                "device_session" => changed.device_session_id = "another-device-session".into(),
                "credential" => changed.credential_version += 1,
                _ => changed.account_security_epoch += 1,
            }
            assert!(
                policy
                    .validate_authorization_registration(&changed, 1)
                    .is_err(),
                "{field}"
            );
        }
    }
}
