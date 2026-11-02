//! Real Native ready/claim/current/store adapter component test. The Account
//! callbacks and matching ordinary World delivery are explicit signed fixtures.
//! Not actual CF, formal World, Native45, or a Read300 lane qualification.
use super::*;
use oasis7_custody_core::native_execution_prepare_v2::ExecutionPreparePhysicalEdgeV2 as Edge;
use oasis7_custody_core::{execution_dispatch_gate_v2::*, native_execution_prepare_v2::*};
#[path = "custody_execution_callback_request_fixture_v2.rs"]
mod request_fixture;
const REPLY_KIND: &str = "custody_execution_prepare_reply_observation_v2";

fn cold_rows(ready: &ReadyNativeRegistryV2, kind: &str) -> Vec<serde_json::Value> {
    let (path, namespace, rp) = {
        let lock = ready.service.lock().unwrap();
        let config = &lock.as_ref().unwrap().config;
        (
            config.sqlite_path.clone(),
            config.namespace.clone(),
            config.rp.clone(),
        )
    };
    let mut cold = identity::store::IdentityStore::open(&path, namespace, rp).unwrap();
    cold.attach_machine_restore_guard_from_env(&path).unwrap();
    cold.require_guarded_machine_storage().unwrap();
    cold.transaction(|tx| {
        Ok(tx
            .all::<serde_json::Value>(kind, None)?
            .into_iter()
            .map(|(_, row)| row.value)
            .collect())
    })
    .unwrap()
}
fn run(name: &str, scenario: &str) {
    if std::env::var("OASIS7_EXECUTION_READY_CHILD")
        .ok()
        .as_deref()
        != Some("true")
    {
        child_for(name, "https://identity.fixture", &[]);
        return;
    }
    with_ready_registry(8, |ready, world| {
        let q = request_fixture::request(&ready);
        let raw = q.wire_bytes().unwrap();
        let authorization = request_fixture::claim(&q, Edge::IdentityActorToNative, &raw, 107);
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&authorization).unwrap());
        let headers = format!(
            "{}\r\nOasis7-Custody-Execution-Dispatch: {}\r\n\r\n",
            ready.headers.strip_suffix("\r\n\r\n").unwrap(),
            header
        );
        let owned = {
            let mut lock = ready.service.lock().unwrap();
            lock.as_mut()
                .unwrap()
                .prepare_execution_funded_v2(
                    "POST",
                    NATIVE_EXECUTION_PREPARE_PATH_V2,
                    &headers,
                    &raw,
                )
                .unwrap()
        };
        assert_eq!(
            cold_rows(&ready, "custody_execution_prepare_gated_v2").len(),
            1
        );
        assert!(cold_rows(&ready, REPLY_KIND).is_empty());
        if scenario == "unknown" {
            // Simulate lost process work after first durable claim, no World I/O.
            drop(owned);
            let mut lock = ready.service.lock().unwrap();
            assert!(
                lock.as_mut()
                    .unwrap()
                    .prepare_execution_funded_v2(
                        "POST",
                        NATIVE_EXECUTION_PREPARE_PATH_V2,
                        &headers,
                        &raw
                    )
                    .is_err()
            );
            drop(lock);
            assert!(cold_rows(&ready, REPLY_KIND).is_empty());
            assert_eq!(world.finish(), 3);
            return;
        }
        let mut count = 0;
        let completed = owned
            .send(|edge, body, _native_reentry, previous| {
                assert_eq!(previous.claim.claim_ordinal.0, 107 + count);
                count += 1;
                Ok(request_fixture::claim(&q, edge, body, 107 + count))
            })
            .unwrap();
        assert_eq!(
            count, 2,
            "two explicit fixture Account leaf responses, no hidden retry"
        );
        assert!(cold_rows(&ready, REPLY_KIND).is_empty());
        if scenario == "current_denied" {
            let sid = headers
                .split("__Host-oasis7_sid=")
                .nth(1)
                .unwrap()
                .split('\r')
                .next()
                .unwrap();
            ready
                .service
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .logout(Some(sid), now_ms())
                .unwrap();
        }
        let result = ready
            .service
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .finish_execution_funded_v2(completed);
        if scenario == "current_denied" {
            assert!(
                result.is_err(),
                "fresh Native Current denial cannot archive a success"
            );
            assert!(cold_rows(&ready, REPLY_KIND).is_empty());
        } else {
            let response = result.unwrap();
            // This is the exact adapter return boundary used before static HTTP
            // serialization. Read a different guarded store before delivering it.
            let rows = cold_rows(&ready, REPLY_KIND);
            assert_eq!(
                rows.len(),
                1,
                "successful adapter return must already be durable"
            );
            assert!(
                rows[0]["reply"] == response,
                "complete reply including both leaf claims is durable"
            );
            let decoded: NativeExecutionPrepareGatedReplyV2 =
                serde_json::from_value(rows[0]["reply"].clone()).unwrap();
            ws::task_context_projection::verify_prepared_delivery_v2(
                &q.original.request,
                &decoded.reply.world_response,
                &q.proof.certificate.world.service_public_key,
            )
            .unwrap();
            let claims = cold_rows(&ready, "custody_execution_prepare_gated_v2");
            assert!(claims[0]["request"] == serde_json::to_value(&q).unwrap());
            assert!(rows[0]["configuration_digest"] == claims[0]["configuration_digest"]);
            {
                let mut lock = ready.service.lock().unwrap();
                assert!(
                    lock.as_mut()
                        .unwrap()
                        .prepare_execution_funded_v2(
                            "POST",
                            NATIVE_EXECUTION_PREPARE_PATH_V2,
                            &headers,
                            &raw
                        )
                        .is_err(),
                    "duplicate original cannot reconstruct owned World work"
                );
                let sid = headers
                    .split("__Host-oasis7_sid=")
                    .nth(1)
                    .unwrap()
                    .split('\r')
                    .next()
                    .unwrap();
                lock.as_mut().unwrap().logout(Some(sid), now_ms()).unwrap();
            }
            assert!(
                cold_rows(&ready, REPLY_KIND) == rows,
                "later revocation cannot erase immutable history or revive Current"
            );
        }
        assert_eq!(
            cold_rows(&ready, "custody_execution_prepare_gated_v2").len(),
            1,
            "failures and duplicates retain the first once claim"
        );
        assert_eq!(
            world.finish(),
            5,
            "three ready reads and exactly two owned World reads"
        );
    });
}
#[test]
fn native_prepare_reply_is_durable_before_return_and_cold_duplicate_cannot_resend() {
    run(
        "native_prepare_reply_is_durable_before_return_and_cold_duplicate_cannot_resend",
        "success",
    );
}
#[test]
fn native_prepare_reply_final_current_denial_keeps_claim_without_archive() {
    run(
        "native_prepare_reply_final_current_denial_keeps_claim_without_archive",
        "current_denied",
    );
}
#[test]
fn native_prepare_reply_legacy_claim_only_unknown_cannot_resend() {
    run(
        "native_prepare_reply_legacy_claim_only_unknown_cannot_resend",
        "unknown",
    );
}
