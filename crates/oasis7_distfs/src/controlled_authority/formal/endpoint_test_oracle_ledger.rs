use super::*;
#[derive(Clone)]
pub(super) struct Ledger {
    pub(super) checkpoint: EndpointCheckpointBindingV1,
    pub(super) native: Vec<u8>,
    pub(super) pending: Option<CheckpointIntentV1>,
    pub(super) seal: Option<EpochSealBodyV1>,
}
