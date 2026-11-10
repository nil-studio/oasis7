//! Fresh pre-accept IWS fixture only. This cannot sign production receipts.
use super::*;
impl FileFormalEndpoint {
    /// Reuses the original controlled storage fixture for content issuance.
    /// Restricted to fresh GEN, IWS profile and the explicit public test keys.
    /// Production constructors and protected signer checks remain unchanged.
    pub fn open_controlled_for_fixture(
        path: &Path,
        authorization: FormalEndpointAuthorizationV1,
        role: EndpointRole,
        key: SigningKey,
        provider: Arc<dyn fencing::MonotonicEndpointProvider>,
    ) -> Result<Self, FormalError> {
        let definition = authorization.authority.definition();
        let primary = hex::encode(SigningKey::from_bytes(&[5; 32]).verifying_key().to_bytes());
        let replica = hex::encode(SigningKey::from_bytes(&[6; 32]).verifying_key().to_bytes());
        let expected = match role {
            EndpointRole::Primary => &primary,
            EndpointRole::Replica => &replica,
        };
        if definition.profile_version != oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_AUTHORITY_PROFILE_VERSION
            || definition.primary.instance.endpoint_public_key != primary
            || definition.replica.instance.endpoint_public_key != replica
            || hex::encode(key.verifying_key().to_bytes()) != *expected
            || authorization.parent.is_some() || authorization.recovery.is_some()
            || path.join("formal-checkpoint-v1.cbor").try_exists().map_err(io)? {
            return Err(invalid("fresh IWS fixture scope required"));
        }
        Self::open_inner(path, authorization, role, key, provider, true, None)
    }
}
