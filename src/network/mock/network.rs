use crate::network::mock::hub::NetworkHub;
use crate::network::{Message, MessageProcessor, Network};
use anyhow::{anyhow, Context};
use std::sync::RwLock;

/// MockNetwork is a mock implementation of the Network trait for testing purposes.
/// It does not perform any real network operations but simulates message routing and processing through a `NetworkHub`.
/// 
/// Thread-safety is handled internally using Mutex for the processor, following a Go-like approach
/// where the struct can be safely shared via Arc<MockNetwork> without external locking.
/// MessageProcessor is inherently thread-safe, so we only need a simple Option wrapper.
pub struct MockNetwork {
    core: RwLock<InnerMockNetwork>,
}

struct InnerMockNetwork {
    hub: NetworkHub,
    processor: Option<MessageProcessor>,
}

impl MockNetwork {
    /// Creates a new instance of MockNetwork with the given NetworkHub.
    pub fn new(hub: NetworkHub) -> Self {
        MockNetwork {
            core: RwLock::new(InnerMockNetwork {
                hub,
                processor: None,
            }),
        }
    }

    /// This is the event handler for processing incoming messages come through the mock network.
    /// Arguments:
    /// * `message`: The incoming message to be processed.
    ///   Returns:
    /// * `Result<(), anyhow::Error>`: Returns Ok if the message was processed successfully, or an error if processing failed.
    pub fn incoming_message(&self, message: Message) -> anyhow::Result<()> {
        let core_guard = self.core
            .read()
            .map_err(|_| anyhow!("Failed to acquire read lock on core"))?;
        
        let processor = match core_guard.processor.as_ref() {
            Some(p) => p,
            None => return Err(anyhow!("No message processor registered")),
        };
        
        processor
            .process_incoming_message(message)
            .context("Failed to process incoming message")
    }
}

impl Clone for MockNetwork {
    fn clone(&self) -> Self {
        let core_guard = self.core.read().unwrap();
        MockNetwork {
            core: RwLock::new(InnerMockNetwork {
                hub: core_guard.hub.clone(),
                processor: core_guard.processor.clone(), // Share processor state between clones
            }),
        }
    }
}

impl Network for MockNetwork {
    /// Sends a message through the mock network by routing it through the NetworkHub.
    fn send_message(&self, message: Message) -> anyhow::Result<()> {
        let core_guard = self.core
            .read()
            .map_err(|_| anyhow!("Failed to acquire read lock on core"))?;
        
        core_guard.hub
            .route_message(message)
            .context("Failed to route message")
    }

    /// Registers a message processor to handle incoming messages.
    /// Only one processor can be registered at a time.
    /// If a processor is already registered, an error is returned.
    fn register_processor(
        &self,
        processor: MessageProcessor,
    ) -> anyhow::Result<()> {
        let mut core_guard = self.core
            .write()
            .map_err(|_| anyhow!("Failed to acquire write lock on core"))?;
        
        match core_guard.processor.as_ref() {
            Some(_) => Err(anyhow!("A message processor is already registered")),
            None => {
                core_guard.processor = Some(processor);
                Ok(())
            }
        }
    }

    fn clone_box(&self) -> Box<dyn Network> {
        Box::new(self.clone())
    }
}
