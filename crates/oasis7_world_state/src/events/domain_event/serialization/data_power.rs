use super::*;

#[derive(serde::Serialize)]
#[serde(rename = "ResourceTransferred")]
struct ResourceTransferredPayload<'a> {
    from_agent_id: &'a String,
    to_agent_id: &'a String,
    kind: &'a ResourceKind,
    amount: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "DataCollected")]
struct DataCollectedPayload<'a> {
    collector_agent_id: &'a String,
    electricity_cost: &'a i64,
    data_amount: &'a i64,
}

#[derive(serde::Serialize)]
#[serde(rename = "DataCollectedAuthenticated")]
struct DataCollectedAuthenticatedPayload<'a> {
    collector_agent_id: &'a String,
    electricity_cost: &'a i64,
    data_amount: &'a i64,
    player_id: &'a String,
    public_key: &'a String,
    nonce: &'a u64,
    signature: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "DataAccessGranted")]
struct DataAccessGrantedPayload<'a> {
    owner_agent_id: &'a String,
    grantee_agent_id: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "DataAccessRevoked")]
struct DataAccessRevokedPayload<'a> {
    owner_agent_id: &'a String,
    grantee_agent_id: &'a String,
}

#[derive(serde::Serialize)]
#[serde(rename = "PowerRedeemed")]
struct PowerRedeemedPayload<'a> {
    node_id: &'a String,
    target_agent_id: &'a String,
    burned_credits: &'a u64,
    granted_power_units: &'a i64,
    reserve_remaining: &'a i64,
    nonce: &'a u64,
}

#[derive(serde::Serialize)]
#[serde(rename = "PowerRedeemRejected")]
struct PowerRedeemRejectedPayload<'a> {
    node_id: &'a String,
    target_agent_id: &'a String,
    redeem_credits: &'a u64,
    nonce: &'a u64,
    reason: &'a String,
}

pub(super) fn serialize<S: serde::Serializer>(
    event: &DomainEvent,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match event {
        DomainEvent::ResourceTransferred {
            from_agent_id,
            to_agent_id,
            kind,
            amount,
        } => serialize_adjacent(
            serializer,
            Tag::ResourceTransferred,
            &ResourceTransferredPayload {
                from_agent_id,
                to_agent_id,
                kind,
                amount,
            },
        ),
        DomainEvent::DataCollected {
            collector_agent_id,
            electricity_cost,
            data_amount,
        } => serialize_adjacent(
            serializer,
            Tag::DataCollected,
            &DataCollectedPayload {
                collector_agent_id,
                electricity_cost,
                data_amount,
            },
        ),
        DomainEvent::DataCollectedAuthenticated {
            collector_agent_id,
            electricity_cost,
            data_amount,
            player_id,
            public_key,
            nonce,
            signature,
        } => serialize_adjacent(
            serializer,
            Tag::DataCollectedAuthenticated,
            &DataCollectedAuthenticatedPayload {
                collector_agent_id,
                electricity_cost,
                data_amount,
                player_id,
                public_key,
                nonce,
                signature,
            },
        ),
        DomainEvent::DataAccessGranted {
            owner_agent_id,
            grantee_agent_id,
        } => serialize_adjacent(
            serializer,
            Tag::DataAccessGranted,
            &DataAccessGrantedPayload {
                owner_agent_id,
                grantee_agent_id,
            },
        ),
        DomainEvent::DataAccessRevoked {
            owner_agent_id,
            grantee_agent_id,
        } => serialize_adjacent(
            serializer,
            Tag::DataAccessRevoked,
            &DataAccessRevokedPayload {
                owner_agent_id,
                grantee_agent_id,
            },
        ),
        DomainEvent::PowerRedeemed {
            node_id,
            target_agent_id,
            burned_credits,
            granted_power_units,
            reserve_remaining,
            nonce,
        } => serialize_adjacent(
            serializer,
            Tag::PowerRedeemed,
            &PowerRedeemedPayload {
                node_id,
                target_agent_id,
                burned_credits,
                granted_power_units,
                reserve_remaining,
                nonce,
            },
        ),
        DomainEvent::PowerRedeemRejected {
            node_id,
            target_agent_id,
            redeem_credits,
            nonce,
            reason,
        } => serialize_adjacent(
            serializer,
            Tag::PowerRedeemRejected,
            &PowerRedeemRejectedPayload {
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                reason,
            },
        ),
        _ => unreachable!("DomainEvent serializer group mismatch"),
    }
}
