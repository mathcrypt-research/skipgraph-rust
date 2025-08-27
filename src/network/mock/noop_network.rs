use crate::network::{Message, MessageProcessor, Network};

/// A no-operation network implementation that ignores all messages and processor registrations.
/// This is useful for testing components that depend on the Network trait without performing any real networking.
pub struct NoopNetwork {

}

impl Network for NoopNetwork {
    fn send_message(&self, _message: Message) -> anyhow::Result<()> {
        Ok(())
    }

    fn register_processor(&self, _processor: MessageProcessor) -> anyhow::Result<()> {
        Ok(())
    }

    fn clone_box(&self) -> Box<dyn Network> {
        Box::new(NoopNetwork {})
    }
}

impl NoopNetwork {
    /// Creates a new instance of NoopNetwork.
    pub fn new() -> Self {
        NoopNetwork {}
    }
}