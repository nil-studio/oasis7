//! Durable Worker sessions survive actor eviction; missing records never reopen.
use crate::store;
use oasis7_custody_core::dev_test::{Grant, Sessions};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
const LIMIT: usize = 64 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    epoch: String,
    reference: String,
    service_grant_id: String,
    grant: Grant,
    caller: String,
    issued_at: u64,
    expires: u64,
    closed: bool,
}
fn key(service: &str, reference: &str) -> Result<String, &'static str> {
    if service.is_empty()
        || service.len() > 128
        || reference.len() != 66
        || !reference.ends_with("-1")
        || !reference.as_bytes()[..64]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return Err("session_unavailable");
    }
    let bytes = serde_json::to_vec(&(service, reference)).map_err(|_| "session_unavailable")?;
    Ok(format!(
        "dev-test-session-v1:{}",
        blake3::hash(&bytes).to_hex()
    ))
}
impl Record {
    fn reconstruct(&self) -> Result<Sessions, &'static str> {
        if self.version != 1
            || self.grant.grant_id != self.service_grant_id
            || self.grant.caller_id != self.caller
            || self.caller.is_empty()
            || self.caller.len() > 128
        {
            return Err("session_unavailable");
        }
        key(&self.service_grant_id, &self.reference)?;
        let mut sessions =
            Sessions::with_epoch(self.epoch.clone()).map_err(|_| "session_unavailable")?;
        let reference = sessions
            .open(&self.caller, &self.grant, self.expires, self.issued_at)
            .map_err(|_| "session_unavailable")?;
        if reference != self.reference {
            return Err("session_unavailable");
        }
        Ok(sessions)
    }
    fn decode(
        raw: &str,
        service: &str,
        reference: &str,
        caller: &str,
        now: u64,
    ) -> Result<Self, &'static str> {
        if raw.is_empty() || raw.len() > LIMIT {
            return Err("session_unavailable");
        }
        let record: Self = oasis7_agent_api::game::parse_bounded_json(raw.as_bytes(), LIMIT)
            .map_err(|_| "session_unavailable")?;
        if record.service_grant_id != service
            || record.reference != reference
            || record.caller != caller
            || record.closed
            || now < record.issued_at
            || now >= record.expires
        {
            return Err("session_unavailable");
        }
        record.reconstruct()?;
        Ok(record)
    }
    fn encode(&self) -> Result<String, &'static str> {
        let raw = serde_json::to_string(self).map_err(|_| "session_unavailable")?;
        if raw.len() > LIMIT {
            return Err("session_capacity_exhausted");
        }
        Ok(raw)
    }
}
pub(crate) struct Store<'a> {
    pub raw: &'a JsValue,
    pub service_grant_id: &'a str,
}
impl Store<'_> {
    fn load(
        &self,
        reference: &str,
        caller: &str,
        now: u64,
    ) -> Result<(String, String, Record), &'static str> {
        let key = key(self.service_grant_id, reference)?;
        let raw = store::load(self.raw, &key).map_err(|_| "session_unavailable")?;
        let record = Record::decode(&raw, self.service_grant_id, reference, caller, now)?;
        Ok((key, raw, record))
    }
    pub fn open(
        &self,
        caller: &str,
        grant: &Grant,
        expires: u64,
        now: u64,
    ) -> Result<String, &'static str> {
        if grant.grant_id != self.service_grant_id {
            return Err("session_scope_denied");
        }
        let epoch = crate::native::nonce()?;
        let mut sessions =
            Sessions::with_epoch(epoch.clone()).map_err(|_| "session_unavailable")?;
        let reference = sessions
            .open(caller, grant, expires, now)
            .map_err(|_| "session_scope_denied")?;
        let record = Record {
            version: 1,
            epoch,
            reference: reference.clone(),
            service_grant_id: self.service_grant_id.into(),
            grant: grant.clone(),
            caller: caller.into(),
            issued_at: now,
            expires,
            closed: false,
        };
        record.reconstruct()?;
        store::commit(
            self.raw,
            &key(self.service_grant_id, &reference)?,
            "",
            &record.encode()?,
        )
        .map_err(|_| "session_unavailable")?;
        Ok(reference)
    }
    pub fn policy_reference(
        &self,
        reference: &str,
        caller: &str,
        now: u64,
    ) -> Result<String, &'static str> {
        Ok(self.load(reference, caller, now)?.2.grant.grant_ref)
    }
    pub fn authorize(
        &self,
        reference: &str,
        caller: &str,
        current: &Grant,
        method: &str,
        now: u64,
    ) -> Result<(), &'static str> {
        let record = self.load(reference, caller, now)?.2;
        record
            .reconstruct()?
            .authorize(reference, caller, current, method, now)
            .map_err(|_| "session_scope_denied")?;
        Ok(())
    }
    pub fn close(&self, reference: &str, caller: &str, now: u64) -> Result<(), &'static str> {
        let (key, raw, mut record) = self.load(reference, caller, now)?;
        record
            .reconstruct()?
            .close(reference, caller)
            .map_err(|_| "session_unavailable")?;
        record.closed = true;
        store::commit(self.raw, &key, &raw, &record.encode()?).map_err(|_| "session_unavailable")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Record {
        let policy: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/policy.json")).unwrap();
        let grant: Grant = serde_json::from_value(policy["grants"][0].clone()).unwrap();
        Record {
            version: 1,
            epoch: "ab".repeat(32),
            reference: format!("{}-1", "ab".repeat(32)),
            service_grant_id: grant.grant_id.clone(),
            caller: grant.caller_id.clone(),
            grant,
            issued_at: 10,
            expires: 100,
            closed: false,
        }
    }
    #[test]
    fn durable_record_reconstructs_core_scope_without_refreshing_session() {
        let record = fixture();
        assert_eq!(
            key("service", &record.reference).unwrap(),
            "dev-test-session-v1:066b532ac8b80dcc2a40fd854432a5c056ccbfd4b2cb3471d745b05d7c33494c"
        );
        let raw = record.encode().unwrap();
        let loaded = Record::decode(&raw, "service", &record.reference, "caller", 20).unwrap();
        loaded
            .reconstruct()
            .unwrap()
            .authorize(&record.reference, "caller", &record.grant, "context", 20)
            .unwrap();
        let mut changed = record.grant.clone();
        changed.binding.authorization_version += 1;
        assert!(
            loaded
                .reconstruct()
                .unwrap()
                .authorize(&record.reference, "caller", &changed, "context", 20)
                .is_err()
        );
        changed = record.grant.clone();
        changed.move_targets.push([1, 2, 3]);
        assert!(
            loaded
                .reconstruct()
                .unwrap()
                .authorize(&record.reference, "caller", &changed, "context", 20)
                .is_err()
        );
    }
    #[test]
    fn missing_corrupt_closed_foreign_and_expired_records_fail_closed() {
        let r = fixture();
        let raw = r.encode().unwrap();
        for invalid in ["", "{}", "null", "garbage"] {
            assert!(Record::decode(invalid, "service", &r.reference, "caller", 20).is_err());
        }
        for (service, caller, now) in [
            ("other", "caller", 20),
            ("service", "other", 20),
            ("service", "caller", 9),
            ("service", "caller", 100),
        ] {
            assert!(Record::decode(&raw, service, &r.reference, caller, now).is_err());
        }
        let mut closed = r.clone();
        closed.closed = true;
        assert!(
            Record::decode(
                &closed.encode().unwrap(),
                "service",
                &r.reference,
                "caller",
                20
            )
            .is_err()
        );
        let mut wrong = r.clone();
        wrong.epoch = "cd".repeat(32);
        assert!(
            Record::decode(
                &wrong.encode().unwrap(),
                "service",
                &r.reference,
                "caller",
                20
            )
            .is_err()
        );
        let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        value["unknown"] = true.into();
        assert!(Record::decode(&value.to_string(), "service", &r.reference, "caller", 20).is_err());
        assert_ne!(
            key("service", &r.reference).unwrap(),
            key("other", &r.reference).unwrap()
        );
    }
}
