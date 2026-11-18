//! The shared monetary reducer's ordered reads from the one Current overlay.
use super::*;
use oasis7_world_state::{
    MainTokenAccountBalance,
    incremental_world_balances::{WorldBalanceFieldV1, read_world_balance_overlay_v1},
    incremental_world_number_maps::{
        WorldNumberMapFieldV1, WorldNumberMapKeyRefV1, WorldNumberV1,
        read_world_number_map_overlay_v1,
    },
    world::main_token_monetary_publication::MainTokenTransferReadV1,
};

pub(super) struct CurrentTransferRead<'a, 't, 's, 'v, M, R, E, C> {
    context: &'a mut WorldFieldOverlayContextV1<'t, 's, 'v, M, R, E, C>,
    work: &'a mut MainTokenTransferWorkV1,
    common: &'a FormalBoundedWorkV1,
    pub before: [Option<WorldBalanceReadV1>; 2],
    pub before_nonce: Option<u64>,
    rows: usize,
}
impl<'a, 't, 's, 'v, M, R, E, C> CurrentTransferRead<'a, 't, 's, 'v, M, R, E, C> {
    pub(super) fn new(
        context: &'a mut WorldFieldOverlayContextV1<'t, 's, 'v, M, R, E, C>,
        work: &'a mut MainTokenTransferWorkV1,
        common: &'a FormalBoundedWorkV1,
    ) -> Self {
        Self {
            context,
            work,
            common,
            before: [None; 2],
            before_nonce: None,
            rows: 0,
        }
    }
}
impl<M, R, E, C> MainTokenTransferReadV1 for CurrentTransferRead<'_, '_, '_, '_, M, R, E, C>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    type Error = BodyMutationError;
    fn transfer_nonce(&mut self, account: &str) -> Result<Option<u64>, Self::Error> {
        self.work
            .spend(self.common, account.len(), &mut self.context.cancelled)?;
        let value = read_world_number_map_overlay_v1(
            WorldNumberMapFieldV1::MainTokenTransferNonces,
            WorldNumberMapKeyRefV1::Text(account),
            self.context,
        )
        .map_err(overlay_error)?;
        let nonce = match value {
            None => None,
            Some(WorldNumberV1::Unsigned(value)) => Some(value),
            _ => return Err(BodyMutationError::Invalid("main token nonce scalar type")),
        };
        self.before_nonce = nonce;
        Ok(nonce)
    }
    fn balance(&mut self, account: &str) -> Result<Option<MainTokenAccountBalance>, Self::Error> {
        if self.rows >= self.before.len() {
            return Err(BodyMutationError::Invalid(
                "main token transfer point-read count",
            ));
        }
        self.work
            .spend(self.common, account.len(), &mut self.context.cancelled)?;
        let limit = usize::try_from(self.work.max_identity_bytes).map_err(|_| limited())?;
        // The enclosing owner reserves both of these exact bounded buffers
        // before this reader exists. String conversion retains that allocation.
        let mut identity = Vec::with_capacity(limit);
        if identity.capacity() > limit {
            return Err(limited().into());
        }
        let value = read_world_balance_overlay_v1(
            WorldBalanceFieldV1::MainToken,
            account,
            self.context,
            self.work.max_identity_bytes,
            &mut identity,
        )
        .map_err(overlay_error)?;
        self.before[self.rows] = value;
        self.rows += 1;
        match value {
            None => Ok(None),
            Some(WorldBalanceReadV1::MainToken {
                identity_utf8_bytes,
                liquid_balance,
                vested_balance,
                restricted_starter_claim_balance,
            }) => {
                if identity.len() as u64 != identity_utf8_bytes {
                    return Err(BodyMutationError::Invalid(
                        "main token account identity length",
                    ));
                }
                let account_id = String::from_utf8(identity)
                    .map_err(|_| BodyMutationError::Invalid("main token account identity UTF-8"))?;
                Ok(Some(MainTokenAccountBalance {
                    account_id,
                    liquid_balance,
                    vested_balance,
                    restricted_starter_claim_balance,
                }))
            }
            _ => Err(BodyMutationError::Invalid("main token account scalar type")),
        }
    }
}
