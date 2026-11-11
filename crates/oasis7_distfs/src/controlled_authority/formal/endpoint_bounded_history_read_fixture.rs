//! Actual original-protocol Qualified history, data source only.
use super::*;
pub(in super::super) fn with_history_read_fixture(run: impl FnOnce(&Store, &[String], &Path)) {
    let mut run = Some(run);
    with_signed_request_states_fixture(|stage, endpoint, _, _| {
        if stage != "Qualified" {
            return;
        }
        let id = endpoint
            .store
            .body
            .head_node
            .as_ref()
            .expect("real Qualified history root")
            .clone();
        let node: store::HistoryNode = endpoint.store.get(&id).unwrap();
        assert_eq!(node.head, endpoint.store.body.head);
        let mut ids = vec![id];
        ids.extend(node.ancestors.iter().map(|link| link.node.clone()));
        assert!(
            ids.len() >= 2,
            "ordinary Qualified must retain prior initial history"
        );
        let path = endpoint.store.path.clone();
        run.take().unwrap()(&endpoint.store, &ids, &path);
    });
    assert!(run.is_none());
}
