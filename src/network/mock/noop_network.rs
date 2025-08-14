use std::sync::{Arc, Mutex};
use crate::network::{Message, MessageProcessor, Network};

/// NoopNetwork is a mock implementation of the Network trait that does not perform any operations.
/// It is used for testing purposes where no actual network operations are needed.
struct  NoopNetwork {

}

impl Network for NoopNetwork {
    fn send_message(&self, _message: Message) -> anyhow::Result<()> {
        Ok(())
    }

    fn register_processor(&mut self, _processor: Box<Arc<Mutex<dyn MessageProcessor>>>) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Creates a new instance of NoopNetwork wrapped in an Arc and Mutex.
/// This is useful for testing scenarios where a network implementation is required but no actual operations are needed
pub(crate) fn new_noop_network() -> Arc<Mutex<dyn Network>> {
    Arc::new(Mutex::new(NoopNetwork {}))
}