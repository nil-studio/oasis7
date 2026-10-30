//! Bounded management projections and independent abuse accounting; never authority.
use crate::DecimalU64;
use serde::{Deserialize, Serialize};

pub const PARENTS_QUERY_PATH: &str = "/v1/management/parents/query";
pub const OPTIONS_PATH: &str = "/api/public/hosted-account/v2/custody/approvals/options";
pub const COMPLETE_PATH: &str = "/api/public/hosted-account/v2/custody/approvals/complete";
pub const MAX_PAGE_BYTES: usize = 32768;
fn id(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 256
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
}
fn revision(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ManagementParentsQueryV1 {
    pub api_version: u8,
    pub limit: u8,
    pub after_custody_account_id: Option<String>,
    pub directory_revision: Option<String>,
}
impl ManagementParentsQueryV1 {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.api_version != 1 || !(1..=16).contains(&self.limit) {
            return Err("invalid management page");
        }
        match (&self.after_custody_account_id, &self.directory_revision) {
            (None, None) => Ok(()),
            (Some(a), Some(r)) if id(a) && revision(r) => Ok(()),
            _ => Err("invalid management cursor"),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ManagementParentV1 {
    pub custody_account_id: String,
    pub provision_id: String,
    pub environment: String,
    pub world_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ManagementParentsPageV1 {
    pub api_version: u8,
    pub directory_revision: String,
    pub frozen: bool,
    pub inventory_complete: bool,
    pub items: Vec<ManagementParentV1>,
    pub next_after_custody_account_id: Option<String>,
}
impl ManagementParentsPageV1 {
    pub fn validate(&self, query: &ManagementParentsQueryV1) -> Result<(), &'static str> {
        query.validate()?;
        if self.api_version != 1
            || !revision(&self.directory_revision)
            || self.items.len() > usize::from(query.limit)
            || query
                .directory_revision
                .as_ref()
                .is_some_and(|r| r != &self.directory_revision)
            || self.items.iter().any(|i| {
                !id(&i.custody_account_id)
                    || !id(&i.provision_id)
                    || !id(&i.environment)
                    || i.world_id.is_empty()
                    || i.world_id.len() > 256
            })
            || self
                .items
                .windows(2)
                .any(|w| w[0].custody_account_id >= w[1].custody_account_id)
            || self.items.first().is_some_and(|i| {
                query
                    .after_custody_account_id
                    .as_ref()
                    .is_some_and(|a| &i.custody_account_id <= a)
            })
            || self
                .next_after_custody_account_id
                .as_ref()
                .is_some_and(|n| {
                    self.items.len() != usize::from(query.limit)
                        || self.items.last().is_none_or(|i| &i.custody_account_id != n)
                })
            || serde_json::to_vec(self)
                .map_err(|_| "invalid management page")?
                .len()
                > MAX_PAGE_BYTES
        {
            return Err("invalid management page");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CustodyManagementCapabilityV1 {
    pub api_version: u8,
    pub available: bool,
    pub audience: Option<String>,
    pub environment: Option<String>,
    pub parents_query_path: String,
    pub options_path: String,
    pub complete_path: String,
}
impl CustodyManagementCapabilityV1 {
    pub fn configured(audience: Option<&str>, environment: &str) -> Self {
        let available =
            audience.is_some_and(|a| !a.is_empty() && a.len() <= 256) && id(environment);
        Self {
            api_version: 1,
            available,
            audience: available.then(|| audience.unwrap().to_owned()),
            environment: available.then(|| environment.to_owned()),
            parents_query_path: PARENTS_QUERY_PATH.into(),
            options_path: OPTIONS_PATH.into(),
            complete_path: COMPLETE_PATH.into(),
        }
    }
}
/// Separate mandatory state for one fixed metadata bucket. No business budget/refund.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagementMetadataQuotaV1 {
    pub api_version: u8,
    pub bucket: String,
    pub last_observed: DecimalU64,
    pub window: DecimalU64,
    pub used: u16,
    pub limit: u16,
}
impl ManagementMetadataQuotaV1 {
    pub fn charge(
        old: Option<Self>,
        bucket: &str,
        limit: u16,
        now: u64,
    ) -> Result<Self, &'static str> {
        if !revision(bucket) || ![16, 32, 1024].contains(&limit) {
            return Err("invalid management quota");
        }
        let mut state = old.unwrap_or(Self {
            api_version: 1,
            bucket: bucket.into(),
            last_observed: DecimalU64(now),
            window: DecimalU64(now / 60_000),
            used: 0,
            limit,
        });
        if state.api_version != 1
            || state.bucket != bucket
            || state.limit != limit
            || state.window.0 != state.last_observed.0 / 60_000
            || state.last_observed.0 > now
            || state.used > limit
        {
            return Err("invalid management quota");
        }
        if state.window.0 != now / 60_000 {
            state.used = 0;
            state.window = DecimalU64(now / 60_000);
        }
        if state.used >= limit {
            return Err("management quota exhausted");
        }
        state.used += 1;
        state.last_observed = DecimalU64(now);
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_limits_reject_without_mutating_original_and_do_not_borrow() {
        let key = "ab".repeat(32);
        for limit in [16, 32, 1024] {
            let mut state = None;
            for _ in 0..limit {
                state = Some(ManagementMetadataQuotaV1::charge(state, &key, limit, 1000).unwrap());
            }
            let before = state.clone();
            assert!(ManagementMetadataQuotaV1::charge(state.clone(), &key, limit, 1001).is_err());
            assert_eq!(state, before);
            assert!(ManagementMetadataQuotaV1::charge(state.clone(), &key, limit, 999).is_err());
            assert!(ManagementMetadataQuotaV1::charge(state, &key, limit, 60000).is_ok());
        }
        assert!(serde_json::from_str::<ManagementMetadataQuotaV1>(r#"{"api_version":1}"#).is_err());
    }
    #[test]
    fn pages_are_closed_bounded_and_cursor_exact() {
        let mut query = ManagementParentsQueryV1 {
            api_version: 1,
            limit: 16,
            after_custody_account_id: None,
            directory_revision: None,
        };
        assert!(query.validate().is_ok());
        query.limit = 17;
        assert!(query.validate().is_err());
        query.limit = 1;
        query.after_custody_account_id = Some("a".into());
        assert!(query.validate().is_err());
        query.directory_revision = Some("ab".repeat(32));
        assert!(query.validate().is_ok());
        let mut page = ManagementParentsPageV1 {
            api_version: 1,
            directory_revision: "ab".repeat(32),
            frozen: false,
            inventory_complete: true,
            items: vec![ManagementParentV1 {
                custody_account_id: "b".into(),
                provision_id: "p".into(),
                environment: "test".into(),
                world_id: "w".into(),
            }],
            next_after_custody_account_id: None,
        };
        assert!(page.validate(&query).is_ok());
        page.next_after_custody_account_id = Some("c".into());
        assert!(page.validate(&query).is_err());
        assert!(
            serde_json::from_str::<ManagementParentsQueryV1>(
                r#"{"api_version":1,"limit":1,"manager":"other"}"#
            )
            .is_err()
        );
        assert!(!CustodyManagementCapabilityV1::configured(None, "test").available);
    }
}
