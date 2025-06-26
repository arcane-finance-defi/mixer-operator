use crate::mixer::MixClientRequest;
use tokio::sync::mpsc;

pub struct MixerState {
    pub client: mpsc::Sender<MixClientRequest>,
}

impl MixerState {
    pub fn new(client: mpsc::Sender<MixClientRequest>) -> Self {
        MixerState { client }
    }
}
