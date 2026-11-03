//! Explicit provider construction and capability validation.
use super::*;

impl BftSignerServiceV3 {
    pub fn open_with_private_retention(
        binding: BftSignerBindingV3,
        provider_key: Box<dyn BftSigningKeyHandleV3>,
        validator_keys: Vec<Box<dyn BftSigningKeyHandleV3>>,
        backend: Box<dyn BftSignerBackendV3>,
        successor: Option<BftSuccessorWindowSupportV1>,
        retention: BftPrivateRetentionSupportV1,
    ) -> Result<Self, BftSignerErrorV3> {
        Self::open_internal(
            binding,
            provider_key,
            validator_keys,
            backend,
            successor,
            Some(retention),
        )
    }
    pub fn open(
        binding: BftSignerBindingV3,
        provider_key: Box<dyn BftSigningKeyHandleV3>,
        validator_keys: Vec<Box<dyn BftSigningKeyHandleV3>>,
        backend: Box<dyn BftSignerBackendV3>,
    ) -> Result<Self, BftSignerErrorV3> {
        Self::open_internal(binding, provider_key, validator_keys, backend, None, None)
    }
    pub fn open_with_successor_window(
        binding: BftSignerBindingV3,
        provider_key: Box<dyn BftSigningKeyHandleV3>,
        validator_keys: Vec<Box<dyn BftSigningKeyHandleV3>>,
        backend: Box<dyn BftSignerBackendV3>,
        support: BftSuccessorWindowSupportV1,
    ) -> Result<Self, BftSignerErrorV3> {
        Self::open_internal(
            binding,
            provider_key,
            validator_keys,
            backend,
            Some(support),
            None,
        )
    }
    fn open_internal(
        binding: BftSignerBindingV3,
        provider_key: Box<dyn BftSigningKeyHandleV3>,
        validator_keys: Vec<Box<dyn BftSigningKeyHandleV3>>,
        mut backend: Box<dyn BftSignerBackendV3>,
        successor_support: Option<BftSuccessorWindowSupportV1>,
        private_retention_support: Option<BftPrivateRetentionSupportV1>,
    ) -> Result<Self, BftSignerErrorV3> {
        binding.validate()?;
        let provider_public_key = provider_key.public_key_hex();
        decode_key(&provider_public_key)?;
        if provider_public_key == binding.requester_auth_public_key
            || binding
                .provisioned_signing_keys
                .iter()
                .any(|identity| identity.public_key_hex == provider_public_key)
        {
            return Err(invalid(
                "BFT V3 provider, requester, and validator identities must be distinct",
            ));
        }
        let mut keys_by_public = BTreeMap::new();
        for key in validator_keys {
            let public_key = key.public_key_hex();
            decode_key(&public_key)?;
            if public_key == provider_public_key
                || public_key == binding.requester_auth_public_key
                || keys_by_public.insert(public_key, key).is_some()
            {
                return Err(invalid("BFT V3 provisioned signing key set is ambiguous"));
            }
        }
        if binding
            .provisioned_signing_keys
            .iter()
            .any(|identity| !keys_by_public.contains_key(&identity.public_key_hex))
            || keys_by_public.len() != binding.provisioned_signing_keys.len()
        {
            return Err(invalid(
                "BFT V3 provider keys differ from the pinned provisioned identities",
            ));
        }
        let binding_digest = binding.digest()?;
        let snapshot = backend.load_provisioned(&binding_digest)?;
        validate_snapshot_shape(&snapshot, None, None)?;
        // With no successor capability, reject malformed original retention before
        // the missing-capability error, including Source AppliedCut snapshots
        // whose public State already has an Active window. This read-only check
        // cannot open the service; authenticated Active history is checked below.
        if snapshot.state.active_window.is_none() || successor_support.is_none() {
            Self::validate_retention_open(
                &binding,
                &provider_public_key,
                &snapshot,
                private_retention_support.as_ref(),
                backend.as_mut(),
                None,
            )?;
        }
        let verified = if let Some(support) = &successor_support {
            support.validate_backend_capability(&snapshot)?;
            support.validate_public_state(
                &binding,
                &snapshot.state,
                &provider_public_key,
                support.deadline(std::time::Instant::now())?,
            )?
        } else {
            if snapshot.state.successor_window.is_some() {
                return Err(invalid(
                    "nonempty successor state requires explicit support",
                ));
            }
            validate_state(&binding, Some(&provider_public_key), &snapshot.state)?
        };
        if snapshot.state.active_window.is_some() {
            Self::validate_retention_open(
                &binding,
                &provider_public_key,
                &snapshot,
                private_retention_support.as_ref(),
                backend.as_mut(),
                Some(&verified),
            )?;
        }
        if verified.schedule.height() < binding.initial_schedule().height()
            || snapshot.lease_generation == 0
            || snapshot.protected_time_floor_ms < 0
        {
            return Err(invalid("BFT V3 provisioned backend snapshot is invalid"));
        }
        Ok(Self {
            candidate_read_policy: None,
            successor_support,
            private_retention_support,
            binding,
            provider_key,
            validator_keys: keys_by_public,
            runtime: Mutex::new(BftSignerServiceRuntimeV3 {
                successor_deadline: None,
                read_window: None,
                backend,
                poisoned: false,
                input_view_epoch: observed_view::OriginalInputViewEpochV1::default(),
                verified: Some(verified),
                active_lease: (!snapshot.lease_token_digest.is_empty()).then_some(ActiveLeaseV3 {
                    generation: snapshot.lease_generation,
                    token_digest: snapshot.lease_token_digest,
                }),
                observed_time_floor_ms: snapshot.protected_time_floor_ms,
                observed_generation: snapshot.lease_generation,
                observed_archive_transfer: snapshot.successor_archive_transfer,
            }),
        })
    }
    fn validate_retention_open(
        binding: &BftSignerBindingV3,
        provider_public_key: &str,
        snapshot: &BftSignerBackendSnapshotV3,
        private_retention_support: Option<&BftPrivateRetentionSupportV1>,
        backend: &mut dyn BftSignerBackendV3,
        verified: Option<&super::super::state::VerifiedBftSignerStateV3>,
    ) -> Result<(), BftSignerErrorV3> {
        let binding_digest = binding.digest()?;
        if let Some(support) = private_retention_support {
            support.validate_config(binding, provider_public_key)?;
            let current = backend.load_private_retention_current(&binding_digest)?;
            if &current.signer != snapshot {
                return Err(invalid("private retention open snapshot mismatch"));
            }
            // The signed map commits the predecessor even for an empty carry.
            // A sealed namespace must be checked from its original fixed role,
            // not against a rewritten coverage floor or a stored lease token.
            if current
                .retention
                .as_ref()
                .is_some_and(|ledger| ledger.map.previous_seal_receipt_digest.is_some())
            {
                let sealed = backend.load_private_retention_sealed_current(&binding_digest)?;
                if sealed.current != current || &sealed.current.signer != snapshot {
                    return Err(invalid("private retention sealed open snapshot mismatch"));
                }
                sealed.validate_inventory_with_verified_state(
                    &binding,
                    &support.attestor,
                    verified,
                )?;
            } else {
                support.validate_current(&current)?;
            }
        } else if snapshot.state.private_retention.is_some() {
            return Err(invalid(
                "private retention State requires explicit external capability",
            ));
        }
        Ok(())
    }
}
