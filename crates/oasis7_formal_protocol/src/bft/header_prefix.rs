//! Public wire-3 consensus ancestry only. Private inputs and World replay are
//! deliberately separate from this DTO and its opaque verification token.
use super::{
    BftHeaderHeightScheduleV3, FormalBftCertifiedHeaderV3, FormalBftExecutionCommitmentV1,
    VerifiedBftHandoffTargetV2, VerifiedBftHeaderCertificateV3,
    formal_handoff::{canonical_bytes, decode_canonical},
    verify_formal_bft_header_certificate_v3,
};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, SeqAccess, Visitor},
};
use std::fmt;

pub const BFT_FORMAL_HEADER_PREFIX_MAX_HEIGHTS_V3: usize = 64;
pub const BFT_FORMAL_HEADER_PREFIX_MAX_BYTES_V3: usize = 16 * 1024 * 1024;

/// The initial revision of this distinct public wire-3 proof schema. The entry
/// shape and every signature domain are incompatible with wire-2 prefixes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftCertifiedHeaderPrefixV3 {
    pub schema_version: u32,
    #[serde(deserialize_with = "bounded_entries")]
    pub entries: Vec<FormalBftCertifiedHeaderV3>,
}

impl FormalBftCertifiedHeaderPrefixV3 {
    fn validate_bounds(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.entries.is_empty()
            || self.entries.len() > BFT_FORMAL_HEADER_PREFIX_MAX_HEIGHTS_V3
        {
            return Err("formal BFT header prefix schema or height capacity".into());
        }
        let mut total = 0usize;
        for entry in &self.entries {
            entry.validate_binding()?;
            let size = serde_cbor::to_vec(entry)
                .map_err(|_| "measure formal BFT header prefix".to_string())?
                .len();
            total = total
                .checked_add(size)
                .ok_or("formal BFT header prefix byte count overflow")?;
            if total > BFT_FORMAL_HEADER_PREFIX_MAX_BYTES_V3 {
                return Err("formal BFT header prefix exceeds cumulative byte bound".into());
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_bounds()?;
        canonical_bytes(
            self,
            BFT_FORMAL_HEADER_PREFIX_MAX_BYTES_V3,
            "formal BFT header prefix",
        )
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, String> {
        let value: Self = decode_canonical(
            bytes,
            BFT_FORMAL_HEADER_PREFIX_MAX_BYTES_V3,
            "formal BFT header prefix",
        )?;
        value.validate_bounds()?;
        Ok(value)
    }

    /// Verify every QC and schedule transition from the externally qualified H.
    /// Initial parent roots remain the authenticated validators' execution
    /// claims; a driver must independently restore H and replay the World.
    /// A consumer must also match its independently pinned exact target.
    pub fn verify(
        &self,
        handoff: &VerifiedBftHandoffTargetV2,
    ) -> Result<VerifiedFormalBftHeaderPrefixV3, String> {
        let encoded_bytes = self.canonical_bytes()?.len();
        let mut schedule = None;
        let mut tip = None;
        for entry in &self.entries {
            let bound = entry.validate_binding()?;
            let current = match schedule.take() {
                Some(schedule) => schedule,
                None => handoff.initial_schedule_for_commitment(&bound)?,
            };
            let verified = verify_formal_bft_header_certificate_v3(&current, entry)?;
            schedule = Some(verified.next_height_schedule().clone());
            tip = Some(verified);
        }
        Ok(VerifiedFormalBftHeaderPrefixV3 {
            handoff: handoff.clone(),
            certificate: tip.ok_or("formal BFT header prefix is empty")?,
            heights: self.entries.len(),
            encoded_bytes,
        })
    }
}

/// Authenticated bounded protocol ancestry. No Deserialize, arbitrary-token
/// constructor, World-executed flag, permission grant or freshness assertion.
#[derive(Debug, Clone)]
pub struct VerifiedFormalBftHeaderPrefixV3 {
    handoff: VerifiedBftHandoffTargetV2,
    certificate: VerifiedBftHeaderCertificateV3,
    heights: usize,
    encoded_bytes: usize,
}

impl VerifiedFormalBftHeaderPrefixV3 {
    pub fn handoff(&self) -> &VerifiedBftHandoffTargetV2 {
        &self.handoff
    }
    pub fn certificate(&self) -> &VerifiedBftHeaderCertificateV3 {
        &self.certificate
    }
    pub fn commitment(&self) -> &FormalBftExecutionCommitmentV1 {
        self.certificate.bound_header().commitment()
    }
    pub fn next_height_schedule(&self) -> &BftHeaderHeightScheduleV3 {
        self.certificate.next_height_schedule()
    }
    pub fn heights(&self) -> usize {
        self.heights
    }
    pub fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }
}

fn bounded_entries<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<FormalBftCertifiedHeaderV3>, D::Error> {
    struct Entries;
    impl<'de> Visitor<'de> for Entries {
        type Value = Vec<FormalBftCertifiedHeaderV3>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded public header certificate prefix")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq
                .size_hint()
                .is_some_and(|n| n > BFT_FORMAL_HEADER_PREFIX_MAX_HEIGHTS_V3)
            {
                return Err(de::Error::custom(
                    "formal BFT header prefix height capacity",
                ));
            }
            let mut entries = Vec::new();
            while entries.len() < BFT_FORMAL_HEADER_PREFIX_MAX_HEIGHTS_V3 {
                match seq.next_element()? {
                    Some(entry) => entries.push(entry),
                    None => return Ok(entries),
                }
            }
            if seq.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom(
                    "formal BFT header prefix height capacity",
                ));
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(Entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_prefix_declared_height_bound_precedes_any_entry() {
        #[derive(Deserialize)]
        struct Entries(
            #[serde(deserialize_with = "bounded_entries")] Vec<FormalBftCertifiedHeaderV3>,
        );
        let error = serde_cbor::from_slice::<Entries>(&[0x98, 65])
            .err()
            .unwrap();
        assert!(error.to_string().contains("height capacity"));
        assert!(
            serde_cbor::from_slice::<Entries>(&[0x80])
                .unwrap()
                .0
                .is_empty()
        );
        assert!(
            FormalBftCertifiedHeaderPrefixV3 {
                schema_version: 1,
                entries: Vec::new()
            }
            .canonical_bytes()
            .is_err()
        );
    }
}
