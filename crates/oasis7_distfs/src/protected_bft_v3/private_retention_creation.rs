//! Fresh namespace creation is distinct from restoring an uncovered snapshot.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::private_retention::{
    BftPrivateRetentionAttestorV1, BftPrivateRetentionSourceV1, hash_value,
};
use super::state::BftSignerStateV3;
use serde::{Deserialize, Serialize};

/// Archivable descriptor, not an authorization capability on its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionCreationMarkerV1 {
    pub schema_version: u32,
    pub creation_identity: String,
    pub source: BftPrivateRetentionSourceV1,
    pub ledger_origin_digest: String,
    pub activation_digest: String,
    pub initial_state_digest: String,
    pub initial_height: u64,
}

impl BftPrivateRetentionCreationMarkerV1 {
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        if self.schema_version != 1 || self.initial_height == 0 {
            return Err(invalid("private retention creation schema/height"));
        }
        self.source.validate()?;
        for value in [
            &self.creation_identity,
            &self.ledger_origin_digest,
            &self.activation_digest,
            &self.initial_state_digest,
        ] {
            hash_value(value)?;
        }
        super::crypto::digest_hex("oasis7.bft.private-retention.v1.source-creation", self)
    }
}

/// Same software namespace preparation survives dropped Runtime builders. Raw
/// disk WAL bytes cannot construct this handle or choose a replacement identity.
pub struct BftPrivateRetentionFreshRecoveryV1 {
    preparation: std::sync::Arc<FreshPreparation>,
}

pub(super) struct FreshPreparation {
    pub(super) creation_identity: String,
    pub(super) state: std::sync::Mutex<FreshPreparationState>,
}

#[derive(Default)]
pub(super) struct FreshPreparationState {
    pub(super) bound: Option<(BftPrivateRetentionCreationMarkerV1, BftSignerStateV3)>,
    pub(super) backend_created: bool,
    pub(super) backend: std::sync::Weak<std::sync::Mutex<super::private_retention_memory::Memory>>,
    pub(super) role_digest: Option<String>,
    pub(super) clock: Option<(std::time::Instant, i64)>,
    pub(super) transfer: Option<super::archive_transfer::BftSuccessorArchiveTransferLedgerV1>,
}

/// Single-use binding handle; no raw-Snapshot or Deserialize constructor.
pub struct BftPrivateRetentionFreshNamespaceV1 {
    pub(super) marker: BftPrivateRetentionCreationMarkerV1,
    pub(super) initial_state: BftSignerStateV3,
    pub(super) preparation: std::sync::Arc<FreshPreparation>,
}

/// Pure seed is generated before Origin and therefore before the binding which
/// commits that Origin. Recovery retains the same protected namespace object.
pub struct BftPrivateRetentionFreshSeedV1 {
    preparation: std::sync::Arc<FreshPreparation>,
    binding: Option<Box<BftSignerBindingV3>>,
}

impl BftPrivateRetentionFreshSeedV1 {
    pub fn generate() -> Result<Self, BftSignerErrorV3> {
        Ok(Self {
            binding: None,
            preparation: std::sync::Arc::new(FreshPreparation {
                creation_identity: super::client::random_nonce()?,
                state: std::sync::Mutex::new(FreshPreparationState::default()),
            }),
        })
    }

    pub fn creation_identity(&self) -> &str {
        &self.preparation.creation_identity
    }

    /// Runtime uses generate -> canonical Origin -> bind_source instead.
    pub fn create(binding: &BftSignerBindingV3) -> Result<Self, BftSignerErrorV3> {
        let mut seed = Self::generate()?;
        seed.binding = Some(Box::new(binding.clone()));
        Ok(seed)
    }

    pub fn bind_origin(
        mut self,
        origin: String,
    ) -> Result<BftPrivateRetentionFreshNamespaceV1, BftSignerErrorV3> {
        let binding = self
            .binding
            .take()
            .ok_or_else(|| invalid("fresh source binding must follow actual Origin preparation"))?;
        self.bind_source(&binding, origin)
    }

    pub fn recovery_handle(&self) -> BftPrivateRetentionFreshRecoveryV1 {
        BftPrivateRetentionFreshRecoveryV1 {
            preparation: self.preparation.clone(),
        }
    }

