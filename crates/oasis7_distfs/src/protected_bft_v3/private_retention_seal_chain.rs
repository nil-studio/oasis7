//! Independently authenticated full sealed-map/carry chain.
use super::*;
/// Authenticates completeness of sealed maps and the current carry map only.
/// This is not a World replay/migration grant or a private ReadGrant.
pub struct ValidatedBftPrivateRetentionSealChainV1 {
    seals: Vec<std::sync::Arc<BftPrivateRetentionSealV1>>,
}
impl ValidatedBftPrivateRetentionSealChainV1 {
    pub fn verify(
        trusted: &BftPrivateRetentionAttestorV1,
        seals: Vec<std::sync::Arc<BftPrivateRetentionSealV1>>,
        current: &BftPrivateRetentionLedgerV1,
    ) -> Result<Self, BftSignerErrorV3> {
        trusted.digest()?;
        let mut floor = trusted.coverage_start_height;
        let mut predecessor = None;
        let mut previous: Option<&BftPrivateRetentionSealV1> = None;
        for seal in &seals {
            seal.body.validate_shape()?;
            verify_map_at_floor(trusted, &seal.body.original, floor)?;
            verify_signature(
                SEAL_DOMAIN,
                &seal.anchor()?.body,
                &seal.signature_hex,
                &trusted.provider_public_key,
            )?;
            if seal.body.original.map.previous_seal_receipt_digest != predecessor {
                return Err(invalid("retention seal predecessor discontinuity"));
            }
            if let Some(previous) = previous {
                require_carry_preserved(previous, &seal.body.original)?;
                if seal.body.cut.height <= previous.body.cut.height {
                    return Err(invalid("retention seal cut did not advance"));
                }
            }
            floor = seal
                .body
                .cut
                .height
                .checked_add(1)
                .ok_or_else(|| invalid("retention seal coverage overflow"))?;
            predecessor = Some(seal.body.digest()?);
            previous = Some(seal);
        }
        verify_map_at_floor(trusted, current, floor)?;
        if current.map.previous_seal_receipt_digest != predecessor {
            return Err(invalid(
                "retention current map differs from original seal chain",
            ));
        }
        if let Some(previous) = previous {
            require_carry_preserved(previous, current)?;
        }
        Ok(Self { seals })
    }
    pub fn seals(&self) -> &[std::sync::Arc<BftPrivateRetentionSealV1>] {
        &self.seals
    }
}

fn require_carry_preserved(
    seal: &BftPrivateRetentionSealV1,
    next: &BftPrivateRetentionLedgerV1,
) -> Result<(), BftSignerErrorV3> {
    if let Some(original) = seal.body.carry_selection_index()? {
        let next_index = next
            .selection_index
            .as_ref()
            .ok_or_else(|| invalid("seal chain lost original choice inventory"))?;
        if next_index.last_sequence < original.last_sequence
            || original
                .selections
                .iter()
                .any(|s| !next_index.selections.contains(s))
        {
            return Err(invalid("seal chain changed original choice or sequence"));
        }
    }
    if next.receipt.body.head.sequence <= seal.body.original.receipt.body.head.sequence {
        return Err(invalid("retention carried head sequence did not advance"));
    }
    for original in seal
        .body
        .original
        .map
        .entries
        .iter()
        .filter(|entry| entry.key.height > seal.body.cut.height)
    {
        let carried = next
            .map
            .entries
            .binary_search_by(|entry| entry.key.cmp(&original.key))
            .ok()
            .map(|index| &next.map.entries[index])
            .ok_or_else(|| invalid("retention seal chain omitted original carry"))?;
        if carried != original {
            return Err(invalid(
                "retention seal chain changed original carry fact/lifecycle",
            ));
        }
    }
    Ok(())
}

/// A changed floor is accepted only inside an already verified explicit seal
/// chain. Ordinary receipt verification retains its fixed initial-floor rule.
pub(crate) fn verify_map_at_floor(
    trusted: &BftPrivateRetentionAttestorV1,
    ledger: &BftPrivateRetentionLedgerV1,
    floor: u64,
) -> Result<(), BftSignerErrorV3> {
    super::super::private_retention_selection::validate_ledger(ledger)?;
    let head = &ledger.receipt.body.head;
    head.validate()?;
    let bytes = ledger.map.canonical_bytes()?;
    if ledger.map.coverage_start_height != floor
        || floor < trusted.coverage_start_height
        || ledger.map.source != trusted.source
        || head.source != trusted.source
        || head.capability_digest != trusted.digest()?
        || head.map_digest != ledger.map.digest()?
        || head.map_byte_hash != blake3::hash(&bytes).to_hex().as_str()
        || usize::try_from(head.entry_count).ok() != Some(ledger.map.entries.len())
        || ledger.receipt.body.after_revision
            != ledger
                .receipt
                .body
                .before_revision
                .checked_add(1)
                .ok_or_else(|| invalid("retention sealed revision overflow"))?
    {
        return Err(invalid("retention sealed receipt/map/floor mismatch"));
    }
    for digest in [
        &ledger.receipt.body.operation_digest,
        &ledger.receipt.body.before_state_digest,
        &ledger.receipt.body.after_state_digest,
    ] {
        crate::protected_bft_v3::state::validate_digest(digest)?;
    }
    if let Some(digest) = &head.previous_receipt_digest {
        crate::protected_bft_v3::state::validate_digest(digest)?;
    }
    verify_signature(
        crate::protected_bft_v3::private_retention::RECEIPT_DOMAIN,
        &ledger.receipt.body,
        &ledger.receipt.signature_hex,
        &trusted.provider_public_key,
    )
}
