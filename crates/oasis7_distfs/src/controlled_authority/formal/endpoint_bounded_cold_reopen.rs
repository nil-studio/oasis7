//! Existing protected endpoint reopen for the fixed cold recovery caller.
//! No directory creation, activation, archive import or new authority occurs.
use super::*;
use crate::controlled_authority::fencing::{
    BoundedAcquireErrorV1, RemoteMonotonicEndpointProvider,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::{
    CborDataLayoutV1, CborDataLimitsV1, reserve_closed_cbor_output_v1,
};
use std::mem::size_of;

impl FileFormalEndpoint {
    pub fn reopen_authenticated_existing_bounded_v1(
        path: &Path,
        authority: &VerifiedAuthorityV1,
        role: EndpointRole,
        provider: Arc<RemoteMonotonicEndpointProvider>,
        publication: FormalEndpointPublicationRefsV1<'_>,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Self, BoundedAcquireErrorV1> {
        let FormalEndpointPublicationRefsV1 {
            provider: actual_provider,
            policy: _,
            memory,
            work,
        } = publication;
        if !std::ptr::eq(provider.as_ref(), actual_provider)
            || authority.definition().predecessor_authority_id.is_some()
            || authority.durable_set_activation_digest().is_some()
            || authority.initial_body().is_none()
        {
            return Err(invalid("bounded endpoint cold initial authority/provider").into());
        }
        // Retain the original wire and merged reservation until endpoint
        // construction finishes; only the borrowed Common ends at this scope.
        let (_wire, fee, _second, original, authority, public_key) = {
            let mut common = work.bounded_context(memory, stop);
            let wire = store::context_wire::encode_paid(
                &(
                    authority.definition(),
                    authority.policy(),
                    authority.activation_ref(),
                    authority.initial_body(),
                    authority.initial_input_digest(),
                ),
                64 << 20,
                &mut common,
                "formal encoding exceeds byte bound",
                "cold endpoint authority clone",
            )?;
            // All reachable variable strings/vectors/maps occur in this closed
            // graph. Initial-authority private digest fields have fixed 32-byte
            // identities; the root also admits their string carriers and capacity.
            let envelope = size_of::<VerifiedAuthorityV1>()
                + 16 * size_of::<(
                    crate::controlled_authority::replicated_protocol::ArtifactRole,
                    String,
                )>();
            let layout = CborDataLayoutV1 {
                root: size_of::<Self>() + 1024,
                sequence_element: envelope,
                map_pair: envelope,
                set_element: envelope,
                box_pointee: envelope,
                default_heap_per_value: envelope,
                symbols_bytes: 0,
                visits: 16,
                missing: None,
            };
            let limits = CborDataLimitsV1 {
                max_wire_bytes: 64 << 20,
                max_graph_bytes: 64 << 20,
                max_depth: 128,
            };
            let map = |error| {
                match error {
                oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::BoundedCborDataErrorV1::Common(error) => error,
                error => invalid(format!("cold endpoint authority admission: {error}")),
            }
            };
            let mut fee =
                reserve_closed_cbor_output_v1(&wire, layout, &limits, &mut common).map_err(map)?;
            let mut second = reserve_closed_cbor_output_v1(
                &wire,
                CborDataLayoutV1 {
                    root: size_of::<VerifiedAuthorityV1>() + 1024,
                    sequence_element: envelope,
                    map_pair: envelope,
                    set_element: envelope,
                    box_pointee: envelope,
                    default_heap_per_value: envelope,
                    symbols_bytes: 0,
                    visits: 16,
                    missing: None,
                },
                &limits,
                &mut common,
            )
            .map_err(map)?;
            fee.merge(&mut second)
                .map_err(|_| invalid("cold endpoint authority fee pool"))?;
            common.charge(wire.len() as u64 * 16, wire.len() as u64 * 2)?;
            let original = authority.clone();
            let authority = authority.clone();
            let public_key = match role {
                EndpointRole::Primary => {
                    &authority.definition().primary.instance.endpoint_public_key
                }
                EndpointRole::Replica => {
                    &authority.definition().replica.instance.endpoint_public_key
                }
            }
            .clone();
            (wire, fee, second, original, authority, public_key)
        };
        let store = Store::reopen_existing_bounded(path, &authority, role, publication, stop)?;
        Ok(Self {
            store,
            authority,
            original,
            recovery: None,
            role,
            key: None,
            public_key,
            provider,
            parent: None,
            poisoned: false,
            physical_maintenance: storage_maintenance::MaintenanceState::default(),
            native_environment_jobs: std::collections::BTreeMap::new(),
            published_closure_jobs: std::collections::BTreeMap::new(),
            iws_page_store: None,
            #[cfg(any(test, feature = "test-support"))]
            controlled: false,
            _bounded_reopen_fee: Some(fee),
        })
    }
}
