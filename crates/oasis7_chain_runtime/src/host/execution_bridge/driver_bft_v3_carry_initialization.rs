//! Cost-only borrowed canonical shapes. No descriptor here grants authority.
use super::*;
use oasis7_distfs::protected_bft_v3::BftSuccessorWindowArchiveRefV1 as Ref;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in super::super::super::super) struct CarryCopyDescriptorV1 {
    pub header: String,
    pub height: u64,
    pub data: Ref,
    pub metadata: Ref,
    pub actions: Ref,
    pub dependencies: String,
    pub alias_bytes: u64,
    pub received_selection_bytes: u64,
}
struct Counter(usize);
impl std::io::Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("carry size overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn length(value: &impl Serialize) -> Result<usize, String> {
    let mut counter = Counter(0);
    serde_cbor::to_writer(&mut counter, value).map_err(|_| "count canonical carry shape")?;
    Ok(counter.0)
}
#[derive(Serialize)]
struct Shape<'a> {
    schema_version: u32,
    domain: &'a str,
    origin: bool,
    ledger_origin_digest: &'a str,
    material_identity: &'a str,
    instance_id: &'a str,
    height: u64,
    value: Option<&'a BftHeaderValueV3>,
    actions_cbor: &'a [u8],
    references: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    publication_reservation_ref: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    publication_original_data_ref: Option<&'a String>,
}
fn pin_bound(references: usize) -> Result<usize, String> {
    // Compact JSON {"pins":["<hash64>",...]}; both original atomic images.
    references
        .checked_mul(67)
        .and_then(|n| n.checked_add(10))
        .and_then(|n| n.checked_mul(2))
        .ok_or("carry pin bound overflow".into())
}
fn selection_bound(data: &Data) -> Result<usize, String> {
    let reference = vec!["00".repeat(32)]; // Size only; never a digest authority.
    let shape = Shape {
        schema_version: 5,
        domain: SELECTION_DOMAIN,
        origin: false,
        ledger_origin_digest: &data.ledger_origin_digest,
        material_identity: &data.material_identity,
        instance_id: &data.instance_id,
        height: data.height,
        value: data.value.as_ref(),
        actions_cbor: &[],
        references: &reference,
        publication_reservation_ref: None,
        publication_original_data_ref: None,
    };
    length(&shape)?
        .checked_mul(2)
        .and_then(|n| n.checked_add(pin_bound(2).ok()?))
        .ok_or("carry selection bound overflow".into())
}
fn protected_root(data: &Data, root: &Data) -> Result<bool, String> {
    if !matches!(root.schema_version, 9 | 10) {
        return Ok(false);
    }
    root.validate()?;
    if !matches!(data.schema_version, 6 | 7)
        || data.protected_source_origin_ref.is_some()
        || data.publication_original_data_ref.as_deref() != Some(root.digest()?.as_str())
        || !same_candidate(data, root)
    {
        return Err("protected carry alias differs from original canonical source".into());
    }
    Ok(true)
}
pub(in super::super) fn migrated_hold(data: &Data, root: &Data) -> Result<usize, String> {
    if !protected_root(data, root)? {
        return Ok(0);
    }
    let pins = data.retained()?.len();
    pin_bound(pins)?
        .checked_add(if data.schema_version == 7 {
            selection_bound(data)?
        } else {
            0
        })
        .ok_or("protected carry hold overflow".into())
}
impl NodePrivateStore {
    pub(in super::super) fn carry_selection_held(&self, data: &Data) -> Result<bool, String> {
        if data.schema_version != 5 {
            return Ok(false);
        }
        let Some(value) = &data.value else {
            return Ok(false);
        };
        let Some(slot) = self.values.get(&(data.height, value.block_id()?)) else {
            return Ok(false);
        };
        if slot.data.schema_version != 7 {
            return Ok(false);
        }
        let root_ref = slot
            .data
            .publication_original_data_ref
            .as_ref()
            .ok_or("carry root reference absent")?;
        let root = read_original_data(&self.store, root_ref)?;
        if !protected_root(&slot.data, &root)? {
            return Ok(false);
        }
        if data.references != [slot.data.digest()?]
            || data.material_identity != slot.data.material_identity
            || data.instance_id != slot.data.instance_id
            || data.ledger_origin_digest != slot.data.ledger_origin_digest
            || !data.actions_cbor.is_empty()
        {
            return Err("carry selection differs from held original".into());
        }
        Ok(true)
    }
}
fn descriptor(
    data: &Data,
    metadata: &PublicationReservationV1,
) -> Result<CarryCopyDescriptorV1, String> {
    data.validate()?;
    if !matches!(data.schema_version, 9 | 10) || data.publication_original_data_ref.is_some() {
        return Err("protected carry planner requires original Source Data9/10".into());
    }
    metadata.validate_identity(&publication_identity(data)?)?;
    let root_ref = data.digest()?;
    let namespace = "00".repeat(32); // C0 namespace recipe fixes length, not pre-install carrier identity.
    let shape = Shape {
        schema_version: 6,
        domain: MIGRATED_DOMAIN,
        origin: false,
        ledger_origin_digest: &namespace,
        material_identity: &data.material_identity,
        instance_id: &data.instance_id,
        height: data.height,
        value: data.value.as_ref(),
        actions_cbor: &data.actions_cbor,
        references: &data.references,
        publication_reservation_ref: data.publication_reservation_ref.as_ref(),
        publication_original_data_ref: Some(&root_ref),
    };
    let alias = length(&shape)?
        .checked_mul(2)
        .and_then(|n| n.checked_add(pin_bound(data.references.len() + 3).ok()?))
        .ok_or("carry alias bound overflow")?;
    Ok(CarryCopyDescriptorV1 {
        header: data
            .value
            .as_ref()
            .ok_or("carry value absent")?
            .block_id()?,
        height: data.height,
        data: Ref {
            digest: root_ref,
            bytes: length(data)? as u64,
        },
        metadata: Ref {
            digest: metadata.digest()?,
            bytes: length(metadata)? as u64,
        },
        actions: Ref {
            digest: blake3_hex(&data.actions_cbor),
            bytes: data.actions_cbor.len() as u64,
        },
        dependencies: dependency_identity(data.references.iter().map(String::as_str)),
        alias_bytes: alias as u64,
        received_selection_bytes: selection_bound(data)? as u64,
    })
}
pub(in super::super::super::super) fn dependency_identity<'a>(
    refs: impl Iterator<Item = &'a str>,
) -> String {
    let mut hash = blake3::Hasher::new();
    for reference in refs {
        hash.update(&(reference.len() as u64).to_be_bytes());
        hash.update(reference.as_bytes());
    }
    hash.finalize().to_hex().to_string()
}
impl PublicationArchiveMaterialV1 {
    pub(in super::super::super::super) fn carry_copy_descriptor(
        &self,
    ) -> Result<CarryCopyDescriptorV1, String> {
        let data = decode_data(&self.data)?;
        let metadata = PublicationReservationV1::decode(&self.reservation)?;
        descriptor(&data, &metadata)
    }
}
impl VerifiedPublicationOriginalV1 {
    pub(in super::super::super::super) fn carry_copy_descriptor(
        &self,
    ) -> Result<CarryCopyDescriptorV1, String> {
        descriptor(&self.root, &self.metadata)
    }
    pub(in super::super::super::super) fn require_carry_originals(
        &self,
        store: &LocalCasStore,
    ) -> Result<(), String> {
        // Installation owns this same original source CAS. No fetch/repair occurs.
        let root = common_storage::read_cas_bounded(store, &self.root.digest()?, MAX_RECORD)?;
        if root.len() != length(&self.root)? {
            return Err("carry target original Data differs".into());
        }
        drop(root);
        if PublicationReservationV1::read(store, &self.metadata.digest()?)? != self.metadata {
            return Err("carry target original metadata differs".into());
        }
        for reference in &self.root.references {
            common_storage::read_cas_bounded(
                store,
                reference,
                formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            )?;
        }
        Ok(())
    }
}

