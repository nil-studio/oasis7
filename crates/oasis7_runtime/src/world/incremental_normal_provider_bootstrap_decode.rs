//! Closed DATA layout of the actual ProviderBacked bootstrap wire. No authority
//! installation or execution permission is minted by decoding these records.
use crate::ProviderBackedBootstrapAuthorityV1;
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_cbor_data::{
        self as cbor, BoundedCborDataErrorV1, CborDataLayoutV1, CborDataLimitsV1, PaidCborDataV1,
        PaidCborRefusalV1,
    },
    bounded_context::FormalBoundedContextV1,
};
/// Every reachable nominal is inline in Authority: identity, authority record,
/// finality proof/binding/certificate, grant subject/audience/issuer/scope and
/// invocation context. Its only collection elements are String and Authority;
/// maps are String→String and sets String. All serde defaults are empty/zero.
/// Using the enclosing actual sizeof bounds every inline child (including the
/// internally tagged CapabilitySubject Content visitor) without fake Action
/// layout or a second list of the field grammar.
pub fn provider_bootstrap_owned_layout_v1() -> CborDataLayoutV1 {
    CborDataLayoutV1 {
        root: std::mem::size_of::<Vec<ProviderBackedBootstrapAuthorityV1>>(),
        sequence_element: std::mem::size_of::<ProviderBackedBootstrapAuthorityV1>(),
        map_pair: std::mem::size_of::<(String, String)>(),
        set_element: std::mem::size_of::<String>(),
        box_pointee: std::mem::size_of::<ProviderBackedBootstrapAuthorityV1>(),
        default_heap_per_value: 0,
        symbols_bytes: 0,
        visits: 1,
        missing: None,
    }
}
pub fn decode_provider_bootstrap_data_v1(
    raw: &[u8],
    limits: &CborDataLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<
    Result<PaidCborDataV1<Vec<ProviderBackedBootstrapAuthorityV1>>, PaidCborRefusalV1>,
    BoundedCborDataErrorV1,
> {
    cbor::decode_closed_cbor_data_v1(raw, provider_bootstrap_owned_layout_v1(), limits, common)
}
#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
    use oasis7_proto::memory_budget::MemoryBudget;
    #[test]
    fn actual_bootstrap_vec_codec_and_declared_hints_are_separate_from_authority() {
        let limits = CborDataLimitsV1 {
            max_wire_bytes: 4096,
            max_graph_bytes: 1 << 20,
            max_depth: 128,
        };
        let memory = MemoryBudget::new(8 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(100_000, 1 << 24).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let empty = decode_provider_bootstrap_data_v1(&[0x80], &limits, &mut common)
            .unwrap()
            .ok()
            .unwrap();
        assert!(
            std::borrow::Borrow::<Vec<ProviderBackedBootstrapAuthorityV1>>::borrow(&empty)
                .is_empty()
        );
        drop(empty);
        assert_eq!(memory.used(), 0);
        // A huge declared sequence is fatal admission, never an accepted empty
        // authority list or a swallowed codec refusal.
        assert!(matches!(
            decode_provider_bootstrap_data_v1(
                &[0x9b, 255, 255, 255, 255, 255, 255, 255, 255],
                &limits,
                &mut common
            ),
            Err(BoundedCborDataErrorV1::Limit(_))
        ));
        assert_eq!(memory.used(), 0);
    }
}
