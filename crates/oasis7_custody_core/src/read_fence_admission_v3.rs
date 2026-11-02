//! Shared authoritative FENCES admission inventory, independent of Current.
//! A persisted signed claim can validate history but never recreate a first send.
use crate::{
    DecimalU64, execution_fence_dispatch_v2::*, native_cleanup_v2::*, read_fence_dispatch_v3::*,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
fn bad() -> String {
    "fence admission inventory unavailable".into()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FenceAdmissionProfileV3 {
    ExecutionV2,
    ReconcileReadV3,
}
impl FenceAdmissionProfileV3 {
    pub fn birth_key(self) -> &'static str {
        match self {
            Self::ExecutionV2 => "execution-fence-read-birth-v2",
            Self::ReconcileReadV3 => "read-fence-read-birth-v3",
        }
    }
    pub fn table_prefix(self) -> &'static str {
        match self {
            Self::ExecutionV2 => "execution_fence_admission_v2",
            Self::ReconcileReadV3 => "read_fence_admission_v3",
        }
    }
}
pub const FENCE_BUSINESS_RECORD_LIMIT_V3: usize = 2;
pub const FENCE_KNOWN_BIRTH_COUNT_V3: usize = 2;
pub fn fence_record_probe_limit_v3() -> Result<usize, String> {
    FENCE_BUSINESS_RECORD_LIMIT_V3
        .checked_add(FENCE_KNOWN_BIRTH_COUNT_V3)
        .and_then(|v| v.checked_add(1))
        .ok_or_else(bad)
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "profile",
    content = "proof",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum FenceAdmissionProofV3 {
    ExecutionV2(SignedExecutionFenceReadV2),
    ReconcileReadV3(SignedReadFenceReadV3),
}
impl FenceAdmissionProofV3 {
    pub fn profile(&self) -> FenceAdmissionProfileV3 {
        match self {
            Self::ExecutionV2(_) => FenceAdmissionProfileV3::ExecutionV2,
            Self::ReconcileReadV3(_) => FenceAdmissionProfileV3::ReconcileReadV3,
        }
    }
    fn issuer(&self) -> &AccountMachineIssuerV2 {
        match self {
            Self::ExecutionV2(p) => &p.claim.binding.issuer,
            Self::ReconcileReadV3(p) => &p.claim.binding.issuer,
        }
    }
    fn attempt(&self) -> &str {
        match self {
            Self::ExecutionV2(p) => &p.claim.binding.fence_attempt_id,
            Self::ReconcileReadV3(p) => &p.claim.binding.fence_attempt_id,
        }
    }
    pub fn expected_epoch(&self) -> DecimalU64 {
        match self {
            Self::ExecutionV2(p) => p.claim.binding.expected_service_epoch,
            Self::ReconcileReadV3(p) => p.claim.binding.expected_service_epoch,
        }
    }
    pub fn expires_at(&self) -> u64 {
        match self {
            Self::ExecutionV2(p) => p.claim.binding.expires_at.0,
            Self::ReconcileReadV3(p) => p.claim.binding.expires_at.0,
        }
    }
    pub fn digest(&self) -> Result<String, String> {
        hash(self)
    }
    pub fn once_id(&self) -> Result<String, String> {
        let i = self.issuer();
        hash(&(
            "oasis7.custody.fence-admission-once.v3",
            self.profile(),
            &i.namespace,
            &i.service_id,
            &i.environment,
            self.attempt(),
        ))
    }
    fn stable_scope(&self) -> Result<String, String> {
        let i = self.issuer();
        hash(&(
            "oasis7.custody.fence-admission-scope.v3",
            self.profile(),
            &i.namespace,
            &i.service_id,
            &i.environment,
        ))
    }
    fn verify(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        object: &str,
        at: u64,
    ) -> Result<(), String> {
        match self {
            Self::ExecutionV2(p) => {
                if p.claim.binding.fence_object_id != object {
                    return Err(bad());
                }
                p.verify(pin, floor, at)
            }
            Self::ReconcileReadV3(p) => p.verify(pin, floor, object, at),
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceAdmissionVerifierV3 {
    pub pin: AccountMachineIssuerPinV2,
    pub floor: AccountMachineIssuerFloorV2,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceAdmissionConfigV3 {
    pub profile: FenceAdmissionProfileV3,
    pub capacity: DecimalU64,
    pub active: Option<FenceAdmissionVerifierV3>,
    pub history: Vec<FenceAdmissionVerifierV3>,
}
impl FenceAdmissionConfigV3 {
    pub fn validate(&self) -> Result<(), String> {
        if self.capacity.0 == 0
            || u32::try_from(self.capacity.0).is_err()
            || self.history.is_empty()
        {
            return Err(bad());
        }
        let expected = match self.profile {
            FenceAdmissionProfileV3::ExecutionV2 => AccountMachinePurposeV2::ExecutionFenceReadV2,
            FenceAdmissionProfileV3::ReconcileReadV3 => AccountMachinePurposeV2::ReadFenceReadV3,
        };
        let mut seen = BTreeSet::new();
        for v in &self.history {
            if v.pin.allowed_purpose != expected || !seen.insert(hash(v)?) {
                return Err(bad());
            }
        }
        if let Some(active) = &self.active {
            if !self.history.iter().any(|v| v == active) {
                return Err(bad());
            }
        }
        Ok(())
    }
    fn historical_verifier(
        &self,
        proof: &FenceAdmissionProofV3,
        object: &str,
        at: u64,
        digest: &str,
    ) -> Result<&FenceAdmissionVerifierV3, String> {
        self.validate()?;
        if proof.profile() != self.profile {
            return Err(bad());
        }
        let verifier = self
            .history
            .iter()
            .find(|v| hash(*v).ok().as_deref() == Some(digest))
            .ok_or_else(bad)?;
        proof.verify(&verifier.pin, &verifier.floor, object, at)?;
        Ok(verifier)
    }
    fn current_verifier(
        &self,
        proof: &FenceAdmissionProofV3,
        object: &str,
        at: u64,
    ) -> Result<&FenceAdmissionVerifierV3, String> {
        self.validate()?;
        if proof.profile() != self.profile {
            return Err(bad());
        }
        let verifier = self.active.as_ref().ok_or_else(bad)?;
        proof.verify(&verifier.pin, &verifier.floor, object, at)?;
        Ok(verifier)
    }
    pub fn verify_current(
        &self,
        proof: &FenceAdmissionProofV3,
        object: &str,
        at: u64,
    ) -> Result<(), String> {
        self.current_verifier(proof, object, at).map(|_| ())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceAdmissionOnceV3 {
    pub id: String,
    pub ordinal: DecimalU64,
    pub claimed_at: DecimalU64,
    pub verifier_digest: String,
    pub proof: FenceAdmissionProofV3,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceAdmissionBirthV3 {
    pub api_version: u8,
    pub profile: FenceAdmissionProfileV3,
    pub object_id: String,
    pub stable_scope: String,
    pub capacity: DecimalU64,
    pub first: FenceAdmissionOnceV3,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceAdmissionHeaderV3 {
    pub birth_digest: String,
    pub count: DecimalU64,
    pub head_digest: String,
}
/// Separate persisted floor remains mandatory even when the header is missing.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceAdmissionFloorV3 {
    pub count: DecimalU64,
    pub head_digest: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceAdmissionRowsV3 {
    pub birth: FenceAdmissionBirthV3,
    pub header: FenceAdmissionHeaderV3,
    pub floor: FenceAdmissionFloorV3,
    pub once: Vec<FenceAdmissionOnceV3>,
}
impl FenceAdmissionRowsV3 {
    pub fn validate(&self, config: &FenceAdmissionConfigV3, object: &str) -> Result<(), String> {
        config.validate()?;
        let b = &self.birth;
        let h = &self.header;
        if b.api_version != 3
            || b.profile != config.profile
            || b.capacity != config.capacity
            || b.object_id != object
            || b.first.ordinal.0 != 1
            || b.first.proof.profile() != b.profile
            || b.stable_scope != b.first.proof.stable_scope()?
            || h.birth_digest != hash(b)?
            || h.count.0 == 0
            || h.count.0 > b.capacity.0
            || self.floor.count != h.count
            || self.floor.head_digest != h.head_digest
            || u64::try_from(self.once.len()).map_err(|_| bad())? != h.count.0
        {
            return Err(bad());
        }
        let mut ordinals = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut first = false;
        let mut last = None;
        for row in &self.once {
            let verifier = config.historical_verifier(
                &row.proof,
                object,
                row.claimed_at.0,
                &row.verifier_digest,
            )?;
            if row.id != row.proof.once_id()?
                || row.ordinal.0 == 0
                || row.ordinal.0 > h.count.0
                || !ordinals.insert(row.ordinal.0)
                || !ids.insert(row.id.clone())
                || row.proof.stable_scope()? != b.stable_scope
                || row.verifier_digest != hash(verifier)?
            {
                return Err(bad());
            }
            if row.ordinal.0 == 1 {
                if row != &b.first {
                    return Err(bad());
                }
                first = true;
            }
            if row.ordinal == h.count {
                last = Some(hash(row)?);
            }
        }
        if !first || last.as_deref() != Some(h.head_digest.as_str()) {
            return Err(bad());
        }
        Ok(())
    }
    /// Called only inside the actual receiver SQL after server config and proof
    /// verification. The returned history is not a send handle or Current.
    pub fn consume(
        old: Option<&Self>,
        config: &FenceAdmissionConfigV3,
        object: &str,
        proof: FenceAdmissionProofV3,
        now: u64,
    ) -> Result<Self, String> {
        let verifier = config.current_verifier(&proof, object, now)?;
        let id = proof.once_id()?;
        let ordinal = if let Some(old) = old {
            old.validate(config, object)?;
            if old.once.iter().any(|v| v.id == id) {
                return Err(bad());
            }
            old.header.count.0.checked_add(1).ok_or_else(bad)?
        } else {
            1
        };
        if ordinal > config.capacity.0 {
            return Err(bad());
        }
        let row = FenceAdmissionOnceV3 {
            id,
            ordinal: DecimalU64(ordinal),
            claimed_at: DecimalU64(now),
            verifier_digest: hash(verifier)?,
            proof,
        };
        let birth = old
            .map(|v| v.birth.clone())
            .unwrap_or(FenceAdmissionBirthV3 {
                api_version: 3,
                profile: config.profile,
                object_id: object.into(),
                stable_scope: row.proof.stable_scope()?,
                capacity: config.capacity,
                first: row.clone(),
            });
        let mut once = old.map(|v| v.once.clone()).unwrap_or_default();
        once.push(row.clone());
        let count = DecimalU64(ordinal);
        let head_digest = hash(&row)?;
        let next = Self {
            header: FenceAdmissionHeaderV3 {
                birth_digest: hash(&birth)?,
                count,
                head_digest: head_digest.clone(),
            },
            floor: FenceAdmissionFloorV3 { count, head_digest },
            birth,
            once,
        };
        next.validate(config, object)?;
        Ok(next)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceBusinessRecordV3 {
    pub id: String,
    pub payload: String,
}
/// Project only the two fully verified births. Unknown or partial state is
/// rejected before any legacy initializer can run. Business bytes are untouched.
#[derive(Clone, PartialEq, Eq)]
pub struct FenceAdmissionRecordKeyV3 {
    pub id: String,
    pub birth_payload: Option<String>,
}
pub fn verify_fence_birth_inventory_v3(
    records: &[FenceAdmissionRecordKeyV3],
    execution: Option<(&FenceAdmissionRowsV3, &FenceAdmissionConfigV3)>,
    read: Option<(&FenceAdmissionRowsV3, &FenceAdmissionConfigV3)>,
    object: &str,
) -> Result<(), String> {
    if records.len() >= fence_record_probe_limit_v3()? {
        return Err(bad());
    }
    let mut ids = BTreeSet::new();
    for r in records {
        if !ids.insert(r.id.clone()) {
            return Err(bad());
        }
        if r.id == "recovery-fence-v1" || r.id == "recovery-fence-ledger-v1" {
            if r.birth_payload.is_some() {
                return Err(bad());
            }
        } else if r.id != FenceAdmissionProfileV3::ExecutionV2.birth_key()
            && r.id != FenceAdmissionProfileV3::ReconcileReadV3.birth_key()
        {
            return Err(bad());
        }
    }
    for (profile, selected) in [
        (FenceAdmissionProfileV3::ExecutionV2, execution),
        (FenceAdmissionProfileV3::ReconcileReadV3, read),
    ] {
        let birth = records.iter().find(|r| r.id == profile.birth_key());
        match (birth, selected) {
            (None, None) => (),
            (Some(row), Some((saved, config))) => {
                if saved.birth.profile != profile || config.profile != profile {
                    return Err(bad());
                }
                saved.validate(config, object)?;
                if row.birth_payload.as_deref()
                    != Some(
                        serde_json::to_string(&saved.birth)
                            .map_err(|_| bad())?
                            .as_str(),
                    )
                {
                    return Err(bad());
                }
            }
            _ => return Err(bad()),
        }
    }
    Ok(())
}

pub fn project_fence_business_records_v3(
    records: &[FenceBusinessRecordV3],
    execution: Option<(&FenceAdmissionRowsV3, &FenceAdmissionConfigV3)>,
    read: Option<(&FenceAdmissionRowsV3, &FenceAdmissionConfigV3)>,
    object: &str,
) -> Result<Vec<FenceBusinessRecordV3>, String> {
    let keys: Vec<_> = records
        .iter()
        .map(|r| FenceAdmissionRecordKeyV3 {
            id: r.id.clone(),
            birth_payload: if r.id == FenceAdmissionProfileV3::ExecutionV2.birth_key()
                || r.id == FenceAdmissionProfileV3::ReconcileReadV3.birth_key()
            {
                Some(r.payload.clone())
            } else {
                None
            },
        })
        .collect();
    verify_fence_birth_inventory_v3(&keys, execution, read, object)?;
    let mut business = Vec::new();
    for row in records {
        if row.id == "recovery-fence-v1" || row.id == "recovery-fence-ledger-v1" {
            business.push(row.clone());
        } else if row.id != FenceAdmissionProfileV3::ExecutionV2.birth_key()
            && row.id != FenceAdmissionProfileV3::ReconcileReadV3.birth_key()
        {
            return Err(bad());
        }
    }
    if business.len() > FENCE_BUSINESS_RECORD_LIMIT_V3 {
        return Err(bad());
    }
    Ok(business)
}

/// Existing fixed read-only HTTP body ceiling; wrapping metadata must not shrink it.
pub const MAX_FENCE_READ_REQUEST_BYTES_V3: usize = 65_536;
pub fn fence_admission_row_limit_v3() -> Result<usize, String> {
    let h = "f".repeat(64);
    let n = u64::MAX.to_string();
    let envelope = serde_json::json!({"api_version":3,"profile":"reconcile_read_v3","object_id":h,"stable_scope":"f".repeat(64),"capacity":n,"first":{"id":"f".repeat(64),"ordinal":u64::MAX.to_string(),"claimed_at":u64::MAX.to_string(),"verifier_digest":"f".repeat(64),"proof":{"profile":"reconcile_read_v3","proof":null}}});
    MAX_FENCE_READ_REQUEST_BYTES_V3
        .checked_add(serde_json::to_vec(&envelope).map_err(|_| bad())?.len())
        .ok_or_else(bad)
}

/// Four original allowed business/internal tables, three tables per compiled
/// profile, at most one automatic primary-key index per known table, plus probe.
pub fn fence_catalogue_probe_limit_v3() -> Result<usize, String> {
    4usize
        .checked_add(FENCE_KNOWN_BIRTH_COUNT_V3.checked_mul(3).ok_or_else(bad)?)
        .and_then(|n| n.checked_mul(2))
        .and_then(|n| n.checked_add(1))
        .ok_or_else(bad)
}
