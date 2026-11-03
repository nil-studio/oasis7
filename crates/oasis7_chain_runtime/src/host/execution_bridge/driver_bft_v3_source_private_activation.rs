//! Exact original source activation carrier and existing global usage reservation.
use super::super::super::source_creation::activation::ValidatedSourceActivationV1;
use super::super::super::window_storage::directories::DirectoryIdentity;
use super::*;
use std::time::Instant;

#[derive(Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceActivationBindingV1 {
    pub(super) reference: String,
    pub(super) encoded_bytes: u64,
    directory: DirectoryIdentity,
}

impl SourceActivationBindingV1 {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.directory.validate()?;
        if !canonical_hash(&self.reference)
            || self.encoded_bytes == 0
            || self.encoded_bytes > 64 * 1024
        {
            return Err("private source activation binding capacity".into());
        }
        Ok(())
    }

    pub(super) fn verify_directory(&self, path: &Path) -> Result<(), String> {
        let file = super::super::super::successor_carriers::open_absolute_directory(path)?;
        if DirectoryIdentity::from_descriptor(&file)? != self.directory {
            return Err("covered private original directory replaced".into());
        }
        Ok(())
    }
    pub(super) fn accounted_bytes(&self) -> Result<usize, String> {
        self.validate()?;
        // Same exact four-copy/two-pin bound used for durable original auxiliary
        // intents. The caller reserves this in the existing ledger, not a new quota.
        super::super::auto_prepare::auxiliary_publication_bytes(
            usize::try_from(self.encoded_bytes).map_err(|_| "source activation byte conversion")?,
            &self.reference,
        )
    }
}

fn live(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("source activation publication expired".into())
    } else {
        Ok(())
    }
}

pub(in super::super) struct OriginalSourcePublicationV1<'a> {
    pub(in super::super) ledger_origin: &'a str,
    pub(in super::super) material: &'a str,
    pub(in super::super) height: u64,
    pub(in super::super) nodes: &'a BTreeMap<String, Vec<u8>>,
    pub(in super::super) activation: &'a ValidatedSourceActivationV1,
    pub(in super::super) deadline: Instant,
    pub(in super::super) verify_owner: &'a mut dyn FnMut() -> Result<(), String>,
}

