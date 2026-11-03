//! Exact immutable seal planning. This module is deliberately separate from
//! ordinary monotonic Reserve/Ready extensions: only the dedicated atomic
//! backend operation may install its carry map and advance the coverage floor.
use super::binding::{BftSignerErrorV3, invalid};
use super::crypto::{digest_hex, encode_bounded, verify_signature};
use super::private_retention::{
    BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1, BftPrivateRetentionAttestorV1,
    BftPrivateRetentionKeyV1, BftPrivateRetentionLifecycleV1, BftPrivateRetentionMapV1,
};
use super::private_retention_backend::BftPrivateRetentionLedgerV1;
use super::state::BftSignerStateV3;
use super::successor_window::BftSuccessorWindowCutV1;
use serde::{Deserialize, Serialize};

const SEAL_DOMAIN: &str = super::private_retention_current_anchor::SEAL_ANCHOR_DOMAIN;
const SEAL_BODY_DOMAIN: &str = "oasis7.bft.private-retention.seal-body.v1";
const MAX_SEAL: usize = 1024 * 1024 + 32 * 1024;

/// Replay material only. Deserialization never supplies a migration grant or
/// permits a backend to replace its current original ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSealBodyV1 {
    pub schema_version: u32,
    pub original: BftPrivateRetentionLedgerV1,
    pub cut: BftSuccessorWindowCutV1,
    #[serde(deserialize_with = "deserialize_carry")]
    pub carry_keys: Vec<BftPrivateRetentionKeyV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPrivateRetentionSealV1 {
    pub body: BftPrivateRetentionSealBodyV1,
    pub signature_hex: String,
}

impl BftPrivateRetentionSealBodyV1 {
    /// The caller cannot choose the carry set. All still-live T+1 originals,
    /// including unsigned alternatives, are derived from the complete old map.
    pub fn from_original(
        original: BftPrivateRetentionLedgerV1,
        cut: BftSuccessorWindowCutV1,
    ) -> Result<Self, BftSignerErrorV3> {
        let carry_keys = original
            .map
            .entries
            .iter()
            .filter(|entry| entry.key.height > cut.height)
            .map(|entry| entry.key.clone())
            .collect();
        let body = Self {
            schema_version: if original.selection_index.is_some() {
                2
            } else {
                1
            },
            original,
            cut,
            carry_keys,
        };
        body.validate_shape()?;
        Ok(body)
    }

    pub fn validate_shape(&self) -> Result<(), BftSignerErrorV3> {
        super::private_retention_selection::validate_ledger(&self.original)?;
        self.original.map.validate()?;
        self.cut.validate()?;
        let next_height = self
            .cut
            .height
            .checked_add(1)
            .ok_or_else(|| invalid("retention seal cut overflow"))?;
        if self.schema_version
            != if self.original.selection_index.is_some() {
                2
            } else {
                1
            }
            || self.cut.height < self.original.map.coverage_start_height
            || self
                .original
                .map
                .entries
                .iter()
                .any(|entry| entry.lifecycle != BftPrivateRetentionLifecycleV1::Ready)
            || self.carry_keys.len() > BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1
            || !self.carry_keys.windows(2).all(|pair| pair[0] < pair[1])
            || self
                .original
                .map
                .entries
                .iter()
                .any(|entry| entry.key.height > next_height)
            || self
                .original
                .map
                .entries
                .iter()
                .filter(|entry| entry.key.height > self.cut.height)
                .map(|entry| &entry.key)
                .ne(self.carry_keys.iter())
        {
            return Err(invalid(
                "retention seal incomplete lifecycle/cut/carry inventory",
            ));
        }
        encode_bounded(self, MAX_SEAL)?;
        Ok(())
    }

    /// Public certificate/install checks are necessary but insufficient: the
    /// configured independent Runtime verifier must also replay the full archive.
    pub fn validate_current(
        &self,
        trusted: &BftPrivateRetentionAttestorV1,
        state: &BftSignerStateV3,
    ) -> Result<(), BftSignerErrorV3> {
        self.validate_shape()?;
        self.original
            .receipt
            .verify_map(trusted, &self.original.map)?;
        self.validate_applied_cut(trusted, state)
    }

