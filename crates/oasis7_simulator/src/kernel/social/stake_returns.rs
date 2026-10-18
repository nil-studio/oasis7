use crate::social::{SocialFactState, SocialStake};

use super::super::super::types::{ResourceKind, ResourceOwner};
use super::super::WorldKernel;
use super::super::types::RejectReason;

impl WorldKernel {
    pub(super) fn ensure_social_fact_stake_returns_fit(
        &self,
        fact: &SocialFactState,
    ) -> Result<(), RejectReason> {
        let mut projected_returns: Vec<(ResourceOwner, ResourceKind, i64)> = Vec::new();
        if let Some(stake) = fact.stake.as_ref() {
            self.ensure_social_stake_return_fits(&fact.actor, stake, &mut projected_returns)?;
        }
        if let Some(challenge) = fact.challenge.as_ref()
            && let Some(stake) = challenge.stake.as_ref()
        {
            self.ensure_social_stake_return_fits(
                &challenge.challenger,
                stake,
                &mut projected_returns,
            )?;
        }
        Ok(())
    }

    fn ensure_social_stake_return_fits(
        &self,
        owner: &ResourceOwner,
        stake: &SocialStake,
        projected_returns: &mut Vec<(ResourceOwner, ResourceKind, i64)>,
    ) -> Result<(), RejectReason> {
        super::validate_social_stake(Some(stake))?;
        self.ensure_owner_exists(owner)?;
        let current = self
            .owner_stock(owner)
            .expect("owner exists after ensure_owner_exists")
            .get(stake.kind);
        let already_projected = projected_returns
            .iter()
            .filter(|(projected_owner, projected_kind, _)| {
                projected_owner == owner && *projected_kind == stake.kind
            })
            .map(|(_, _, amount)| *amount)
            .try_fold(0_i64, |total, amount| total.checked_add(amount))
            .ok_or(RejectReason::InvalidAmount {
                amount: stake.amount,
            })?;
        if current
            .checked_add(already_projected)
            .and_then(|total| total.checked_add(stake.amount))
            .is_none()
        {
            return Err(RejectReason::InvalidAmount {
                amount: stake.amount,
            });
        }
        projected_returns.push((owner.clone(), stake.kind, stake.amount));
        Ok(())
    }
}
