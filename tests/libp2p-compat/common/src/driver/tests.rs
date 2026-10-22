use super::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

struct Fake {
    events: VecDeque<Event<u64, ()>>,
    actions: Rc<RefCell<Vec<Vec<u8>>>>,
    consumed: Rc<Cell<usize>>,
}
impl Adapter for Fake {
    type PeerId = u64;
    type ResponseChannel = ();
    type PublishError = ();
    fn identity() {
        unreachable!()
    }
    fn addresses(_: &[String]) {
        unreachable!()
    }
    fn new(_: bool, _: &str) -> Self {
        unreachable!()
    }
    async fn next(&mut self) -> Event<u64, ()> {
        self.consumed.set(self.consumed.get() + 1);
        self.events
            .pop_front()
            .expect("client exited before fixture exhausted")
    }
    fn request(&mut self, peer: &u64, bytes: Vec<u8>) {
        assert_eq!(*peer, 7);
        self.actions.borrow_mut().push(bytes);
    }
    fn respond(&mut self, _: (), _: Vec<u8>) {
        unreachable!()
    }
    fn publish(&mut self, bytes: Vec<u8>) -> Result<(), ()> {
        self.actions.borrow_mut().push(bytes);
        Ok(())
    }
}
fn drive(events: Vec<Event<u64, ()>>) -> Vec<Vec<u8>> {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let consumed = Rc::new(Cell::new(0));
    let expected = events.len();
    let network = Fake {
        events: events.into(),
        actions: actions.clone(),
        consumed: consumed.clone(),
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(exchange(false, network));
    assert_eq!(
        consumed.get(),
        expected,
        "client returned before all required events"
    );
    Rc::try_unwrap(actions).unwrap().into_inner()
}
#[test]
fn client_requires_rpc_and_gossip_after_publish() {
    assert_eq!(
        drive(vec![
            Event::Connected(7),
            Event::Subscribed(7),
            Event::Gossip(b"gossip-response".to_vec()),
            Event::Response(b"response".to_vec())
        ]),
        vec![b"request".to_vec(), b"gossip".to_vec()]
    );
}
#[test]
#[should_panic(expected = "assertion")]
fn unexpected_subscription_peer_is_not_accepted() {
    drive(vec![Event::Connected(7), Event::Subscribed(8)]);
}
#[test]
#[should_panic(expected = "RPC failure: disconnected")]
fn outbound_failure_remains_fatal() {
    drive(vec![Event::OutboundFailure("disconnected".into())]);
}
#[test]
#[should_panic(expected = "assertion")]
fn malformed_response_remains_fatal() {
    drive(vec![
        Event::Connected(7),
        Event::Response(b"wrong".to_vec()),
    ]);
}
