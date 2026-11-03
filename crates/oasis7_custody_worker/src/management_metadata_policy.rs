//! Fixed, independently enabled management metadata policy. Never session funds.
use crate::management_common::denied;
#[cfg(feature = "identity")]
use crate::management_common::digest;
use worker::{Env, Result, Url};
#[cfg(feature = "identity")]
pub(crate) const BINDING: &str = "CLIENT_ENROLLMENTS";
pub(crate) struct Config {
    pub origin: String,
    pub realm: String,
}
impl Config {
    pub(crate) fn enabled(env: &Env) -> Result<Self> {
        let get = |k| env.var(k).map(|v| v.to_string()).map_err(|_| denied());
        if get("CUSTODY_MANAGEMENT_METADATA_ENABLED")? != "true" {
            return Err(denied());
        }
        let origin = get("CUSTODY_PUBLIC_ORIGIN")?;
        let url = Url::parse(&origin).map_err(|_| denied())?;
        if url.scheme() != "https"
            || url.origin().ascii_serialization() != origin
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || get("CUSTODY_NATIVE_ORIGIN")? != origin
        {
            return Err(denied());
        }
        let realm = get("CUSTODY_MANAGEMENT_METADATA_REALM")?;
        if realm.is_empty() || realm.len() > 256 || realm.chars().any(char::is_control) {
            return Err(denied());
        }
        Ok(Self { origin, realm })
    }
    #[cfg(feature = "identity")]
    pub(crate) fn actor_name(&self, bucket: &str) -> Result<String> {
        if bucket.len() != 64
            || !bucket
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(denied());
        }
        Ok(format!(
            "management-metadata:{}",
            digest(
                "oasis7.custody.management-metadata-actor.v1",
                &(&self.realm, &self.origin, bucket)
            )?
        ))
    }
}
