//! Feature-gated fixed-key protocol simulation, never physical custody evidence.
use super::*;
use crate::controlled_authority::fencing::*;
use ed25519_dalek::Signer;
use std::{
    net::{TcpListener, TcpStream},
    sync::atomic::AtomicBool,
    time::Duration,
};
fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n; 32])
}
fn public(n: u8) -> String {
    hex::encode(key(n).verifying_key().to_bytes())
}
fn h(n: u8) -> String {
    hex::encode([n; 32])
}
fn signed(bytes: Vec<u8>, n: u8) -> String {
    hex::encode(key(n).sign(&bytes).to_bytes())
}
// Both harnesses include the SAME original state and framing/command handler.
#[path = "endpoint_test_oracle_ledger.rs"]
mod oracle_ledger;
use oracle_ledger::Ledger;
#[path = "endpoint_test_oracle_frame.rs"]
mod oracle_frame;
use oracle_frame::{recv, send};
struct Fixture {
    authority: VerifiedAuthorityV1,
}
#[path = "endpoint_test_oracle.rs"]
mod oracle;
pub(in crate::controlled_authority) fn with_genesis_remote_pair_for_fixture(
    authority: &VerifiedAuthorityV1,
    run: impl FnOnce(Arc<RemoteMonotonicEndpointProvider>, Arc<RemoteMonotonicEndpointProvider>),
) -> Result<(), FormalError> {
    if authority.definition().profile_version != 3
        || authority.definition().primary.instance.endpoint_public_key != public(5)
        || authority.definition().replica.instance.endpoint_public_key != public(6)
        || authority.policy().fencing_provider_public_key != public(3)
    {
        return Err(FormalError::Invalid(
            "fixed-key GEN remote fixture scope".into(),
        ));
    }
    let fixture = Fixture {
        authority: authority.clone(),
    };
    let primary = oracle::Oracle::start(&fixture, EndpointRole::Primary);
    let replica = oracle::Oracle::start(&fixture, EndpointRole::Replica);
    let policy = TrustedFencingPolicyV1 {
        provider_public_key: public(3),
        authorized_resource_bindings: authority
            .policy()
            .authorized_fencing_resource_bindings
            .clone(),
    };
    let provider = |address| {
        RemoteMonotonicEndpointProvider::new(
            address,
            Duration::from_secs(5),
            key(17),
            policy.clone(),
        )
        .map(Arc::new)
        .map_err(fencing_error)
    };
    run(provider(primary.address)?, provider(replica.address)?);
    Ok(())
}
