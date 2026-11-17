//! Initial ledger publication with an atomic request/owner binding option.
use super::*;

type InitialBinding<'a> = (
    Vec<u8>,
    std::sync::Arc<super::super::super::process::ProcessOwnership>,
    &'a Path,
);

impl Ledger {
    /// Explicit first initialization only; never called as an open fallback.
    pub fn create(path: &Path, identity: Identity, policy: Policy) -> Result<Self, String> {
        Self::create_with_material_limit(path, identity, policy, 0)
    }

    pub fn create_with_material_limit(
        path: &Path,
        identity: Identity,
        policy: Policy,
        material_limit: u64,
    ) -> Result<Self, String> {
        Self::create_initial(path, identity, policy, material_limit, None)
    }

    /// Publish the request and native owner together with the initial ledger.
    /// An uncertain write is reconciled by opening the same identity, never by
    /// creating another operation. This does not admit an attempt or install.
    pub fn create_bound<T: Serialize>(
        path: &Path,
        identity: Identity,
        policy: Policy,
        material_limit: u64,
        request: &T,
        owner: std::sync::Arc<super::super::super::process::ProcessOwnership>,
        data: &Path,
    ) -> Result<Self, String> {
        let bytes = serde_json::to_vec(request).map_err(|error| error.to_string())?;
        Self::create_initial(
            path,
            identity,
            policy,
            material_limit,
            Some((bytes, owner, data)),
        )
    }

    fn create_initial(
        path: &Path,
        identity: Identity,
        policy: Policy,
        material_limit: u64,
        binding: Option<InitialBinding<'_>>,
    ) -> Result<Self, String> {
        let owner_binding = binding
            .as_ref()
            .map(|(_, owner, _)| owner.binding_digest())
            .transpose()?;
        if let Some((_, owner, data)) = &binding {
            Self::validate_owner_path(path, owner, data)?;
        }
        let lock = Self::lock(path)?;
        if path.try_exists().map_err(|e| e.to_string())? {
            return Err("operation ledger already exists; reopen its original identity".into());
        }
        let state = Snapshot {
            api_version: API.into(),
            identity,
            owner_binding,
            revision: 0,
            policy,
            encoded_tx_actual: 0,
            encoded_rx_actual: 0,
            unknown_final_charged: 0,
            exchanges_started: 0,
            original_request_json: binding.as_ref().map(|(bytes, _, _)| bytes.clone()),
            verified_target_json: None,
            publication: None,
            publication_journal_version: Some(1),
            installation_journal_version: Some(1),
            installation: None,
            proof_pages: 0,
            proof_bytes: 0,
            material_limit,
            material_bytes: 0,
            materials: Default::default(),
            window_seq: 0,
            attempts_used: 0,
            interrupted_attempts: Vec::new(),
            active_attempt_nonce: None,
            active_request: None,
            active_request_nonce: None,
            tx_credit: 0,
            rx_credit: 0,
            reservation: None,
        };
        state.validate()?;
        let ledger = Self {
            path: path.into(),
            _lock: lock,
            state,
            poisoned: false,
            recovered: false,
            reopen_deadline: None,
            owner: binding.as_ref().map(|(_, owner, _)| owner.clone()),
        };
        if let Some((_, owner, data)) = &binding {
            ledger.require_owner_path(owner, data)?;
        }
        // Failure may have published the complete initial state. No attempt is
        // returned and the caller must read the same identity before continuing.
        local_io::write(path, &ledger.state)?;
        let actual: Snapshot = local_io::read(path)?;
        if actual != ledger.state {
            return Err("operation initialization readback mismatch".into());
        }
        if let Some((_, owner, data)) = &binding {
            ledger.require_owner_path(owner, data)?;
        }
        Ok(ledger)
    }
}
