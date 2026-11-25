//! Bounded, ordered original-admission input byte staging; never grants a read.
//!
//! Caller must authenticate the Noise/issuer purpose, original reference and
//! nonce *before* invoking this adapter, and must retain its own rate, memory,
//! persistent-transfer and 120-second deadline reservations. Completing this
//! staging step does not validate canonical AdmissionInput, publish a CAS pin,
//! authorize a signer, or release bytes to any peer.
use std::io::Write;

use super::admission_input_transfer_plan::BftAdmissionInputTransferPlanV2;
use super::binding::{BftSignerErrorV3, invalid};
use super::private_retention_input::BftPrivateRetentionAdmissionInputRefV1;

/// Owns a caller-provisioned staging sink; this struct buffers no entire input.
/// Any invalid frame or partial sink failure poisons the *whole* attempt.
pub struct BftAdmissionInputV2StreamStage<W: Write> {
    plan: BftAdmissionInputTransferPlanV2,
    expected_digest: String,
    sink: W,
    hash: blake3::Hasher,
    next_index: u32,
    received_bytes: u64,
    poisoned: bool,
}

impl<W: Write> BftAdmissionInputV2StreamStage<W> {
    pub fn new(
        reference: &BftPrivateRetentionAdmissionInputRefV1,
        sink: W,
    ) -> Result<Self, BftSignerErrorV3> {
        let plan = BftAdmissionInputTransferPlanV2::for_original(reference)?;
        Ok(Self {
            plan,
            expected_digest: reference.object.digest.clone(),
            sink,
            hash: blake3::Hasher::new(),
            next_index: 0,
            received_bytes: 0,
            poisoned: false,
        })
    }

    pub fn messages(&self) -> u32 {
        self.plan.messages()
    }

    /// A local observation only, not a resumable authorization or rate token.
    pub fn next_index(&self) -> u32 {
        self.next_index
    }

    /// Accept slices that the separately authenticated wire decoder has
    /// extracted from one *complete* application frame. The caller must pass
    /// the actual measured full-frame byte count, including every envelope.
    /// This method does not authenticate the frame or grant a budget.
    pub fn append_decoded_message(
        &mut self,
        index: u32,
        encoded_frame_bytes: usize,
        parts: &[&[u8]],
    ) -> Result<(), BftSignerErrorV3> {
        if self.poisoned {
            return Err(invalid("admission v2 staging attempt already invalid"));
        }
        // A replay, gap, invalid length or sink failure is terminal for this
        // attempt. In particular, do not let a caller retry a partial write.
        self.poisoned = true;
        BftAdmissionInputTransferPlanV2::check_full_frame_bytes(encoded_frame_bytes)?;
        if index != self.next_index {
            return Err(invalid("admission v2 repeated or out-of-order message"));
        }
        let planned = self.plan.message(index)?;
        let expected: Vec<_> = planned.parts.into_iter().flatten().collect();
        if parts.len() != expected.len() {
            return Err(invalid("admission v2 part count differs from original"));
        }
        let mut next_offset = self.received_bytes;
        let mut raw_len = 0usize;
        for (part, range) in parts.iter().zip(expected.iter()) {
            if range.offset != next_offset || part.len() != usize::from(range.bytes) {
                return Err(invalid("admission v2 part layout differs from original"));
            }
            next_offset = next_offset
                .checked_add(u64::from(range.bytes))
                .ok_or_else(|| invalid("admission v2 offset overflow"))?;
            raw_len = raw_len
                .checked_add(part.len())
                .ok_or_else(|| invalid("admission v2 fragment length overflow"))?;
        }
        if encoded_frame_bytes < raw_len || next_offset > self.plan.encoded_original_bytes() {
            return Err(invalid("admission v2 encoded frame or original overflow"));
        }

        // Validate *all* geometry before a single byte reaches the sink.
        for part in parts {
            self.sink
                .write_all(part)
                .map_err(|_| invalid("admission v2 staging sink write failed"))?;
            self.hash.update(part);
        }
        self.received_bytes = next_offset;
        self.next_index = self
            .next_index
            .checked_add(1)
            .ok_or_else(|| invalid("admission v2 message index overflow"))?;
        self.poisoned = false;
        Ok(())
    }

    /// Returns only raw BLAKE3/length-checked staged bytes. Caller must still
    /// validate canonical AdmissionInput and its admission_digest against the
    /// independently authenticated reference, CAS-persist and exact-read-back,
    /// and maintain original transfer/buffer charges and grant lifetimes.
    pub fn finish(mut self) -> Result<W, BftSignerErrorV3> {
        if self.poisoned
            || self.next_index != self.plan.messages()
            || self.received_bytes != self.plan.encoded_original_bytes()
            || self.hash.finalize().to_hex().as_str() != self.expected_digest
        {
            return Err(invalid("admission v2 incomplete or digest mismatch"));
        }
        self.sink
            .flush()
            .map_err(|_| invalid("admission v2 staging sink flush failed"))?;
        Ok(self.sink)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1, BftPrivateRetentionKeyV1,
        BftPrivateRetentionSourceV1, BftSuccessorWindowArchiveRefV1,
    };
    use super::*;
    use std::io;