pub(in super::super) fn original_and_carry_hold(
    store: &LocalCasStore,
    data: &Data,
    reference: &str,
) -> Result<usize, String> {
    let root = read_original_data(store, reference)?;
    length(&root)?
        .checked_add(migrated_hold(data, &root)?)
        .ok_or("carry original/hold overflow".into())
}

#[cfg(test)]
pub(in super::super) fn check_source_alias_for_test(
    original: &VerifiedPublicationOriginalV1,
    alias: &Data,
    raw: &PublicationArchiveMaterialV1,
) -> Result<(), String> {
    let descriptor = original.carry_copy_descriptor()?;
    // Independent actual Data canonical bytes and JSON pin serialization.
    #[derive(Serialize)]
    struct Pins {
        pins: BTreeSet<String>,
    }
    let pin = serde_json::to_vec(&Pins {
        pins: alias.retained()?,
    })
    .map_err(|_| "test pin encode")?
    .len();
    assert_eq!(
        descriptor.alias_bytes as usize,
        2 * alias.bytes()?.len() + 2 * pin
    );
    let old_bound = 4
        * (2 * original.root.actions_cbor.len() + 128 * original.root.references.len() + 16 * 1024);
    assert!(
        descriptor.alias_bytes <= old_bound as u64,
        "actual local carry must prove old byte bound covers alias"
    );
    let missing = PublicationArchiveMaterialV1 {
        data: raw.data.clone(),
        original_data: None,
        reservation: raw.reservation.clone(),
    };
    let owner = || PublicationArchiveOwnerV1 {
        origin: &alias.ledger_origin_digest,
        material: &alias.material_identity,
        instance: &alias.instance_id,
    };
    assert!(
        VerifiedPublicationOriginalV1::verify(&missing, owner(), std::slice::from_ref(original))
            .is_err()
    );
    let mut changed_root = original.root.clone();
    changed_root.protected_source_origin_ref = Some("ad".repeat(32));
    let mut changed_alias = alias.clone();
    changed_alias.publication_original_data_ref = Some(changed_root.digest()?);
    let replacement = PublicationArchiveMaterialV1 {
        data: changed_alias.bytes()?,
        original_data: Some(changed_root.bytes()?),
        reservation: raw.reservation.clone(),
    };
    assert_eq!(
        replacement.data.len(),
        raw.data.len(),
        "same-price foreign original fixture"
    );
    assert!(
        VerifiedPublicationOriginalV1::verify(
            &replacement,
            owner(),
            std::slice::from_ref(original)
        )
        .is_err()
    );
    Ok(())
}

