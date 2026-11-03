//! Pinned real transport codec, not a second handwritten encoding oracle.
use libp2p::request_response::Codec;
use oasis7_proto::distributed_net::{NetworkRequest, NetworkResponse};
#[test]
fn provider_original_transfer_bound_matches_pinned_real_response_codec() {
    for length in [1usize, 23, 24, 255, 256, 65_535, 65_536] {
        let payload = (0..length).map(|n| (n % 256) as u8).collect::<Vec<_>>();
        let expected =
            oasis7_distfs::protected_bft_v3::bft_original_input_network_response_bytes_v1(&payload)
                .unwrap();
        let mut codec = libp2p::request_response::cbor::codec::Codec::<
            NetworkRequest,
            NetworkResponse,
        >::default();
        let mut stream = futures::io::Cursor::new(Vec::<u8>::new());
        futures::executor::block_on(codec.write_response(
            &libp2p::StreamProtocol::new("/oasis7/test/original-input/1"),
            &mut stream,
            NetworkResponse { payload },
        ))
        .unwrap();
        assert_eq!(stream.into_inner().len() as u64, expected);
    }
}
