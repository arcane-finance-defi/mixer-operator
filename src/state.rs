use crate::mixer::MixerClientSender;

#[derive(Debug)]
pub struct MixerState {
    pub client: MixerClientSender,
}

impl MixerState {
    pub fn new(client: MixerClientSender) -> Self {
        MixerState { client }
    }
}
