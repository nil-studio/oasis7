//! Original fixed-role capsule metadata, without a live directory witness.
use super::*;
pub(in super::super::super) struct VerifiedSourceArchiveActivationV1 {
    first: oasis7_distfs::protected_bft_v3::BftPrivateRetentionLedgerV1,
    h_refs: BTreeSet<String>,
}
impl VerifiedSourceArchiveActivationV1 {
    pub(in super::super::super) fn first(
        &self,
    ) -> &oasis7_distfs::protected_bft_v3::BftPrivateRetentionLedgerV1 {
        &self.first
    }
    pub(in super::super::super) fn h_refs(&self) -> &BTreeSet<String> {
        &self.h_refs
    }
}
pub(in super::super::super) fn verify_source_archive_activation(
    original: &[u8],
    capsule: &[u8],
    binding: &BftSignerBindingV3,
    role: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionAttestorV1,
    origin: &str,
    material: &str,
    instance: &str,
) -> Result<VerifiedSourceArchiveActivationV1, String> {
    let data: Data =
        serde_cbor::from_slice(original).map_err(|_| "decode archive source Origin")?;
    data.validate()?;
    if data.bytes()? != original
        || data.schema_version != 8
        || !data.origin
        || data.ledger_origin_digest != origin
        || data.material_identity != material
        || data.instance_id != instance
        || data.height.checked_add(1) != Some(binding.initial_schedule().height())
    {
        return Err("archived source Origin differs from original H owner".into());
    }
    let activation = data
        .source_activation
        .as_ref()
        .ok_or("archive source capsule binding absent")?;
    if capsule.len() as u64 != activation.encoded_bytes
        || blake3_hex(capsule) != activation.reference
    {
        return Err("archive source capsule exact ref differs".into());
    }
    let first = super::super::super::source_creation::activation::validate_archive_capsule(
        capsule, binding, role, origin, material, instance,
    )?;
    Ok(VerifiedSourceArchiveActivationV1 {
        first,
        h_refs: data.references.into_iter().collect(),
    })
}
impl NodePrivateStore {
    pub(in super::super) fn source_archive_activation_materials(
        &self,
        records: &Path,
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        let origin = self
            .origin
            .as_ref()
            .ok_or("archive source original Origin absent")?;
        if origin.data.schema_version != 8 {
            return Err("archive source is not covered original Origin".into());
        }
        let activation = origin
            .data
            .source_activation
            .as_ref()
            .ok_or("archive source capsule binding absent")?;
        source_activation::verify_activation_material(&self.store, &origin.data)?;
        let bytes = common_storage::read_cas_bounded(
            &self.store,
            &activation.reference,
            usize::try_from(activation.encoded_bytes)
                .map_err(|_| "archive source capsule length")?,
        )?;
        let original = source_input::read_source_origin(records, &self.store)?;
        if origin.data.bytes()? != original {
            return Err("archive source original changed since open".into());
        }
        Ok((original, bytes))
    }
}
