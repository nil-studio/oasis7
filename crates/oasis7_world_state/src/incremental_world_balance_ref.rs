use super::*;
/// Borrow original named contents, not account identity or business permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldBalanceRefV1<'a> {
    MainToken {
        account_id: &'a str,
        liquid_balance: u64,
        vested_balance: u64,
        restricted_starter_claim_balance: u64,
    },
    NodeAsset {
        node_id: &'a str,
        power_credit_balance: u64,
        total_minted_credits: u64,
        total_burned_credits: u64,
    },
}
impl<'a> From<&'a MainTokenAccountBalance> for WorldBalanceRefV1<'a> {
    fn from(value: &'a MainTokenAccountBalance) -> Self {
        let MainTokenAccountBalance {
            account_id,
            liquid_balance,
            vested_balance,
            restricted_starter_claim_balance,
        } = value;
        Self::MainToken {
            account_id,
            liquid_balance: *liquid_balance,
            vested_balance: *vested_balance,
            restricted_starter_claim_balance: *restricted_starter_claim_balance,
        }
    }
}
impl<'a> From<&'a NodeAssetBalance> for WorldBalanceRefV1<'a> {
    fn from(value: &'a NodeAssetBalance) -> Self {
        let NodeAssetBalance {
            node_id,
            power_credit_balance,
            total_minted_credits,
            total_burned_credits,
        } = value;
        Self::NodeAsset {
            node_id,
            power_credit_balance: *power_credit_balance,
            total_minted_credits: *total_minted_credits,
            total_burned_credits: *total_burned_credits,
        }
    }
}
impl WorldBalanceRefV1<'_> {
    pub fn field(self) -> WorldBalanceFieldV1 {
        match self {
            Self::MainToken { .. } => WorldBalanceFieldV1::MainToken,
            Self::NodeAsset { .. } => WorldBalanceFieldV1::NodeAsset,
        }
    }
    pub(super) fn identity(&self) -> &str {
        match self {
            Self::MainToken { account_id, .. } => account_id,
            Self::NodeAsset { node_id, .. } => node_id,
        }
    }
    pub(super) fn amounts(self) -> [u64; 3] {
        match self {
            Self::MainToken {
                liquid_balance,
                vested_balance,
                restricted_starter_claim_balance,
                ..
            } => [
                liquid_balance,
                vested_balance,
                restricted_starter_claim_balance,
            ],
            Self::NodeAsset {
                power_credit_balance,
                total_minted_credits,
                total_burned_credits,
                ..
            } => [
                power_credit_balance,
                total_minted_credits,
                total_burned_credits,
            ],
        }
    }
    pub(super) fn summary(self) -> WorldBalanceReadV1 {
        summary(self.field(), self.identity().len() as u64, self.amounts())
    }
}
