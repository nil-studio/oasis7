//! A forwarding TCP peer changes only the charged counter, then signs with the
//! configured provider key. The real service accepts and charges its normal
//! request; malformed counters are rejected by the receiving client.
use super::*;
use crate::protected_bft_v3::wire::BftSignerReplyKindV3;
use crate::protected_bft_v3::wire::{
    SignedBftSignerReplyV3, SignedBftSignerRequestV3, encode_frame, read_frame,
};
use std::io::Write;
use std::net::{SocketAddr, TcpStream};

#[test]
fn private_retention_client_rejects_validly_signed_u64_max_transfer() {
    run_retention_case(3, false);
}

#[test]
fn private_retention_client_rejects_validly_signed_transfer_above_cap() {
    run_retention_case(4, false);
}

#[test]
fn private_retention_client_accepts_validly_signed_exact_transfer_boundary() {
    run_retention_case(5, false);
}

#[test]
fn private_retention_client_rejects_validly_signed_combined_material_overflow() {
    run_retention_case(6, false);
}

pub(super) fn proxy(
    address: SocketAddr,
    binding: BftSignerBindingV3,
    mode: Arc<AtomicUsize>,
) -> (SocketAddr, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let downstream = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut client, _) = listener.accept().unwrap();
        let mut upstream = TcpStream::connect(address).unwrap();
        while let Ok(request) = read_frame::<SignedBftSignerRequestV3>(&mut client) {
            upstream
                .write_all(&encode_frame(&request).unwrap())
                .unwrap();
            let mut reply: SignedBftSignerReplyV3 = match read_frame(&mut upstream) {
                Ok(reply) => reply,
                Err(_) => break,
            };
            let fault = mode.load(Ordering::SeqCst);
            if fault != 0
                && let BftSignerReplyKindV3::PrivateRetention {
                    reserved_response_bytes,
                    ..
                } = &mut reply.body.result
            {
                *reserved_response_bytes = match fault {
                    3 => u64::MAX,
                    4 => BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1 + 1,
                    5 => {
                        BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1
                            - reply
                                .body
                                .checkpoint
                                .as_ref()
                                .unwrap()
                                .state
                                .private_retention
                                .as_ref()
                                .unwrap()
                                .retained_material_bytes
                                .unwrap()
                    }
                    6 => BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1,
                    _ => unreachable!(),
                };
                reply = SignedBftSignerReplyV3::sign(
                    reply.body,
                    &Handle {
                        key: key(33),
                        disabled: None,
                    },
                )
                .unwrap();
                // Authentication remains valid; only budget admission can
                // reject this otherwise unchanged, request-bound response.
                crate::protected_bft_v3::wire::validate_reply(
                    &binding,
                    &BftSignerTrustV3 {
                        schema_version: 3,
                        provider_public_key: public(33),
                        binding_digest: request.body.binding_digest.clone(),
                    },
                    &request,
                    &reply,
                )
                .unwrap();
            }
            client.write_all(&encode_frame(&reply).unwrap()).unwrap();
        }
    });
    (downstream, worker)
}
