//! Pure admission-input v2 fragment geometry, not a transport or a read grant.
//!
//! A plan binds the existing original CAS descriptor's *exact encoded size*.
//! It does not authenticate the descriptor, reserve a request/response budget,
//! authorize release, send anything, or establish a 120-second throughput SLO.
//! A real Noise transport must separately check its full encoded envelope,
//! provider/issuer purpose, original nonce and grant, rolling rate windows,
//! common deadline, and temporary memory/operation ledger before sending.
use super::binding::{BftSignerErrorV3, invalid};
use super::private_retention_input::{
    BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1, BftPrivateRetentionAdmissionInputRefV1,
};

/// Maximum original payload bytes per contiguous part.
pub const BFT_ADMISSION_INPUT_V2_PART_BYTES: u64 = 16 * 1024;
/// Maximum original payload parts in one authenticated application message.
pub const BFT_ADMISSION_INPUT_V2_PARTS_PER_MESSAGE: usize = 3;
/// The full serialized request/reply frame, including its envelope.
pub const BFT_ADMISSION_INPUT_V2_FRAME_BYTES: usize = 64 * 1024;
/// A lower-bound feasibility calculation only, not a network rate reservation.
const PER_PEER_MESSAGES_PER_SECOND: u64 = 16;
const OPERATION_DEADLINE_SECONDS: u64 = 120;
const MESSAGE_DATA_BYTES: u64 =
    BFT_ADMISSION_INPUT_V2_PART_BYTES * BFT_ADMISSION_INPUT_V2_PARTS_PER_MESSAGE as u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BftAdmissionInputPartV2 {
    pub offset: u64,
    pub bytes: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BftAdmissionInputMessageV2 {
    pub index: u32,
    pub parts: [Option<BftAdmissionInputPartV2>; BFT_ADMISSION_INPUT_V2_PARTS_PER_MESSAGE],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BftAdmissionInputTransferPlanV2 {
    bytes: u64,
    messages: u32,
}

impl BftAdmissionInputTransferPlanV2 {
    /// Only exact descriptor capacity and fragmentation are proven here.
    /// The referenced original, grant, signer, deadline and external budgets
    /// require independent validation before a request can be transmitted.
    pub fn for_original(
        reference: &BftPrivateRetentionAdmissionInputRefV1,
    ) -> Result<Self, BftSignerErrorV3> {
        reference.validate()?;
        let bytes = reference.object.bytes;
        if bytes == 0 || bytes > BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1 as u64 {
            return Err(invalid(
                "admission v2 original size outside transport contract",
            ));
        }
        let messages = u32::try_from(bytes.div_ceil(MESSAGE_DATA_BYTES))
            .map_err(|_| invalid("admission v2 message count overflow"))?;
        if u64::from(messages).div_ceil(PER_PEER_MESSAGES_PER_SECOND) > OPERATION_DEADLINE_SECONDS {
            return Err(invalid(
                "admission v2 minimum message rate exceeds operation deadline",
            ));
        }
        Ok(Self { bytes, messages })
    }

    pub fn encoded_original_bytes(&self) -> u64 {
        self.bytes
    }

    pub fn messages(&self) -> u32 {
        self.messages
    }

    /// Theoretical floor at the per-peer message limit, assuming instantaneous
    /// responses. This is never an elapsed-time acceptance or a rate permit.
    pub fn minimum_seconds_at_peer_limit(&self) -> u64 {
        u64::from(self.messages).div_ceil(PER_PEER_MESSAGES_PER_SECOND)
    }

    /// Return no more than 3 contiguous 16 KiB raw part ranges. They cover
    /// exactly the pinned encoded original without overlap or extra bytes.
    pub fn message(&self, index: u32) -> Result<BftAdmissionInputMessageV2, BftSignerErrorV3> {
        if index >= self.messages {
            return Err(invalid("admission v2 message index outside original"));
        }
        let start = u64::from(index)
            .checked_mul(MESSAGE_DATA_BYTES)
            .ok_or_else(|| invalid("admission v2 offset overflow"))?;
        let mut parts = [None; BFT_ADMISSION_INPUT_V2_PARTS_PER_MESSAGE];
        let mut cursor = start;
        for part in &mut parts {
            let remaining = self.bytes.saturating_sub(cursor);
            if remaining == 0 {
                break;
            }
            let take = remaining.min(BFT_ADMISSION_INPUT_V2_PART_BYTES);
            *part = Some(BftAdmissionInputPartV2 {
                offset: cursor,
                bytes: u16::try_from(take)
                    .map_err(|_| invalid("admission v2 part size overflow"))?,
            });
            cursor = cursor
                .checked_add(take)
                .ok_or_else(|| invalid("admission v2 original offset overflow"))?;
        }
        Ok(BftAdmissionInputMessageV2 { index, parts })
    }

    /// Must be called on the *complete encoded* application response, not
    /// solely on its raw fragment payload. Does not imply permission to send.
    pub fn check_full_frame_bytes(encoded: usize) -> Result<(), BftSignerErrorV3> {
        if encoded == 0 || encoded > BFT_ADMISSION_INPUT_V2_FRAME_BYTES {
            return Err(invalid("admission v2 complete encoded frame exceeds bound"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        BftPrivateRetentionKeyV1, BftPrivateRetentionSourceV1, BftSuccessorWindowArchiveRefV1,
    };
    use super::*;

    fn pinned(bytes: u64) -> BftPrivateRetentionAdmissionInputRefV1 {
        BftPrivateRetentionAdmissionInputRefV1 {
            source: BftPrivateRetentionSourceV1 {
                binding_digest: "01".repeat(32),
                handoff_digest: "02".repeat(32),
                material_source_digest: "03".repeat(32),
            },
            key: BftPrivateRetentionKeyV1 {
                ledger_origin_digest: "04".repeat(32),
                height: 1,
                header_id: "05".repeat(32),
            },
            object: BftSuccessorWindowArchiveRefV1 {
                digest: "06".repeat(32),
                bytes,
            },
            admission_digest: "07".repeat(32),
        }
    }

    #[test]
    fn near_66_mib_original_has_exact_bounded_part_coverage_without_materializing_it() {
        let plan = BftAdmissionInputTransferPlanV2::for_original(&pinned(
            BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1 as u64,
        ))
        .unwrap();
        assert_eq!(plan.messages(), 1408);
        assert_eq!(plan.minimum_seconds_at_peer_limit(), 88);
        assert_eq!(plan.encoded_original_bytes(), 66 * 1024 * 1024);
        let mut cursor = 0u64;
        for index in 0..plan.messages() {
            let frame = plan.message(index).unwrap();
            assert_eq!(frame.index, index);
            for part in frame.parts.into_iter().flatten() {
                assert_eq!(part.offset, cursor);
                assert!(
                    part.bytes > 0 && u64::from(part.bytes) <= BFT_ADMISSION_INPUT_V2_PART_BYTES
                );
                cursor += u64::from(part.bytes);
            }
        }
        assert_eq!(cursor, plan.encoded_original_bytes());
        assert!(plan.message(plan.messages()).is_err());
    }

    #[test]
    fn partial_last_message_rejects_out_of_range_and_zero_originals() {
        for bytes in [
            1u64,
            BFT_ADMISSION_INPUT_V2_PART_BYTES,
            BFT_ADMISSION_INPUT_V2_PART_BYTES + 1,
            MESSAGE_DATA_BYTES,
            MESSAGE_DATA_BYTES + 1,
        ] {
            let plan = BftAdmissionInputTransferPlanV2::for_original(&pinned(bytes)).unwrap();
            let mut recovered = 0u64;
            for index in 0..plan.messages() {
                for part in plan.message(index).unwrap().parts.into_iter().flatten() {
                    assert_eq!(part.offset, recovered);
                    recovered += u64::from(part.bytes);
                }
            }
            assert_eq!(recovered, bytes);
            assert!(plan.message(plan.messages()).is_err());
        }
        assert!(BftAdmissionInputTransferPlanV2::for_original(&pinned(0)).is_err());
        assert!(
            BftAdmissionInputTransferPlanV2::for_original(&pinned(
                BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1 as u64 + 1,
            ))
            .is_err()
        );
    }

    #[test]
    fn original_source_digest_and_complete_encoded_frame_are_not_implicitly_trusted() {
        let mut wrong = pinned(1);
        wrong.source.binding_digest = "not a real digest".into();
        assert!(BftAdmissionInputTransferPlanV2::for_original(&wrong).is_err());
        assert!(BftAdmissionInputTransferPlanV2::check_full_frame_bytes(0).is_err());
        assert!(
            BftAdmissionInputTransferPlanV2::check_full_frame_bytes(
                BFT_ADMISSION_INPUT_V2_FRAME_BYTES + 1,
            )
            .is_err()
        );
        BftAdmissionInputTransferPlanV2::check_full_frame_bytes(BFT_ADMISSION_INPUT_V2_FRAME_BYTES)
            .unwrap();
    }
}