impl NodePrivateStore {
    pub(in super::super) fn activate_original_source(
        &mut self,
        request: OriginalSourcePublicationV1<'_>,
        reserve: &mut impl FnMut(
            usize,
            super::super::super::super::super::formal_ledger::PublicationPromiseInventoryV1,
        ) -> Result<(), String>,
    ) -> Result<(), String> {
        let OriginalSourcePublicationV1 {
            ledger_origin,
            material,
            height,
            nodes,
            activation,
            deadline,
            verify_owner,
        } = request;
        verify_owner()?;
        live(deadline)?;
        let bytes = activation.bytes()?;
        let reference = activation.reference()?;
        let marker = activation.marker();
        if marker.creation_identity != self.instance_id
            || marker.ledger_origin_digest != ledger_origin
            || marker.source.material_source_digest != material
            || height.checked_add(1) != Some(marker.initial_height)
        {
            return Err("source activation does not authorize this original private owner".into());
        }
        // The protected marker and authenticated empty receipt already exist.
        // Persisted private-origin bytes bind this actual private directory too;
        // replacing the parent while preserving child inodes cannot alias it.
        live(deadline)?;
        super::ensure_private_directory(&self.dir)?;
        let directory =
            super::super::super::successor_carriers::open_absolute_directory(&self.dir)?;
        let directory_identity = DirectoryIdentity::from_descriptor(&directory)?;
        let data = Data {
            schema_version: 8,
            domain: COVERED_SOURCE_ORIGIN_DOMAIN.into(),
            origin: true,
            ledger_origin_digest: ledger_origin.into(),
            material_identity: material.into(),
            instance_id: self.instance_id.clone(),
            height,
            value: None,
            actions_cbor: Vec::new(),
            references: nodes.keys().cloned().collect(),
            publication_reservation_ref: None,
            publication_original_data_ref: None,
            protected_source_origin_ref: None,
            source_activation: Some(SourceActivationBindingV1 {
                reference,
                directory: directory_identity,
                encoded_bytes: u64::try_from(bytes.len())
                    .map_err(|_| "source activation byte conversion")?,
            }),
        };
        data.validate()?;
        if self
            .origin
            .as_ref()
            .is_some_and(|original| original.data != data)
        {
            return Err("source activation changes original private initialization".into());
        }
        if self.origin.is_none()
            && self
                .store
                .list_pin_scope_shards(SCOPE)
                .map_err(|_| "inspect covered original pins")?
                .iter()
                .any(|shard| shard.starts_with(&format!("ledger-{ledger_origin}-")))
        {
            return Err(
                "covered original private metadata missing; cannot recreate from receipt".into(),
            );
        }
        if self.origin.as_ref().is_some_and(|slot| slot.complete) {
            verify_owner()?;
            verify_material(&self.store, &data)?;
            return verify_original_slot(
                &self.store,
                &self.dir.join("origin.cbor"),
                self.origin.as_ref().ok_or("original source missing")?,
            );
        }
        self.origin.get_or_insert(Slot {
            data: data.clone(),
            complete: false,
        });
        if self.dir.exists() {
            self.load_inventory(ledger_origin, material, height)?;
        }
        if !self.candidates.is_empty() || !self.values.is_empty() || !self.selections.is_empty() {
            return Err(
                "source initialization cannot relabel existing unsigned candidate material".into(),
            );
        }
        reserve(self.usage_with(None, nodes)?, self.publication_promises()?)?;
        live(deadline)?;
        super::ensure_private_directory(&self.dir)?;
        let path = self.dir.join("origin.cbor");
        let has_pins = self
            .store
            .list_pin_scope_shards(SCOPE)
            .map_err(|_| "inspect original source pins")?
            .contains(&data.shard());
        if retention_required(&path)? || has_pins {
            verify_material(&self.store, &data)?;
        }
        // The exact Data reservation is durable before the auxiliary blob. Its
        // length/ref preserve the whole original liability if publication stops.
        if !retention_required(&path)? {
            verify_owner()?;
            live(deadline)?;
            files::publish(&path.with_extension("reservation.cbor"), &data.bytes()?)?;
        }
        live(deadline)?;
        #[cfg(test)]
        super::super::super::source_creation::activation::before_capsule_for_test()?;
        verify_owner()?;
        common_storage::put_cas_durable(&self.store, &bytes, 64 * 1024)?;
        live(deadline)?;
        publish_checked(
            &self.store,
            &path,
            &data,
            nodes,
            self.origin.as_ref().unwrap().complete,
            true,
            &mut || {
                live(deadline)?;
                verify_owner()?;
                data.source_activation
                    .as_ref()
                    .ok_or("source activation directory binding missing")?
                    .verify_directory(&self.dir)
            },
        )?;
        live(deadline)?;
        data.source_activation
            .as_ref()
            .ok_or("source activation directory binding missing")?
            .verify_directory(&self.dir)?;
        self.origin = Some(Slot {
            data,
            complete: true,
        });
        live(deadline)
    }
}

pub(super) fn verify_activation_material(store: &LocalCasStore, data: &Data) -> Result<(), String> {
    if let Some(binding) = &data.source_activation {
        let bytes = common_storage::read_cas_bounded(store, &binding.reference, 64 * 1024)?;
        if u64::try_from(bytes.len()).map_err(|_| "source capsule byte conversion")?
            != binding.encoded_bytes
        {
            return Err("original source activation length changed".into());
        }
        super::super::super::source_creation::activation::validate_stored_owner(
            &bytes,
            &data.ledger_origin_digest,
            &data.material_identity,
            &data.instance_id,
            data.height,
        )?;
    }
    Ok(())
}
impl NodePrivateStore {
    pub(in super::super) fn open_covered(
        records: &Path,
        store: LocalCasStore,
        fallback: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
        instance: &str,
    ) -> Result<(Self, Arc<dyn BusinessIndexNodeReader + Send + Sync>), String> {
        let dir = records
            .join(storage::DIRECTORY)
            .join(storage::NODE_PRIVATE_DIRECTORY);
        let (mut owner, reader) = Self::open_directory(dir, store, fallback, Some(instance))?;
        owner.auxiliary_root = Some(records.into());
        Ok((owner, reader))
    }
    pub(in super::super) fn original_activation_bytes(&self) -> Result<Option<Vec<u8>>, String> {
        let Some(slot) = &self.origin else {
            return Ok(None);
        };
        let binding = slot
            .data
            .source_activation
            .as_ref()
            .ok_or("private original source has legacy activation schema")?;
        match common_storage::read_cas_bounded(&self.store, &binding.reference, 64 * 1024) {
            Ok(bytes) => {
                verify_activation_material(&self.store, &slot.data)?;
                Ok(Some(bytes))
            }
            Err(_)
                if !retention_required(&self.dir.join("origin.cbor"))?
                    && !self
                        .store
                        .list_pin_scope_shards(SCOPE)
                        .map_err(|_| "inspect source original pins")?
                        .contains(&slot.data.shard()) =>
            {
                Ok(None)
            }
            Err(_) => Err("original retained source activation material missing".into()),
        }
    }
}