#[cfg(test)]
impl VerifiedPublicationOriginalV1 {
    pub(in super::super::super::super) fn verify_carry_alias_negatives_for_test(
        &self,
    ) -> Result<(), String> {
        let mut alias = self.root.clone();
        alias.schema_version = 6;
        alias.domain = MIGRATED_DOMAIN.into();
        alias.ledger_origin_digest = "fd".repeat(32);
        alias.protected_source_origin_ref = None;
        alias.publication_original_data_ref = Some(self.root.digest()?);
        let raw = PublicationArchiveMaterialV1 {
            data: alias.bytes()?,
            original_data: Some(self.root.bytes()?),
            reservation: self.metadata.bytes()?,
        };
        let owner = || PublicationArchiveOwnerV1 {
            origin: &alias.ledger_origin_digest,
            material: &alias.material_identity,
            instance: &alias.instance_id,
        };
        assert!(
            VerifiedPublicationOriginalV1::verify(&raw, owner(), &[]).is_err(),
            "alias never mints a grant"
        );
        VerifiedPublicationOriginalV1::verify(&raw, owner(), std::slice::from_ref(self))?;
        check_source_alias_for_test(self, &alias, &raw)
    }
}

#[cfg(test)]
impl VerifiedPublicationOriginalV1 {
    pub(in super::super::super::super) fn verify_alias_only_cold_target_for_test(
        &self,
        parent: &Path,
    ) -> Result<(), String> {
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "test clock")?
            .as_nanos();
        let directory = parent.join(format!("carry-alias-only-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory).map_err(|_| "create alias-only test target")?;
        let _cleanup = Cleanup(directory.clone());
        let store = LocalCasStore::new(directory);
        let mut alias = self.root.clone();
        alias.schema_version = 6;
        alias.domain = MIGRATED_DOMAIN.into();
        alias.ledger_origin_digest = "fd".repeat(32);
        alias.protected_source_origin_ref = None;
        alias.publication_original_data_ref = Some(self.root.digest()?);
        common_storage::put_cas_durable(&store, &alias.bytes()?, MAX_RECORD)?;
        assert!(
            self.require_carry_originals(&store).is_err(),
            "cold alias cannot replace missing original root/meta/deps"
        );
        Ok(())
    }
}

pub(in super::super::super::super) fn carry_origin_usage(
    material: &str,
    instance: &str,
    height: u64,
) -> Result<usize, String> {
    let namespace = "00".repeat(32);
    let shape = Shape {
        schema_version: 3,
        domain: DOMAIN,
        origin: true,
        ledger_origin_digest: &namespace,
        material_identity: material,
        instance_id: instance,
        height,
        value: None,
        actions_cbor: &[],
        references: &[],
        publication_reservation_ref: None,
        publication_original_data_ref: None,
    };
    length(&shape)?
        .checked_mul(2)
        .ok_or("carry origin size overflow".into())
}
impl VerifiedPublicationOriginalV1 {
    pub(in super::super::super::super) fn carry_local_initial_usage(
        &self,
        local: bool,
    ) -> Result<usize, String> {
        let descriptor = self.carry_copy_descriptor()?;
        (descriptor.alias_bytes as usize)
            .checked_add(if local {
                0
            } else {
                descriptor.received_selection_bytes as usize
            })
            .and_then(|n| n.checked_add(self.metadata.accounted_bytes().ok()?))
            .and_then(|n| n.checked_add(length(&self.root).ok()?))
            .ok_or("carry local initialization size overflow".into())
    }
}
