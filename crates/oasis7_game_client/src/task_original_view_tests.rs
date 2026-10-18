use super::*;
use crate::{authority, task_client::WorldServiceTaskClient};
use std::{
    io::{Read, Write},
    net::TcpListener,
};
fn body(stream: &mut std::net::TcpStream) -> (String, Vec<u8>) {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).unwrap();
    let count: usize = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap();
    let mut body = vec![0; count];
    stream.read_exact(&mut body).unwrap();
    (head, body)
}
#[test]
fn original_task_read_authenticates_exact_http_projection_before_exposing_narrow_dto() {
    for edit in 0..6 {
        let (request, mut response) = crate::verified_task_view::tests::fixture("agent:a");
        response.view["state"]["agents"] = serde_json::json!({});
        response.view["state"]["pending_material_transits"] =
            serde_json::json!({u64::MAX.to_string():{"requester_agent_id":"a","amount":3}});
        response.view["state"]["pending_factory_builds"] =
            serde_json::json!({"7":{"builder_agent_id":"a"}});
        response.view["state"]["governance_votes"] =
            serde_json::json!({"7":{"votes_by_agent":{"007":{}}}});
        if edit == 2 {
            response.view["scheduler_wakes"] = serde_json::json!([{"agent_id":"other"}]);
        }
        if edit == 4 {
            response.version.commit.world.world_id = "other-world".into();
        }
        if edit == 5 {
            response.view["canonical_agent_owner"] = serde_json::json!({"agent_id":"other","world_id":"world","reorg_epoch":0,"canonical_authority":{"branch_id":"main","agent_identity_generation":1}});
        }
        let response: ReadWorldViewResponse<OriginalTaskReadProjectionV1> =
            serde_json::from_value(serde_json::to_value(response).unwrap()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = crate::tests::config(format!("http://{}", listener.local_addr().unwrap()));
        let client = WorldServiceTaskClient::new(config).unwrap();
        let service = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let (head, body) = body(&mut stream);
            assert!(head.starts_with(&format!("POST {VIEW_PATH} ")));
            let signed: SignedReadRequest<ReadWorldViewRequest> =
                serde_json::from_slice(&body).unwrap();
            authority::verify_read_request(VIEW_PATH, &signed).unwrap();
            assert_eq!(signed.request.scope_id, "agent:a");
            let digest = authority::request_digest(VIEW_PATH, &signed).unwrap();
            let mut reply = authority::sign_service_response(
                VIEW_PATH,
                digest,
                response,
                &hex::encode([if edit == 1 { 8 } else { 9 }; 32]),
            )
            .unwrap();
            if edit == 3 {
                reply.request_digest = "changed-request".into();
            }
            let bytes = serde_json::to_vec(&reply).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            stream.write_all(&bytes).unwrap();
        });
        let result = client.read_original_view(request);
        if edit == 0 {
            let view = result.unwrap();
            assert_eq!(view.logical_tick(), 4);
            assert_eq!(view.projection().observation().state.time, 4);
            assert!(view.projection().observation().state.agents.is_empty());
        } else {
            assert!(result.is_err(), "tamper {edit} must fail closed");
        }
        service.join().unwrap();
    }
}
