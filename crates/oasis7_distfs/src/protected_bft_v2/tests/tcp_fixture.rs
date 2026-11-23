use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub(super) struct TcpFixture {
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    accepted: Arc<AtomicUsize>,
    join: Option<thread::JoinHandle<()>>,
}

impl TcpFixture {
    pub(super) fn address(&self) -> SocketAddr {
        self.address
    }

    pub(super) fn finish(mut self) {
        self.stop();
        self.join.take().unwrap().join().unwrap();
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_secs(3));
    }
}

impl Drop for TcpFixture {
    fn drop(&mut self) {
        if let Some(join) = self.join.take() {
            self.stop();
            let result = join.join();
            if !thread::panicking() {
                result.unwrap();
            }
        }
    }
}

pub(super) fn spawn(service: Arc<BftSignerServiceV2>) -> TcpFixture {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let accepted = Arc::new(AtomicUsize::new(0));
    let stop = stopped.clone();
    let count = accepted.clone();
    let join = thread::spawn(move || {
        let mut workers = Vec::new();
        let mut over_budget = false;
        loop {
            let (mut stream, _) = listener.accept().unwrap();
            if stop.load(Ordering::Acquire) {
                break;
            }
            // Three logical leases plus the one authenticated conflict in the
            // regression below. Stop owns termination, not the accept count.
            if workers.len() == 4 {
                over_budget = true;
                break;
            }
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            count.fetch_add(1, Ordering::Release);
            let service = service.clone();
            workers.push(thread::spawn(move || {
                service
                    .serve_connection(&mut stream)
                    .expect("test signer TCP connection should close cleanly");
            }));
        }
        let mut clean = true;
        for worker in workers {
            clean &= worker.join().is_ok();
        }
        assert!(clean, "all signer connection workers must close cleanly");
        assert!(
            !over_budget,
            "signer fixture exceeded its four-connection budget"
        );
    });
    TcpFixture {
        address,
        stopped,
        accepted,
        join: Some(join),
    }
}

#[test]
fn released_preparation_conflict_keeps_the_fourth_connection_available() {
    let fixture = harness();
    let server = spawn(fixture.service.clone());
    let provider = RemoteBftSignerProviderV2::new(
        server.address(),
        Duration::from_secs(3),
        fixture.binding.clone(),
        fixture.requester.clone(),
        fixture.trust.clone(),
    )
    .unwrap();
    let mut first = provider.acquire(&fixture.binding).unwrap();
    let first_checkpoint = first.checkpoint().unwrap();
    drop(first);
    security::wait_for_released_generation(&fixture, first_checkpoint.lease_generation + 1);

    let mut recovered = provider.acquire(&fixture.binding).unwrap();
    let checkpoint = recovered.checkpoint().unwrap();
    let mut competing = TcpStream::connect(server.address()).unwrap();
    let (_, prepared) = direct_exchange(
        &mut competing,
        &fixture.binding,
        &fixture.requester,
        None,
        BftSignerCommandV2::PrepareAcquire {
            binding: Box::new(fixture.binding.clone()),
        },
        101,
        &fixture.trust,
    );
    let BftSignerReplyKindV2::Prepared {
        expected_generation,
        prepare_nonce,
    } = prepared.body.result
    else {
        panic!("third connection must prepare against the still-live recovered lease");
    };
    assert_eq!(expected_generation, checkpoint.lease_generation);
    drop(recovered);
    security::wait_for_released_generation(&fixture, expected_generation + 1);
    let (_, conflict) = direct_exchange(
        &mut competing,
        &fixture.binding,
        &fixture.requester,
        None,
        BftSignerCommandV2::Acquire {
            binding: Box::new(fixture.binding.clone()),
            expected_generation,
            prepare_nonce,
        },
        102,
        &fixture.trust,
    );
    assert!(matches!(
        conflict.body.result,
        BftSignerReplyKindV2::AcquireConflict { current_generation, lease_active: false }
            if current_generation == expected_generation + 1
    ));
    drop(competing);

    let mut final_lease = provider
        .acquire(&fixture.binding)
        .expect("authenticated released-generation conflict must permit a fresh fourth connection");
    let final_checkpoint = final_lease.checkpoint().unwrap();
    assert_eq!(final_checkpoint.state, checkpoint.state);
    assert_eq!(final_checkpoint.lease_generation, expected_generation + 2);
    assert_eq!(server.accepted.load(Ordering::Acquire), 4);
    drop(final_lease);
    server.finish();
}
