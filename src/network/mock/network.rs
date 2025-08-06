use crate::network::mock::hub::NetworkHub;
use crate::network::{Message, MessageProcessor, Network};
use anyhow::Context;
use std::cell::RefCell;
use std::rc::Rc;

/// MockNetwork is a mock implementation of the Network trait for testing purposes.
/// It does not perform any real network operations but simulates message routing and processing through a `NetworkHub`.
pub struct MockNetwork {
    hub: Rc<RefCell<NetworkHub>>,
    processor: Option<Box<Rc<RefCell<dyn MessageProcessor>>>>,
}

impl MockNetwork {
    /// Creates a new instance of MockNetwork with the given NetworkHub.
    pub fn new(hub: Rc<RefCell<NetworkHub>>) -> Self {
        MockNetwork {
            hub,
            processor: None,
        }
    }

    /// This is the event handler for processing incoming messages come through the mock network.
    /// Arguments:
    /// * `message`: The incoming message to be processed.
    ///
    /// Returns:
    /// * `Result<(), anyhow::Error>`: Returns Ok if the message was processed successfully, or an error if processing failed.
    pub fn incoming_message(&self, message: Message) -> anyhow::Result<()> {
        if let Some(ref processor) = self.processor {
            processor
                .borrow_mut()
                .process_incoming_message(message)
                .context("Failed to process incoming message")?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("No message processor registered"))
        }
    }
}

impl Network for MockNetwork {
    /// Sends a message through the mock network by routing it through the NetworkHub.
    fn send_message(&self, message: Message) -> anyhow::Result<()> {
        self.hub
            .borrow()
            .route_message(message)
            .context("Failed to route message")?;
        Ok(())
    }

    /// Registers a message processor to handle incoming messages.
    /// Only one processor can be registered at a time.
    /// If a processor is already registered, an error is returned.
    fn register_processor(
        &mut self,
        processor: Box<Rc<RefCell<dyn MessageProcessor>>>,
    ) -> anyhow::Result<()> {
        if self.processor.is_some() {
            return Err(anyhow::anyhow!("A message processor is already registered"));
        }
        self.processor = Some(processor);
        Ok(())
    }
}
