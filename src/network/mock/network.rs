use crate::network::mock::hub::NetworkHub;
use crate::network::{Event, MessageProcessor, Network};
use anyhow::{anyhow, Context};
use parking_lot::RwLock;
use crate::core::Identifier;

/// MockNetwork is a mock implementation of the Network trait for testing purposes.
/// It does not perform any real network operations but simulates event routing and processing through a `NetworkHub`.
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
    id: Identifier, // Identifier of the mock network
}

impl MockNetwork {
    /// Creates a new instance of MockNetwork with the given NetworkHub.
    pub fn new(id: Identifier, hub: NetworkHub) -> Self {
        MockNetwork {
            core: RwLock::new(InnerMockNetwork {
                hub,
                processor: None,
                id,
            }),
        }
    }

    /// This is the event handler for processing incoming events come through the mock network.
    /// Arguments:
    /// * `event`: The incoming event to be processed.
    ///   Returns:
    /// * `Result<(), anyhow::Error>`: Returns Ok if the event was processed successfully, or an error if processing failed.
    pub fn incoming_event(&self, origin_id: Identifier, event: Event) -> anyhow::Result<()> {
        let core_guard = self.core.read();
        
        let processor = match core_guard.processor.as_ref() {
            Some(p) => p,
            None => return Err(anyhow!("no event processor registered")),
        };
        
        processor
            .process_incoming_event(origin_id, event)
            .context("failed to process incoming event")
    }
}

impl Clone for MockNetwork {
    fn clone(&self) -> Self {
        let core_guard = self.core.read();
        MockNetwork {
            core: RwLock::new(InnerMockNetwork {
                hub: core_guard.hub.clone(),
                processor: core_guard.processor.clone(), // Share processor state between clones
                id: core_guard.id,
            }),
        }
    }
}

impl Network for MockNetwork {
    /// Sends an event through the mock network by routing it through the NetworkHub.
    fn send_event(&self, target_id: Identifier, event: Event) -> anyhow::Result<()> {
        let core_guard = self.core.read();
        
        core_guard.hub
            .route_event(core_guard.id, target_id, event)
            .context("failed to route event")
    }

    /// Registers an event processor to handle incoming events.
    /// Only one processor can be registered at a time.
    /// If a processor is already registered, an error is returned.
    fn register_processor(
        &self,
        processor: MessageProcessor,
    ) -> anyhow::Result<()> {
        let mut core_guard = self.core.write();
        
        match core_guard.processor.as_ref() {
            Some(_) => Err(anyhow!("an event processor is already registered")),
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