    /// Only callers which have independently authenticated the complete old
    /// receipt/map history may use this cut-only check after a prior seal.
    pub(crate) fn validate_applied_cut(
        &self,
        trusted: &BftPrivateRetentionAttestorV1,
        state: &BftSignerStateV3,
    ) -> Result<(), BftSignerErrorV3> {
        self.validate_shape()?;
        let certificate = state
            .history
            .last()
            .ok_or_else(|| invalid("retention seal lacks certified cut"))?;
        let commitment = &certificate.execution_commitment;
        if state.private_retention.as_ref() != Some(&self.original.receipt.body.head)
            || state.binding_digest != trusted.source.binding_digest
            || state.pending_driver_apply.is_some()
            || commitment.height != self.cut.height
            || certificate.header.block_id().map_err(invalid)? != self.cut.header_id
            || blake3::hash(&certificate.canonical_bytes().map_err(invalid)?)
                .to_hex()
                .as_str()
                != self.cut.certificate_digest
            || commitment.digest().map_err(invalid)? != self.cut.execution_commitment_digest
            || commitment.execution_state_root != self.cut.snapshot_ref.digest
            || commitment.state_root != self.cut.semantic_state_root
            || commitment.post_manifest_hash != self.cut.post_manifest_hash
            || commitment.result_root != self.cut.business_result_root
            || state.applied_installs.last().is_none_or(|applied| {
                applied.height != self.cut.height
                    || applied.header_id != self.cut.header_id
                    || applied.driver_install_digest != self.cut.driver_install_digest
            })
        {
            return Err(invalid(
                "retention seal differs from current original applied cut",
            ));
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        self.validate_shape()?;
        digest_hex(
            if self.schema_version == 2 {
                "oasis7.bft.private-retention.seal-body.v2"
            } else {
                SEAL_BODY_DOMAIN
            },
            self,
        )
    }

    pub fn carry_selection_index(
        &self,
    ) -> Result<Option<super::BftPrivateRetentionSelectionIndexV1>, BftSignerErrorV3> {
        self.validate_shape()?;
        Ok(self.original.selection_index.as_ref().map(|original| {
            super::BftPrivateRetentionSelectionIndexV1 {
                source: original.source.clone(),
                last_sequence: original.last_sequence,
                selections: original
                    .selections
                    .iter()
                    .filter(|s| s.key.height > self.cut.height)
                    .cloned()
                    .collect(),
            }
        }))
    }

    /// This DTO is an expected atomic result, never authorization to write it.
    pub fn carry_map(&self) -> Result<BftPrivateRetentionMapV1, BftSignerErrorV3> {
        self.validate_shape()?;
        let map = BftPrivateRetentionMapV1 {
            schema_version: self.original.map.schema_version,
            source: self.original.map.source.clone(),
            coverage_start_height: self
                .cut
                .height
                .checked_add(1)
                .ok_or_else(|| invalid("retention carry floor overflow"))?,
            previous_seal_receipt_digest: Some(self.digest()?),
            entries: self
                .original
                .map
                .entries
                .iter()
                .filter(|entry| entry.key.height > self.cut.height)
                .cloned()
                .collect(),
        };
        map.validate()?;
        Ok(map)
    }
}

impl BftPrivateRetentionSealV1 {
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, BftSignerErrorV3> {
        let seal: Self = super::crypto::decode_bounded(bytes, MAX_SEAL)?;
        seal.body.validate_shape()?;
        Ok(seal)
    }

    pub fn verify(&self, trusted: &BftPrivateRetentionAttestorV1) -> Result<(), BftSignerErrorV3> {
        self.body.validate_shape()?;
        self.body
            .original
            .receipt
            .verify_map(trusted, &self.body.original.map)?;
        verify_signature(
            SEAL_DOMAIN,
            &self.anchor()?.body,
            &self.signature_hex,
            &trusted.provider_public_key,
        )
    }
    pub fn anchor(
        &self,
    ) -> Result<
        super::private_retention_current_anchor::BftPrivateRetentionSealAnchorV1,
        BftSignerErrorV3,
    > {
        Ok(super::private_retention_current_anchor::BftPrivateRetentionSealAnchorV1 {
            body: super::private_retention_current_anchor::BftPrivateRetentionSealAnchorBodyV1::from_original(&self.body)?,
            signature_hex: self.signature_hex.clone(),
        })
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, BftSignerErrorV3> {
        self.body.validate_shape()?;
        encode_bounded(self, MAX_SEAL)
    }
}

fn deserialize_carry<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<BftPrivateRetentionKeyV1>, D::Error> {
    struct Carry;
    impl<'de> serde::de::Visitor<'de> for Carry {
        type Value = Vec<BftPrivateRetentionKeyV1>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("bounded exact original T+1 carry keys")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            if sequence
                .size_hint()
                .is_some_and(|n| n > BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1)
            {
                return Err(serde::de::Error::custom("retention carry count capacity"));
            }
            let mut keys = Vec::new();
            while let Some(key) = sequence.next_element()? {
                if keys.len() == BFT_PRIVATE_RETENTION_MAX_ALTERNATIVES_V1 {
                    return Err(serde::de::Error::custom("retention carry count capacity"));
                }
                keys.push(key);
            }
            Ok(keys)
        }
    }
    deserializer.deserialize_seq(Carry)
}

#[path = "private_retention_seal_chain.rs"]
mod chain;
pub use chain::ValidatedBftPrivateRetentionSealChainV1;
pub(crate) use chain::verify_map_at_floor;
#[path = "private_retention_seal_memory.rs"]
mod memory;
pub use memory::BftPrivateRetentionSealCasV1;
#[cfg(test)]
#[path = "private_retention_seal_tests.rs"]
mod tests;
