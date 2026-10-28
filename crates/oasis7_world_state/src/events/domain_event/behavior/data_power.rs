use super::super::*;

#[derive(Debug, PartialEq)]
pub(super) enum DataPowerView<'a> {
    ResourceTransferred {
        from_agent_id: &'a String,
        to_agent_id: &'a String,
        kind: &'a ResourceKind,
        amount: &'a i64,
    },
    DataCollected {
        collector_agent_id: &'a String,
        electricity_cost: &'a i64,
        data_amount: &'a i64,
    },
    DataCollectedAuthenticated {
        collector_agent_id: &'a String,
        electricity_cost: &'a i64,
        data_amount: &'a i64,
        player_id: &'a String,
        public_key: &'a String,
        nonce: &'a u64,
        signature: &'a String,
    },
    DataAccessGranted {
        owner_agent_id: &'a String,
        grantee_agent_id: &'a String,
    },
    DataAccessRevoked {
        owner_agent_id: &'a String,
        grantee_agent_id: &'a String,
    },
    PowerRedeemed {
        node_id: &'a String,
        target_agent_id: &'a String,
        burned_credits: &'a u64,
        granted_power_units: &'a i64,
        reserve_remaining: &'a i64,
        nonce: &'a u64,
    },
    PowerRedeemRejected {
        node_id: &'a String,
        target_agent_id: &'a String,
        redeem_credits: &'a u64,
        nonce: &'a u64,
        reason: &'a String,
    },
}

impl<'a> DataPowerView<'a> {
    pub(super) fn from_event(event: &'a DomainEvent) -> Self {
        match event {
            DomainEvent::ResourceTransferred {
                from_agent_id,
                to_agent_id,
                kind,
                amount,
            } => Self::ResourceTransferred {
                from_agent_id,
                to_agent_id,
                kind,
                amount,
            },
            DomainEvent::DataCollected {
                collector_agent_id,
                electricity_cost,
                data_amount,
            } => Self::DataCollected {
                collector_agent_id,
                electricity_cost,
                data_amount,
            },
            DomainEvent::DataCollectedAuthenticated {
                collector_agent_id,
                electricity_cost,
                data_amount,
                player_id,
                public_key,
                nonce,
                signature,
            } => Self::DataCollectedAuthenticated {
                collector_agent_id,
                electricity_cost,
                data_amount,
                player_id,
                public_key,
                nonce,
                signature,
            },
            DomainEvent::DataAccessGranted {
                owner_agent_id,
                grantee_agent_id,
            } => Self::DataAccessGranted {
                owner_agent_id,
                grantee_agent_id,
            },
            DomainEvent::DataAccessRevoked {
                owner_agent_id,
                grantee_agent_id,
            } => Self::DataAccessRevoked {
                owner_agent_id,
                grantee_agent_id,
            },
            DomainEvent::PowerRedeemed {
                node_id,
                target_agent_id,
                burned_credits,
                granted_power_units,
                reserve_remaining,
                nonce,
            } => Self::PowerRedeemed {
                node_id,
                target_agent_id,
                burned_credits,
                granted_power_units,
                reserve_remaining,
                nonce,
            },
            DomainEvent::PowerRedeemRejected {
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                reason,
            } => Self::PowerRedeemRejected {
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                reason,
            },
            _ => unreachable!("event was routed to the wrong data_power view"),
        }
    }
}