    pub fn bind_source(
        self,
        binding: &BftSignerBindingV3,
        ledger_origin_digest: String,
    ) -> Result<BftPrivateRetentionFreshNamespaceV1, BftSignerErrorV3> {
        hash_value(&ledger_origin_digest)?;
        let initial_state = BftSignerStateV3::from_handoff(binding)?;
        let marker = BftPrivateRetentionCreationMarkerV1 {
            schema_version: 1,
            creation_identity: self.creation_identity().into(),
            source: BftPrivateRetentionSourceV1 {
                binding_digest: binding.digest()?,
                handoff_digest: binding.handoff_record_id.clone(),
                material_source_digest: binding.material_identity.clone(),
            },
            ledger_origin_digest,
            activation_digest: binding.activation_digest.clone(),
            initial_state_digest: initial_state.state_digest.clone(),
            initial_height: binding.initial_schedule().height(),
        };
        marker.digest()?;
        {
            let mut record = self
                .preparation
                .state
                .lock()
                .map_err(|_| invalid("private retention source preparation poisoned"))?;
            if let Some((original, state)) = &record.bound {
                if original != &marker || state != &initial_state {
                    return Err(invalid("private retention original source binding changed"));
                }
            } else {
                record.bound = Some((marker.clone(), initial_state.clone()));
            }
        }
        Ok(BftPrivateRetentionFreshNamespaceV1 {
            marker,
            initial_state,
            preparation: self.preparation,
        })
    }
}

impl BftPrivateRetentionFreshRecoveryV1 {
    pub fn recover_seed(&self) -> BftPrivateRetentionFreshSeedV1 {
        BftPrivateRetentionFreshSeedV1 {
            preparation: self.preparation.clone(),
            binding: None,
        }
    }
}

impl BftPrivateRetentionFreshNamespaceV1 {
    pub fn marker(&self) -> &BftPrivateRetentionCreationMarkerV1 {
        &self.marker
    }
}

/// Original backend creation readback under a current fenced lease. It grants
/// neither private read access nor permission to write an arbitrary source.
#[derive(Debug)]
pub struct ValidatedBftPrivateRetentionCreationV1 {
    marker: BftPrivateRetentionCreationMarkerV1,
    capability_digest: String,
    lease_generation: u64,
    lease_token_digest: String,
}

impl ValidatedBftPrivateRetentionCreationV1 {
    pub(super) fn from_fresh_backend(
        marker: &BftPrivateRetentionCreationMarkerV1,
        role: &BftPrivateRetentionAttestorV1,
        snapshot: &super::backend::BftSignerBackendSnapshotV3,
    ) -> Result<Self, BftSignerErrorV3> {
        if marker.digest()? != role.creation_marker_digest
            || marker.source != role.source
            || marker.initial_height != role.coverage_start_height
            || snapshot.state.state_digest != marker.initial_state_digest
            || snapshot.state.binding_digest != marker.source.binding_digest
            || snapshot.state.private_retention.is_some()
            || snapshot.lease_token_digest.is_empty()
        {
            return Err(invalid(
                "private retention original creation readback mismatch",
            ));
        }
        snapshot.state.validate_digest()?;
        Ok(Self {
            marker: marker.clone(),
            capability_digest: role.digest()?,
            lease_generation: snapshot.lease_generation,
            lease_token_digest: snapshot.lease_token_digest.clone(),
        })
    }

    pub fn marker(&self) -> &BftPrivateRetentionCreationMarkerV1 {
        &self.marker
    }

    pub fn validate_for(
        &self,
        binding: &BftSignerBindingV3,
        role: &BftPrivateRetentionAttestorV1,
        snapshot: &super::backend::BftSignerBackendSnapshotV3,
    ) -> Result<(), BftSignerErrorV3> {
        if self.marker.digest()? != role.creation_marker_digest
            || self.capability_digest != role.digest()?
            || self.marker.source != role.source
            || self.marker.source.binding_digest != binding.digest()?
            || self.marker.activation_digest != binding.activation_digest
            || self.marker.initial_state_digest != snapshot.state.state_digest
            || snapshot.state.private_retention.is_some()
            || self.lease_generation != snapshot.lease_generation
            || self.lease_token_digest != snapshot.lease_token_digest
        {
            return Err(invalid(
                "private retention fresh-source proof binding mismatch",
            ));
        }
        Ok(())
    }
}
