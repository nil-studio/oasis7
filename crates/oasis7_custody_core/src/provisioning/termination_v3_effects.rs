//! Complete immutable effect candidates and independent once send/lookup slots.
//! Lookup and recomputation never consume the other permanent effect slot.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationLookupKindV3 {
    Automatic,
    FreshCold,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationKeyTouchAttemptV3 {
    pub id: String,
    pub claimed_at: UnixMillis,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationEffectSlotV3 {
    pub candidate: OwnerTerminationCandidateV3,
    pub allocated_at: UnixMillis,
    pub initial_key_touch: Option<TerminationKeyTouchAttemptV3>,
    pub recomputations: BTreeMap<String, UnixMillis>,
    pub original: Option<OwnerTerminationSignedV3>,
    pub send_claimed_at: Option<UnixMillis>,
    pub lookups: BTreeMap<TerminationLookupKindV3, TerminationKeyTouchAttemptV3>,
}
impl AccountProvisionApprovalState {
    fn termination_v3_effect_current(
        &self,
        subject: &str,
        source: &str,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerTerminationProfileV3> {
        if self.session_inventory != SessionInventoryV1::Known || self.frozen() {
            return Err(invalid());
        }
        let s = self.test_sessions.get(subject).ok_or_else(invalid)?;
        if s.closing.is_none() {
            return Err(invalid());
        }
        let checks = self.termination_source_checks_v3(subject, source, current, now)?;
        verifier.verify(&checks)?;
        let profile = self.owner_termination_profile_v3(subject, now)?;
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        if a.owner_profile_receipt
            .as_ref()
            .is_none_or(|r| r.profile != profile)
        {
            return Err(invalid());
        }
        Ok(profile)
    }
    pub fn allocate_termination_stop_v3(
        &mut self,
        subject: &str,
        invocation: &str,
        source: &str,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerTerminationCandidateV3> {
        let profile =
            self.termination_v3_effect_current(subject, source, current, now, verifier)?;
        let grant = OwnerTerminationGrantV3 {
            profile: profile.clone(),
            invocation_id: invocation.into(),
            request: OwnerTerminationRequestV3::Stop(profile.unsigned_stop()?),
        };
        let candidate = OwnerTerminationCandidateV3::from_grant(grant)?;
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        if let Some(old) = a.effects.get(&OwnerTerminationEffectV3::Stop) {
            return if old.candidate == candidate {
                Ok(candidate)
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if a.effects.len() >= 2 {
            return Err(invalid());
        }
        let mut next = self.clone();
        next.termination_v3
            .get_mut(subject)
            .ok_or_else(invalid)?
            .effects
            .insert(
                OwnerTerminationEffectV3::Stop,
                TerminationEffectSlotV3 {
                    candidate: candidate.clone(),
                    allocated_at: now,
                    initial_key_touch: None,
                    recomputations: BTreeMap::new(),
                    original: None,
                    send_claimed_at: None,
                    lookups: BTreeMap::new(),
                },
            );
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(candidate)
    }
    pub fn claim_termination_key_touch_v3(
        &mut self,
        subject: &str,
        effect: OwnerTerminationEffectV3,
        attempt: &str,
        source: &str,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerTerminationCandidateAuthorityV3> {
        self.termination_v3_effect_current(subject, source, current, now, verifier)?;
        if !id(attempt) {
            return Err(invalid());
        }
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        let old = a.effects.get(&effect).ok_or_else(invalid)?;
        if old.original.is_some() {
            return Ok(OwnerTerminationCandidateAuthorityV3 {
                candidate: old.candidate.clone(),
                account_version_anchor: a.version_anchor,
                key_touch: OwnerTerminationKeyTouchV3::KnownOnly,
            });
        }
        if old.initial_key_touch.is_some() || old.send_claimed_at.is_some() {
            return Err(ProvisionApprovalError::Conflict);
        }
        let mut next = self.clone();
        next.prepay_termination_signing_invocation_v3(subject, attempt, now)?;
        let a = next.termination_v3.get_mut(subject).ok_or_else(invalid)?;
        let old = a.effects.get_mut(&effect).ok_or_else(invalid)?;
        old.initial_key_touch = Some(TerminationKeyTouchAttemptV3 {
            id: attempt.into(),
            claimed_at: now,
        });
        let out = OwnerTerminationCandidateAuthorityV3 {
            candidate: old.candidate.clone(),
            account_version_anchor: a.version_anchor,
            key_touch: OwnerTerminationKeyTouchV3::InitialClaim {
                attempt_id: attempt.into(),
            },
        };
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(out)
    }
    pub fn claim_termination_recompute_v3(
        &mut self,
        subject: &str,
        effect: OwnerTerminationEffectV3,
        attempt: &str,
        source: &str,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<OwnerTerminationCandidateAuthorityV3> {
        self.termination_v3_effect_current(subject, source, current, now, verifier)?;
        if !id(attempt) {
            return Err(invalid());
        }
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        let slot = a.effects.get(&effect).ok_or_else(invalid)?;
        if slot.original.is_some() {
            return Ok(OwnerTerminationCandidateAuthorityV3 {
                candidate: slot.candidate.clone(),
                account_version_anchor: a.version_anchor,
                key_touch: OwnerTerminationKeyTouchV3::KnownOnly,
            });
        }
        // Both complete persisted candidates must still be reconciled by the
        // Owner adapter. A send claim permanently disallows recomputation.
        if slot.initial_key_touch.is_none()
            || slot.send_claimed_at.is_some()
            || slot.recomputations.contains_key(attempt)
            || slot.recomputations.len() as u64
                >= a.plan
                    .owner_signing_recovery_policy
                    .max_recomputations_per_effect()
        {
            return Err(invalid());
        }
        let sequence = DecimalU64(slot.recomputations.len() as u64 + 1);
        let mut next = self.clone();
        next.prepay_termination_signing_invocation_v3(subject, attempt, now)?;
        let a = next.termination_v3.get_mut(subject).ok_or_else(invalid)?;
        let slot = a.effects.get_mut(&effect).ok_or_else(invalid)?;
        slot.recomputations.insert(attempt.into(), now);
        let out = OwnerTerminationCandidateAuthorityV3 {
            candidate: slot.candidate.clone(),
            account_version_anchor: a.version_anchor,
            key_touch: OwnerTerminationKeyTouchV3::ExactOriginalRecompute {
                attempt_id: attempt.into(),
                sequence,
            },
        };
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(out)
    }
    /// Exact immutable signature archival is not a live send/admit permission.
    pub fn archive_termination_original_v3(
        &mut self,
        subject: &str,
        effect: OwnerTerminationEffectV3,
        original: OwnerTerminationSignedV3,
    ) -> Result<()> {
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        let slot = a.effects.get(&effect).ok_or_else(invalid)?;
        if slot.initial_key_touch.is_none() {
            return Err(invalid());
        }
        slot.candidate.verify_original(&original)?;
        if let Some(old) = &slot.original {
            return if old == &original {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let mut next = self.clone();
        next.termination_v3
            .get_mut(subject)
            .ok_or_else(invalid)?
            .effects
            .get_mut(&effect)
            .ok_or_else(invalid)?
            .original = Some(original);
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(())
    }
    pub(super) fn validate_termination_effects_v3(&self, subject: &str) -> Result<()> {
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        if a.effects.len() > 2 {
            return Err(invalid());
        }
        let mut attempts = std::collections::BTreeSet::new();
        for (effect, slot) in &a.effects {
            slot.candidate.validate()?;
            if effect != &slot.candidate.grant.effect()
                || slot.candidate.grant.profile.plan != a.plan
                || a.owner_profile_receipt
                    .as_ref()
                    .is_none_or(|r| r.profile != slot.candidate.grant.profile)
                || slot.allocated_at.get() >= a.plan.cleanup_until.get()
                || slot.recomputations.len() as u64
                    > a.plan
                        .owner_signing_recovery_policy
                        .max_recomputations_per_effect()
                || (slot.original.is_some() || !slot.recomputations.is_empty())
                    && slot.initial_key_touch.is_none()
                || slot.send_claimed_at.is_some() && slot.original.is_none()
                || slot.lookups.len() > 2
            {
                return Err(invalid());
            }
            for attempt in slot.initial_key_touch.iter().chain(slot.lookups.values()) {
                if !id(&attempt.id)
                    || !attempts.insert(&attempt.id)
                    || attempt.claimed_at.get() < slot.allocated_at.get()
                    || attempt.claimed_at.get() >= a.plan.cleanup_until.get()
                {
                    return Err(invalid());
                }
            }
            for (id, at) in &slot.recomputations {
                if !id_is_valid(id)
                    || !attempts.insert(id)
                    || at.get()
                        < slot
                            .initial_key_touch
                            .as_ref()
                            .ok_or_else(invalid)?
                            .claimed_at
                            .get()
                    || at.get() >= a.plan.cleanup_until.get()
                {
                    return Err(invalid());
                }
            }
            if let Some(original) = &slot.original {
                slot.candidate.verify_original(original)?;
            }
        }
        Ok(())
    }
}
fn id_is_valid(value: &str) -> bool {
    id(value)
}