    fn reference(bytes: u64, digest: String) -> BftPrivateRetentionAdmissionInputRefV1 {
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
            object: BftSuccessorWindowArchiveRefV1 { digest, bytes },
            admission_digest: "07".repeat(32),
        }
    }

    #[test]
    fn bounded_streaming_near_66_mib_requires_no_original_sized_staging_buffer() {
        let part = [0u8; 16 * 1024];
        let bytes = BFT_PRIVATE_RETENTION_INPUT_MAX_BYTES_V1 as u64;
        let mut digest = blake3::Hasher::new();
        for _ in 0..bytes / part.len() as u64 {
            digest.update(&part);
        }
        let reference = reference(bytes, digest.finalize().to_hex().to_string());
        let mut stage = BftAdmissionInputV2StreamStage::new(&reference, io::sink()).unwrap();
        assert_eq!(stage.messages(), 1408);
        for index in 0..stage.messages() {
            let fragments = [&part[..], &part[..], &part[..]];
            stage.append_decoded_message(index, 50_000, &fragments).unwrap();
        }
        assert_eq!(stage.next_index(), 1408);
        stage.finish().unwrap();
    }

    #[test]
    fn refuses_replay_wrong_shape_early_finish_and_wrong_hash() {
        let reference = reference(5, blake3::hash(b"hello").to_hex().to_string());
        let early = BftAdmissionInputV2StreamStage::new(&reference, Vec::new()).unwrap();
        assert!(early.finish().is_err());

        let mut wrong = BftAdmissionInputV2StreamStage::new(&reference, Vec::new()).unwrap();
        assert!(wrong.append_decoded_message(0, 65_537, &[b"hello"]).is_err());
        assert!(wrong.append_decoded_message(0, 7, &[b"hello"]).is_err());

        let mut wrong = BftAdmissionInputV2StreamStage::new(&reference, Vec::new()).unwrap();
        assert!(wrong.append_decoded_message(0, 4, &[b"hello"]).is_err());
        assert!(wrong.finish().is_err());

        let mut duplicate = BftAdmissionInputV2StreamStage::new(&reference, Vec::new()).unwrap();
        duplicate.append_decoded_message(0, 7, &[b"hello"]).unwrap();
        assert!(duplicate.append_decoded_message(0, 7, &[b"hello"]).is_err());
        assert!(duplicate.finish().is_err());

        let mut wrong_digest =
            BftAdmissionInputV2StreamStage::new(&reference, Vec::new()).unwrap();
        wrong_digest.append_decoded_message(0, 7, &[b"world"]).unwrap();
        assert!(wrong_digest.finish().is_err());

        let mut extra = BftAdmissionInputV2StreamStage::new(&reference, Vec::new()).unwrap();
        assert!(extra.append_decoded_message(0, 10, &[b"hello", b"!"]).is_err());
        assert!(extra.finish().is_err());
    }

    #[test]
    fn refuses_out_of_order_and_truncated_part_before_writing() {
        let input = vec![1u8; 16 * 1024 + 1];
        let reference = reference(input.len() as u64, blake3::hash(&input).to_hex().to_string());
        let mut stage = BftAdmissionInputV2StreamStage::new(&reference, Vec::new()).unwrap();
        assert!(stage.append_decoded_message(1, 100, &[b"x"]).is_err());
        assert!(stage.finish().is_err());

        let mut stage = BftAdmissionInputV2StreamStage::new(&reference, Vec::new()).unwrap();
        let first = &input[..16 * 1024];
        assert!(stage.append_decoded_message(0, 20_000, &[first]).is_err());
        assert!(stage.finish().is_err());

        let mut stage = BftAdmissionInputV2StreamStage::new(&reference, Vec::new()).unwrap();
        let pieces = [&input[..16 * 1024], &input[16 * 1024..]];
        stage.append_decoded_message(0, 16_500, &pieces).unwrap();
        assert_eq!(stage.finish().unwrap(), input);
    }

    struct BrokenSink {
        writes: u8,
    }
    impl Write for BrokenSink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.writes == 0 {
                self.writes = 1;
                return Ok(bytes.len().min(1));
            }
            Err(io::Error::other("simulated partial source loss"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn sink_failure_poisoning_cannot_be_retried_as_a_fresh_message() {
        let reference = reference(5, blake3::hash(b"hello").to_hex().to_string());
        let mut stage =
            BftAdmissionInputV2StreamStage::new(&reference, BrokenSink { writes: 0 }).unwrap();
        assert!(stage.append_decoded_message(0, 7, &[b"hello"]).is_err());
        assert!(stage.append_decoded_message(0, 7, &[b"hello"]).is_err());
        assert!(stage.finish().is_err());
    }
}
