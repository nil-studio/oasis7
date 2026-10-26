//! Bounded consumed originals and permanent old-epoch tombstones.
use super::*;
use crate::approval::bytes_digest;
pub const MAX_RECOVERY_ACTIVATIONS: usize = 64;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryFenceAuditV1 {
    pub revision: DecimalU64,
    pub state_digest: String,
    pub epoch: DecimalU64,
    pub disabled: bool,
}
type Result<T> = core::result::Result<T, InvalidRecoveryActivation>;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppliedRecoveryActivationV1 {
    pub signed: SignedRecoveryActivationV1,
    pub applied_at: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LedgerWire {
    api_version: u8,
    policy: RecoveryAuthorityPolicyV1,
    object_id: String,
    initial_epoch: DecimalU64,
    revision: DecimalU64,
    fence: RecoveryFence,
    frozen_epochs: Vec<DecimalU64>,
    activations: Vec<AppliedRecoveryActivationV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "LedgerWire")]
pub struct RecoveryFenceLedgerV1 {
    api_version: u8,
    policy: RecoveryAuthorityPolicyV1,
    object_id: String,
    initial_epoch: DecimalU64,
    revision: DecimalU64,
    fence: RecoveryFence,
    frozen_epochs: Vec<DecimalU64>,
    activations: Vec<AppliedRecoveryActivationV1>,
}
impl TryFrom<LedgerWire> for RecoveryFenceLedgerV1 {
    type Error = &'static str;
    fn try_from(w: LedgerWire) -> core::result::Result<Self, Self::Error> {
        let value = Self {
            api_version: w.api_version,
            policy: w.policy,
            object_id: w.object_id,
            initial_epoch: w.initial_epoch,
            revision: w.revision,
            fence: w.fence,
            frozen_epochs: w.frozen_epochs,
            activations: w.activations,
        };
        value
            .validate()
            .map_err(|_| "invalid recovery fence ledger")?;
        Ok(value)
    }
}
impl RecoveryFenceLedgerV1 {
    /// Legacy active records without independent history cannot bootstrap.
    pub fn new(
        policy: RecoveryAuthorityPolicyV1,
        object_id: String,
        initial: RecoveryFence,
    ) -> Result<Self> {
        policy.validate()?;
        if !initial.disabled()
            || object_id.is_empty()
            || object_id.len() > 256
            || !object_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
        {
            return Err(InvalidRecoveryActivation);
        }
        Ok(Self {
            api_version: 1,
            policy,
            object_id,
            initial_epoch: DecimalU64(initial.epoch()),
            revision: DecimalU64(0),
            fence: initial,
            frozen_epochs: vec![DecimalU64(initial.epoch())],
            activations: Vec::new(),
        })
    }
    pub fn policy(&self) -> &RecoveryAuthorityPolicyV1 {
        &self.policy
    }
    pub fn object_id(&self) -> &str {
        &self.object_id
    }
    pub fn fence(&self) -> RecoveryFence {
        self.fence
    }
    pub fn revision(&self) -> DecimalU64 {
        self.revision
    }
    pub fn frozen_epochs(&self) -> &[DecimalU64] {
        &self.frozen_epochs
    }
    pub fn activations(&self) -> &[AppliedRecoveryActivationV1] {
        &self.activations
    }
    pub fn digest(&self) -> Result<String> {
        let bytes =
            oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.recovery-ledger.v1", self))
                .map_err(|_| InvalidRecoveryActivation)?;
        Ok(bytes_digest(&bytes))
    }
    pub fn current<'a>(&'a self, digest: &'a str) -> RecoveryActivationCurrent<'a> {
        RecoveryActivationCurrent {
            environment: &self.policy.environment,
            authority_id: &self.policy.authority_id,
            object_id: &self.object_id,
            state_digest: digest,
            fence: &self.fence,
        }
    }
    pub fn original(&self, operation_id: &str) -> Option<&AppliedRecoveryActivationV1> {
        self.activations
            .iter()
            .find(|v| v.signed.manifest.operation_id == operation_id)
    }
    fn remember_frozen(&mut self, epoch: u64) {
        if !self.frozen_epochs.iter().any(|v| v.0 == epoch) {
            self.frozen_epochs.push(DecimalU64(epoch));
        }
    }
    fn disable_raw(&mut self) -> Result<()> {
        if !self.fence.disabled() {
            self.fence
                .tighten(self.fence.epoch(), true)
                .map_err(|_| InvalidRecoveryActivation)?;
            self.remember_frozen(self.fence.epoch());
            self.revision.0 = self
                .revision
                .0
                .checked_add(1)
                .ok_or(InvalidRecoveryActivation)?;
        }
        Ok(())
    }
    pub fn disable(&mut self) -> Result<()> {
        self.disable_raw()?;
        self.validate()
    }
    fn apply_raw(&mut self, original: AppliedRecoveryActivationV1) -> Result<()> {
        self.remember_frozen(self.fence.epoch());
        self.fence = RecoveryFence::new(original.signed.manifest.to_epoch.0, false);
        self.activations.push(original);
        self.revision.0 = self
            .revision
            .0
            .checked_add(1)
            .ok_or(InvalidRecoveryActivation)?;
        Ok(())
    }
    /// Retry returns the current fence, never the old activation's active state.
    pub fn activate(
        &mut self,
        signed: SignedRecoveryActivationV1,
        now: u64,
        verifier: &mut impl RecoveryActivationVerifier,
    ) -> Result<RecoveryFence> {
        if let Some(old) = self.original(&signed.manifest.operation_id) {
            return if old.signed == signed {
                Ok(self.fence)
            } else {
                Err(InvalidRecoveryActivation)
            };
        }
        if self.activations.len() >= MAX_RECOVERY_ACTIVATIONS {
            return Err(InvalidRecoveryActivation);
        }
        let digest = self.digest()?;
        signed.authorize(&self.policy, &self.current(&digest), now, verifier)?;
        let mut next = self.clone();
        next.apply_raw(AppliedRecoveryActivationV1 {
            signed,
            applied_at: DecimalU64(now),
        })?;
        next.validate()?;
        *self = next;
        Ok(self.fence)
    }
    /// Structural recovery does not authenticate arbitrary snapshots. Only the
    /// independent SQL authority may retain a ledger after real verification.
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1 || self.activations.len() > MAX_RECOVERY_ACTIVATIONS {
            return Err(InvalidRecoveryActivation);
        }
        let mut prefix = Self::new(
            self.policy.clone(),
            self.object_id.clone(),
            RecoveryFence::new(self.initial_epoch.0, true),
        )?;
        let mut operations = std::collections::BTreeSet::new();
        for original in &self.activations {
            if !operations.insert(&original.signed.manifest.operation_id) {
                return Err(InvalidRecoveryActivation);
            }
            if prefix.digest()? != original.signed.manifest.previous_state_digest {
                prefix.disable_raw()?;
            }
            let digest = prefix.digest()?;
            original.signed.signature_checks(
                &prefix.policy,
                &prefix.current(&digest),
                original.applied_at.0,
            )?;
            prefix.apply_raw(original.clone())?;
        }
        if &prefix != self {
            prefix.disable_raw()?;
        }
        if &prefix == self {
            Ok(())
        } else {
            Err(InvalidRecoveryActivation)
        }
    }
    /// Reconstruct every authority revision so audit rows cannot be accepted
    /// merely because their count matches a structurally valid ledger.
    pub fn audit_history(&self) -> Result<Vec<RecoveryFenceAuditV1>> {
        self.validate()?;
        let mut prefix = Self::new(
            self.policy.clone(),
            self.object_id.clone(),
            RecoveryFence::new(self.initial_epoch.0, true),
        )?;
        let mut audit = Vec::new();
        for original in &self.activations {
            if prefix.digest()? != original.signed.manifest.previous_state_digest {
                let revision = prefix.revision;
                prefix.disable_raw()?;
                if prefix.revision != revision {
                    audit.push(prefix.audit()?);
                }
            }
            prefix.apply_raw(original.clone())?;
            audit.push(prefix.audit()?);
        }
        if &prefix != self {
            let revision = prefix.revision;
            prefix.disable_raw()?;
            if prefix.revision != revision {
                audit.push(prefix.audit()?);
            }
        }
        if &prefix != self || audit.len() as u64 != self.revision.0 {
            return Err(InvalidRecoveryActivation);
        }
        Ok(audit)
    }
    fn audit(&self) -> Result<RecoveryFenceAuditV1> {
        Ok(RecoveryFenceAuditV1 {
            revision: self.revision,
            state_digest: self.digest()?,
            epoch: DecimalU64(self.fence.epoch()),
            disabled: self.fence.disabled(),
        })
    }
}
