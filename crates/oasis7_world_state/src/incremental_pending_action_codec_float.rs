//! Exact IEEE-754 storage for the original settlement score fields.
use super::*;
impl Codec for f64 {
    fn encode<W: ValueNodeWriter + ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
        &self,
        _p: &TypedRecordProfileV1,
        _ctx: &mut MemberRecordWriteContext<'_, W, C>,
        _w: &mut TypedRecordWriteWorkV1,
    ) -> Result<Encoded<'_>, MemberRecordError> {
        Ok(Encoded::Scalar(MemberPayloadRefV1::U64(self.to_bits())))
    }
    fn decode<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        cell: ColumnCellV1,
        d: &mut Decoder<'_, '_, M, R, E, C>,
    ) -> Result<Self, ValueTreeError> {
        Ok(Self::from_bits(u64::decode(cell, d)?))
    }
    fn accept_changed<
        M: MerkleNodeReader,
        R: ValueNodeReader,
        E: FnMut() -> Option<ValueTreeError>,
        C: FnMut() -> bool,
    >(
        p: &TypedRecordProfileV1,
        a: Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
        b: Option<&crate::incremental_typed_record_diff::TypedCellRefV1<'_>>,
        d: &mut crate::incremental_typed_record_diff::ChangeDecoder<'_, '_, M, R, E, C>,
    ) -> Result<(), ValueTreeError> {
        u64::accept_changed(p, a, b, d)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::incremental_typed_record_test_support::{roundtrip, source_admission};
    #[derive(Debug)]
    struct Bits {
        value: f64,
    }
    impl PartialEq for Bits {
        fn eq(&self, other: &Self) -> bool {
            self.value.to_bits() == other.value.to_bits()
        }
    }
    dto!(Bits, { value });
    #[test]
    fn settlement_float_cold_recovery_preserves_all_bits() {
        let _admission = source_admission();
        for bits in [
            0_u64,
            1_u64 << 63,
            f64::MAX.to_bits(),
            f64::MIN.to_bits(),
            1,
            0x0010_0000_0000_0000,
            f64::INFINITY.to_bits(),
            f64::NEG_INFINITY.to_bits(),
            0x7ff8_0000_0000_0001,
            0xfff8_0000_0000_00a5,
            0x7ff0_0000_0000_0001,
        ] {
            roundtrip(
                &Bits {
                    value: f64::from_bits(bits),
                },
                profile(),
            );
        }
    }
}
