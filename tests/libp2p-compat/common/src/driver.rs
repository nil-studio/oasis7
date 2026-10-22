//! CLI execution and protocol exchange state machine.
use crate::protocol::{Adapter, Event};
use std::time::Duration;

pub fn main<A: Adapter>() {
    let args: Vec<_> = std::env::args().collect();
    match args[1].as_str() {
        "identity" => return A::identity(),
        "address" => return A::addresses(&args[2..]),
        _ => {}
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(25), run::<A>(&args))
            .await
            .expect("peer completes protocol exchange");
    });
}

async fn run<A: Adapter>(args: &[String]) {
    let server = args[1] == "server";
    exchange(server, A::new(server, &args[2])).await;
}

async fn exchange<A: Adapter>(server: bool, mut network: A) {
    let mut rpc_done = false;
    let mut gossip_done = false;
    let mut published = false;
    let mut connected = None;
    loop {
        match network.next().await {
            Event::Listening(address) if server => println!("READY {address}"),
            Event::Connected(peer) if !server => {
                connected = Some(peer);
                network.request(&peer, b"request".to_vec());
            }
            Event::Request(request, channel) if server => {
                assert_eq!(request, b"request");
                network.respond(channel, b"response".to_vec());
            }
            Event::Response(response) if !server => {
                assert_eq!(response, b"response");
                rpc_done = true;
            }
            Event::Subscribed(peer) if !server => {
                assert_eq!(Some(peer), connected);
                published = network.publish(b"gossip".to_vec()).is_ok();
            }
            Event::Gossip(message) => {
                if server {
                    assert_eq!(message, b"gossip");
                    network.publish(b"gossip-response".to_vec()).unwrap();
                } else {
                    assert_eq!(message, b"gossip-response");
                    gossip_done = true;
                }
            }
            Event::OutboundFailure(error) => panic!("RPC failure: {error}"),
            _ => {}
        }
        if !server && published && rpc_done && gossip_done {
            println!("PASS rpc gossip");
            return;
        }
    }
}

#[cfg(test)]
mod tests;
