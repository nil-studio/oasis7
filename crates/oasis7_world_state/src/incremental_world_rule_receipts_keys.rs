//! Borrowed canonical keys for original organization estate scope sets.
use super::*;
use crate::incremental_record_columns_read::ColumnKeyV1;
use oasis7_agent_api::world_action::{
    OrganizationEstateRightV1 as Right, OrganizationEstateScopeV1,
};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("organization estate scope key")
}
fn ordinal(right: &Right) -> u64 {
    match right {
        Right::Preserve => 0,
        Right::ReturnTrust => 1,
        Right::DistributeResidual => 2,
        Right::FinalizeCase => 3,
        Right::AdvanceCase => 4,
    }
}
impl CollectionKey for OrganizationEstateScopeV1 {
    fn key_decode_layout_v1<LE>(
        visit: &mut dyn FnMut() -> Result<(), LE>,
    ) -> Result<crate::incremental_pending_action_codec::PendingActionOwnedLayoutV1, LE> {
        visit()?;
        let mut out = crate::incremental_pending_action_codec::PendingActionOwnedLayoutV1::scalar::<Self>();
        out.merge(<String as Codec>::owned_decode_layout_v1(visit)?);
        for symbol in ["object_id", "right", "preserve", "return_trust", "distribute_residual", "finalize_case", "advance_case"] {
            out.symbol(symbol);
        }
        Ok(out)
    }

    type Semantic<'a> = (&'a str, u64);
    fn semantic_key(&self) -> Self::Semantic<'_> {
        (self.object_id.as_str(), ordinal(&self.right))
    }
    fn accepts_column(key: &ColumnKeyV1) -> bool {
        matches!(key, ColumnKeyV1::Pair(left,right)
            if matches!(&**left, ColumnKeyV1::Text(_))
                && matches!(&**right, ColumnKeyV1::Ordinal(0..=4)))
    }
    fn from_column(key: ColumnKeyV1) -> Result<Self, ValueTreeError> {
        let ColumnKeyV1::Pair(left, right) = key else {
            return Err(invalid());
        };
        let ColumnKeyV1::Text(object_id) = *left else {
            return Err(invalid());
        };
        let right = match *right {
            ColumnKeyV1::Ordinal(0) => Right::Preserve,
            ColumnKeyV1::Ordinal(1) => Right::ReturnTrust,
            ColumnKeyV1::Ordinal(2) => Right::DistributeResidual,
            ColumnKeyV1::Ordinal(3) => Right::FinalizeCase,
            ColumnKeyV1::Ordinal(4) => Right::AdvanceCase,
            _ => return Err(invalid()),
        };
        Ok(Self { object_id, right })
    }
}
