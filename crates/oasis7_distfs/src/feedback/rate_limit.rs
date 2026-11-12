use super::*;

impl FeedbackStore {
    pub(super) fn enforce_rate_limit(
        &self,
        submit_ip: &str,
        actor_public_key_hex: &str,
        now_ms: i64,
    ) -> Result<(), WorldError> {
        validate_ip(submit_ip)?;
        validate_token(actor_public_key_hex, "actor_public_key_hex")?;
        let counts = self.collect_recent_accepted_audit_counts(now_ms)?;
        let ip_identity = rate_limit_ip_identity(submit_ip)?;
        let public_key_identity = rate_limit_public_key_identity(actor_public_key_hex)?;
        let ip_count = counts.ip_counts.get(&ip_identity).copied().unwrap_or(0);
        if ip_count >= self.config.max_actions_per_ip_window {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "rate limit exceeded for ip={} count={} window_ms={}",
                    submit_ip, ip_count, self.config.rate_limit_window_ms
                ),
            });
        }
        let pubkey_count = counts
            .pubkey_counts
            .get(&public_key_identity)
            .copied()
            .unwrap_or(0);
        if pubkey_count >= self.config.max_actions_per_pubkey_window {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "rate limit exceeded for pubkey={} count={} window_ms={}",
                    actor_public_key_hex, pubkey_count, self.config.rate_limit_window_ms
                ),
            });
        }
        Ok(())
    }

    fn collect_recent_accepted_audit_counts(
        &self,
        now_ms: i64,
    ) -> Result<FeedbackRateLimitCounts, WorldError> {
        let files = self.store.list_files()?;
        let mut counts = FeedbackRateLimitCounts::default();
        let min_timestamp = now_ms.saturating_sub(self.config.rate_limit_window_ms.max(0));
        for file in files {
            if !file.path.starts_with(feedback_audit_prefix().as_str()) {
                continue;
            }
            if feedback_audit_path_timestamp_ms(file.path.as_str())
                .is_some_and(|timestamp_ms| timestamp_ms < min_timestamp)
            {
                continue;
            }
            let bytes = self.store.read_file(file.path.as_str())?;
            let record: FeedbackAuditRecord = serde_json::from_slice(&bytes)?;
            if !record.accepted {
                continue;
            }
            if record.timestamp_ms < min_timestamp {
                continue;
            }
            let ip_identity = rate_limit_ip_identity(record.submit_ip.as_str())?;
            let public_key_identity =
                rate_limit_public_key_identity(record.actor_public_key_hex.as_str())?;
            increment_counter(&mut counts.ip_counts, ip_identity.as_str());
            increment_counter(&mut counts.pubkey_counts, public_key_identity.as_str());
        }
        Ok(counts)
    }
}

// Canonicalize only accounting identities, not signed payloads or stored records.
// Apply the same mapping when reading historical audit entries and checking requests.
pub(super) fn rate_limit_ip_identity(value: &str) -> Result<String, WorldError> {
    IpAddr::from_str(value.trim())
        .map(|address| address.to_string())
        .map_err(|error| WorldError::DistributedValidationFailed {
            reason: format!("submit_ip is invalid ip address: {error}"),
        })
}

pub(super) fn rate_limit_public_key_identity(value: &str) -> Result<String, WorldError> {
    decode_hex_array::<32>(value.trim(), "feedback public key").map(hex::encode)
}
