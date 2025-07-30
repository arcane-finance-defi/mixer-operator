use crate::mixer::MixerClentSender;

#[derive(Debug)]
pub struct MixerState {
    pub client: MixerClentSender,
}

impl MixerState {
    pub fn new(client: MixerClentSender) -> Self {
        MixerState { client }
    }
}
