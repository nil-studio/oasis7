//! Independent Read Directory birth/header/full-once. Scope and account IDs
//! come from protected namespace derivation; a row is historical, never Current.
use crate::{
    DecimalU64,
    native_cleanup_v2::{hash, hex64, id},
    read_directory_v3::*,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
fn denied() -> String {
    "read directory admission rejected".into()
}
pub const READ_DIRECTORY_BIRTH_KEY_V3: &str = "manager-directory-read-birth-v3";
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDirectoryAdmissionConfigV3 {
    pub capacity: DecimalU64,
    pub active: Option<ReadDirectoryVerifierV3>,
    pub history: Vec<ReadDirectoryVerifierV3>,
}
impl ReadDirectoryAdmissionConfigV3 {
    pub fn validate(&self) -> Result<(), String> {
        if self.capacity.0 == 0
            || u32::try_from(self.capacity.0).is_err()
            || self.history.is_empty()
        {
            return Err(denied());
        }
        let mut seen = BTreeSet::new();
        for verifier in &self.history {
            verifier.validate()?;
            if !seen.insert(verifier.digest()?) {
                return Err(denied());
            }
        }
        if self
            .active
            .as_ref()
            .is_some_and(|v| !self.history.contains(v))
        {
            return Err(denied());
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDirectoryAdmissionScopeV3 {
    pub object_id: String,
    pub environment: String,
    pub manager_identity_namespace: String,
    pub manager_identity_instance_epoch: DecimalU64,
    pub manager_account_id: String,
    pub issuer_namespace: String,
    pub issuer_service_id: String,
}
impl ReadDirectoryAdmissionScopeV3 {
    pub fn validate(&self) -> Result<(), String> {
        if !hex64(&self.object_id)
            || [
                &self.environment,
                &self.manager_identity_namespace,
                &self.manager_account_id,
                &self.issuer_namespace,
                &self.issuer_service_id,
            ]
            .iter()
            .any(|s| !id(s))
            || self.manager_identity_instance_epoch.0 == 0
        {
            return Err(denied());
        }
        Ok(())
    }
    fn check_request(&self, proof: &SignedReadDirectoryV3) -> Result<(), String> {
        self.validate()?;
        let c = &proof.claim.coordinator.claim;
        let r = &proof.claim.query.manager_reference;
        if proof.claim.directory_object_id != self.object_id
            || c.issuer.environment != self.environment
            || c.issuer.namespace != self.issuer_namespace
            || c.issuer.service_id != self.issuer_service_id
            || r.identity_namespace != self.manager_identity_namespace
            || r.identity_instance_epoch != self.manager_identity_instance_epoch
            || r.account_id != self.manager_account_id
        {
            return Err(denied());
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDirectoryOnceV3 {
    pub ordinal: DecimalU64,
    pub claimed_at: DecimalU64,
    pub verifier_digest: String,
    pub request: SignedReadDirectoryV3,
}
impl ReadDirectoryOnceV3 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&("oasis7.custody.read-directory-once.v3", self))
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDirectoryBirthV3 {
    pub api_version: u8,
    pub scope: ReadDirectoryAdmissionScopeV3,
    pub capacity: DecimalU64,
    pub first: ReadDirectoryOnceV3,
}
impl ReadDirectoryBirthV3 {
    pub fn digest(&self) -> Result<String, String> {
        hash(&("oasis7.custody.read-directory-birth.v3", self))
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDirectoryHeaderV3 {
    pub api_version: u8,
    pub scope: ReadDirectoryAdmissionScopeV3,
    pub capacity: DecimalU64,
    pub birth_digest: String,
    pub count: DecimalU64,
    pub head_digest: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadDirectoryRowsV3 {
    pub birth: ReadDirectoryBirthV3,
    pub header: ReadDirectoryHeaderV3,
    pub claims: BTreeMap<String, ReadDirectoryOnceV3>,
}
fn once_id(request: &SignedReadDirectoryV3) -> Result<String, String> {
    let c = &request.claim.coordinator.claim;
    hash(&(
        "oasis7.custody.read-directory-physical-once.v3",
        &c.issuer.namespace,
        &c.issuer.service_id,
        &c.issuer.environment,
        &request.claim.physical_attempt_id,
    ))
}
impl ReadDirectoryRowsV3 {
    /// The resolver is pure deployment namespace ID derivation, not a Current
    /// predicate or caller-supplied proof of payment.
    pub fn verify<F>(
        &self,
        config: &ReadDirectoryAdmissionConfigV3,
        scope: &ReadDirectoryAdmissionScopeV3,
        mut account_id: F,
    ) -> Result<(), String>
    where
        F: FnMut(&str) -> Result<String, String>,
    {
        config.validate()?;
        scope.validate()?;
        let n = u64::try_from(self.claims.len()).map_err(|_| denied())?;
        if self.birth.api_version != 3
            || self.header.api_version != 3
            || self.birth.scope != *scope
            || self.header.scope != *scope
            || self.birth.capacity != config.capacity
            || self.header.capacity != config.capacity
            || n == 0
            || n > config.capacity.0
            || self.header.count.0 != n
            || self.header.birth_digest != self.birth.digest()?
        {
            return Err(denied());
        }
        let mut ordered = BTreeMap::new();
        for (key, row) in &self.claims {
            scope.check_request(&row.request)?;
            row.request.wire_bytes()?;
            if *key != once_id(&row.request)?
                || row.ordinal.0 == 0
                || row.ordinal.0 > n
                || ordered.insert(row.ordinal.0, row).is_some()
            {
                return Err(denied());
            }
            let mut selected = config
                .history
                .iter()
                .filter(|v| v.digest().ok().as_deref() == Some(row.verifier_digest.as_str()));
            let verifier = selected.next().ok_or_else(denied)?;
            if selected.next().is_some() {
                return Err(denied());
            }
            let actual_account = account_id(&row.request.claim.query.custody_account_id)?;
            row.request.verify(
                verifier,
                &scope.object_id,
                &actual_account,
                row.claimed_at.0,
            )?;
        }
        let first = ordered.get(&1).ok_or_else(denied)?;
        let last = ordered.get(&n).ok_or_else(denied)?;
        if **first != self.birth.first || self.header.head_digest != last.digest()? {
            return Err(denied());
        }
        for ordinal in 1..=n {
            if !ordered.contains_key(&ordinal) {
                return Err(denied());
            }
        }
        Ok(())
    }
    pub fn consume<F>(
        old: Option<Self>,
        config: &ReadDirectoryAdmissionConfigV3,
        scope: ReadDirectoryAdmissionScopeV3,
        request: SignedReadDirectoryV3,
        actual_account: &str,
        now: u64,
        mut account_id: F,
    ) -> Result<(Self, ReadDirectoryFirstV3), String>
    where
        F: FnMut(&str) -> Result<String, String>,
    {
        config.validate()?;
        scope.check_request(&request)?;
        let active = config.active.as_ref().ok_or_else(denied)?;
        request.wire_bytes()?;
        request.verify(active, &scope.object_id, actual_account, now)?;
        let key = once_id(&request)?;
        let count = old.as_ref().map(|r| r.header.count.0).unwrap_or(0);
        if let Some(rows) = &old {
            rows.verify(config, &scope, &mut account_id)?;
            if rows.claims.contains_key(&key) {
                return Err(denied());
            }
        }
        if count >= config.capacity.0 {
            return Err(denied());
        }
        let next_count = count.checked_add(1).ok_or_else(denied)?;
        let row = ReadDirectoryOnceV3 {
            ordinal: DecimalU64(next_count),
            claimed_at: DecimalU64(now),
            verifier_digest: active.digest()?,
            request: request.clone(),
        };
        let mut next = if let Some(rows) = old {
            rows
        } else {
            let birth = ReadDirectoryBirthV3 {
                api_version: 3,
                scope: scope.clone(),
                capacity: config.capacity,
                first: row.clone(),
            };
            Self {
                header: ReadDirectoryHeaderV3 {
                    api_version: 3,
                    scope: scope.clone(),
                    capacity: config.capacity,
                    birth_digest: birth.digest()?,
                    count: DecimalU64(0),
                    head_digest: String::new(),
                },
                birth,
                claims: BTreeMap::new(),
            }
        };
        next.header.count = DecimalU64(next_count);
        next.header.head_digest = row.digest()?;
        next.claims.insert(key, row);
        next.verify(config, &scope, &mut account_id)?;
        let handle = ReadDirectoryFirstV3 {
            proof_digest: request.digest()?,
            claimed_at: now,
            until: request.claim.expires_at.0,
        };
        Ok((next, handle))
    }
}
/// First receipt only. Stored rows cannot reconstruct an observation handle.
pub struct ReadDirectoryFirstV3 {
    proof_digest: String,
    claimed_at: u64,
    until: u64,
}
impl ReadDirectoryFirstV3 {
    pub fn proof_digest(&self) -> &str {
        &self.proof_digest
    }
    pub fn require_observed(&self, observed_at: u64, now: u64) -> Result<(), String> {
        if observed_at < self.claimed_at
            || observed_at > now
            || now >= self.until
            || now < self.claimed_at
        {
            Err(denied())
        } else {
            Ok(())
        }
    }
}

/// Bound derived from the actual accepted wire profile plus checked serialized
/// metadata. A legal maximum-size signed reply is never shrunk to make room for
/// a storage wrapper; HTTP's independent wire limit is unchanged.
pub fn read_directory_row_bound_v3(scope: &ReadDirectoryAdmissionScopeV3) -> Result<usize, String> {
    scope.validate()?;
    let maximum = DecimalU64(u64::MAX);
    let hash = "f".repeat(64);
    let once = serde_json::to_vec(&serde_json::json!({
        "ordinal": maximum, "claimed_at": maximum,
        "verifier_digest": hash, "request": null,
    }))
    .map_err(|_| denied())?
    .len()
    .checked_sub(4)
    .ok_or_else(denied)?;
    let birth = serde_json::to_vec(&serde_json::json!({
        "api_version": 3, "scope": scope, "capacity": maximum, "first": null,
    }))
    .map_err(|_| denied())?
    .len()
    .checked_sub(4)
    .ok_or_else(denied)?;
    let header = serde_json::to_vec(&ReadDirectoryHeaderV3 {
        api_version: 3,
        scope: scope.clone(),
        capacity: maximum,
        birth_digest: hash.clone(),
        count: maximum,
        head_digest: hash,
    })
    .map_err(|_| denied())?
    .len();
    let row = READ_DIRECTORY_MAX_WIRE_BYTES_V3
        .checked_add(once)
        .and_then(|n| n.checked_add(birth))
        .ok_or_else(denied)?;
    Ok(row.max(header))
}
impl ReadDirectoryRowsV3 {
    pub fn decode_storage_rows(
        birth: &[u8],
        header: &[u8],
        claims: &[(String, String)],
        scope: &ReadDirectoryAdmissionScopeV3,
        config: &ReadDirectoryAdmissionConfigV3,
    ) -> Result<Self, String> {
        config.validate()?;
        let bound = read_directory_row_bound_v3(scope)?;
        if claims.is_empty()
            || u64::try_from(claims.len()).map_err(|_| denied())? > config.capacity.0
        {
            return Err(denied());
        }
        let mut decoded = BTreeMap::new();
        for (key, raw) in claims {
            if !hex64(key)
                || decoded
                    .insert(
                        key.clone(),
                        decode_saved_read_directory_v3(raw.as_bytes(), bound)?,
                    )
                    .is_some()
            {
                return Err(denied());
            }
        }
        Ok(Self {
            birth: decode_saved_read_directory_v3(birth, bound)?,
            header: decode_saved_read_directory_v3(header, bound)?,
            claims: decoded,
        })
    }
    pub fn storage_transition(
        &self,
        request: &SignedReadDirectoryV3,
    ) -> Result<Vec<String>, String> {
        let key = once_id(request)?;
        let row = self.claims.get(&key).ok_or_else(denied)?;
        let bound = read_directory_row_bound_v3(&self.birth.scope)?;
        let values = vec![
            serde_json::to_string(&self.birth),
            serde_json::to_string(&self.header),
            Ok(key),
            serde_json::to_string(row),
            Ok(request.claim.expires_at.0.to_string()),
        ];
        // The key and decimal deadline are independently fixed-size. Only the
        // three stored JSON rows use the derived wrapper bound.
        let result: Vec<String> = values
            .into_iter()
            .collect::<Result<_, _>>()
            .map_err(|_| denied())?;
        if [0, 1, 3].into_iter().any(|i| result[i].len() > bound) {
            return Err(denied());
        }
        Ok(result)
    }
}

/// Metadata-only scan ceiling derived from the existing `id` profile (256
/// UTF-8 bytes). Backslash has the largest permitted JSON expansion per byte.
/// This does not authorize any scope or expand the HTTP wire profile.
pub fn read_directory_scan_row_bound_v3() -> Result<usize, String> {
    let largest_id = "\\".repeat(256);
    read_directory_row_bound_v3(&ReadDirectoryAdmissionScopeV3 {
        object_id: "f".repeat(64),
        environment: largest_id.clone(),
        manager_identity_namespace: largest_id.clone(),
        manager_identity_instance_epoch: DecimalU64(u64::MAX),
        manager_account_id: largest_id.clone(),
        issuer_namespace: largest_id.clone(),
        issuer_service_id: largest_id,
    })
}
impl ReadDirectoryBirthV3 {
    pub fn decode_storage(raw: &[u8]) -> Result<Self, String> {
        decode_saved_read_directory_v3(raw, read_directory_scan_row_bound_v3()?)
    }
}
