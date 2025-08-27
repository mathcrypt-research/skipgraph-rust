pub mod mock;
mod processor;

use crate::core::Identifier;
#[allow(unused)]
pub use processor::MessageProcessor;

/// Payload enum defines the semantics of the message payload that can be sent over the network.
#[derive(Debug)]
pub enum Payload {
    TestMessage(String), // A payload for testing purposes, it is a simple string message, and is not used in production.
}

/// Message struct represents a message that can be sent over the network.
pub struct Message {
    pub payload: Payload,
    pub target_node_id: Identifier,
}

/// Core message processing logic that implementations must provide.
/// This trait is deliberately simple and doesn't require thread-safety concerns.
/// The MessageProcessor wrapper handles all synchronization automatically.
pub trait MessageProcessorCore: Send + Sync {
    /// Process an incoming message. This method will be called with proper synchronization.
    fn process_incoming_message(&self, message: Message) -> anyhow::Result<()>;
}

/// Network trait defines the interface for a network service that can send and receive messages.
#[unimock::unimock(api=NetworkMock)]
pub trait Network: Send + Sync {
    /// Sends a message to the network.
    fn send_message(&self, message: Message) -> anyhow::Result<()>;

    /// Registers a message processor to handle incoming messages.
    /// At any point in time, there can be only one processor registered.
    /// Registering a new processor is illegal if there is already a processor registered, and causes an error.
    fn register_processor(&self, processor: MessageProcessor) -> anyhow::Result<()>;

    /// Creates a shallow copy of this networking layer instance.
    ///
    /// Implementations should ensure that cloned instances share the same underlying data
    /// (e.g., using Arc for shared ownership). Changes made through one instance should be
    /// visible in all cloned instances. This is the standard cloning behavior for all
    /// Network implementations.
    fn clone_box(&self) -> Box<dyn Network>;
}

impl Clone for Box<dyn Network> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
