//! Keep protected charging coupled to the actual pinned libp2p codec, rather
//! than assuming serde_cbor's independent encoding equals wire output.
use libp2p::request_response::Codec;
use oasis7_proto::distributed_net::{NetworkRequest, NetworkResponse};
#[test]
fn provider_input_charge_matches_actual_libp2p_codec_response_at_length_and_integer_edges() {
    let protocol = libp2p::StreamProtocol::new("/oasis7/bft/admission-input/1.0.0");
    for length in [1usize, 23, 24, 255, 256, 65_535, 65_536] {
        for value in [0u8, 23, 24, 255] {
            let payload = vec![value; length];
            let response = NetworkResponse {
                payload: payload.clone(),
            };
            let mut codec =
                libp2p::request_response::cbor::codec::Codec::<NetworkRequest, NetworkResponse>::default();
            let mut wire = futures::io::Cursor::new(Vec::new());
            futures::executor::block_on(codec.write_response(&protocol, &mut wire, response))
                .unwrap();
            let encoded = wire.into_inner();
            assert_eq!(
                oasis7_distfs::protected_bft_v3::bft_original_input_network_response_bytes_v1(
                    &payload
                )
                .unwrap(),
                encoded.len() as u64,
                "actual Codec::write_response length={length} byte={value}"
            );
        }
    }
}
